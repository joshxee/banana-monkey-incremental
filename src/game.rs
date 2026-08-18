use std::time::Duration;

use bevy::{
    diagnostic::FrameCount,
    input::touch::Touches,
    prelude::*,
    sprite::{SpriteImageMode, SpriteScalingMode},
    window::PrimaryWindow,
};

use crate::{domain::Treasury, persistence};

const BANANA_FRAMES: usize = 12;
const BANANA_FRAME_SIZE: u32 = 16;
const KEYBOARD_HARVEST_SECONDS: f32 = 0.42;
const SUCCESS_PULSE_SECONDS: f32 = 0.18;
const TOUCH_MOUSE_SUPPRESSION_SECONDS: f32 = 0.5;
const SAVE_RETRY_INITIAL_SECONDS: f32 = 1.0;
const SAVE_RETRY_MAX_SECONDS: f32 = 30.0;

const INK: Color = Color::srgb(0.16, 0.08, 0.06);
const CREAM: Color = Color::srgb(1.0, 0.94, 0.72);
const BROWN: Color = Color::srgb(0.33, 0.14, 0.08);
const BROWN_LIGHT: Color = Color::srgb(0.52, 0.25, 0.12);
const GOLD: Color = Color::srgb(1.0, 0.75, 0.05);

macro_rules! diagnostic_log {
    ($frame:expr, $event:expr, $pointer:expr; $($detail:tt)*) => {
        push_web_diagnostic(
            $frame.0,
            $event,
            $pointer,
            format_args!($($detail)*),
        );
    };
    ($frame:expr, $event:expr, $($detail:tt)*) => {
        push_web_diagnostic($frame.0, $event, None, format_args!($($detail)*));
    };
}

pub struct HarvestGamePlugin;

impl Plugin for HarvestGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneLayout>()
            .init_resource::<HarvestController>()
            .init_resource::<PendingSettlement>()
            .init_resource::<PersistenceDirty>()
            .init_resource::<Feedback>()
            .init_resource::<MenuState>()
            .init_resource::<PointerGuard>()
            .init_resource::<DiagnosticPointerTrace>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    refresh_layout,
                    apply_layout,
                    apply_responsive_hud,
                    handle_menu,
                    sync_menu_visibility,
                    handle_harvest_input,
                    move_keyboard_harvest,
                    settle_harvest,
                    persist_changes,
                    animate_banana,
                    update_feedback,
                    sync_counter,
                    style_buttons,
                    sync_web_test_state,
                )
                    .chain(),
            );
    }
}

#[derive(Component)]
struct MainCamera;

#[derive(Component)]
struct Sky;

#[derive(Component)]
struct GroundFill;

#[derive(Component)]
struct GroundEdge;

#[derive(Component)]
struct HarvestZone;

#[derive(Component)]
struct DepositZone;

#[derive(Component)]
struct DepositGlow;

#[derive(Component)]
struct HarvestLabel;

#[derive(Component)]
struct DepositLabel;

#[derive(Component)]
struct Banana;

#[derive(Component)]
struct BananaAnimation {
    timer: Timer,
}

#[derive(Component)]
struct CounterText;

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
struct OpenMenuButton;

#[derive(Component, Clone, Copy)]
enum LayoutElement {
    Sky,
    GroundFill,
    GroundEdge,
    HarvestZone,
    DepositZone,
    DepositGlow,
    Banana,
    HarvestLabel,
    DepositLabel,
}

#[derive(Component, Clone, Copy)]
enum MenuView {
    Scrim,
    Main,
    Restart,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum ButtonAction {
    OpenMenu,
    Resume,
    #[cfg(target_arch = "wasm32")]
    Diagnostics,
    Restart,
    ConfirmRestart,
    CancelRestart,
}

#[derive(Component, Clone, Copy)]
enum MenuButtonTone {
    Primary,
    Emphasized,
    #[cfg(target_arch = "wasm32")]
    Secondary,
}

#[derive(Resource, Debug, Clone, Copy)]
struct SceneLayout {
    viewport: Vec2,
    zone_size: f32,
    harvest: Vec2,
    deposit: Vec2,
    banana_home: Vec2,
    banana_size: f32,
    harvest_bounds: Rect,
    deposit_bounds: Rect,
    ground_top: f32,
}

impl Default for SceneLayout {
    fn default() -> Self {
        Self::for_viewport(Vec2::new(1280.0, 720.0))
    }
}

impl SceneLayout {
    fn for_viewport(viewport: Vec2) -> Self {
        let width = viewport.x.max(320.0);
        let height = viewport.y.max(320.0);
        let available_zone_size = (width * 0.28).min(height * 0.38);
        let zone_size = if available_zone_size >= 192.0 {
            256.0
        } else {
            128.0
        };
        let horizontal_offset = (width * 0.31).clamp(96.0, 360.0);
        let ground_top = -height * 0.28;
        let zone_y = ground_top + zone_size * 0.5;
        let harvest = Vec2::new(-horizontal_offset, zone_y);
        let deposit = Vec2::new(horizontal_offset, zone_y);
        let banana_size = if width < 720.0 { 48.0 } else { 64.0 };
        let banana_home = harvest + Vec2::new(zone_size * 0.34, zone_size * 0.2);
        let banana_hitbox = Rect::from_center_size(banana_home, Vec2::splat(72.0));
        let harvest_bounds =
            Rect::from_center_size(harvest, Vec2::splat(zone_size)).union(banana_hitbox);

        Self {
            viewport: Vec2::new(width, height),
            zone_size,
            harvest,
            deposit,
            banana_home,
            banana_size,
            harvest_bounds,
            deposit_bounds: Rect::from_center_size(
                deposit + Vec2::new(0.0, zone_size * 0.02),
                Vec2::splat(zone_size + 36.0),
            ),
            ground_top,
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn world_to_screen(self, point: Vec2) -> Vec2 {
        Vec2::new(
            point.x + self.viewport.x * 0.5,
            self.viewport.y * 0.5 - point.y,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PointerId {
    Mouse,
    Touch(u64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum HarvestInteraction {
    Idle,
    Dragging { pointer: PointerId, position: Vec2 },
    KeyboardHarvest { elapsed: f32, warmup_frames: u8 },
}

#[derive(Resource, Debug)]
struct HarvestController {
    interaction: HarvestInteraction,
}

impl Default for HarvestController {
    fn default() -> Self {
        Self {
            interaction: HarvestInteraction::Idle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettlementSource {
    Pointer(PointerId),
    Keyboard,
}

#[derive(Resource, Debug, Default)]
struct PendingSettlement(Option<SettlementSource>);

#[derive(Resource, Debug)]
struct PersistenceDirty {
    pending: bool,
    retry_in_seconds: f32,
    next_retry_delay_seconds: f32,
}

impl Default for PersistenceDirty {
    fn default() -> Self {
        Self {
            pending: false,
            retry_in_seconds: 0.0,
            next_retry_delay_seconds: SAVE_RETRY_INITIAL_SECONDS,
        }
    }
}

impl PersistenceDirty {
    fn mark_pending(&mut self) {
        self.pending = true;
        self.retry_in_seconds = 0.0;
    }
}

#[derive(Resource, Debug, Default)]
struct PointerGuard {
    suppress_mouse_for: f32,
}

#[derive(Resource, Debug, Default)]
struct DiagnosticPointerTrace {
    pointer: Option<PointerId>,
    last_raw_position: Option<Vec2>,
    missing_reported: bool,
}

impl DiagnosticPointerTrace {
    fn begin(&mut self, pointer: PointerId, raw_position: Vec2) {
        self.pointer = Some(pointer);
        self.last_raw_position = Some(raw_position);
        self.missing_reported = false;
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Resource, Debug, Default)]
struct Feedback {
    success: Option<Timer>,
}

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
enum MenuState {
    #[default]
    Closed,
    Open,
    ConfirmRestart,
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    commands.spawn((Camera2d, MainCamera));

    commands.spawn((
        Sprite {
            image: asset_server.load("Background/Background_2.png"),
            image_mode: SpriteImageMode::Scale(SpriteScalingMode::FillCenter),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -20.0),
        Sky,
        LayoutElement::Sky,
    ));

    commands.spawn((
        Sprite::from_color(BROWN, Vec2::ONE),
        Transform::from_xyz(0.0, 0.0, -11.0),
        GroundFill,
        LayoutElement::GroundFill,
    ));

    commands.spawn((
        Sprite {
            image: asset_server.load("Background/Assets.png"),
            rect: Some(Rect::new(336.0, 384.0, 400.0, 400.0)),
            image_mode: SpriteImageMode::Tiled {
                tile_x: true,
                tile_y: false,
                stretch_value: 1.0,
            },
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -10.0),
        GroundEdge,
        LayoutElement::GroundEdge,
    ));

    commands.spawn((
        Sprite {
            image: asset_server.load("Harvest/monki_banana_tree.png"),
            rect: Some(Rect::new(0.0, 0.0, 128.0, 128.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
        HarvestZone,
        LayoutElement::HarvestZone,
    ));

    commands.spawn((
        Sprite::from_color(Color::srgba(1.0, 0.78, 0.08, 0.0), Vec2::ONE),
        Transform::from_xyz(0.0, 0.0, -0.5),
        DepositGlow,
        LayoutElement::DepositGlow,
    ));

    commands.spawn((
        Sprite {
            image: asset_server.load("Deposit/monkistall.png"),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
        DepositZone,
        LayoutElement::DepositZone,
    ));

    let banana_layout = texture_atlas_layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(BANANA_FRAME_SIZE),
        BANANA_FRAMES as u32,
        1,
        None,
        None,
    ));
    commands.spawn((
        Sprite {
            image: asset_server.load("Banana/Banana.png"),
            texture_atlas: Some(TextureAtlas::from(banana_layout)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 3.0),
        Banana,
        LayoutElement::Banana,
        BananaAnimation {
            timer: Timer::new(Duration::from_secs_f32(1.0 / 12.0), TimerMode::Repeating),
        },
    ));

    commands.spawn((
        Text2d::new("HARVEST"),
        TextFont::from_font_size(26.0),
        TextColor(INK),
        Transform::from_xyz(0.0, 0.0, 2.0),
        HarvestLabel,
        LayoutElement::HarvestLabel,
    ));
    commands.spawn((
        Text2d::new("DEPOSIT"),
        TextFont::from_font_size(26.0),
        TextColor(INK),
        Transform::from_xyz(0.0, 0.0, 2.0),
        DepositLabel,
        LayoutElement::DepositLabel,
    ));

    setup_hud(&mut commands);
    setup_menu(&mut commands);
}

fn setup_hud(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexStart,
                padding: UiRect::top(px(16)),
                ..default()
            },
            Pickable::IGNORE,
            HudRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    min_width: px(210),
                    min_height: px(58),
                    padding: UiRect::axes(px(22), px(10)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(4)),
                    border_radius: BorderRadius::all(px(12)),
                    ..default()
                },
                BackgroundColor(CREAM),
                BorderColor::all(BROWN),
                children![(
                    Text::new("Bananas: 0"),
                    TextFont::from_font_size(30.0),
                    TextColor(INK),
                    CounterText,
                )],
            ));
        });

    commands
        .spawn((
            Button,
            ButtonAction::OpenMenu,
            OpenMenuButton,
            Node {
                position_type: PositionType::Absolute,
                top: px(16),
                right: px(16),
                min_width: px(96),
                min_height: px(52),
                padding: UiRect::axes(px(16), px(8)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(BROWN),
            BorderColor::all(CREAM),
        ))
        .with_child((
            Text::new("MENU"),
            TextFont::from_font_size(22.0),
            TextColor(CREAM),
        ));
}

fn setup_menu(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::all(px(16)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.06, 0.03, 0.02, 0.72)),
            GlobalZIndex(100),
            MenuView::Scrim,
        ))
        .with_children(|scrim| {
            scrim
                .spawn((
                    menu_panel_node(),
                    BackgroundColor(CREAM),
                    BorderColor::all(BROWN),
                    MenuView::Main,
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("BANANA BREAK"),
                        TextFont::from_font_size(34.0),
                        TextColor(INK),
                    ));
                    panel.spawn((
                        Text::new("CONTROLS"),
                        TextFont::from_font_size(20.0),
                        TextColor(BROWN_LIGHT),
                    ));
                    let controls = if cfg!(target_arch = "wasm32") {
                        "Drag banana from tree to stall\nPress H to harvest\nPress L for input logs"
                    } else {
                        "Drag banana from tree to stall\nPress H to harvest"
                    };
                    panel.spawn((
                        Text::new(controls),
                        TextFont::from_font_size(19.0),
                        TextColor(INK),
                        TextLayout::justify(Justify::Center),
                    ));
                    panel.spawn(menu_button_row()).with_children(|row| {
                        row.spawn(row_menu_button(
                            ButtonAction::Resume,
                            MenuButtonTone::Emphasized,
                        ))
                        .with_child(menu_button_text("RESUME"));
                        #[cfg(target_arch = "wasm32")]
                        row.spawn(row_menu_button(
                            ButtonAction::Diagnostics,
                            MenuButtonTone::Secondary,
                        ))
                        .with_child(menu_button_text("INPUT LOGS"));
                    });
                    panel
                        .spawn(menu_button(ButtonAction::Restart))
                        .with_child(menu_button_text("RESTART GAME"));
                });

            scrim
                .spawn((
                    Node {
                        display: Display::None,
                        ..menu_panel_node()
                    },
                    BackgroundColor(CREAM),
                    BorderColor::all(BROWN),
                    MenuView::Restart,
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("RESET BANANAS?"),
                        TextFont::from_font_size(30.0),
                        TextColor(INK),
                    ));
                    panel.spawn((
                        Text::new("Reset bananas to 0?\nThis cannot be undone."),
                        TextFont::from_font_size(19.0),
                        TextColor(INK),
                        TextLayout::justify(Justify::Center),
                    ));
                    panel
                        .spawn(menu_button(ButtonAction::ConfirmRestart))
                        .with_child(menu_button_text("RESET TO 0"));
                    panel
                        .spawn(menu_button(ButtonAction::CancelRestart))
                        .with_child(menu_button_text("CANCEL"));
                });
        });
}

fn menu_panel_node() -> Node {
    Node {
        width: percent(92),
        max_width: px(440),
        padding: UiRect::all(px(24)),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(14),
        border: UiRect::all(px(5)),
        border_radius: BorderRadius::all(px(14)),
        ..default()
    }
}

fn menu_button(action: ButtonAction) -> impl Bundle {
    (
        Button,
        action,
        MenuButtonTone::Primary,
        Node {
            width: percent(100),
            min_height: px(52),
            padding: UiRect::axes(px(18), px(10)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(BROWN),
        BorderColor::all(BROWN_LIGHT),
    )
}

fn menu_button_row() -> Node {
    Node {
        width: percent(100),
        column_gap: px(10),
        ..default()
    }
}

fn row_menu_button(action: ButtonAction, tone: MenuButtonTone) -> impl Bundle {
    (
        Button,
        action,
        tone,
        Node {
            min_width: px(0),
            min_height: px(52),
            flex_grow: 1.0,
            flex_basis: px(0),
            padding: UiRect::axes(px(8), px(10)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(match tone {
            MenuButtonTone::Primary | MenuButtonTone::Emphasized => BROWN,
            #[cfg(target_arch = "wasm32")]
            MenuButtonTone::Secondary => BROWN,
        }),
        BorderColor::all(match tone {
            MenuButtonTone::Primary => CREAM,
            MenuButtonTone::Emphasized => GOLD,
            #[cfg(target_arch = "wasm32")]
            MenuButtonTone::Secondary => BROWN_LIGHT,
        }),
    )
}

fn menu_button_text(label: &'static str) -> impl Bundle {
    (
        Text::new(label),
        TextFont::from_font_size(21.0),
        TextColor(CREAM),
    )
}

fn refresh_layout(window: Single<&Window, With<PrimaryWindow>>, mut layout: ResMut<SceneLayout>) {
    let next = SceneLayout::for_viewport(Vec2::new(window.width(), window.height()));
    if next.viewport != layout.viewport {
        *layout = next;
    }
}

#[allow(clippy::type_complexity)]
fn apply_layout(
    layout: Res<SceneLayout>,
    controller: Res<HarvestController>,
    mut elements: Query<(&LayoutElement, Option<&mut Sprite>, &mut Transform)>,
) {
    let ground_height = (layout.viewport.y * 0.5 + layout.ground_top).max(96.0);
    let ground_center_y = -layout.viewport.y * 0.5 + ground_height * 0.5;
    let zone_scale = layout.zone_size / 128.0;
    let label_y = layout.harvest.y + layout.zone_size * 0.5 + 24.0;

    for (element, sprite, mut transform) in &mut elements {
        match element {
            LayoutElement::Sky => {
                sprite.expect("sky has sprite").custom_size = Some(layout.viewport);
            }
            LayoutElement::GroundFill => {
                sprite.expect("ground fill has sprite").custom_size =
                    Some(Vec2::new(layout.viewport.x + 128.0, ground_height));
                transform.translation = Vec3::new(0.0, ground_center_y, -11.0);
            }
            LayoutElement::GroundEdge => {
                sprite.expect("ground texture has sprite").custom_size =
                    Some(Vec2::new(layout.viewport.x + 128.0, 16.0));
                transform.translation = Vec3::new(0.0, layout.ground_top - 8.0, -10.0);
            }
            LayoutElement::HarvestZone => {
                transform.translation = layout.harvest.extend(0.0);
                transform.scale = Vec3::splat(zone_scale);
            }
            LayoutElement::DepositZone => {
                transform.translation = layout.deposit.extend(0.0);
                transform.scale = Vec3::splat(zone_scale);
            }
            LayoutElement::DepositGlow => {
                sprite.expect("deposit glow has sprite").custom_size =
                    Some(Vec2::splat(layout.zone_size + 36.0));
                transform.translation = layout.deposit.extend(-0.5);
            }
            LayoutElement::Banana => {
                if matches!(controller.interaction, HarvestInteraction::Idle) {
                    transform.translation = layout.banana_home.extend(3.0);
                }
                let lift = if matches!(controller.interaction, HarvestInteraction::Dragging { .. })
                {
                    1.12
                } else {
                    1.0
                };
                transform.scale = Vec3::splat(layout.banana_size / BANANA_FRAME_SIZE as f32 * lift);
            }
            LayoutElement::HarvestLabel => {
                transform.translation = Vec3::new(layout.harvest.x, label_y, 2.0);
            }
            LayoutElement::DepositLabel => {
                transform.translation = Vec3::new(layout.deposit.x, label_y, 2.0);
            }
        }
    }
}

fn apply_responsive_hud(
    window: Single<&Window, With<PrimaryWindow>>,
    mut hud: Single<&mut Node, (With<HudRoot>, Without<OpenMenuButton>)>,
    mut menu_button: Single<&mut Node, (With<OpenMenuButton>, Without<HudRoot>)>,
) {
    if window.width() < 600.0 {
        hud.padding = UiRect {
            top: px(10),
            right: px(110),
            ..default()
        };
        menu_button.top = px(10);
        menu_button.right = px(10);
    } else {
        hud.padding = UiRect::top(px(16));
        menu_button.top = px(16);
        menu_button.right = px(16);
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_menu(
    keys: Res<ButtonInput<KeyCode>>,
    touches: Res<Touches>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut interactions: Query<(&Interaction, &ButtonAction), Changed<Interaction>>,
    buttons: Query<(&ButtonAction, &ComputedNode, &UiGlobalTransform)>,
    mut menu: ResMut<MenuState>,
    mut controller: ResMut<HarvestController>,
    mut pending: ResMut<PendingSettlement>,
    mut treasury: ResMut<Treasury>,
    mut dirty: ResMut<PersistenceDirty>,
    mut feedback: ResMut<Feedback>,
    layout: Res<SceneLayout>,
    mut banana: Single<&mut Transform, With<Banana>>,
) {
    let mut requested = None;
    let mut pressed_actions = Vec::new();
    #[cfg(target_arch = "wasm32")]
    if *menu == MenuState::Open && keys.just_pressed(KeyCode::KeyL) {
        open_web_diagnostics();
    }
    if keys.just_pressed(KeyCode::Escape) {
        requested = Some(match *menu {
            MenuState::Closed => MenuState::Open,
            MenuState::Open => MenuState::Closed,
            MenuState::ConfirmRestart => MenuState::Open,
        });
    }

    for (interaction, action) in &mut interactions {
        if *interaction == Interaction::Pressed {
            pressed_actions.push(*action);
        }
    }

    for touch in touches.iter_just_pressed() {
        let position =
            pointer_in_camera_space(touch.position(), window.resolution.base_scale_factor());
        for (action, node, transform) in &buttons {
            let center = transform.translation * node.inverse_scale_factor;
            let size = node.size() * node.inverse_scale_factor;
            if contains_inclusive(Rect::from_center_size(center, size), position)
                && !pressed_actions.contains(action)
            {
                pressed_actions.push(*action);
            }
        }
    }

    for action in pressed_actions {
        match action {
            ButtonAction::OpenMenu if *menu == MenuState::Closed => {
                requested = Some(MenuState::Open);
            }
            ButtonAction::Resume if *menu == MenuState::Open => {
                requested = Some(MenuState::Closed);
            }
            #[cfg(target_arch = "wasm32")]
            ButtonAction::Diagnostics if *menu == MenuState::Open => {
                open_web_diagnostics();
            }
            ButtonAction::Restart if *menu == MenuState::Open => {
                requested = Some(MenuState::ConfirmRestart);
            }
            ButtonAction::ConfirmRestart if *menu == MenuState::ConfirmRestart => {
                cancel_harvest(&mut controller, &mut pending);
                treasury.restart();
                dirty.mark_pending();
                feedback.success = None;
                banana.translation = layout.banana_home.extend(3.0);
                requested = Some(MenuState::Closed);
            }
            ButtonAction::CancelRestart if *menu == MenuState::ConfirmRestart => {
                requested = Some(MenuState::Open);
            }
            _ => {}
        }
    }

    if let Some(next) = requested {
        if next != MenuState::Closed {
            cancel_harvest(&mut controller, &mut pending);
            banana.translation = layout.banana_home.extend(3.0);
        }
        *menu = next;
    }
}

fn sync_menu_visibility(menu: Res<MenuState>, mut views: Query<(&MenuView, &mut Node)>) {
    for (view, mut node) in &mut views {
        node.display = match view {
            MenuView::Scrim if *menu != MenuState::Closed => Display::Flex,
            MenuView::Main if *menu == MenuState::Open => Display::Flex,
            MenuView::Restart if *menu == MenuState::ConfirmRestart => Display::Flex,
            _ => Display::None,
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_harvest_input(
    frame_count: Res<FrameCount>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    menu: Res<MenuState>,
    layout: Res<SceneLayout>,
    mut controller: ResMut<HarvestController>,
    mut pointer_guard: ResMut<PointerGuard>,
    mut pending: ResMut<PendingSettlement>,
    mut diagnostic_trace: ResMut<DiagnosticPointerTrace>,
    mut banana: Single<&mut Transform, With<Banana>>,
) {
    pointer_guard.suppress_mouse_for =
        (pointer_guard.suppress_mouse_for - time.delta_secs()).max(0.0);
    if touches.any_just_pressed() || touches.any_just_released() || touches.any_just_canceled() {
        pointer_guard.suppress_mouse_for = TOUCH_MOUSE_SUPPRESSION_SECONDS;
    }

    if web_diagnostics_panel_open() {
        if controller.interaction != HarvestInteraction::Idle {
            diagnostic_log!(
                frame_count,
                "input_blocked",
                "reason=diagnostics_panel interaction={:?}",
                controller.interaction
            );
            cancel_harvest(&mut controller, &mut pending);
            diagnostic_trace.clear();
            banana.translation = layout.banana_home.extend(3.0);
        }
        return;
    }

    if *menu != MenuState::Closed {
        return;
    }

    let interaction = controller.interaction;
    match interaction {
        HarvestInteraction::Idle => {
            let mut touch_start = None;
            for touch in touches.iter_just_pressed() {
                let raw = touch.position();
                let camera_position =
                    pointer_in_camera_space(raw, window.resolution.base_scale_factor());
                let world = screen_to_world(camera_position, &camera);
                let accepted = world
                    .is_some_and(|position| contains_inclusive(layout.harvest_bounds, position));
                diagnostic_log!(
                    frame_count,
                    "touch_start",
                    Some(PointerId::Touch(touch.id()));
                    "id={} raw=({:.2},{:.2}) camera=({:.2},{:.2}) world_valid={} world=({:.2},{:.2}) harvest_min=({:.2},{:.2}) harvest_max=({:.2},{:.2}) accepted={} window_logical=({:.2},{:.2}) window_physical=({},{}) scale={:.3} base_scale={:.3} scale_override={:?}",
                    touch.id(),
                    raw.x,
                    raw.y,
                    camera_position.x,
                    camera_position.y,
                    world.is_some(),
                    world.map_or(f32::NAN, |position| position.x),
                    world.map_or(f32::NAN, |position| position.y),
                    layout.harvest_bounds.min.x,
                    layout.harvest_bounds.min.y,
                    layout.harvest_bounds.max.x,
                    layout.harvest_bounds.max.y,
                    accepted,
                    window.width(),
                    window.height(),
                    window.physical_width(),
                    window.physical_height(),
                    window.resolution.scale_factor(),
                    window.resolution.base_scale_factor(),
                    window.resolution.scale_factor_override(),
                );
                if accepted
                    && touch_start
                        .as_ref()
                        .is_none_or(|(selected_id, _, _)| touch.id() < *selected_id)
                {
                    touch_start = world.map(|position| (touch.id(), raw, position));
                }
            }

            if let Some((id, raw, position)) = touch_start {
                controller.interaction = HarvestInteraction::Dragging {
                    pointer: PointerId::Touch(id),
                    position,
                };
                diagnostic_trace.begin(PointerId::Touch(id), raw);
                banana.translation = position.extend(4.0);
                diagnostic_log!(
                    frame_count,
                    "drag_begin",
                    Some(PointerId::Touch(id));
                    "pointer=touch:{} world=({:.2},{:.2})",
                    id,
                    position.x,
                    position.y
                );
                return;
            }

            if mouse.just_pressed(MouseButton::Left) {
                let raw = window.cursor_position();
                let camera_position = raw.map(|position| {
                    pointer_in_camera_space(position, window.resolution.base_scale_factor())
                });
                let world = camera_position.and_then(|position| screen_to_world(position, &camera));
                let in_harvest = world
                    .is_some_and(|position| contains_inclusive(layout.harvest_bounds, position));
                let accepted = pointer_guard.suppress_mouse_for == 0.0 && in_harvest;
                diagnostic_log!(
                    frame_count,
                    "mouse_start",
                    Some(PointerId::Mouse);
                    "raw_valid={} raw=({:.2},{:.2}) camera=({:.2},{:.2}) world_valid={} world=({:.2},{:.2}) in_harvest={} suppression_seconds={:.3} accepted={}",
                    raw.is_some(),
                    raw.map_or(f32::NAN, |position| position.x),
                    raw.map_or(f32::NAN, |position| position.y),
                    camera_position.map_or(f32::NAN, |position| position.x),
                    camera_position.map_or(f32::NAN, |position| position.y),
                    world.is_some(),
                    world.map_or(f32::NAN, |position| position.x),
                    world.map_or(f32::NAN, |position| position.y),
                    in_harvest,
                    pointer_guard.suppress_mouse_for,
                    accepted,
                );
                if accepted && let (Some(raw), Some(position)) = (raw, world) {
                    controller.interaction = HarvestInteraction::Dragging {
                        pointer: PointerId::Mouse,
                        position,
                    };
                    diagnostic_trace.begin(PointerId::Mouse, raw);
                    banana.translation = position.extend(4.0);
                    diagnostic_log!(
                        frame_count,
                        "drag_begin",
                        Some(PointerId::Mouse);
                        "pointer=mouse world=({:.2},{:.2})",
                        position.x,
                        position.y
                    );
                    return;
                }
            }

            if keys.just_pressed(KeyCode::KeyH) {
                controller.interaction = HarvestInteraction::KeyboardHarvest {
                    elapsed: 0.0,
                    warmup_frames: 0,
                };
            }
        }
        HarvestInteraction::Dragging {
            pointer: PointerId::Touch(id),
            ..
        } => {
            if touches.just_canceled(id) {
                diagnostic_log!(
                    frame_count,
                    "touch_cancel",
                    Some(PointerId::Touch(id));
                    "id={}",
                    id
                );
                cancel_harvest(&mut controller, &mut pending);
                diagnostic_trace.clear();
                banana.translation = layout.banana_home.extend(3.0);
            } else if let Some(touch) = touches.get_released(id) {
                let raw = touch.position();
                let camera_position =
                    pointer_in_camera_space(raw, window.resolution.base_scale_factor());
                let position = screen_to_world(camera_position, &camera);
                let in_deposit = position
                    .is_some_and(|position| contains_inclusive(layout.deposit_bounds, position));
                diagnostic_log!(
                    frame_count,
                    "touch_release",
                    Some(PointerId::Touch(id));
                    "id={} raw=({:.2},{:.2}) camera=({:.2},{:.2}) world_valid={} world=({:.2},{:.2}) deposit_min=({:.2},{:.2}) deposit_max=({:.2},{:.2}) in_deposit={}",
                    id,
                    raw.x,
                    raw.y,
                    camera_position.x,
                    camera_position.y,
                    position.is_some(),
                    position.map_or(f32::NAN, |position| position.x),
                    position.map_or(f32::NAN, |position| position.y),
                    layout.deposit_bounds.min.x,
                    layout.deposit_bounds.min.y,
                    layout.deposit_bounds.max.x,
                    layout.deposit_bounds.max.y,
                    in_deposit,
                );
                finish_pointer_drag(
                    PointerId::Touch(id),
                    position,
                    &layout,
                    &mut controller,
                    &mut pending,
                    &mut banana,
                );
                diagnostic_trace.clear();
            } else if let Some(touch) = touches.get_pressed(id) {
                let raw = touch.position();
                let moved = diagnostic_trace.pointer != Some(PointerId::Touch(id))
                    || diagnostic_trace
                        .last_raw_position
                        .is_none_or(|last| last.distance_squared(raw) >= 0.25);
                let camera_position =
                    pointer_in_camera_space(raw, window.resolution.base_scale_factor());
                let position = screen_to_world(camera_position, &camera);
                if moved {
                    diagnostic_log!(
                        frame_count,
                        "touch_move",
                        Some(PointerId::Touch(id));
                        "id={} raw=({:.2},{:.2}) camera=({:.2},{:.2}) world_valid={} world=({:.2},{:.2})",
                        id,
                        raw.x,
                        raw.y,
                        camera_position.x,
                        camera_position.y,
                        position.is_some(),
                        position.map_or(f32::NAN, |position| position.x),
                        position.map_or(f32::NAN, |position| position.y),
                    );
                    diagnostic_trace.last_raw_position = Some(raw);
                }
                if let Some(position) = position {
                    controller.interaction = HarvestInteraction::Dragging {
                        pointer: PointerId::Touch(id),
                        position,
                    };
                    banana.translation = position.extend(4.0);
                }
            } else if !diagnostic_trace.missing_reported {
                diagnostic_log!(
                    frame_count,
                    "touch_missing",
                    Some(PointerId::Touch(id));
                    "id={} no_pressed_released_or_canceled_state",
                    id
                );
                diagnostic_trace.missing_reported = true;
            }
        }
        HarvestInteraction::Dragging {
            pointer: PointerId::Mouse,
            ..
        } => {
            let position = window
                .cursor_position()
                .map(|position| {
                    pointer_in_camera_space(position, window.resolution.base_scale_factor())
                })
                .and_then(|position| screen_to_world(position, &camera));
            if mouse.just_released(MouseButton::Left) {
                let in_deposit = position
                    .is_some_and(|position| contains_inclusive(layout.deposit_bounds, position));
                diagnostic_log!(
                    frame_count,
                    "mouse_release",
                    Some(PointerId::Mouse);
                    "world_valid={} world=({:.2},{:.2}) in_deposit={}",
                    position.is_some(),
                    position.map_or(f32::NAN, |position| position.x),
                    position.map_or(f32::NAN, |position| position.y),
                    in_deposit,
                );
                finish_pointer_drag(
                    PointerId::Mouse,
                    position,
                    &layout,
                    &mut controller,
                    &mut pending,
                    &mut banana,
                );
                diagnostic_trace.clear();
            } else if mouse.pressed(MouseButton::Left) {
                if let Some(position) = position {
                    controller.interaction = HarvestInteraction::Dragging {
                        pointer: PointerId::Mouse,
                        position,
                    };
                    banana.translation = position.extend(4.0);
                }
            } else {
                diagnostic_log!(
                    frame_count,
                    "mouse_missing",
                    Some(PointerId::Mouse);
                    "button_no_longer_pressed"
                );
                cancel_harvest(&mut controller, &mut pending);
                diagnostic_trace.clear();
                banana.translation = layout.banana_home.extend(3.0);
            }
        }
        HarvestInteraction::KeyboardHarvest { .. } => {}
    }
}

fn finish_pointer_drag(
    pointer: PointerId,
    position: Option<Vec2>,
    layout: &SceneLayout,
    controller: &mut HarvestController,
    pending: &mut PendingSettlement,
    banana: &mut Transform,
) {
    if position.is_some_and(|position| contains_inclusive(layout.deposit_bounds, position)) {
        pending.0 = Some(SettlementSource::Pointer(pointer));
    } else {
        cancel_harvest(controller, pending);
        banana.translation = layout.banana_home.extend(3.0);
    }
}

fn move_keyboard_harvest(
    time: Res<Time>,
    layout: Res<SceneLayout>,
    mut controller: ResMut<HarvestController>,
    mut pending: ResMut<PendingSettlement>,
    mut banana: Single<&mut Transform, With<Banana>>,
) {
    let HarvestInteraction::KeyboardHarvest {
        elapsed,
        warmup_frames,
    } = controller.interaction
    else {
        return;
    };

    if warmup_frames < 2 {
        controller.interaction = HarvestInteraction::KeyboardHarvest {
            elapsed,
            warmup_frames: warmup_frames + 1,
        };
        return;
    }

    let elapsed = elapsed + time.delta_secs();
    let progress = (elapsed / KEYBOARD_HARVEST_SECONDS).clamp(0.0, 1.0);
    let eased = progress * progress * (3.0 - 2.0 * progress);
    let mut position = layout.banana_home.lerp(layout.deposit, eased);
    position.y += (std::f32::consts::PI * progress).sin() * 72.0;
    banana.translation = position.extend(4.0);

    controller.interaction = HarvestInteraction::KeyboardHarvest {
        elapsed,
        warmup_frames,
    };
    if progress >= 1.0 && pending.0.is_none() {
        pending.0 = Some(SettlementSource::Keyboard);
    }
}

#[allow(clippy::too_many_arguments)]
fn settle_harvest(
    frame_count: Res<FrameCount>,
    mut controller: ResMut<HarvestController>,
    mut pending: ResMut<PendingSettlement>,
    mut treasury: ResMut<Treasury>,
    mut dirty: ResMut<PersistenceDirty>,
    mut feedback: ResMut<Feedback>,
    layout: Res<SceneLayout>,
    mut banana: Single<&mut Transform, With<Banana>>,
) {
    let Some(source) = pending.0.take() else {
        return;
    };

    let interaction_before = controller.interaction;
    let diagnostic_pointer = match source {
        SettlementSource::Pointer(pointer) => Some(pointer),
        SettlementSource::Keyboard => None,
    };
    let matched = settlement_matches(interaction_before, source);
    let before = treasury.display_count();
    let mut committed = false;
    if matched {
        committed = treasury.commit_harvest();
        if committed {
            dirty.mark_pending();
            feedback.success = Some(Timer::new(
                Duration::from_secs_f32(SUCCESS_PULSE_SECONDS),
                TimerMode::Once,
            ));
        }
        controller.interaction = HarvestInteraction::Idle;
        banana.translation = layout.banana_home.extend(3.0);
    }
    diagnostic_log!(
        frame_count,
        "settlement",
        diagnostic_pointer;
        "source={:?} interaction={:?} matched={} commit_harvest={} treasury_before={} treasury_after={}",
        source,
        interaction_before,
        matched,
        committed,
        before,
        treasury.display_count(),
    );
}

fn settlement_matches(interaction: HarvestInteraction, source: SettlementSource) -> bool {
    matches!(
        (interaction, source),
        (
            HarvestInteraction::Dragging { pointer, .. },
            SettlementSource::Pointer(source_pointer)
        ) if pointer == source_pointer
    ) || matches!(
        (interaction, source),
        (
            HarvestInteraction::KeyboardHarvest { .. },
            SettlementSource::Keyboard
        )
    )
}

fn cancel_harvest(controller: &mut HarvestController, pending: &mut PendingSettlement) {
    controller.interaction = HarvestInteraction::Idle;
    pending.0 = None;
}

fn persist_changes(time: Res<Time>, mut dirty: ResMut<PersistenceDirty>, treasury: Res<Treasury>) {
    if !dirty.pending {
        return;
    }

    dirty.retry_in_seconds = (dirty.retry_in_seconds - time.delta_secs()).max(0.0);
    if dirty.retry_in_seconds > 0.0 {
        return;
    }

    match persistence::store_treasury(*treasury) {
        Ok(()) => {
            dirty.pending = false;
            dirty.next_retry_delay_seconds = SAVE_RETRY_INITIAL_SECONDS;
        }
        Err(error) => {
            bevy::log::warn!("Could not save progress: {error}");
            dirty.retry_in_seconds = dirty.next_retry_delay_seconds;
            dirty.next_retry_delay_seconds =
                (dirty.next_retry_delay_seconds * 2.0).min(SAVE_RETRY_MAX_SECONDS);
        }
    }
}

fn animate_banana(
    time: Res<Time>,
    mut banana: Single<(&mut BananaAnimation, &mut Sprite), With<Banana>>,
) {
    banana.0.timer.tick(time.delta());
    if banana.0.timer.just_finished()
        && let Some(atlas) = banana.1.texture_atlas.as_mut()
    {
        atlas.index = (atlas.index + 1) % BANANA_FRAMES;
    }
}

fn update_feedback(
    time: Res<Time>,
    controller: Res<HarvestController>,
    layout: Res<SceneLayout>,
    mut feedback: ResMut<Feedback>,
    mut glow: Single<&mut Sprite, With<DepositGlow>>,
    mut counter: Single<&mut TextFont, With<CounterText>>,
) {
    let drag_highlight = matches!(
        controller.interaction,
        HarvestInteraction::Dragging { position, .. }
            if contains_inclusive(layout.deposit_bounds, position)
    );

    let mut pulse = 0.0;
    if let Some(timer) = feedback.success.as_mut() {
        timer.tick(time.delta());
        let progress = timer.fraction();
        pulse = (std::f32::consts::PI * progress).sin();
        if timer.is_finished() {
            feedback.success = None;
        }
    }

    let alpha = if drag_highlight { 0.34 } else { pulse * 0.46 };
    glow.color = Color::srgba(1.0, 0.78, 0.08, alpha);
    counter.font_size = FontSize::Px(30.0 + pulse * 6.0);
}

fn sync_counter(treasury: Res<Treasury>, mut text: Single<&mut Text, With<CounterText>>) {
    text.0 = format!("Bananas: {}", treasury.display_count());
}

#[allow(clippy::type_complexity)]
fn style_buttons(
    mut buttons: Query<
        (
            &Interaction,
            Option<&MenuButtonTone>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        (With<ButtonAction>, Changed<Interaction>),
    >,
) {
    for (interaction, tone, mut background, mut border) in &mut buttons {
        match interaction {
            Interaction::Pressed => {
                *background = BackgroundColor(GOLD);
                *border = BorderColor::all(CREAM);
            }
            Interaction::Hovered => {
                *background = BackgroundColor(BROWN_LIGHT);
                *border = BorderColor::all(GOLD);
            }
            Interaction::None => {
                *background = BackgroundColor(BROWN);
                *border = BorderColor::all(match tone {
                    Some(MenuButtonTone::Emphasized) => GOLD,
                    #[cfg(target_arch = "wasm32")]
                    Some(MenuButtonTone::Secondary) => BROWN_LIGHT,
                    _ => CREAM,
                });
            }
        }
    }
}

fn contains_inclusive(bounds: Rect, point: Vec2) -> bool {
    point.x >= bounds.min.x
        && point.x <= bounds.max.x
        && point.y >= bounds.min.y
        && point.y <= bounds.max.y
}

fn screen_to_world(position: Vec2, camera: &(&Camera, &GlobalTransform)) -> Option<Vec2> {
    camera.0.viewport_to_world_2d(camera.1, position).ok()
}

/// Bevy input positions and camera viewport positions share the window's logical space.
/// The OS scale factor remains useful for diagnostics, but Bevy has already applied it.
fn pointer_in_camera_space(position: Vec2, _base_scale_factor: f32) -> Vec2 {
    position
}

#[cfg(target_arch = "wasm32")]
fn push_web_diagnostic(
    frame: u32,
    event: &str,
    pointer: Option<PointerId>,
    detail: std::fmt::Arguments<'_>,
) {
    use wasm_bindgen::{JsCast, JsValue};

    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(callback) =
        js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("__BANANA_DIAG_PUSH__"))
    else {
        return;
    };
    let Some(callback) = callback.dyn_ref::<js_sys::Function>() else {
        return;
    };
    let rust_performance_ms = window
        .performance()
        .map_or(f64::NAN, |performance| performance.now());
    let message = format!("frame={frame} rust_ms={rust_performance_ms:.2} {detail}");
    let pointer = match pointer {
        Some(PointerId::Touch(id)) => JsValue::from_str(&format!("touch:{id}")),
        Some(PointerId::Mouse) => JsValue::from_str("mouse"),
        None => JsValue::NULL,
    };
    let _ = callback.call3(
        &JsValue::NULL,
        &JsValue::from_str(event),
        &JsValue::from_str(&message),
        &pointer,
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn push_web_diagnostic(
    _frame: u32,
    _event: &str,
    _pointer: Option<PointerId>,
    _detail: std::fmt::Arguments<'_>,
) {
}

#[cfg(target_arch = "wasm32")]
fn open_web_diagnostics() {
    use wasm_bindgen::{JsCast, JsValue};

    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(callback) =
        js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("__BANANA_DIAG_OPEN__"))
    else {
        return;
    };
    let Some(callback) = callback.dyn_ref::<js_sys::Function>() else {
        return;
    };
    let _ = callback.call0(&JsValue::NULL);
}

#[cfg(target_arch = "wasm32")]
fn web_diagnostics_panel_open() -> bool {
    use wasm_bindgen::JsValue;

    web_sys::window()
        .and_then(|window| {
            js_sys::Reflect::get(
                window.as_ref(),
                &JsValue::from_str("__BANANA_DIAG_PANEL_OPEN__"),
            )
            .ok()
        })
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn web_diagnostics_panel_open() -> bool {
    false
}

#[cfg(not(target_arch = "wasm32"))]
fn sync_web_test_state() {}

#[cfg(target_arch = "wasm32")]
fn sync_web_test_state(
    treasury: Res<Treasury>,
    controller: Res<HarvestController>,
    menu: Res<MenuState>,
    layout: Res<SceneLayout>,
    primary_window: Single<&Window, With<PrimaryWindow>>,
    touches: Res<Touches>,
    banana_transform: Single<&Transform, With<Banana>>,
    buttons: Query<(&ButtonAction, &ComputedNode, &UiGlobalTransform)>,
    mut warmup_frames: Local<u8>,
) {
    use wasm_bindgen::JsValue;

    if *warmup_frames < 3 {
        *warmup_frames += 1;
        return;
    }

    let interaction = match controller.interaction {
        HarvestInteraction::Idle => "idle",
        HarvestInteraction::Dragging { .. } => "dragging",
        HarvestInteraction::KeyboardHarvest { .. } => "keyboard-harvest",
    };
    let menu = match *menu {
        MenuState::Closed => "closed",
        MenuState::Open => "open",
        MenuState::ConfirmRestart => "confirm-restart",
    };
    let banana = layout.world_to_screen(banana_transform.translation.truncate());
    let harvest = layout.world_to_screen(layout.harvest);
    let harvest_bounds_min = layout.world_to_screen(Vec2::new(
        layout.harvest_bounds.min.x,
        layout.harvest_bounds.max.y,
    ));
    let harvest_bounds_max = layout.world_to_screen(Vec2::new(
        layout.harvest_bounds.max.x,
        layout.harvest_bounds.min.y,
    ));
    let deposit = layout.world_to_screen(layout.deposit);
    let active_touches = touches.iter().count();
    let touch = touches
        .iter()
        .next()
        .map_or(Vec2::ZERO, |touch| touch.position());
    let mut button_centers = [Vec2::ZERO; 6];
    for (action, node, transform) in &buttons {
        let index = match action {
            ButtonAction::OpenMenu => 0,
            ButtonAction::Resume => 1,
            ButtonAction::Diagnostics => 2,
            ButtonAction::Restart => 3,
            ButtonAction::ConfirmRestart => 4,
            ButtonAction::CancelRestart => 5,
        };
        button_centers[index] = transform.translation * node.inverse_scale_factor;
    }
    let state = format!(
        r#"{{"ready":true,"bananas":{},"interaction":"{}","menu":"{}","viewport":{{"x":{:.2},"y":{:.2}}},"activeTouches":{},"touch":{{"x":{:.2},"y":{:.2}}},"banana":{{"x":{:.2},"y":{:.2}}},"harvest":{{"x":{:.2},"y":{:.2}}},"harvestBounds":{{"min":{{"x":{:.2},"y":{:.2}}},"max":{{"x":{:.2},"y":{:.2}}}}},"deposit":{{"x":{:.2},"y":{:.2}}},"buttons":{{"menu":{{"x":{:.2},"y":{:.2}}},"resume":{{"x":{:.2},"y":{:.2}}},"logs":{{"x":{:.2},"y":{:.2}}},"restart":{{"x":{:.2},"y":{:.2}}},"confirmRestart":{{"x":{:.2},"y":{:.2}}},"cancelRestart":{{"x":{:.2},"y":{:.2}}}}}}}"#,
        treasury.display_count(),
        interaction,
        menu,
        primary_window.width(),
        primary_window.height(),
        active_touches,
        touch.x,
        touch.y,
        banana.x,
        banana.y,
        harvest.x,
        harvest.y,
        harvest_bounds_min.x,
        harvest_bounds_min.y,
        harvest_bounds_max.x,
        harvest_bounds_max.y,
        deposit.x,
        deposit.y,
        button_centers[0].x,
        button_centers[0].y,
        button_centers[1].x,
        button_centers[1].y,
        button_centers[2].x,
        button_centers[2].y,
        button_centers[3].x,
        button_centers[3].y,
        button_centers[4].x,
        button_centers[4].y,
        button_centers[5].x,
        button_centers[5].y,
    );

    if let Some(window) = web_sys::window() {
        let _ = js_sys::Reflect::set(
            window.as_ref(),
            &JsValue::from_str("__BANANA_MONKEY_TEST_STATE__"),
            &JsValue::from_str(&state),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_settlement_requires_matching_active_pointer() {
        let interaction = HarvestInteraction::Dragging {
            pointer: PointerId::Touch(7),
            position: Vec2::ZERO,
        };

        assert!(settlement_matches(
            interaction,
            SettlementSource::Pointer(PointerId::Touch(7))
        ));
        assert!(!settlement_matches(
            interaction,
            SettlementSource::Pointer(PointerId::Touch(8))
        ));
        assert!(!settlement_matches(
            interaction,
            SettlementSource::Pointer(PointerId::Mouse)
        ));
    }

    #[test]
    fn keyboard_settlement_only_matches_keyboard_flight() {
        assert!(settlement_matches(
            HarvestInteraction::KeyboardHarvest {
                elapsed: 10.0,
                warmup_frames: 2,
            },
            SettlementSource::Keyboard
        ));
        assert!(!settlement_matches(
            HarvestInteraction::Idle,
            SettlementSource::Keyboard
        ));
    }

    #[test]
    fn cancel_harvest_clears_interaction_and_pending_settlement() {
        let mut controller = HarvestController {
            interaction: HarvestInteraction::KeyboardHarvest {
                elapsed: 0.2,
                warmup_frames: 2,
            },
        };
        let mut pending = PendingSettlement(Some(SettlementSource::Keyboard));

        cancel_harvest(&mut controller, &mut pending);

        assert_eq!(controller.interaction, HarvestInteraction::Idle);
        assert_eq!(pending.0, None);
    }

    #[test]
    fn drop_bounds_include_every_boundary() {
        let bounds = Rect::from_corners(Vec2::new(-1.0, -2.0), Vec2::new(3.0, 4.0));

        assert!(contains_inclusive(bounds, Vec2::new(-1.0, -2.0)));
        assert!(contains_inclusive(bounds, Vec2::new(3.0, 4.0)));
        assert!(!contains_inclusive(bounds, Vec2::new(3.01, 4.0)));
    }

    #[test]
    fn bevy_pointer_coordinates_are_already_in_camera_space_at_fractional_dpr() {
        let phone_pointer = Vec2::new(92.39, 443.51);

        assert_eq!(pointer_in_camera_space(phone_pointer, 2.625), phone_pointer);
    }

    #[test]
    fn responsive_layout_preserves_left_to_right_flow_and_minimum_targets() {
        for viewport in [Vec2::new(320.0, 568.0), Vec2::new(1920.0, 1080.0)] {
            let layout = SceneLayout::for_viewport(viewport);

            assert!(layout.harvest.x < layout.deposit.x);
            assert!(layout.banana_size >= 48.0);
            assert!(contains_inclusive(layout.harvest_bounds, layout.harvest));
            assert!(contains_inclusive(
                layout.harvest_bounds,
                layout.banana_home
            ));
            assert!(layout.harvest_bounds.width() >= layout.zone_size);
            assert!(layout.deposit_bounds.width() >= 148.0);
        }
    }
}
