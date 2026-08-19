use std::time::Duration;

use bevy::{
    diagnostic::FrameCount,
    input::touch::Touches,
    prelude::*,
    sprite::{SpriteImageMode, SpriteScalingMode},
    window::PrimaryWindow,
};

use crate::{
    domain::{
        BANANAS_PER_HARVEST, CycleSpec, CycleTerms, EconomySnapshot, HarvestCycle, Multipliers,
        SIM_HZ, Treasury, Workforce, plan_hire, restart_run,
    },
    hud, persistence,
    worker::{self, RestoredCycle, Worker},
};

const BANANA_FRAMES: usize = 12;
const BANANA_FRAME_SIZE: u32 = 16;
const KEYBOARD_HARVEST_SECONDS: f32 = 0.42;
/// A pulse for the deposit the player just made, and is looking at.
const SUCCESS_PULSE_SECONDS: f32 = 0.18;
/// A worker delivers while the player may be watching the tree instead, so its
/// pulse is longer and softer. Two lengths keep the two sources distinguishable
/// even when a delivery lands mid-drag.
const DELIVERY_PULSE_SECONDS: f32 = 0.5;
const TOUCH_MOUSE_SUPPRESSION_SECONDS: f32 = 0.5;
const SAVE_RETRY_INITIAL_SECONDS: f32 = 1.0;
const SAVE_RETRY_MAX_SECONDS: f32 = 30.0;
/// Wages move the treasury twenty times a second. Without a floor on the write
/// rate that would be a synchronous `localStorage.setItem` every frame.
const SAVE_INTERVAL_SECONDS: f32 = 5.0;

const FLOATER_SECONDS: f32 = 0.9;
const FLOATER_RISE: f32 = 78.0;

pub(crate) const INK: Color = Color::srgb(0.16, 0.08, 0.06);
pub(crate) const CREAM: Color = Color::srgb(1.0, 0.94, 0.72);
pub(crate) const BROWN: Color = Color::srgb(0.33, 0.14, 0.08);
pub(crate) const BROWN_LIGHT: Color = Color::srgb(0.52, 0.25, 0.12);
pub(crate) const GOLD: Color = Color::srgb(1.0, 0.75, 0.05);
pub(crate) const MUTED: Color = Color::srgb(0.55, 0.47, 0.36);

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

/// The simulation, at a fixed 20 Hz. Mirrors stages 1-7 of the architecture
/// doc's §7 schedule; stages 8 and 9 arrive with the units that need them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Sim {
    Purchase,
    Spawn,
    Advance,
    Settle,
    Snapshot,
}

/// Presentation, every frame. Nothing in here may write simulation state.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Present {
    Layout,
    Input,
    Render,
    Export,
}

impl Plugin for HarvestGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneLayout>()
            .init_resource::<Multipliers>()
            .init_resource::<EconomySnapshot>()
            .init_resource::<HarvestController>()
            .init_resource::<PendingSettlement>()
            .init_resource::<DeliveryQueue>()
            .init_resource::<HireRequests>()
            .init_resource::<RestartRequest>()
            .init_resource::<PersistenceDirty>()
            .init_resource::<Feedback>()
            .init_resource::<MenuState>()
            .init_resource::<PointerGuard>()
            .init_resource::<DiagnosticPointerTrace>()
            .insert_resource(Time::<Fixed>::from_hz(SIM_HZ))
            .add_systems(Startup, setup)
            .add_systems(Startup, apply_test_time_scale)
            .configure_sets(
                FixedUpdate,
                (
                    Sim::Purchase,
                    Sim::Spawn,
                    Sim::Advance,
                    Sim::Settle,
                    Sim::Snapshot,
                )
                    .chain(),
            )
            .configure_sets(
                Update,
                (
                    Present::Layout,
                    Present::Input,
                    Present::Render,
                    Present::Export,
                )
                    .chain(),
            )
            .add_systems(
                FixedUpdate,
                (
                    (apply_restart, apply_purchases)
                        .chain()
                        .in_set(Sim::Purchase),
                    worker::spawn_missing_workers.in_set(Sim::Spawn),
                    advance_cycles.in_set(Sim::Advance),
                    settle.in_set(Sim::Settle),
                    snapshot_economy.in_set(Sim::Snapshot),
                ),
            )
            .add_systems(
                Update,
                // Chained, not merely grouped: `apply_responsive_hud` reads the
                // `SceneLayout` that `refresh_layout` writes, and without an
                // ordering edge Bevy is free to run it against last frame's
                // viewport on the frame a resize lands.
                (
                    refresh_layout,
                    apply_layout,
                    hud::apply_responsive_hud,
                    hud::apply_store_layout,
                )
                    .chain()
                    .in_set(Present::Layout),
            )
            .add_systems(
                Update,
                (
                    (handle_menu, hud::sync_menu_visibility).chain(),
                    handle_harvest_input,
                    move_keyboard_harvest,
                    queue_manual_settlement,
                )
                    .chain()
                    .in_set(Present::Input),
            )
            .add_systems(
                Update,
                (
                    worker::position_workers,
                    worker::animate_workers,
                    animate_banana,
                    update_feedback,
                    update_floaters,
                    hud::sync_readout,
                    hud::sync_shop,
                    hud::style_buttons,
                )
                    .in_set(Present::Render),
            )
            .add_systems(
                Update,
                (persist_changes, sync_web_test_state).in_set(Present::Export),
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

/// The banana the *player* drags. Several systems reach for it through
/// `Single`, which silently skips the whole system when the query does not
/// match exactly one entity, so nothing else may ever carry this marker.
/// Workers carry [`worker::CarriedBanana`] instead.
#[derive(Component)]
struct Banana;

#[derive(Component)]
struct BananaAnimation {
    timer: Timer,
}

/// A rising "+n" over the stall. Two sources, two colours, two magnitudes, so
/// a worker's delivery is still legible as someone else's work when it lands
/// during the player's own drag.
#[derive(Component)]
struct Floater {
    elapsed: f32,
    origin: Vec2,
}

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

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonAction {
    OpenMenu,
    HireWorker,
    Resume,
    #[cfg(target_arch = "wasm32")]
    Diagnostics,
    Restart,
    ConfirmRestart,
    CancelRestart,
}

impl ButtonAction {
    /// Which menu state the button is live in. The touch hit-test iterates
    /// every button regardless of what is actually on screen, so without this
    /// a tap on the scrim would reach the shop card underneath it.
    fn active_in(self, menu: MenuState) -> bool {
        match self {
            ButtonAction::OpenMenu | ButtonAction::HireWorker => menu == MenuState::Closed,
            ButtonAction::Resume | ButtonAction::Restart => menu == MenuState::Open,
            #[cfg(target_arch = "wasm32")]
            ButtonAction::Diagnostics => menu == MenuState::Open,
            ButtonAction::ConfirmRestart | ButtonAction::CancelRestart => {
                menu == MenuState::ConfirmRestart
            }
        }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct SceneLayout {
    pub(crate) viewport: Vec2,
    zone_size: f32,
    harvest: Vec2,
    deposit: Vec2,
    banana_home: Vec2,
    banana_size: f32,
    harvest_bounds: Rect,
    deposit_bounds: Rect,
    ground_top: f32,
    /// Where a worker stands to pick, and where it stands to unload. The route
    /// between the two is the whole visible economy.
    pub(crate) grove_stand: f32,
    pub(crate) stall_stand: f32,
    /// Whole-number sprite scale, shared with the tree and the stall so every
    /// texel in the scene is the same size. A fractional scale here would make
    /// a walking monkey shimmer against a world drawn on the integer grid.
    pub(crate) world_scale: f32,
}

impl Default for SceneLayout {
    fn default() -> Self {
        Self::for_viewport(Vec2::new(1280.0, 720.0))
    }
}

impl SceneLayout {
    pub(crate) fn for_viewport(viewport: Vec2) -> Self {
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
            grove_stand: harvest.x + zone_size * 0.30,
            stall_stand: deposit.x - zone_size * 0.34,
            world_scale: zone_size / 128.0,
        }
    }

    /// Ground level, where a worker's feet go. Lanes step *down* from here,
    /// toward the camera; stepping up would stand a monkey on the sky.
    pub(crate) fn ground_top(self) -> f32 {
        self.ground_top
    }

    pub(crate) fn stall_glow_anchor(self) -> Vec2 {
        self.deposit + Vec2::new(0.0, self.zone_size * 0.34)
    }

    /// Snap to the world's texel grid, so pixel-art detail does not crawl at
    /// the low speeds a walking monkey moves at.
    pub(crate) fn snap(self, value: f32) -> f32 {
        (value / self.world_scale).round() * self.world_scale
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

/// Every banana that moves the treasury arrives through here, whether the player
/// dragged it, a worker carried it, or a worker ate it. One queue, one
/// settlement path, one place the economy can be wrong.
#[derive(Resource, Debug, Default)]
struct DeliveryQueue {
    entries: Vec<Delivery>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Delivery {
    /// Always positive. [`DeliveryKind::Snack`] is the one that subtracts.
    amount: f64,
    kind: DeliveryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeliveryKind {
    /// The player dragged a banana to the stall.
    Manual,
    /// A worker unloaded its payload.
    Worker,
    /// A worker ate its wage, out of the delivery it just made.
    Snack,
}

impl DeliveryKind {
    fn is_income(self) -> bool {
        !matches!(self, DeliveryKind::Snack)
    }
}

/// Counted rather than a flag, so clicking twice between two fixed ticks does
/// not silently drop a purchase.
#[derive(Resource, Debug, Default)]
pub(crate) struct HireRequests(pub(crate) u32);

#[derive(Resource, Debug, Default)]
struct RestartRequest(bool);

#[derive(Resource, Debug)]
struct PersistenceDirty {
    pending: bool,
    /// Set for things the player did, so their progress reaches disk before
    /// they can close the tab. The wage drain never sets it.
    immediate: bool,
    since_last_save: f32,
    retry_in_seconds: f32,
    next_retry_delay_seconds: f32,
}

impl Default for PersistenceDirty {
    fn default() -> Self {
        Self {
            pending: false,
            immediate: false,
            since_last_save: 0.0,
            retry_in_seconds: 0.0,
            next_retry_delay_seconds: SAVE_RETRY_INITIAL_SECONDS,
        }
    }
}

impl PersistenceDirty {
    fn mark_pending(&mut self) {
        self.pending = true;
    }

    fn mark_immediate(&mut self) {
        self.pending = true;
        self.immediate = true;
        self.retry_in_seconds = 0.0;
    }
}

#[derive(Resource, Debug, Default)]
struct PointerGuard {
    suppress_mouse_for: f32,
    /// `handle_menu` collects presses from `Changed<Interaction>` *and* from a
    /// manual touch hit-test, deduplicated only within a single frame. Every
    /// other action is idempotent - `OpenMenu` when the menu is already open is
    /// a no-op - but hiring is not, so one tap resolving on two frames would
    /// buy two workers. A human cannot tap twice inside this window anyway.
    suppress_hire_for: f32,
}

const HIRE_DEBOUNCE_SECONDS: f32 = 0.25;

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
pub(crate) struct Feedback {
    success: Option<Timer>,
    /// 0.0..=1.0, shared with the HUD so the counter can accent a delivery.
    pub(crate) pulse: f32,
}

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuState {
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

    hud::setup_hud(&mut commands);
    hud::setup_menu(&mut commands);
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
    let zone_scale = layout.world_scale;
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

/// Run the clock faster, for tests only.
///
/// A worker's cycle is 50 *simulated* seconds, and an end-to-end test that
/// watches a delivery therefore takes 50 real ones. Scaling `Time<Virtual>` is
/// the honest way to shorten that: `Time<Fixed>` is driven by virtual time, so
/// `advance_cycles` still sees a constant 50 ms `dt` and the *fixed-step
/// simulation* is bit-identical per tick. Shortening the cycle constants
/// instead would test a different game.
///
/// What is **not** identical is anything paced by frames rather than ticks, and
/// a test that cares about these should ask for real time:
///
/// - `queue_manual_settlement` runs in `Update`, so a hand-harvest is capped at
///   one per frame. Per *simulated* second, manual income falls by roughly the
///   scale factor.
/// - `persist_changes` and `PointerGuard` both read `Res<Time>`, which is
///   virtual. `SAVE_INTERVAL_SECONDS` and the touch-suppression window shrink
///   by the scale factor in wall-clock terms.
///
/// Behind a default-off cargo feature rather than a plain `#[cfg(debug)]`,
/// because the shipped artefact is a wasm build like the test one: without the
/// feature gate, `?speed=100` would be a cheat code in the released game.
///
/// `max_delta` is deliberately left alone. It clamps the **raw** delta before
/// the scale is applied (`bevy_time::virt`), not after, so a 60 Hz frame's
/// 16.7 ms is already 15x under the 250 ms default and the clamp never fires at
/// any scale this hook allows. Raising it "to match" would only widen the
/// spiral-of-death guard: at 60x, a 30-second stall - a breakpoint, a
/// backgrounded tab, a lost wgpu device - would become 1800 virtual seconds and
/// 36,000 fixed ticks in a single frame.
#[cfg(all(feature = "test-hooks", target_arch = "wasm32"))]
fn apply_test_time_scale(mut virtual_time: ResMut<Time<Virtual>>) {
    /// Beyond this the fixed-step catch-up loop does more work per frame than a
    /// frame has time for, and the run stops being faster in wall-clock terms.
    const MAX_SCALE: f64 = 60.0;

    let Some(scale) = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| {
            search
                .trim_start_matches('?')
                .split('&')
                .find_map(|pair| pair.strip_prefix("speed=").map(str::to_owned))
        })
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|scale| scale.is_finite() && (1.0..=MAX_SCALE).contains(scale))
    else {
        return;
    };

    virtual_time.set_relative_speed_f64(scale);
    bevy::log::info!("test-hooks: simulation running at {scale}x");
}

#[cfg(not(all(feature = "test-hooks", target_arch = "wasm32")))]
fn apply_test_time_scale() {}

// ────────────────────────────────────────────────── simulation, at 20 Hz

/// Stage 1. [`Treasury`] is written here and in [`settle`], both inside
/// `FixedUpdate`; no presentation system may write it.
fn apply_purchases(
    mut requests: ResMut<HireRequests>,
    mut treasury: ResMut<Treasury>,
    mut workforce: ResMut<Workforce>,
    mut dirty: ResMut<PersistenceDirty>,
    multipliers: Res<Multipliers>,
) {
    let requested = std::mem::take(&mut requests.0);
    for _ in 0..requested {
        let plan = plan_hire(*workforce, *treasury, *multipliers);
        if !plan.affordable {
            break;
        }
        treasury.charge(plan.cost);
        workforce.hire();
        dirty.mark_immediate();
    }
}

/// Restart fans out across five pieces of state. Routing it through a request
/// keeps that fan-out in one place instead of inlining it into menu handling.
#[allow(clippy::too_many_arguments)]
fn apply_restart(
    mut request: ResMut<RestartRequest>,
    mut commands: Commands,
    mut treasury: ResMut<Treasury>,
    mut workforce: ResMut<Workforce>,
    mut queue: ResMut<DeliveryQueue>,
    mut requests: ResMut<HireRequests>,
    mut dirty: ResMut<PersistenceDirty>,
    mut restored: ResMut<worker::RestoreWorkers>,
    workers: Query<Entity, With<Worker>>,
    floaters: Query<Entity, With<Floater>>,
) {
    if !std::mem::take(&mut request.0) {
        return;
    }

    restart_run(&mut treasury, &mut workforce);
    restored.clear();
    queue.entries.clear();
    requests.0 = 0;
    for entity in &workers {
        commands.entity(entity).despawn();
    }
    // Otherwise up to 0.9 s of "+5" keeps rising over a stall that just went
    // back to zero.
    for entity in &floaters {
        commands.entity(entity).despawn();
    }
    dirty.mark_immediate();
}

/// Stage 5. The only writer of [`HarvestCycle`] once a worker is in flight.
///
/// The larder is the running balance a worker is allowed to eat from: the
/// treasury as it stood at the top of the tick, plus whatever has already been
/// delivered during it. Threading it through every worker in turn is what makes
/// the treasury structurally non-negative - a worker that cannot afford its meal
/// stalls instead of overdrawing - and it is why the shop can quote a bare
/// signing fee with no wage reserve bolted on.
fn advance_cycles(
    time: Res<Time<Fixed>>,
    multipliers: Res<Multipliers>,
    treasury: Res<Treasury>,
    mut queue: ResMut<DeliveryQueue>,
    mut workers: Query<(Entity, &mut HarvestCycle, &CycleSpec, Option<&RestoredCycle>)>,
    mut commands: Commands,
) {
    // f64 from the fixed clock. `delta_secs()` is f32, which would make the
    // economy's determinism depend on f32 rounding; 50 ms is exactly
    // representable as a `Duration`, so this is reproducible across platforms.
    let dt = time.delta().as_secs_f64();
    // Anything already queued is income the player earned last frame and that
    // `settle` will credit ahead of any meal charged below, so it is edible.
    // Leaving it out would stall a worker over a banana the player has in hand.
    let pending: f64 = queue
        .entries
        .iter()
        .filter(|delivery| delivery.kind.is_income())
        .map(|delivery| delivery.amount)
        .sum();
    let mut larder = treasury.bananas() + pending;

    for (entity, mut cycle, spec, restored) in &mut workers {
        // D2: what a unit earns and what it eats both come off its own
        // `CycleSpec`, never off a count times a constant.
        let terms = if restored.is_some() {
            CycleTerms {
                payload: 0.0,
                meal: 0.0,
            }
        } else {
            CycleTerms::new(*spec, *multipliers)
        };
        let output = cycle.advance(dt, *spec, *multipliers, terms, &mut larder);

        if restored.is_some() && cycle.segment() == crate::domain::Segment::ToGrove {
            commands.entity(entity).remove::<RestoredCycle>();
        }

        if output.delivered > 0.0 {
            queue.entries.push(Delivery {
                amount: output.delivered,
                kind: DeliveryKind::Worker,
            });
        }
        if output.eaten > 0.0 {
            queue.entries.push(Delivery {
                amount: output.eaten,
                kind: DeliveryKind::Snack,
            });
        }
    }
}

/// Stage 6. The queue is settled strictly in the order it was filled, which is
/// what keeps the treasury non-negative: a worker's meal is always queued behind
/// the delivery that funds it.
fn settle(
    mut treasury: ResMut<Treasury>,
    mut queue: ResMut<DeliveryQueue>,
    mut dirty: ResMut<PersistenceDirty>,
    mut feedback: ResMut<Feedback>,
    mut commands: Commands,
    layout: Res<SceneLayout>,
) {
    for delivery in queue.entries.drain(..) {
        match delivery.kind {
            DeliveryKind::Manual => {
                treasury.credit(delivery.amount);
                dirty.mark_immediate();
            }
            DeliveryKind::Worker => {
                treasury.credit(delivery.amount);
                // Not immediate: these arrive `W / 50` times a second, so the
                // immediate path would put the save write rate back on the
                // treadmill the throttle exists to stop, and would reset the
                // retry backoff every time.
                dirty.mark_pending();
            }
            DeliveryKind::Snack => {
                treasury.charge(delivery.amount);
                dirty.mark_pending();
            }
        }

        if delivery.kind.is_income() {
            feedback.success = Some(Timer::new(
                Duration::from_secs_f32(match delivery.kind {
                    DeliveryKind::Worker => DELIVERY_PULSE_SECONDS,
                    _ => SUCCESS_PULSE_SECONDS,
                }),
                TimerMode::Once,
            ));
        }
        spawn_floater(&mut commands, &layout, delivery);
    }
}

/// Stage 7. Counted from the world rather than from `Workforce`, because a
/// stalled worker is still hired: the count cannot tell you how many are
/// actually working, and reporting a rate the world is not producing is what
/// the readout used to get wrong.
fn snapshot_economy(
    workforce: Res<Workforce>,
    multipliers: Res<Multipliers>,
    cycles: Query<&HarvestCycle, With<Worker>>,
    mut snapshot: ResMut<EconomySnapshot>,
) {
    let stalled = cycles.iter().filter(|cycle| cycle.is_hungry()).count() as u32;
    *snapshot = EconomySnapshot::project(workforce.count(), stalled, *multipliers);
}

/// Three sources, three sizes, three colours - readable without reading. The
/// snack is the smallest and the dimmest on purpose: it is the cost of doing
/// business, not an event the player has to act on.
fn spawn_floater(commands: &mut Commands, layout: &SceneLayout, delivery: Delivery) {
    let origin = layout.stall_glow_anchor();
    let (label, size, colour) = match delivery.kind {
        DeliveryKind::Worker => (format!("+{:.0}", delivery.amount), 34.0, GOLD),
        DeliveryKind::Manual => (format!("+{:.0}", delivery.amount), 26.0, CREAM),
        DeliveryKind::Snack => (format!("-{:.1}", delivery.amount), 22.0, MUTED),
    };
    commands.spawn((
        Text2d::new(label),
        TextFont::from_font_size(size),
        TextColor(colour),
        Transform::from_translation(origin.extend(5.0)),
        Floater {
            elapsed: 0.0,
            origin,
        },
    ));
}

// ─────────────────────────────────────────────────────────────── input

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
    mut restart: ResMut<RestartRequest>,
    mut hire_requests: ResMut<HireRequests>,
    mut pointer_guard: ResMut<PointerGuard>,
    mut feedback: ResMut<Feedback>,
    time: Res<Time>,
    layout: Res<SceneLayout>,
    mut banana: Single<&mut Transform, With<Banana>>,
) {
    pointer_guard.suppress_hire_for =
        (pointer_guard.suppress_hire_for - time.delta_secs()).max(0.0);
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
            // `active_in` is the guard that stops a tap on the menu scrim from
            // reaching the shop card sitting underneath it.
            if action.active_in(*menu)
                && contains_inclusive(Rect::from_center_size(center, size), position)
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
            ButtonAction::HireWorker
                if *menu == MenuState::Closed && pointer_guard.suppress_hire_for == 0.0 =>
            {
                hire_requests.0 += 1;
                pointer_guard.suppress_hire_for = HIRE_DEBOUNCE_SECONDS;
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
                restart.0 = true;
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
    mut hire_requests: ResMut<HireRequests>,
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

    if keys.just_pressed(KeyCode::KeyB) {
        hire_requests.0 += 1;
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

/// The player's own harvest joins the queue every worker delivers into, so
/// there is exactly one path from "bananas were earned" to "the treasury says
/// so". The interaction resets here, in the frame the player acted; the credit
/// lands on the next fixed tick.
fn queue_manual_settlement(
    frame_count: Res<FrameCount>,
    mut controller: ResMut<HarvestController>,
    mut pending: ResMut<PendingSettlement>,
    mut queue: ResMut<DeliveryQueue>,
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
    if matched {
        queue.entries.push(Delivery {
            amount: BANANAS_PER_HARVEST,
            kind: DeliveryKind::Manual,
        });
        controller.interaction = HarvestInteraction::Idle;
        banana.translation = layout.banana_home.extend(3.0);
    }
    diagnostic_log!(
        frame_count,
        "settlement",
        diagnostic_pointer;
        "source={:?} interaction={:?} matched={} queued={}",
        source,
        interaction_before,
        matched,
        matched,
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

// ────────────────────────────────────────────────────────── presentation

fn persist_changes(
    time: Res<Time>,
    mut dirty: ResMut<PersistenceDirty>,
    treasury: Res<Treasury>,
    workforce: Res<Workforce>,
) {
    dirty.since_last_save += time.delta_secs();
    if !dirty.pending {
        return;
    }

    dirty.retry_in_seconds = (dirty.retry_in_seconds - time.delta_secs()).max(0.0);
    if dirty.retry_in_seconds > 0.0 {
        return;
    }
    if !dirty.immediate && dirty.since_last_save < SAVE_INTERVAL_SECONDS {
        return;
    }

    let run = persistence::SavedRun {
        treasury: *treasury,
        workforce: *workforce,
    };
    match persistence::store_run(run) {
        Ok(()) => {
            dirty.pending = false;
            dirty.immediate = false;
            dirty.since_last_save = 0.0;
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
    feedback.pulse = pulse;

    let alpha = if drag_highlight { 0.34 } else { pulse * 0.46 };
    glow.color = Color::srgba(1.0, 0.78, 0.08, alpha);
}

fn update_floaters(
    time: Res<Time>,
    mut commands: Commands,
    mut floaters: Query<(Entity, &mut Floater, &mut Transform, &mut TextColor)>,
) {
    for (entity, mut floater, mut transform, mut color) in &mut floaters {
        floater.elapsed += time.delta_secs();
        let progress = (floater.elapsed / FLOATER_SECONDS).clamp(0.0, 1.0);
        // Ease out, so it leaps off the stall and settles as it fades.
        let eased = 1.0 - (1.0 - progress) * (1.0 - progress);
        transform.translation = (floater.origin + Vec2::new(0.0, FLOATER_RISE * eased)).extend(5.0);
        color.0.set_alpha(1.0 - eased);

        if progress >= 1.0 {
            commands.entity(entity).despawn();
        }
    }
}

pub(crate) fn contains_inclusive(bounds: Rect, point: Vec2) -> bool {
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
#[derive(serde::Serialize)]
struct TestPoint {
    x: f32,
    y: f32,
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Serialize)]
struct TestBounds {
    min: TestPoint,
    max: TestPoint,
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Serialize)]
struct TestWorker {
    x: f32,
    y: f32,
    segment: &'static str,
    carrying: bool,
    hungry: bool,
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TestButtons {
    menu: TestPoint,
    hire_worker: TestPoint,
    resume: TestPoint,
    logs: TestPoint,
    restart: TestPoint,
    confirm_restart: TestPoint,
    cancel_restart: TestPoint,
}

/// Serialised through `serde` rather than hand-rolled positional formatting:
/// one transposed argument in a thirty-argument `format!` is a silent lie in
/// the test oracle, and this struct is about to grow several fields.
#[cfg(target_arch = "wasm32")]
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TestState {
    ready: bool,
    bananas: f64,
    workers: u32,
    next_cost: f64,
    meal: f64,
    can_hire: bool,
    gross_per_sec: f64,
    wages_per_sec: f64,
    net_per_sec: f64,
    interaction: &'static str,
    menu: &'static str,
    viewport: TestPoint,
    active_touches: usize,
    touch: TestPoint,
    banana: TestPoint,
    harvest: TestPoint,
    harvest_bounds: TestBounds,
    deposit: TestPoint,
    monkeys: Vec<TestWorker>,
    buttons: TestButtons,
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
fn sync_web_test_state(
    treasury: Res<Treasury>,
    workforce: Res<Workforce>,
    multipliers: Res<Multipliers>,
    snapshot: Res<EconomySnapshot>,
    controller: Res<HarvestController>,
    menu: Res<MenuState>,
    layout: Res<SceneLayout>,
    primary_window: Single<&Window, With<PrimaryWindow>>,
    touches: Res<Touches>,
    banana_transform: Single<&Transform, With<Banana>>,
    workers: Query<(&HarvestCycle, &Transform), With<Worker>>,
    buttons: Query<(&ButtonAction, &ComputedNode, &UiGlobalTransform)>,
    mut warmup_frames: Local<u8>,
) {
    use crate::domain::Segment;
    use wasm_bindgen::JsValue;

    if *warmup_frames < 3 {
        *warmup_frames += 1;
        return;
    }

    let point = |value: Vec2| TestPoint {
        x: value.x,
        y: value.y,
    };
    let screen = |value: Vec2| point(layout.world_to_screen(value));

    let mut button_centers = [Vec2::ZERO; 7];
    for (action, node, transform) in &buttons {
        let index = match action {
            ButtonAction::OpenMenu => 0,
            ButtonAction::HireWorker => 1,
            ButtonAction::Resume => 2,
            ButtonAction::Diagnostics => 3,
            ButtonAction::Restart => 4,
            ButtonAction::ConfirmRestart => 5,
            ButtonAction::CancelRestart => 6,
        };
        button_centers[index] = transform.translation * node.inverse_scale_factor;
    }

    let plan = plan_hire(*workforce, *treasury, *multipliers);
    let state = TestState {
        ready: true,
        bananas: treasury.bananas(),
        workers: workforce.count(),
        next_cost: plan.cost,
        meal: plan.meal,
        can_hire: plan.affordable,
        gross_per_sec: snapshot.gross_per_sec,
        wages_per_sec: snapshot.wages_per_sec,
        net_per_sec: snapshot.net_per_sec,
        interaction: match controller.interaction {
            HarvestInteraction::Idle => "idle",
            HarvestInteraction::Dragging { .. } => "dragging",
            HarvestInteraction::KeyboardHarvest { .. } => "keyboard-harvest",
        },
        menu: match *menu {
            MenuState::Closed => "closed",
            MenuState::Open => "open",
            MenuState::ConfirmRestart => "confirm-restart",
        },
        viewport: point(Vec2::new(primary_window.width(), primary_window.height())),
        active_touches: touches.iter().count(),
        touch: point(
            touches
                .iter()
                .next()
                .map_or(Vec2::ZERO, |touch| touch.position()),
        ),
        banana: screen(banana_transform.translation.truncate()),
        harvest: screen(layout.harvest),
        harvest_bounds: TestBounds {
            min: screen(Vec2::new(
                layout.harvest_bounds.min.x,
                layout.harvest_bounds.max.y,
            )),
            max: screen(Vec2::new(
                layout.harvest_bounds.max.x,
                layout.harvest_bounds.min.y,
            )),
        },
        deposit: screen(layout.deposit),
        monkeys: workers
            .iter()
            .map(|(cycle, transform)| {
                let position = layout.world_to_screen(transform.translation.truncate());
                TestWorker {
                    x: position.x,
                    y: position.y,
                    segment: match cycle.segment() {
                        Segment::ToGrove => "to-grove",
                        Segment::Pick => "pick",
                        Segment::ToDepot => "to-depot",
                        Segment::Unload => "unload",
                        Segment::Snack => "snack",
                    },
                    carrying: cycle.segment().holds_banana(),
                    hungry: cycle.is_hungry(),
                }
            })
            .collect(),
        buttons: TestButtons {
            menu: point(button_centers[0]),
            hire_worker: point(button_centers[1]),
            resume: point(button_centers[2]),
            logs: point(button_centers[3]),
            restart: point(button_centers[4]),
            confirm_restart: point(button_centers[5]),
            cancel_restart: point(button_centers[6]),
        },
    };

    let Ok(encoded) = serde_json::to_string(&state) else {
        return;
    };
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Reflect::set(
            window.as_ref(),
            &JsValue::from_str("__BANANA_MONKEY_TEST_STATE__"),
            &JsValue::from_str(&encoded),
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

    #[test]
    fn the_worker_route_runs_between_the_two_zones_at_every_viewport() {
        for viewport in [
            Vec2::new(320.0, 568.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(1920.0, 1080.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);

            assert!(layout.grove_stand < layout.stall_stand);
            // Workers stand clear of both sprite centres, so they do not walk
            // through the tree or the stall.
            assert!(layout.grove_stand > layout.harvest.x);
            assert!(layout.stall_stand < layout.deposit.x);
        }
    }

    #[test]
    fn world_positions_snap_to_a_whole_texel_grid() {
        let desktop = SceneLayout::for_viewport(Vec2::new(1280.0, 720.0));
        let phone = SceneLayout::for_viewport(Vec2::new(390.0, 844.0));

        // A fractional scale would make the monkey shimmer against a scene
        // whose every other sprite is drawn on the integer grid.
        assert_eq!(desktop.world_scale, 2.0);
        assert_eq!(phone.world_scale, 1.0);
        assert_eq!(desktop.snap(10.4), 10.0);
        assert_eq!(desktop.snap(11.4), 12.0);
    }

    #[test]
    fn buttons_are_only_live_in_the_view_that_shows_them() {
        // A tap on the menu scrim must never reach the shop card underneath.
        assert!(ButtonAction::HireWorker.active_in(MenuState::Closed));
        assert!(!ButtonAction::HireWorker.active_in(MenuState::Open));
        assert!(!ButtonAction::HireWorker.active_in(MenuState::ConfirmRestart));
        assert!(ButtonAction::OpenMenu.active_in(MenuState::Closed));
        assert!(ButtonAction::Resume.active_in(MenuState::Open));
        assert!(!ButtonAction::Resume.active_in(MenuState::Closed));
        assert!(ButtonAction::ConfirmRestart.active_in(MenuState::ConfirmRestart));
        assert!(!ButtonAction::ConfirmRestart.active_in(MenuState::Open));
    }
}
