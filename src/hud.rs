//! The heads-up display and the pause menu.
//!
//! Three bands, matching the scene underneath them: a banner across the top
//! holding the banana count and the rates, the sky and the monkeys in the
//! middle, and the store dug into the dirt along the bottom.
//!
//! Putting the store underground is what makes the unit rows work. As a card in
//! the top bar it had to share a 1280 px line with a centred readout and a menu
//! button, which left it ~316 px - not enough for a hire button and three stat
//! columns, and nowhere near enough at 390. Along the bottom it has the whole
//! viewport width and room to grow downwards, so a second unit is another row
//! rather than a layout problem.
//!
//! The bar keeps three cells with equal outer widths, because that is what
//! centres the readout optically. An earlier layout centred it inside a box
//! carrying 110 px of right padding, which put it visibly off-centre against a
//! symmetric scene.

use bevy::prelude::*;

use crate::{
    domain::{
        Committed, EconomySnapshot, FedStaff, Multipliers, SUPPORT_MEAL_PERIOD, Staff, SupportRole,
        Treasury, UnitKind, Workforce, plan_hire,
    },
    game::{
        BROWN, BROWN_LIGHT, ButtonAction, CREAM, Feedback, GOLD, INK, MUTED, MenuState, SceneLayout,
    },
};

/// Below this width the banner and the store both go compact.
const NARROW_WIDTH: f32 = 600.0;
/// The MENU button fills the side cell exactly, so the cell width is the button
/// width and nothing is wasted beside it. Both outer cells carry it - the left
/// one is an empty spacer whose only job is to keep the banner centred.
const MENU_BUTTON_WIDTH: f32 = 88.0;
/// ...except on the very smallest screens, where two 88 px cells and their gaps
/// leave the banner 112 px and its rate lines wrap mid-number. The button is
/// still a 56x52 touch target, comfortably above the 44 px floor.
const MENU_BUTTON_WIDTH_TINY: f32 = 56.0;
/// Below this the banner cannot afford a full-width menu button beside it.
const TINY_WIDTH: f32 = 360.0;

fn menu_button_width(viewport_width: f32) -> f32 {
    if viewport_width < TINY_WIDTH {
        MENU_BUTTON_WIDTH_TINY
    } else {
        MENU_BUTTON_WIDTH
    }
}
const BAR_GAP_DESKTOP: f32 = 12.0;
const BAR_GAP_NARROW: f32 = 8.0;

/// The banner is one card: count on top, rates under it. Wide enough to read as
/// a banner and narrow enough that the number stays the thing you look at.
const BANNER_MAX_WIDTH_DESKTOP: f32 = 360.0;
const BANNER_MAX_WIDTH_NARROW: f32 = 244.0;
const COUNTER_FONT_DESKTOP: f32 = 34.0;
const COUNTER_FONT_NARROW: f32 = 24.0;

/// The dirt runs from `ground_top` to the bottom of the screen, which is a flat
/// 22% of the viewport height (`ground_top = -0.28 h`). The store is dug into
/// it, so this fraction is the ceiling on how tall the store can be - and at
/// 390 px of landscape height that is only 86 px, which is what the compact
/// padding below exists for.
const DIRT_FRACTION: f32 = 0.22;

#[derive(Component)]
pub(crate) struct HudRoot;

#[derive(Component)]
pub(crate) struct SideCell;

/// The store panel along the bottom of the screen.
#[derive(Component)]
pub(crate) struct StoreRoot;

#[derive(Component)]
pub(crate) struct StoreHeading;

/// A column that a narrow viewport drops. EATS is the one that goes: its value
/// also appears on the banner's FEEDING line and in the `-1.5` floater, so it
/// is the least load-bearing of the five.
#[derive(Component)]
pub(crate) struct OptionalColumn;

/// A table cell, carrying the width it wants at full size. Every cell in a
/// column - the header's and every row's - carries the same one, which is what
/// keeps them aligned when the table is scaled down to fit a phone.
#[derive(Component, Clone, Copy)]
pub(crate) struct TableCell(f32);

/// The banner: banana count and, once there is production to explain, rates.
#[derive(Component)]
pub(crate) struct Banner;

/// The word on the bar's menu button, which shrinks with its cell.
#[derive(Component)]
pub(crate) struct MenuLabel;

/// A shade darker than the dirt sprite behind it, so the store reads as a
/// cut-away into the soil rather than as a panel lying on top of it.
const STORE_SOIL: Color = Color::srgb(0.20, 0.10, 0.05);
/// Column labels underground. The rest of the HUD is dark-on-cream and this one
/// panel is the reverse, so it needs its own two tones: `INK` and `BROWN_LIGHT`
/// on soil are within a few percent of the background and simply vanish.
const STORE_LABEL: Color = Color::srgb(0.68, 0.55, 0.40);

#[derive(Component)]
pub(crate) struct CounterText;

#[derive(Component)]
pub(crate) struct RatePanel;

/// The scrolling list inside the drawer. Separate from [`StoreRoot`] so the
/// grip stays pinned while the rows move under it.
#[derive(Component)]
pub(crate) struct StoreScroll;

#[derive(Component)]
pub(crate) struct StoreGrip;

/// Whether the drawer is pulled up. Persisted with the run, so a player who
/// opened it stays opened.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StoreExpanded(pub bool);

/// The row a rate line lives in, so a line can be hidden label and all.
#[derive(Component, Clone, Copy)]
pub(crate) struct RateLineRow(pub(crate) RateLine);

#[derive(Component, Clone, Copy)]
pub(crate) enum RateLine {
    Farming,
    Feeding,
    Net,
    /// Only shown while someone is starving, so the panel explains a dead rate
    /// instead of contradicting it.
    Hungry,
}

/// Which unit a shop row is about.
///
/// The shop is one row per unit type, and everything in a row is addressed
/// through this rather than through a marker component per field per unit.
/// Adding the Chef is then a variant here, one [`spawn_unit_row`] call, and one
/// arm in [`sync_shop`] - not four new components and four new queries.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unit {
    Worker,
    Support(SupportRole),
}

impl Unit {
    /// Fixed, and deliberately not sorted by price. Ascending cost is not a
    /// stable order - a worker passes the Chef's 25 base after thirteen hires -
    /// so a price-sorted table would re-sort itself under the player's finger
    /// in a panel that gets tapped repeatedly. Declaration order it is.
    const ROWS: [Unit; 4] = [
        Unit::Worker,
        Unit::Support(SupportRole::Chef),
        Unit::Support(SupportRole::Unpacker),
        Unit::Support(SupportRole::Technologist),
    ];

    /// Singular, because the row reads as a spec sheet for one of them: the
    /// count is a column in the row, not part of the heading.
    fn name(self) -> &'static str {
        match self {
            // "WORKER", not "MONKEY": chefs, unpackers and technologists are
            // all monkeys too.
            Unit::Worker => "WORKER",
            Unit::Support(role) => role.name(),
        }
    }

    fn kind(self) -> UnitKind {
        match self {
            Unit::Worker => UnitKind::Worker,
            Unit::Support(role) => UnitKind::Support(role),
        }
    }
}

/// Which number in a unit's row a text node holds. One component and one query
/// beats four of each, and it keeps the columns in a fixed, declared order.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitStat {
    /// Signing fee, on the hire button.
    Price,
    /// Bananas per minute the economy gains from one more of them, right now.
    ///
    /// Was "FARMING", which only a harvester has. A static effect string
    /// ("TRAVEL +15%") would have been honest about the mechanism and useless
    /// for the comparison this table exists to support: three different units
    /// down one column, and a figure that means +12.6% throughput in a
    /// worker-heavy world and about +1% in a cart-heavy one. The live marginal
    /// rate is one unit for every row, and it visibly decays as a role stops
    /// being the bottleneck.
    Gain,
    /// What one of them eats, per trip or per shift.
    Feeding,
    Owned,
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct UnitField {
    pub(crate) unit: Unit,
    pub(crate) stat: UnitStat,
}

/// Marks a unit's hire button so [`style_buttons`] leaves its colours alone -
/// affordability, not hover, is the dominant signal there.
#[derive(Component)]
pub(crate) struct HireButton(pub(crate) Unit);

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
            // An empty cell, the same width as the one holding MENU. It exists
            // only so the banner between them is centred on the viewport
            // rather than on what is left over beside a button.
            root.spawn((
                Node {
                    width: px(MENU_BUTTON_WIDTH),
                    ..default()
                },
                Pickable::IGNORE,
                SideCell,
            ));

            root.spawn((
                Node {
                    // Flexes into whatever the two equal side cells leave, so
                    // it is exactly centred and can never overflow a phone.
                    flex_grow: 1.0,
                    flex_basis: px(0),
                    min_width: px(0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|centre| {
                centre
                    .spawn((
                        Node {
                            // Fills its cell up to a maximum, rather than
                            // sizing to its text: the value gains a decimal and
                            // the font pulses on delivery, and a card that
                            // resized for either would breathe horizontally
                            // forever.
                            width: percent(100),
                            max_width: px(BANNER_MAX_WIDTH_DESKTOP),
                            padding: UiRect::axes(px(18), px(10)),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: px(6),
                            border: UiRect::all(px(4)),
                            border_radius: BorderRadius::all(px(12)),
                            ..default()
                        },
                        BackgroundColor(CREAM),
                        BorderColor::all(BROWN),
                        Banner,
                    ))
                    .with_children(|banner| {
                        // Label above value, rather than "Bananas: 12.3" on one
                        // line: the inline form needs ~170 px, and a 320 px
                        // phone leaves the middle cell only 136.
                        banner.spawn((
                            Text::new("BANANAS"),
                            TextFont::from_font_size(13.0),
                            TextColor(BROWN_LIGHT),
                        ));
                        banner.spawn((
                            Text::new("0.0"),
                            TextFont::from_font_size(COUNTER_FONT_DESKTOP),
                            TextColor(INK),
                            CounterText,
                        ));

                        banner
                            .spawn((
                                Node {
                                    display: Display::None,
                                    width: percent(100),
                                    padding: UiRect::top(px(6)),
                                    flex_direction: FlexDirection::Column,
                                    row_gap: px(2),
                                    // A rule under the count rather than a
                                    // second card: the rates explain the number
                                    // above them, so they belong inside it.
                                    border: UiRect::top(px(2)),
                                    ..default()
                                },
                                BorderColor::all(BROWN_LIGHT),
                                RatePanel,
                            ))
                            .with_children(|panel| {
                                // Aligned on the sign so the three lines read
                                // as arithmetic rather than as three unrelated
                                // numbers.
                                spawn_rate_line(panel, RateLine::Farming, "FARMING", INK, false);
                                spawn_rate_line(
                                    panel,
                                    RateLine::Feeding,
                                    "FEEDING",
                                    BROWN_LIGHT,
                                    false,
                                );
                                spawn_rate_line(panel, RateLine::Net, "NET", INK, true);
                                spawn_rate_line(panel, RateLine::Hungry, "HUNGRY", GOLD, false);
                            });
                    });
            });

            root.spawn((
                Node {
                    width: px(MENU_BUTTON_WIDTH),
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
                        width: percent(100),
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
                    MenuLabel,
                ));
            });
        });

    spawn_store(commands);
}

/// The store, dug into the dirt along the bottom of the screen.
///
/// Dark on dark rather than a cream card: it is underground, and a cream panel
/// down there read as a UI element that had fallen off the top bar. The gold
/// rule along its top edge is the cut line through the soil.
///
/// A **table**: one header, one line per unit, fixed column widths. The first
/// shape put each unit's labels inside its own block, which cost 114 of the
/// 158 px of desktop dirt for a single unit - so the panel overflowed at unit
/// two - and, worse, defeated its own purpose: comparing a Chef against a
/// Worker means reading down a column, and every row repeating its own labels
/// is exactly what stops you doing that.
///
/// Measured capacity, from `store_capacity` in the tests: 2 rows in landscape
/// (86 px of dirt), 2 at 320x640, 3 on a 720p desktop, 3 in portrait, 5 at
/// 1080p. Enough for the Chef and the Unpacker. It is **not** enough for all
/// five MVP units on a small screen, and that is the open decision this shape
/// defers rather than solves: at four units the panel needs either a scroll
/// region (`Overflow::scroll_y` plus a wheel/drag handler) or a pull-up drawer
/// that may temporarily cover the sky. Deciding it now, with one unit built,
/// would be guessing at which.
fn spawn_store(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0),
                left: px(0),
                width: percent(100),
                padding: UiRect::axes(px(14), px(10)),
                flex_direction: FlexDirection::Column,
                // Centred as a block, which keeps the table's own columns
                // aligned - header and rows are identical widths - while
                // matching the symmetry the banner enforces up top. Hard-left
                // it sat in about a thousand pixels of empty soil and read as
                // unfinished rather than as a deliberate table origin.
                align_items: AlignItems::Center,
                row_gap: px(6),
                border: UiRect::top(px(3)),
                // `max_height` bounds this node's box but not its children,
                // which overflow visibly by default. Clipping is the backstop
                // that makes "the store never covers the grass" true by
                // construction rather than by arithmetic; the table shape and
                // `apply_store_layout` going compact are what stop it being
                // needed.
                overflow: Overflow::clip_y(),
                ..default()
            },
            BackgroundColor(STORE_SOIL),
            BorderColor::all(GOLD),
            // Explicitly below the pause scrim (`GlobalZIndex(100)`). The store
            // is its own UI root rather than a child of the bar, so its
            // stacking would otherwise be decided by spawn order in another
            // function - and a shop that stayed lit while the rest of the scene
            // dimmed would be a lie about what is interactive.
            GlobalZIndex(1),
            Pickable::IGNORE,
            StoreRoot,
        ))
        .with_children(|store| {
            spawn_store_grip(store);
            store
                .spawn((
                    StoreScroll,
                    Node {
                        width: percent(100),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(6),
                        // A definite bound is what makes the region scrollable
                        // rather than merely clipped; `apply_store_layout`
                        // writes it from the drawer's current height.
                        min_height: px(0),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                ))
                .with_children(|list| {
                    spawn_store_header(list);
                    for unit in Unit::ROWS {
                        spawn_unit_row(list, unit);
                    }
                });
        });
}

/// The drawer's handle: a gold bar that reads as something to pull.
///
/// The store has no fully closed state. It rests at the height of the dirt it
/// is dug into, which is the one size that cannot cover the grass, the walking
/// route or the deposit - so the player can always see and buy *something* -
/// and pulls up to two thirds when they want to read the whole table. A drawer
/// that could close entirely would hide the only progression surface in a game
/// that opens as a manual clicker.
fn spawn_store_grip(store: &mut ChildSpawnerCommands) {
    store.spawn((
        Button,
        ButtonAction::ToggleStore,
        StoreGrip,
        Node {
            width: px(74),
            height: px(14),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(7)),
            ..default()
        },
        BackgroundColor(BROWN_LIGHT),
        BorderColor::all(GOLD),
    ));
}

/// Column widths, shared by the header and every unit line. Fixed rather than
/// content-sized: content-sized columns re-align themselves per row, which is
/// the one thing a comparison table must not do.
const COL_NAME: f32 = 104.0;
const COL_HIRE: f32 = 84.0;
/// Wide enough for a signed four-digit-and-a-decimal rate plus its unit:
/// "-12.0/min" wrapped at 70 and broke after the slash, which reads as two
/// numbers stacked rather than one rate.
const COL_FARMING: f32 = 80.0;
const COL_EATS: f32 = 74.0;
const COL_OWNED: f32 = 46.0;
const COL_GAP: f32 = 10.0;

/// Column width the table needs, with and without the column a narrow screen
/// drops. Gaps are counted separately because they do not scale with the
/// columns - see `column_scale`.
fn table_columns(with_eats: bool) -> f32 {
    COL_NAME + COL_HIRE + COL_FARMING + COL_OWNED + if with_eats { COL_EATS } else { 0.0 }
}

fn table_gaps(with_eats: bool) -> f32 {
    (if with_eats { 4.0 } else { 3.0 }) * COL_GAP
}

fn table_width(with_eats: bool) -> f32 {
    table_columns(with_eats) + table_gaps(with_eats)
}

/// How far the columns have to shrink to fit `room`, never above 1.
///
/// A 320 px phone gives the store 304 px and the four essential columns want
/// 334, so without this the table is simply clipped: "UNIT" renders as "NIT"
/// and the OWNED value falls off the right edge. Scaling every column by one
/// factor keeps the header lined up with the rows, which dropping or wrapping
/// individual columns would not.
fn column_scale(room: f32, with_eats: bool) -> f32 {
    let columns = table_columns(with_eats);
    if columns <= 0.0 {
        return 1.0;
    }
    ((room - table_gaps(with_eats)) / columns).clamp(0.4, 1.0)
}

fn spawn_store_header(store: &mut ChildSpawnerCommands) {
    store
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: px(COL_GAP),
                ..default()
            },
            StoreHeading,
        ))
        .with_children(|header| {
            for (label, width, optional) in [
                ("UNIT", COL_NAME, false),
                ("HIRE", COL_HIRE, false),
                ("GAIN", COL_FARMING, false),
                ("EATS", COL_EATS, true),
                ("OWNED", COL_OWNED, false),
            ] {
                let mut cell = header.spawn((
                    TableCell(width),
                    Node {
                        width: px(width),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    children![(
                        Text::new(label),
                        TextFont::from_font_size(10.0),
                        TextColor(STORE_LABEL),
                    )],
                ));
                if optional {
                    cell.insert(OptionalColumn);
                }
            }
        });
}

/// One unit: one line, columns aligned with the header above it.
fn spawn_unit_row(store: &mut ChildSpawnerCommands, unit: Unit) {
    store
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: px(COL_GAP),
                ..default()
            },
            unit,
        ))
        .with_children(|row| {
            row.spawn((
                TableCell(COL_NAME),
                Node {
                    width: px(COL_NAME),
                    flex_shrink: 0.0,
                    ..default()
                },
                children![(
                    Text::new(unit.name()),
                    TextFont::from_font_size(14.0),
                    TextColor(CREAM),
                )],
            ));

            row.spawn((
                Button,
                ButtonAction::Hire(unit.kind()),
                HireButton(unit),
                TableCell(COL_HIRE),
                Node {
                    width: px(COL_HIRE),
                    min_height: px(34),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(7)),
                    ..default()
                },
                BackgroundColor(BROWN_LIGHT),
                BorderColor::all(GOLD),
                // Just the price. The column header says HIRE, so repeating the
                // word inside every button costs a word per unit for nothing.
                children![(
                    Text::new("4.0"),
                    TextFont::from_font_size(17.0),
                    TextColor(GOLD),
                    UnitField {
                        unit,
                        stat: UnitStat::Price,
                    },
                )],
            ));

            spawn_unit_cell(row, unit, UnitStat::Gain, COL_FARMING, false);
            spawn_unit_cell(row, unit, UnitStat::Feeding, COL_EATS, true);
            spawn_unit_cell(row, unit, UnitStat::Owned, COL_OWNED, false);
        });
}

fn spawn_unit_cell(
    row: &mut ChildSpawnerCommands,
    unit: Unit,
    stat: UnitStat,
    width: f32,
    optional: bool,
) {
    let mut cell = row.spawn((
        TableCell(width),
        Node {
            width: px(width),
            flex_shrink: 0.0,
            ..default()
        },
        children![(
            Text::new("-"),
            TextFont::from_font_size(14.0),
            TextColor(CREAM),
            // A rate is one token. Allowed to wrap it breaks after the slash.
            TextLayout::linebreak(LineBreak::NoWrap),
            UnitField { unit, stat },
        )],
    ));
    if optional {
        cell.insert(OptionalColumn);
    }
}

fn spawn_rate_line(
    panel: &mut ChildSpawnerCommands,
    line: RateLine,
    label: &str,
    colour: Color,
    annotated: bool,
) {
    panel.spawn((
        RateLineRow(line),
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
                        Text::new("+0.0/min"),
                        TextFont::from_font_size(14.0),
                        TextColor(colour),
                        // A rate is one token. Allowed to wrap it breaks after
                        // the slash, which reads as two numbers.
                        TextLayout::linebreak(LineBreak::NoWrap),
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
    if viewport_width < NARROW_WIDTH {
        8.0
    } else {
        16.0
    }
}

#[allow(clippy::type_complexity)]
pub fn apply_responsive_hud(
    layout: Res<SceneLayout>,
    mut root: Single<&mut Node, (With<HudRoot>, Without<SideCell>, Without<Banner>)>,
    mut cells: Query<&mut Node, (With<SideCell>, Without<HudRoot>, Without<Banner>)>,
    mut banner: Single<&mut Node, (With<Banner>, Without<HudRoot>, Without<SideCell>)>,
    mut menu_label: Single<&mut TextFont, With<MenuLabel>>,
) {
    let narrow = layout.viewport.x < NARROW_WIDTH;
    let pad = bar_padding(layout.viewport.x);
    let cell = menu_button_width(layout.viewport.x);

    set_if_changed(&mut root.padding, UiRect::axes(px(pad), px(pad)));
    set_if_changed(
        &mut root.column_gap,
        px(if narrow {
            BAR_GAP_NARROW
        } else {
            BAR_GAP_DESKTOP
        }),
    );
    for mut side in &mut cells {
        set_if_changed(&mut side.width, px(cell));
    }
    set_if_changed(
        &mut menu_label.font_size,
        FontSize::Px(if cell < MENU_BUTTON_WIDTH { 15.0 } else { 20.0 }),
    );
    set_if_changed(
        &mut banner.max_width,
        px(if narrow {
            BANNER_MAX_WIDTH_NARROW
        } else {
            BANNER_MAX_WIDTH_DESKTOP
        }),
    );
    // A rate line is a label and a value on one line, and at 320 px the banner
    // has 176 to hold "FARMING" and "+6.0/min" in. Padding is the cheapest 16
    // of those pixels to give back - the alternative is the value wrapping to
    // "+6.0/" over "min", which reads as two different numbers.
    set_if_changed(
        &mut banner.padding,
        UiRect::axes(
            px(if cell < MENU_BUTTON_WIDTH { 10.0 } else { 18.0 }),
            px(10.0),
        ),
    );
}

/// Fit the drawer to the dirt it is dug into, or to two thirds of the screen
/// when the player has pulled it up.
///
/// At rest the store may not grow up onto the grass: a shop panel overlapping
/// the ground line reads as a rendering fault, and it would cover the monkeys
/// the player is there to watch. `DIRT_FRACTION` is derived from the same
/// `ground_top` the scene is laid out against, so the two cannot drift.
///
/// That resting height fits two to three rows, which is fewer than the table
/// has. The rest are reached by scrolling, or by pulling the drawer up - a
/// deliberate act the player can undo, and the one state that is allowed to
/// cover the scene. Landscape is the binding case: 390 px of height leaves the
/// dirt 86 px, and a full row needs about 100, so below `COMPACT_DIRT` the panel
/// drops its heading and shortens its buttons rather than spilling over.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn apply_store_layout(
    layout: Res<SceneLayout>,
    expanded: Res<StoreExpanded>,
    mut store: Single<
        &mut Node,
        (
            With<StoreRoot>,
            Without<StoreHeading>,
            Without<OptionalColumn>,
            Without<StoreScroll>,
        ),
    >,
    mut scroll: Single<
        &mut Node,
        (
            With<StoreScroll>,
            Without<StoreRoot>,
            Without<StoreHeading>,
            Without<OptionalColumn>,
        ),
    >,
    mut heading: Single<
        &mut Node,
        (
            With<StoreHeading>,
            Without<StoreRoot>,
            Without<OptionalColumn>,
            Without<StoreScroll>,
        ),
    >,
    mut cells: Query<
        (&TableCell, Option<&OptionalColumn>, &mut Node),
        (
            Without<StoreRoot>,
            Without<StoreHeading>,
            Without<StoreScroll>,
        ),
    >,
    // Measured rather than assumed. The first version of this hard-coded a row
    // pitch and a header height read off a screenshot's *baselines*, which are
    // not box heights - the header came out 9 px short and the "peek" below
    // grew into two thirds of a row of legible text, which is exactly the
    // mid-row fold the quantising was added to prevent.
    heading_size: Single<&ComputedNode, With<StoreHeading>>,
    row_sizes: Query<&ComputedNode, With<Unit>>,
) {
    /// Below this much soil the header row is the first thing to go. A unit
    /// line is self-describing enough without it - "+6.0/min", "1.5/trip" - and
    /// it buys back a whole unit's worth of height in landscape.
    const COMPACT_DIRT: f32 = 120.0;
    /// How much of the screen the drawer covers when pulled up. Two thirds
    /// leaves the sky and the tops of both zones visible, so the scene is still
    /// recognisably there behind the shop.
    const EXPANDED_FRACTION: f32 = 2.0 / 3.0;
    /// The grip's height plus its border, which the scroll region does not get.
    const GRIP_HEIGHT: f32 = 18.0;
    /// Fallbacks for the first frame, before anything has been measured.
    const ROW_FALLBACK: f32 = 34.0;
    const HEADER_FALLBACK: f32 = 14.0;
    /// How much of the next row to leave showing under the clip.
    ///
    /// The store is soil-on-soil with no scrollbar, so a list that ends flush
    /// with the panel edge looks finished rather than scrollable. A sliver of
    /// the next row's gold button border is the universal "there is more" tell,
    /// and it is the reason the height below is quantised at all: clipped to an
    /// arbitrary fraction, the fold lands through the middle of a row's text
    /// and reads as a rendering fault instead of an affordance.
    const PEEK: f32 = 11.0;

    let dirt = layout.viewport.y * DIRT_FRACTION;
    let target = if expanded.0 {
        (layout.viewport.y * EXPANDED_FRACTION).max(dirt)
    } else {
        dirt
    };
    let compact = target < COMPACT_DIRT;
    let pad = bar_padding(layout.viewport.x);
    let vertical = if compact { 4.0 } else { 10.0 };
    let gap = if compact { 4.0 } else { 6.0 };

    // Floor *and* ceiling. The store is dug into the dirt, so its gold top edge
    // is the cut line through the soil and has to sit exactly on the ground
    // line - if the panel shrinks to fit a short list it slides down and
    // uncovers the serrated grass-to-dirt seam in the background art, which
    // reads as a torn sprite. The floor keeps it pinned; the ceiling is what
    // stops it climbing onto the grass.
    set_if_changed(&mut store.min_height, px(dirt));
    set_if_changed(&mut store.max_height, px(target));
    set_if_changed(&mut store.padding, UiRect::axes(px(pad), px(vertical)));
    set_if_changed(&mut store.row_gap, px(gap));
    // A definite bound is what makes the region scroll rather than merely clip.
    // Quantised to whole rows plus a peek, so the fold never cuts through text.
    let measured = |size: f32, fallback: f32| if size > 0.0 { size } else { fallback };
    let row_height = measured(
        row_sizes
            .iter()
            .map(|node| node.size().y * node.inverse_scale_factor)
            .fold(0.0, f32::max),
        ROW_FALLBACK,
    ) + gap;
    let header_height = if compact {
        0.0
    } else {
        measured(
            heading_size.size().y * heading_size.inverse_scale_factor,
            HEADER_FALLBACK,
        ) + gap
    };

    let available = (target - GRIP_HEIGHT - vertical * 2.0).max(0.0);
    let body = (available - header_height).max(0.0);
    let whole_rows = (body / row_height).floor().max(1.0);
    let list = header_height
        + if whole_rows * row_height + PEEK <= body {
            // Room for a clean peek at the next row.
            whole_rows * row_height + PEEK
        } else {
            whole_rows * row_height
        };
    // A *definite* height, not a maximum. `max_height` lets the node size to its
    // content and overflow, in which case the only thing still clipping is the
    // panel's own `clip_y` - at the panel edge, through the middle of whatever
    // row happens to be there. Capped at the content so a short list does not
    // leave a band of empty soil under it.
    let content = header_height + Unit::ROWS.len() as f32 * row_height;
    set_if_changed(&mut scroll.height, px(list.min(available).min(content)));
    set_if_changed(
        &mut heading.display,
        if compact {
            Display::None
        } else {
            Display::Flex
        },
    );

    // Drop the EATS column when the full table will not fit, then scale what is
    // left to whatever room remains. Both are applied to header and value cells
    // through the same component, so the two can never disagree about a column.
    let room = layout.viewport.x - 2.0 * pad;
    let show_eats = table_width(true) <= room;
    let scale = column_scale(room, show_eats);
    for (cell, optional, mut node) in &mut cells {
        let shown = show_eats || optional.is_none();
        set_if_changed(
            &mut node.display,
            if shown { Display::Flex } else { Display::None },
        );
        set_if_changed(&mut node.width, px((cell.0 * scale).floor()));
    }
}

/// Wheel and drag scrolling for the drawer.
///
/// The store is soil-on-soil with no scrollbar, so without this the rows below
/// the fold are unreachable on desktop - and the resting height shows only two
/// of them in landscape.
pub fn scroll_store(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut scroll: Single<(&mut ScrollPosition, &ComputedNode), With<StoreScroll>>,
) {
    /// A wheel notch in `Line` mode carries a small number of lines, not pixels.
    const LINE_HEIGHT: f32 = 22.0;

    let delta: f32 = wheel
        .read()
        .map(|event| match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => event.y * LINE_HEIGHT,
            bevy::input::mouse::MouseScrollUnit::Pixel => event.y,
        })
        .sum();
    if delta == 0.0 {
        return;
    }

    let (position, node) = &mut *scroll;
    let overflow = (node.content_size().y - node.size().y).max(0.0);
    let next = (position.y - delta).clamp(0.0, overflow);
    if position.y != next {
        position.y = next;
    }
}

pub fn sync_menu_visibility(menu: Res<MenuState>, mut views: Query<(&MenuView, &mut Node)>) {
    for (view, mut node) in &mut views {
        let display = match view {
            MenuView::Scrim if *menu != MenuState::Closed => Display::Flex,
            MenuView::Main if *menu == MenuState::Open => Display::Flex,
            MenuView::Restart if *menu == MenuState::ConfirmRestart => Display::Flex,
            _ => Display::None,
        };
        // Guarded like every other UI write. `DerefMut` marks `Node` changed
        // whatever the value, and the scrim is a full-viewport root: writing it
        // unconditionally re-solved that whole subtree every frame, menu open
        // or not.
        set_if_changed(&mut node.display, display);
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
    mut panel: Single<&mut Node, (With<RatePanel>, Without<RateLineRow>)>,
    mut rows: Query<(&RateLineRow, &mut Node), Without<RatePanel>>,
    mut lines: Query<(&RateLine, &mut Text, &mut TextFont), Without<CounterText>>,
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
    // lines would be noise on an otherwise clean opening screen. Staff count as
    // a hire: a player who hand-harvests their way to a Chef before a Worker
    // has a wage bill and nothing else on screen that would say so.
    let show = workforce.count() > 0 || snapshot.staff > 0;
    set_if_changed(
        &mut panel.display,
        if show { Display::Flex } else { Display::None },
    );
    if !show {
        return;
    }

    for (row, mut node) in &mut rows {
        // Every line is permanent except HUNGRY, which appears only when it has
        // something to say. "HUNGRY 0/5" on a healthy run would be noise.
        let shown = !matches!(row.0, RateLine::Hungry) || snapshot.hungry > 0;
        set_if_changed(
            &mut node.display,
            if shown { Display::Flex } else { Display::None },
        );
    }

    // Three steps, not two. The rates were sized when a lone worker read
    // "+6.0/min"; a staffed economy reads "+85.7/min", which is wide enough to
    // wrap inside a 390 px banner and break as "+85.7/" over "min".
    let rate_font = if layout.viewport.x < TINY_WIDTH {
        11.0
    } else if layout.viewport.x < NARROW_WIDTH {
        12.0
    } else {
        14.0
    };
    for (line, mut text, mut font) in &mut lines {
        set_if_changed(&mut font.font_size, FontSize::Px(rate_font));
        if let RateLine::Hungry = line {
            // A count of *support* monkeys, not of harvesters. A harvester's
            // meal is reserved out of the delivery that funds it, so nothing
            // else can spend it and it cannot go hungry; the monkeys who live
            // on somebody else's surplus are the ones who can. Without this
            // line an idle chef would read as an unexplained slowdown, because
            // its whole output is a number inside somebody else's cycle time.
            set_if_changed(
                &mut text.0,
                if snapshot.hungry > 0 {
                    format!("{}/{}", snapshot.hungry, snapshot.staff)
                } else {
                    String::new()
                },
            );
            continue;
        }
        let per_second = match line {
            RateLine::Farming => snapshot.gross_per_sec,
            RateLine::Feeding => -snapshot.wages_per_sec,
            RateLine::Net => snapshot.net_per_sec,
            RateLine::Hungry => unreachable!("handled above"),
        };
        // Per minute. One worker is +0.10, -0.03 and +0.07 per second, which
        // are three numbers a player has to squint at and multiply to make any
        // use of; per minute they are +6.0, -1.8 and +4.2, and the arithmetic
        // reads off the panel.
        set_if_changed(&mut text.0, format!("{:+.1}/min", per_second * 60.0));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync_shop(
    treasury: Res<Treasury>,
    workforce: Res<Workforce>,
    staff: Res<Staff>,
    fed: Res<FedStaff>,
    committed: Res<Committed>,
    multipliers: Res<Multipliers>,
    mut fields: Query<(&UnitField, &mut Text, &mut TextColor)>,
    mut buttons: Query<(
        &HireButton,
        &Interaction,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
) {
    let plan_for = |unit: Unit| {
        plan_hire(
            unit.kind(),
            *workforce,
            *staff,
            *fed,
            *treasury,
            committed.0,
            *multipliers,
        )
    };

    for (field, mut text, mut colour) in &mut fields {
        let plan = plan_for(field.unit);
        // The price greys out with its button, rather than staying gold on a
        // dimmed fill and reading as mud.
        set_if_changed(
            &mut colour.0,
            match field.stat {
                UnitStat::Price if plan.affordable => GOLD,
                UnitStat::Price => STORE_LABEL,
                _ => CREAM,
            },
        );
        let owned = match field.unit {
            Unit::Worker => workforce.count(),
            Unit::Support(role) => staff.count(role),
        };
        let value = match field.stat {
            UnitStat::Price => format!("{:.1}", plan.cost),
            // Per minute, like the readout. Per second a worker reads 0.10 and
            // a meal reads 0.03, and three numbers that all round to nothing
            // are worse than no numbers at all.
            UnitStat::Gain => format!("{:+.1}/min", plan.gain_per_min),
            // Per trip for a harvester, per shift for support. The two eat on
            // entirely different clocks and a shared "/min" would hide that a
            // worker's meal is a lump out of a delivery while a chef's is a
            // standing charge.
            UnitStat::Feeding => {
                if plan.meal_period == SUPPORT_MEAL_PERIOD {
                    format!("{:.1}/{:.0}s", plan.meal, SUPPORT_MEAL_PERIOD)
                } else {
                    format!("{:.1}/trip", plan.meal)
                }
            }
            UnitStat::Owned => owned.to_string(),
        };
        set_if_changed(&mut text.0, value);
    }

    for (button, interaction, mut background, mut border) in &mut buttons {
        let affordable = plan_for(button.0).affordable;
        // Affordability outranks hover: a button the player cannot press must
        // not light up under the cursor.
        //
        // These are the store's own tones, not the bar's. Inheriting the
        // cream-card palette inverted the signal underground - `MUTED` on
        // `STORE_SOIL` made the *disabled* state the brightest object in the
        // panel, while the enabled `BROWN` was the same colour as the dirt
        // sprite behind it. So the opening screen shouted the one thing the
        // player could not do. Disabled now recedes into the soil and enabled
        // is the lit thing in the dark.
        let (fill, edge) = if !affordable {
            (STORE_SOIL, STORE_LABEL)
        } else {
            match interaction {
                Interaction::Pressed => (GOLD, CREAM),
                Interaction::Hovered => (GOLD, CREAM),
                Interaction::None => (BROWN_LIGHT, GOLD),
            }
        };
        set_if_changed(&mut *background, BackgroundColor(fill));
        set_if_changed(&mut *border, BorderColor::all(edge));
    }
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
        (
            With<ButtonAction>,
            Without<HireButton>,
            Changed<Interaction>,
        ),
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

    /// Height the store needs, from the constants that decide it.
    ///
    /// A model, not a measurement - Bevy's text layout is not available here.
    /// It is deliberately pessimistic (1.4x line height) so it fails before the
    /// real thing overflows, and it is the only guard that couples the store's
    /// content to the band it has to live in.
    fn modelled_store_height(dirt: f32, units: u32) -> f32 {
        let compact = dirt < 120.0;
        let line = |font: f32| font * 1.4;

        let pad = if compact { 4.0 } else { 10.0 };
        let gap = if compact { 4.0 } else { 6.0 };
        // One line per unit, its height set by the hire button.
        let row: f32 = 34.0_f32.max(line(14.0));
        let header = if compact { 0.0 } else { line(10.0) + gap };

        3.0 + pad * 2.0 + header + units as f32 * row + (units.saturating_sub(1)) as f32 * gap
    }

    /// How many units the store can show before it runs out of dirt.
    fn store_capacity(dirt: f32) -> u32 {
        (1..=8)
            .take_while(|units| modelled_store_height(dirt, *units) <= dirt)
            .last()
            .unwrap_or(0)
    }

    #[test]
    fn the_banner_stays_centred_and_fits_at_every_viewport() {
        // Both flanking cells get one width - the left one holds nothing and
        // exists only to be as wide as the right - so the banner's centre must
        // land on the viewport's centre. An earlier layout centred it inside a
        // box carrying 110 px of right padding, which put it visibly
        // off-centre against a symmetric scene.
        // A rate line is a label and a signed per-minute value side by side,
        // and below this the value wraps mid-number - "+6.0/" over "min".
        const BANNER_MIN_WIDTH: f32 = 140.0;

        for width in [320.0, 390.0, 599.0, 600.0, 700.0, 844.0, 1280.0, 1920.0] {
            let pad = bar_padding(width);
            let gap = if width < NARROW_WIDTH {
                BAR_GAP_NARROW
            } else {
                BAR_GAP_DESKTOP
            };
            let cell = menu_button_width(width);
            let left_edge = pad + cell + gap;
            let right_edge = width - pad - cell - gap;
            let centre = (left_edge + right_edge) * 0.5;

            assert!(
                (centre - width * 0.5).abs() < 0.001,
                "width={width} centre={centre}"
            );
            assert!(
                right_edge - left_edge >= BANNER_MIN_WIDTH,
                "width={width} leaves only {} for the banner",
                right_edge - left_edge
            );
        }
    }

    #[test]
    fn the_store_stays_underground_at_every_viewport() {
        // The store is dug into the dirt. If it were ever taller than the dirt
        // band it would cover the ground line and the monkeys walking along it,
        // which reads as a rendering fault rather than as a design.
        for (width, height) in [
            (320.0, 640.0),
            (390.0, 844.0),
            (844.0, 390.0),
            (1280.0, 720.0),
            (1920.0, 1080.0),
        ] {
            let layout = SceneLayout::for_viewport(Vec2::new(width, height));
            let dirt_top_from_bottom = layout.viewport.y * 0.5 + layout.ground_top();

            assert!(
                (dirt_top_from_bottom - layout.viewport.y * DIRT_FRACTION).abs() < 0.5,
                "{width}x{height}: dirt is {dirt_top_from_bottom}, not \
                 {DIRT_FRACTION} of the viewport"
            );
            // ...and, more to the point, that what the store puts in the band
            // fits inside it. The geometry assertion above passes happily while
            // the content overflows, which is exactly what it did: `max_height`
            // bounds the node's own box and nothing else, so rows painted up
            // over the grass and the monkeys. `Overflow::clip_y` is the
            // backstop; this is the check that stops it ever being needed.
            let dirt = layout.viewport.y * DIRT_FRACTION;
            assert!(
                modelled_store_height(dirt, 1) <= dirt,
                "{width}x{height}: store models {} px into {dirt} px of dirt",
                modelled_store_height(dirt, 1)
            );
            // The shape that preceded this one used 114 of 158 px for a single
            // unit and overflowed at the second. The table has to leave room
            // for the Chef, which is the next unit due.
            assert!(
                store_capacity(dirt) >= 2,
                "{width}x{height}: only {} unit rows fit",
                store_capacity(dirt)
            );
        }
    }
}
