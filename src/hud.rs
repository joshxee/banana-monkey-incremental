//! The heads-up display and the pause menu.
//!
//! The top bar is one row with three cells - shop, readout, menu - whose outer
//! cells are kept the same width so the readout is optically centred at every
//! viewport. The previous layout centred the counter inside a box that had
//! 110 px of right padding on narrow screens, which put it visibly off-centre
//! against a symmetric scene.

use bevy::prelude::*;

use crate::{
    domain::{EconomySnapshot, Multipliers, Treasury, Workforce, plan_hire},
    game::{
        BROWN, BROWN_LIGHT, ButtonAction, CREAM, Feedback, GOLD, INK, MUTED, MenuState, SceneLayout,
    },
};

/// Below this width the shop drops out of the bar and onto its own row.
///
/// Set by arithmetic, not by taste: two 234 px cells plus padding and gaps need
/// 630 px before the readout gets a usable 106 px between them, so the bar
/// layout is simply not viable below that.
const NARROW_WIDTH: f32 = 700.0;
/// Outer-cell width. Both sides use it, which is what centres the readout.
const SIDE_CELL_DESKTOP: f32 = 234.0;
/// Just wide enough for the MENU button, which is all the right cell holds on a
/// phone. Anything larger and the counter has nowhere left to go: at 390 px,
/// two 106 px cells leave only 134 px between them.
const SIDE_CELL_NARROW: f32 = 88.0;
const BAR_GAP_DESKTOP: f32 = 12.0;
const BAR_GAP_NARROW: f32 = 8.0;
/// The MENU button fills the narrow side cell exactly, so the cell width is the
/// button width and nothing is wasted beside it.
const MENU_BUTTON_WIDTH: f32 = 88.0;

const READOUT_MAX_WIDTH: f32 = 230.0;
/// Where the narrow-layout shop row sits, clearing the counter card alone and
/// clearing the counter plus the rate panel once there is a worker to report on.
const NARROW_SHOP_TOP: f32 = 74.0;
const NARROW_SHOP_TOP_WITH_RATES: f32 = 164.0;
const COUNTER_FONT_DESKTOP: f32 = 32.0;
const COUNTER_FONT_NARROW: f32 = 22.0;

#[derive(Component)]
pub(crate) struct HudRoot;

#[derive(Component)]
pub(crate) struct SideCell;

#[derive(Component)]
pub(crate) struct ShopCard;

#[derive(Component)]
pub(crate) struct CounterText;

#[derive(Component)]
pub(crate) struct RatePanel;

#[derive(Component, Clone, Copy)]
pub(crate) enum RateLine {
    Picking,
    Feeding,
    Net,
}

#[derive(Component)]
pub(crate) struct WorkerCountText;

#[derive(Component)]
pub(crate) struct HirePriceText;

#[derive(Component)]
pub(crate) struct HireRequirementText;

/// Marks the shop's button so [`style_buttons`] leaves its colours alone -
/// affordability, not hover, is the dominant signal there.
#[derive(Component)]
pub(crate) struct HireButton;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuView {
    Scrim,
    Main,
    Restart,
}

#[derive(Component, Clone, Copy)]
pub(crate) enum MenuButtonTone {
    Primary,
    Emphasized,
    #[cfg(target_arch = "wasm32")]
    Secondary,
}

pub(crate) fn setup_hud(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::FlexStart,
                column_gap: px(BAR_GAP_DESKTOP),
                padding: UiRect::axes(px(16), px(16)),
                ..default()
            },
            Pickable::IGNORE,
            HudRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(SIDE_CELL_DESKTOP),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    ..default()
                },
                Pickable::IGNORE,
                SideCell,
            ))
            .with_children(spawn_shop_card);

            root.spawn((
                Node {
                    // Flexes into whatever the two equal side cells leave, so
                    // it is exactly centred and can never overflow a phone.
                    flex_grow: 1.0,
                    flex_basis: px(0),
                    min_width: px(0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(8),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|centre| {
                centre.spawn((
                    Node {
                        // Fills its cell up to a maximum, rather than sizing to
                        // its text: the value gains a decimal and the font
                        // pulses on delivery, and a card that resized for
                        // either would breathe horizontally forever.
                        width: percent(100),
                        max_width: px(READOUT_MAX_WIDTH),
                        min_height: px(58),
                        padding: UiRect::axes(px(18), px(10)),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(4)),
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    BackgroundColor(CREAM),
                    BorderColor::all(BROWN),
                    // Label above value, rather than "Bananas: 12.3" on one
                    // line: the inline form needs ~170 px, and a 320 px phone
                    // leaves the middle cell only 136. Stacking also matches
                    // the rate panel's label/value idiom directly below it.
                    children![
                        (
                            Text::new("BANANAS"),
                            TextFont::from_font_size(13.0),
                            TextColor(BROWN_LIGHT),
                        ),
                        (
                            Text::new("0.0"),
                            TextFont::from_font_size(COUNTER_FONT_DESKTOP),
                            TextColor(INK),
                            CounterText,
                        ),
                    ],
                ));

                centre
                    .spawn((
                        Node {
                            display: Display::None,
                            width: percent(100),
                            max_width: px(READOUT_MAX_WIDTH),
                            padding: UiRect::axes(px(14), px(8)),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(2),
                            border: UiRect::all(px(3)),
                            border_radius: BorderRadius::all(px(10)),
                            ..default()
                        },
                        BackgroundColor(CREAM),
                        BorderColor::all(BROWN_LIGHT),
                        RatePanel,
                    ))
                    .with_children(|panel| {
                        // Aligned on the sign so the three lines read as
                        // arithmetic rather than as three unrelated numbers.
                        spawn_rate_line(panel, RateLine::Picking, "PICKING", INK, false);
                        spawn_rate_line(panel, RateLine::Feeding, "FEEDING", BROWN_LIGHT, false);
                        spawn_rate_line(panel, RateLine::Net, "NET", INK, true);
                    });
            });

            root.spawn((
                Node {
                    width: px(SIDE_CELL_DESKTOP),
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                },
                Pickable::IGNORE,
                SideCell,
            ))
            .with_children(|cell| {
                cell.spawn((
                    Button,
                    ButtonAction::OpenMenu,
                    Node {
                        width: px(MENU_BUTTON_WIDTH),
                        min_height: px(52),
                        padding: UiRect::axes(px(10), px(8)),
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
                    TextFont::from_font_size(20.0),
                    TextColor(CREAM),
                ));
            });
        });
}

fn spawn_shop_card(cell: &mut ChildSpawnerCommands) {
    cell.spawn((
        Node {
            width: percent(100),
            padding: UiRect::axes(px(12), px(10)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(CREAM),
        BorderColor::all(BROWN),
        ShopCard,
    ))
    .with_children(|card| {
        card.spawn((
            Node {
                width: percent(100),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            children![
                (
                    // "WORKERS", not "MONKEYS": chefs, unpackers and
                    // technologists are all monkeys too.
                    Text::new("WORKERS"),
                    TextFont::from_font_size(17.0),
                    TextColor(BROWN_LIGHT),
                ),
                (
                    Text::new("0"),
                    TextFont::from_font_size(17.0),
                    TextColor(INK),
                    WorkerCountText,
                ),
            ],
        ));

        card.spawn((
            Button,
            ButtonAction::HireWorker,
            HireButton,
            Node {
                width: percent(100),
                min_height: px(48),
                padding: UiRect::axes(px(12), px(8)),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: px(8),
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::all(px(9)),
                ..default()
            },
            BackgroundColor(BROWN),
            BorderColor::all(GOLD),
            children![
                (
                    Text::new("HIRE WORKER"),
                    TextFont::from_font_size(17.0),
                    TextColor(CREAM),
                    // Without this the label is the flexible child and wraps
                    // onto two lines before the price cell gives up any width.
                    Node {
                        flex_shrink: 0.0,
                        ..default()
                    },
                ),
                (
                    // The price gets its own cell. "HIRE WORKER - 4.6" reads
                    // as minus four.
                    Text::new("4.0"),
                    TextFont::from_font_size(19.0),
                    TextColor(GOLD),
                    Node {
                        flex_shrink: 0.0,
                        ..default()
                    },
                    HirePriceText,
                ),
            ],
        ));

        card.spawn((
            // A greyed button showing "4.0" to a player holding 4 bananas is
            // indistinguishable from a bug, so the real requirement is spelled
            // out whenever the reserve is what blocks the purchase.
            Text::new(""),
            TextFont::from_font_size(13.0),
            TextColor(MUTED),
            HireRequirementText,
        ));
    });
}

fn spawn_rate_line(
    panel: &mut ChildSpawnerCommands,
    line: RateLine,
    label: &str,
    colour: Color,
    annotated: bool,
) {
    panel.spawn((
        Node {
            width: percent(100),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: px(6),
            ..default()
        },
        children![
            (
                Text::new(label),
                TextFont::from_font_size(14.0),
                TextColor(colour),
            ),
            (
                Node {
                    align_items: AlignItems::Baseline,
                    column_gap: px(5),
                    ..default()
                },
                children![
                    (
                        Text::new("+0.00 /s"),
                        TextFont::from_font_size(15.0),
                        TextColor(colour),
                        line,
                    ),
                    (
                        // Demoted so it annotates the net line rather than
                        // competing with it. Whitepaper §7: without this,
                        // players report a lumpy treasury as a bug.
                        Text::new(if annotated { "avg" } else { "" }),
                        TextFont::from_font_size(11.0),
                        TextColor(MUTED),
                    ),
                ],
            ),
        ],
    ));
}

pub(crate) fn setup_menu(commands: &mut Commands) {
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
                        "Drag banana from tree to stall\nPress H to harvest\nPress B to hire a worker\nPress L for input logs"
                    } else {
                        "Drag banana from tree to stall\nPress H to harvest\nPress B to hire a worker"
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
                        Text::new("RESET RUN?"),
                        TextFont::from_font_size(30.0),
                        TextColor(INK),
                    ));
                    panel.spawn((
                        Text::new("Reset bananas to 0 and\ndismiss every worker?\nThis cannot be undone."),
                        TextFont::from_font_size(19.0),
                        TextColor(INK),
                        TextLayout::justify(Justify::Center),
                    ));
                    panel
                        .spawn(menu_button(ButtonAction::ConfirmRestart))
                        .with_child(menu_button_text("RESET RUN"));
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

/// Both outer cells get this, which is the whole trick: the readout sits in the
/// middle cell of a `SpaceBetween` row, so it is only centred on screen while
/// the cells flanking it are the same width.
fn side_cell_width(viewport_width: f32) -> f32 {
    if viewport_width < NARROW_WIDTH {
        SIDE_CELL_NARROW
    } else {
        SIDE_CELL_DESKTOP
    }
}

/// Write only on a real change.
///
/// `Node`, `Text` and `TextFont` are all consumed through `Changed<>` filters:
/// `ui_layout_system` re-pushes taffy style and the text pipeline re-measures
/// glyphs. Assigning an identical value every frame therefore costs a full UI
/// relayout and a text re-measure, forever, on a game that targets a phone.
fn set_if_changed<T: PartialEq>(slot: &mut T, value: T) {
    if *slot != value {
        *slot = value;
    }
}

fn bar_padding(viewport_width: f32) -> f32 {
    if viewport_width < NARROW_WIDTH { 8.0 } else { 16.0 }
}

#[allow(clippy::type_complexity)]
pub fn apply_responsive_hud(
    layout: Res<SceneLayout>,
    workforce: Res<Workforce>,
    mut root: Single<&mut Node, (With<HudRoot>, Without<SideCell>, Without<ShopCard>)>,
    mut cells: Query<&mut Node, (With<SideCell>, Without<HudRoot>, Without<ShopCard>)>,
    mut shop: Single<&mut Node, (With<ShopCard>, Without<HudRoot>, Without<SideCell>)>,
) {
    let narrow = layout.viewport.x < NARROW_WIDTH;
    let side = side_cell_width(layout.viewport.x);
    let pad = bar_padding(layout.viewport.x);

    let gap = px(if narrow {
        BAR_GAP_NARROW
    } else {
        BAR_GAP_DESKTOP
    });
    set_if_changed(&mut root.padding, UiRect::axes(px(pad), px(pad)));
    set_if_changed(&mut root.column_gap, gap);
    for mut cell in &mut cells {
        set_if_changed(&mut cell.width, px(side));
    }

    if narrow {
        // No room in the bar at 390 px - the counter and the menu button
        // already consume it - so the shop takes its own row below, where
        // there is nothing but sky until the tree line. It has to clear the
        // whole centre column, which grows by the rate panel's height the
        // moment the first worker is hired.
        let top = px(if workforce.count() > 0 {
            NARROW_SHOP_TOP_WITH_RATES
        } else {
            NARROW_SHOP_TOP
        });
        set_if_changed(&mut shop.position_type, PositionType::Absolute);
        set_if_changed(&mut shop.top, top);
        set_if_changed(&mut shop.left, px(0.0));
        set_if_changed(&mut shop.width, px(214.0));
    } else {
        set_if_changed(&mut shop.position_type, PositionType::Relative);
        set_if_changed(&mut shop.top, px(0.0));
        set_if_changed(&mut shop.left, px(0.0));
        set_if_changed(&mut shop.width, percent(100));
    }
}

pub fn sync_menu_visibility(menu: Res<MenuState>, mut views: Query<(&MenuView, &mut Node)>) {
    for (view, mut node) in &mut views {
        node.display = match view {
            MenuView::Scrim if *menu != MenuState::Closed => Display::Flex,
            MenuView::Main if *menu == MenuState::Open => Display::Flex,
            MenuView::Restart if *menu == MenuState::ConfirmRestart => Display::Flex,
            _ => Display::None,
        };
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn sync_readout(
    layout: Res<SceneLayout>,
    treasury: Res<Treasury>,
    workforce: Res<Workforce>,
    snapshot: Res<EconomySnapshot>,
    feedback: Res<Feedback>,
    mut counter: Single<(&mut Text, &mut TextFont), With<CounterText>>,
    mut panel: Single<&mut Node, With<RatePanel>>,
    mut lines: Query<(&RateLine, &mut Text), Without<CounterText>>,
) {
    let base = if layout.viewport.x < NARROW_WIDTH {
        COUNTER_FONT_NARROW
    } else {
        COUNTER_FONT_DESKTOP
    };
    set_if_changed(&mut counter.0.0, treasury.display_string());
    set_if_changed(
        &mut counter.1.font_size,
        FontSize::Px(base + feedback.pulse * base * 0.2),
    );

    // Before the first hire there is no production to explain, and three zeroed
    // lines would be noise on an otherwise clean opening screen.
    let show = workforce.count() > 0;
    set_if_changed(
        &mut panel.display,
        if show { Display::Flex } else { Display::None },
    );
    if !show {
        return;
    }

    for (line, mut text) in &mut lines {
        let value = match line {
            RateLine::Picking => snapshot.gross_per_sec,
            RateLine::Feeding => -snapshot.wages_per_sec,
            RateLine::Net => snapshot.net_per_sec,
        };
        set_if_changed(&mut text.0, format!("{value:+.2} /s"));
    }
}

#[allow(clippy::type_complexity)]
pub fn sync_shop(
    treasury: Res<Treasury>,
    workforce: Res<Workforce>,
    multipliers: Res<Multipliers>,
    mut count: Single<&mut Text, With<WorkerCountText>>,
    mut price: Single<&mut Text, (With<HirePriceText>, Without<WorkerCountText>)>,
    mut requirement: Single<
        &mut Text,
        (
            With<HireRequirementText>,
            Without<WorkerCountText>,
            Without<HirePriceText>,
        ),
    >,
    mut button: Single<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        With<HireButton>,
    >,
) {
    let plan = plan_hire(*workforce, *treasury, *multipliers);

    set_if_changed(&mut count.0, workforce.count().to_string());
    set_if_changed(&mut price.0, format!("{:.1}", plan.cost));
    set_if_changed(
        &mut requirement.0,
        if plan.affordable {
            String::new()
        } else {
            format!(
                "needs {:.1} ({:.1} + {:.1} feed)",
                plan.required, plan.cost, plan.reserve
            )
        },
    );

    let (interaction, background, border) = &mut *button;
    // Affordability outranks hover here: a button the player cannot press must
    // not light up under the cursor.
    let (fill, edge) = if !plan.affordable {
        (MUTED, BROWN_LIGHT)
    } else {
        match interaction {
            Interaction::Pressed => (GOLD, CREAM),
            Interaction::Hovered => (BROWN_LIGHT, GOLD),
            Interaction::None => (BROWN, GOLD),
        }
    };
    set_if_changed(&mut **background, BackgroundColor(fill));
    set_if_changed(&mut **border, BorderColor::all(edge));
}

#[allow(clippy::type_complexity)]
pub fn style_buttons(
    mut buttons: Query<
        (
            &Interaction,
            Option<&MenuButtonTone>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        (With<ButtonAction>, Without<HireButton>, Changed<Interaction>),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_readout_stays_centred_and_fits_at_every_viewport() {
        // Both flanking cells get one width, and the counter card is fixed, so
        // the readout's centre must land on the viewport's centre. The previous
        // layout centred it inside a box carrying 110 px of right padding,
        // which put it visibly off-centre against a symmetric scene.
        // "BANANAS" over a value, plus padding and border, needs about this.
        const READOUT_MIN_WIDTH: f32 = 106.0;

        for width in [320.0, 390.0, 599.0, 600.0, 699.0, 700.0, 844.0, 1280.0, 1920.0] {
            let side = side_cell_width(width);
            let pad = bar_padding(width);
            let gap = if width < NARROW_WIDTH {
                BAR_GAP_NARROW
            } else {
                BAR_GAP_DESKTOP
            };
            let left_edge = pad + side + gap;
            let right_edge = width - pad - side - gap;
            let centre = (left_edge + right_edge) * 0.5;

            assert!(
                (centre - width * 0.5).abs() < 0.001,
                "width={width} centre={centre}"
            );
            // The card fills this cell up to `READOUT_MAX_WIDTH`, so the cell
            // only has to stay wide enough to read a banana count in.
            assert!(
                right_edge - left_edge >= READOUT_MIN_WIDTH,
                "width={width} leaves only {} for the readout",
                right_edge - left_edge
            );
        }
    }

    #[test]
    fn the_narrow_shop_row_clears_the_bar_and_the_scene() {
        // At 390x844 the zones occupy screen y 530..658, so a card dropped to
        // y 62 collides with neither the bar above nor the tree line below.
        let layout = SceneLayout::for_viewport(Vec2::new(390.0, 844.0));
        let zone_top_on_screen = layout.viewport.y * 0.5 - (layout.ground_top() + 128.0);

        assert!(layout.viewport.x < NARROW_WIDTH);
        assert!(62.0 + 120.0 < zone_top_on_screen);
    }
}
