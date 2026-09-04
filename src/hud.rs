//! The heads-up display and the pause menu.
//!
//! Portrait screens use three stacked bands: economy summary, square village,
//! and tabbed store. Short landscape screens keep the summary at the top and
//! place the square village beside the store so both remain usable.
//!
//! The bar keeps three cells with equal outer widths, because that is what
//! centres the readout optically. An earlier layout centred it inside a box
//! carrying 110 px of right padding, which put it visibly off-centre against a
//! symmetric scene.

use bevy::prelude::*;

use crate::{
    domain::{
        CART_TECH_REQUIREMENT, Carts, Committed, CycleSpec, EconomySnapshot, EconomyState,
        FedStaff, Multipliers, RESEARCH_PER_TECHNOLOGIST, Research, SUPPORT_MEAL_PERIOD, Segment,
        Staff, SupportRole, Treasury, UnitKind, Workforce, cycle_time, plan_hire,
    },
    game::{
        BROWN, BROWN_LIGHT, ButtonAction, CREAM, Feedback, GOLD, INK, MenuState, SceneLayout,
        UiTouchGesture,
    },
};

/// Below this width the banner and the store both go compact.
const NARROW_WIDTH: f32 = 600.0;
/// The MENU button fills the side cell exactly, so the cell width is the button
/// width and nothing is wasted beside it. Both outer cells carry it - the left
/// one is an empty spacer whose only job is to keep the banner centred.
const MENU_BUTTON_WIDTH: f32 = 88.0;
/// Mobile gives the summary card the room saved by a shorter MENU target.
/// The button remains wider than the 44 px minimum touch target.
const MENU_BUTTON_WIDTH_NARROW: f32 = 64.0;
/// ...except on the very smallest screens, where two 88 px cells and their gaps
/// leave the banner 112 px and its rate lines wrap mid-number. The button is
/// still a 56x52 touch target, comfortably above the 44 px floor.
const MENU_BUTTON_WIDTH_TINY: f32 = 56.0;
/// Below this the banner cannot afford a full-width menu button beside it.
const TINY_WIDTH: f32 = 360.0;

fn menu_button_width(viewport_width: f32) -> f32 {
    if viewport_width < TINY_WIDTH {
        MENU_BUTTON_WIDTH_TINY
    } else if viewport_width < NARROW_WIDTH {
        MENU_BUTTON_WIDTH_NARROW
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

/// Warm paper tones shared by the mobile store and its cards.
const STORE_SOIL: Color = Color::srgb(0.97, 0.93, 0.88);
const STORE_LABEL: Color = Color::srgb(0.50, 0.39, 0.34);
const STORE_CARD: Color = Color::srgb(0.91, 0.84, 0.80);

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

#[derive(Component)]
pub(crate) struct StoreScrollCue;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShopTab {
    #[default]
    Monkeys,
    Buildings,
    Research,
}

impl ShopTab {
    const ALL: [Self; 3] = [Self::Monkeys, Self::Buildings, Self::Research];

    pub(crate) fn previous(self) -> Self {
        match self {
            Self::Monkeys => Self::Research,
            Self::Buildings => Self::Monkeys,
            Self::Research => Self::Buildings,
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Monkeys => Self::Buildings,
            Self::Buildings => Self::Research,
            Self::Research => Self::Monkeys,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Monkeys => "MONKEYS",
            Self::Buildings => "BUILDINGS",
            Self::Research => "RESEARCH",
        }
    }
}

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActiveShopTab(pub(crate) ShopTab);

#[derive(Component)]
pub(crate) struct TabStrip;

#[derive(Component, Clone, Copy)]
pub(crate) struct TabLabel(ShopTab);

#[derive(Component, Clone, Copy)]
pub(crate) struct TabContent(ShopTab);

#[derive(Component)]
pub(crate) struct StoreBody;

/// Whether the drawer is pulled up. Persisted with the run, so a player who
/// opened it stays opened.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StoreExpanded(pub bool);

/// The unit whose live breakdown is open in the information panel.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InfoOpen(pub Option<Unit>);

#[derive(Component)]
pub(crate) struct InfoPanel;

#[derive(Component, Clone, Copy)]
pub(crate) enum InfoText {
    Title,
    Body,
}

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
/// arm in [`sync_shop_new`] - not four new components and four new queries.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unit {
    Worker,
    Support(SupportRole),
    /// Present from the first frame, and locked until research says otherwise.
    ///
    /// §10 records the gap this closes: a player following payback order never
    /// buys a Technologist, so they never unlock the Cart and never see the
    /// second half of the game. A row that simply is not there cannot teach
    /// that; a locked row that names what would unlock it can.
    Cart,
}

impl Unit {
    /// Fixed, and deliberately not sorted by price. Ascending cost is not a
    /// stable order - a worker passes the Chef's 25 base after thirteen hires -
    /// so a price-sorted table would re-sort itself under the player's finger
    /// in a panel that gets tapped repeatedly. Declaration order it is.
    const ROWS: [Unit; 5] = [
        Unit::Worker,
        Unit::Support(SupportRole::Chef),
        Unit::Support(SupportRole::Unpacker),
        Unit::Support(SupportRole::Technologist),
        Unit::Cart,
    ];

    /// Singular, because the row reads as a spec sheet for one of them: the
    /// count is a column in the row, not part of the heading.
    fn name(self) -> &'static str {
        match self {
            // "WORKER", not "MONKEY": chefs, unpackers and technologists are
            // all monkeys too.
            Unit::Worker => "WORKER",
            Unit::Support(role) => role.name(),
            Unit::Cart => "CART",
        }
    }

    fn kind(self) -> UnitKind {
        match self {
            Unit::Worker => UnitKind::Worker,
            Unit::Support(role) => UnitKind::Support(role),
            Unit::Cart => UnitKind::Cart,
        }
    }
}

/// Which number in a unit's row a text node holds. One component and one query
/// beats four of each, and it keeps the columns in a fixed, declared order.
#[allow(dead_code)]
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
    /// The unit's own name, which the Cart greys out while it is locked.
    Name,
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

/// Stands in for a hire button while its unit is locked.
#[derive(Component, Clone, Copy)]
pub(crate) struct LockedPlaque(pub(crate) Unit);

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
            BackgroundColor(Color::srgb(0.94, 0.91, 0.83)),
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
                            Text::new("TOTAL BANANAS"),
                            TextFont::from_font_size(11.0),
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
                                    flex_direction: FlexDirection::Row,
                                    column_gap: px(5),
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
                                spawn_rate_line(panel, RateLine::Farming, "FARMING", INK);
                                spawn_rate_line(panel, RateLine::Feeding, "FEEDING", BROWN_LIGHT);
                                spawn_rate_line(panel, RateLine::Net, "GROW AVG", INK);
                                spawn_rate_line(panel, RateLine::Hungry, "HUNGRY", GOLD);
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

/// The persistent tabbed store. Only the offer list scrolls, leaving the tab
/// strip and its 48 px navigation targets pinned in place.
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
            BorderColor::all(BROWN),
            // Explicitly below the pause scrim (`GlobalZIndex(100)`). The store
            // is its own UI root rather than a child of the bar, so its
            // stacking would otherwise be decided by spawn order in another
            // function - and a shop that stayed lit while the rest of the scene
            // dimmed would be a lie about what is interactive.
            GlobalZIndex(1),
            Interaction::default(),
            StoreRoot,
        ))
        .with_children(|store| {
            spawn_store_grip(store);
            store
                .spawn((
                    TabStrip,
                    Node {
                        width: percent(100),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        min_height: px(48),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .with_children(|tabs| {
                    spawn_tab_arrow(tabs, ButtonAction::PreviousShopTab, "<");
                    tabs.spawn((
                        Node {
                            flex_grow: 1.0,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            column_gap: px(10),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ))
                    .with_children(|labels| {
                        for tab in ShopTab::ALL {
                            labels.spawn((
                                Text::new(tab.label()),
                                TextFont::from_font_size(14.0),
                                TextColor(STORE_LABEL),
                                TabLabel(tab),
                            ));
                        }
                    });
                    spawn_tab_arrow(tabs, ButtonAction::NextShopTab, ">");
                });
            store
                .spawn((
                    StoreBody,
                    Node {
                        width: percent(100),
                        flex_grow: 1.0,
                        min_height: px(0),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|body| {
                    body.spawn((
                        StoreScroll,
                        TabContent(ShopTab::Monkeys),
                        ScrollPosition::default(),
                        Node {
                            width: percent(100),
                            height: percent(100),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: px(8),
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
                    for tab in [ShopTab::Buildings, ShopTab::Research] {
                        body.spawn((
                            TabContent(tab),
                            Node {
                                position_type: PositionType::Absolute,
                                width: percent(100),
                                height: percent(100),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                display: Display::None,
                                ..default()
                            },
                            Pickable::IGNORE,
                            children![(
                                Text::new(format!("{}\nCOMING SOON", tab.label())),
                                TextFont::from_font_size(19.0),
                                TextColor(BROWN),
                                TextLayout::justify(Justify::Center),
                            )],
                        ));
                    }
                });
            store.spawn((
                StoreScrollCue,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(18),
                    bottom: px(4),
                    padding: UiRect::axes(px(8), px(3)),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(CREAM),
                Visibility::Hidden,
                Pickable::IGNORE,
                children![(
                    Text::new("SCROLL v"),
                    TextFont::from_font_size(10.0),
                    TextColor(BROWN),
                )],
            ));
        });
    commands
        .spawn((
            InfoPanel,
            Node {
                position_type: PositionType::Absolute,
                top: percent(18.0),
                left: percent(5.0),
                width: percent(90.0),
                max_width: px(560.0),
                align_self: AlignSelf::Center,
                padding: UiRect::all(px(16)),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                display: Display::None,
                border: UiRect::all(px(3)),
                ..default()
            },
            BackgroundColor(BROWN),
            BorderColor::all(GOLD),
            GlobalZIndex(20),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("UNIT INFO"),
                TextFont::from_font_size(18.0),
                TextColor(GOLD),
                InfoText::Title,
            ));
            panel.spawn((
                Text::new(""),
                TextFont::from_font_size(14.0),
                TextColor(CREAM),
                InfoText::Body,
            ));
            panel.spawn((
                Text::new("Tap the i button again to close"),
                TextFont::from_font_size(11.0),
                TextColor(STORE_LABEL),
            ));
        });
}

fn spawn_tab_arrow(tabs: &mut ChildSpawnerCommands, action: ButtonAction, label: &'static str) {
    tabs.spawn((
        Button,
        action,
        Node {
            width: px(48),
            min_height: px(48),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BackgroundColor(BROWN),
        BorderColor::all(BROWN_LIGHT),
        children![(
            Text::new(label),
            TextFont::from_font_size(30.0),
            TextColor(CREAM),
        )],
    ));
}

/// The drawer handle. The store stays present at rest and expands over part of
/// the portrait board when the player wants more room for the list.
fn spawn_store_grip(store: &mut ChildSpawnerCommands) {
    store
        .spawn((
            Button,
            ButtonAction::ToggleStore,
            StoreGrip,
            Node {
                width: px(74),
                height: px(44),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_child((
            Node {
                width: px(74),
                height: px(12),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(7)),
                ..default()
            },
            BackgroundColor(BROWN_LIGHT),
            BorderColor::all(GOLD),
            Pickable::IGNORE,
        ));
}

/// Column widths, shared by the header and every unit line. Fixed rather than
/// content-sized: content-sized columns re-align themselves per row, which is
/// the one thing a comparison table must not do.
const COL_NAME: f32 = 104.0;
const COL_COST: f32 = 84.0;
/// D21: the live marginal rate, recomputed every tick. Narrow and numeric on
/// purpose, sitting next to COST rather than under the prose DESCRIPTION
/// cell, so the figure lines up down one column and stays comparable row to
/// row - a value anchored to a variable-length description would not.
const COL_RATE: f32 = 104.0;
/// The RATE cell's unscaled font size. `apply_store_layout` scales this down
/// by the same factor it scales `COL_RATE`'s width by, so the cell's longest
/// word ("TECHNOLOGIST") keeps the fit it has at full size instead of running
/// past a box that shrank under it.
const RATE_FONT_SIZE: f32 = 12.0;
/// Cut by exactly `COL_RATE`'s own width plus the gap it added, so the
/// table's total budget - and therefore `column_scale`'s factor at every
/// viewport - is unchanged from before D21's column existed. Verified against
/// a real 390 px render: growing the table instead of trading width for the
/// new column let RATE's `NoWrap` text run straight through this cell at the
/// narrowest supported width, on every row, not only the long locked-Cart
/// message.
const COL_DESCRIPTION: f32 = 280.0 - COL_RATE - COL_GAP;
const COL_INFO: f32 = 48.0;
const COL_GAP: f32 = 10.0;

/// Column width the table needs, with and without the column a narrow screen
/// drops. Gaps are counted separately because they do not scale with the
/// columns - see `column_scale`.
fn table_columns() -> f32 {
    COL_NAME + COL_COST + COL_RATE + COL_DESCRIPTION + COL_INFO
}

fn table_gaps() -> f32 {
    4.0 * COL_GAP
}

#[allow(dead_code)]
fn table_width() -> f32 {
    table_columns() + table_gaps()
}

/// How far the columns have to shrink to fit `room`, never above 1.
///
/// A 320 px phone gives the store 304 px and the four essential columns want
/// 334, so without this the table is simply clipped: "UNIT" renders as "NIT"
/// and the OWNED value falls off the right edge. Scaling every column by one
/// factor keeps the header lined up with the rows, which dropping or wrapping
/// individual columns would not.
fn column_scale(room: f32) -> f32 {
    let columns = table_columns();
    if columns <= 0.0 {
        return 1.0;
    }
    ((room - table_gaps()) / columns).clamp(0.4, 1.0)
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
            for (label, width) in [
                ("UNIT", COL_NAME),
                ("COST", COL_COST),
                ("RATE", COL_RATE),
                ("DESCRIPTION", COL_DESCRIPTION),
                ("INFO", COL_INFO),
            ] {
                header.spawn((
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
            }
        });
}

/// One unit: one line, columns aligned with the header above it.
fn spawn_unit_row(store: &mut ChildSpawnerCommands, unit: Unit) {
    store
        .spawn((
            Node {
                width: percent(100),
                max_width: px(620),
                align_items: AlignItems::Center,
                column_gap: px(COL_GAP),
                padding: UiRect::axes(px(10), px(8)),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(STORE_CARD),
            BorderColor::all(Color::srgb(0.82, 0.72, 0.67)),
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
                    TextColor(INK),
                    UnitField {
                        unit,
                        stat: UnitStat::Name,
                    },
                )],
            ));

            spawn_hire_button(row, unit, unit.kind());
            if matches!(unit, Unit::Cart) {
                // Shown in the button's place while the Cart is locked. A
                // dimmed button and a unit that does not exist yet are
                // different states, and a dimmed button says the first when it
                // means the second - it also *takes taps*, queues a hire that
                // is silently dropped, and burns the debounce doing it.
                spawn_locked_plaque(row, unit);
            }
            spawn_rate_cell(row, unit);
            row.spawn((
                TableCell(COL_DESCRIPTION),
                Node {
                    width: px(COL_DESCRIPTION),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                },
            ))
            .with_children(|details| {
                details.spawn((
                    Text::new(unit.description()),
                    TextFont::from_font_size(13.0),
                    TextColor(INK),
                    TextLayout::linebreak(LineBreak::WordBoundary),
                ));
                details.spawn((
                    Text::new("OWNED 0"),
                    TextFont::from_font_size(11.0),
                    TextColor(STORE_LABEL),
                    UnitField {
                        unit,
                        stat: UnitStat::Owned,
                    },
                ));
            });
            row.spawn((
                Button,
                ButtonAction::Info(unit),
                TableCell(COL_INFO),
                Node {
                    width: px(COL_INFO),
                    min_height: px(44),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(7)),
                    ..default()
                },
                BackgroundColor(BROWN),
                BorderColor::all(BROWN_LIGHT),
                children![(
                    Text::new("i"),
                    TextFont::from_font_size(18.0),
                    TextColor(GOLD),
                )],
            ));
        });
}

/// D21's live marginal rate, recomputed every tick by [`sync_shop_new`].
///
/// `NoWrap`, verified against the actual failure it exists to avoid: at
/// 390 px, word-boundary wrap breaks "+4.2/min" after the slash and leaves an
/// orphaned "min" on its own line - the exact wrap the old, pre-redesign rate
/// column's own comment warned about, and confirmed live in this column
/// before this fix landed. `NoWrap` still honours an explicit `\n`
/// (`bevy_text::LineBreak::NoWrap` docs), which is what the locked Cart row's
/// longer messages use instead of relying on word-wrap to break them well.
fn spawn_rate_cell(row: &mut ChildSpawnerCommands, unit: Unit) {
    row.spawn((
        TableCell(COL_RATE),
        Node {
            width: px(COL_RATE),
            flex_shrink: 0.0,
            ..default()
        },
        children![(
            Text::new(""),
            TextFont::from_font_size(RATE_FONT_SIZE),
            TextColor(CREAM),
            TextLayout::linebreak(LineBreak::NoWrap),
            UnitField {
                unit,
                stat: UnitStat::Gain,
            },
        )],
    ));
}

impl Unit {
    fn description(self) -> &'static str {
        match self {
            Unit::Worker => "Harvests 6 bananas from the jungle per minute.",
            Unit::Support(SupportRole::Chef) => "Makes harvesting monkeys travel 15% faster.",
            Unit::Support(SupportRole::Unpacker) => "Unloads bananas 20% faster.",
            Unit::Support(SupportRole::Technologist) => "Produces 1.0 research per second.",
            Unit::Cart => "Carries 100 bananas per trip with a 3-monkey crew.",
        }
    }
}

fn spawn_hire_button(row: &mut ChildSpawnerCommands, unit: Unit, kind: UnitKind) {
    row.spawn((
        Button,
        ButtonAction::Hire(kind),
        HireButton(unit),
        TableCell(COL_COST),
        Node {
            width: px(COL_COST),
            min_height: px(44),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(7)),
            ..default()
        },
        BackgroundColor(BROWN_LIGHT),
        BorderColor::all(GOLD),
        // Just the price. The column header says HIRE, so repeating the word
        // inside every button costs a word per unit for nothing.
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
}

fn spawn_locked_plaque(row: &mut ChildSpawnerCommands, unit: Unit) {
    row.spawn((
        LockedPlaque(unit),
        TableCell(COL_COST),
        Node {
            display: Display::None,
            width: px(COL_COST),
            min_height: px(44),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(7)),
            ..default()
        },
        BackgroundColor(STORE_SOIL),
        BorderColor::all(STORE_LABEL),
        children![(
            Text::new("LOCKED"),
            TextFont::from_font_size(12.0),
            TextColor(STORE_LABEL),
        )],
    ));
}

#[allow(dead_code)]
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

fn spawn_rate_line(panel: &mut ChildSpawnerCommands, line: RateLine, label: &str, colour: Color) {
    panel.spawn((
        RateLineRow(line),
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(1),
            ..default()
        },
        children![
            (
                Text::new(label),
                TextFont::from_font_size(9.0),
                TextColor(colour),
            ),
            (
                Text::new("+0.0/min"),
                TextFont::from_font_size(14.0),
                TextColor(colour),
                // A rate is one token. Allowed to wrap it breaks after the
                // slash, which reads as two numbers.
                TextLayout::linebreak(LineBreak::NoWrap),
                line,
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
    let vertical_pad = if layout.short_landscape() { 4.0 } else { pad };

    set_if_changed(&mut root.height, px(layout.header_height()));
    set_if_changed(&mut root.padding, UiRect::axes(px(pad), px(vertical_pad)));
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
            px(if layout.short_landscape() { 4.0 } else { 10.0 }),
        ),
    );
    set_if_changed(
        &mut banner.row_gap,
        px(if layout.short_landscape() { 2.0 } else { 6.0 }),
    );
}

/// Partition the viewport with the same measurements used by the world scene.
/// Portrait stacks the store below the square. Short landscape puts the store
/// to its right. Expanding the portrait drawer is the only state allowed to
/// cover part of the board.
#[allow(clippy::type_complexity)]
pub fn apply_store_layout(
    layout: Res<SceneLayout>,
    expanded: Res<StoreExpanded>,
    mut store: Single<
        &mut Node,
        (
            With<StoreRoot>,
            Without<StoreHeading>,
            Without<TableCell>,
            Without<StoreScroll>,
        ),
    >,
    mut heading: Single<
        &mut Node,
        (
            With<StoreHeading>,
            Without<StoreRoot>,
            Without<TableCell>,
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
    mut scroll: Single<
        (&mut ScrollPosition, &ComputedNode),
        (
            With<StoreScroll>,
            Without<StoreRoot>,
            Without<StoreHeading>,
            Without<TableCell>,
        ),
    >,
    mut grip: Single<
        &mut Node,
        (
            With<StoreGrip>,
            Without<StoreRoot>,
            Without<StoreHeading>,
            Without<TableCell>,
            Without<StoreScroll>,
        ),
    >,
    mut scroll_cue: Single<&mut Visibility, With<StoreScrollCue>>,
    mut rate_fonts: Query<(&UnitField, &mut TextFont)>,
) {
    let base_height = layout.store_height();
    let target_height = if expanded.0 && !layout.short_landscape() {
        base_height.max(layout.viewport.y * 0.68)
    } else {
        base_height
    };
    let pad = if layout.store_width() < NARROW_WIDTH {
        8.0
    } else {
        14.0
    };

    set_if_changed(&mut store.width, px(layout.store_width()));
    set_if_changed(&mut store.height, px(target_height));
    set_if_changed(&mut store.min_height, px(0));
    set_if_changed(&mut store.max_height, px(target_height));
    set_if_changed(
        &mut store.left,
        px(if layout.short_landscape() {
            layout.scene_side()
        } else {
            0.0
        }),
    );
    set_if_changed(&mut store.padding, UiRect::axes(px(pad), px(6)));
    set_if_changed(&mut store.row_gap, px(5));
    set_if_changed(
        &mut grip.display,
        if layout.short_landscape() {
            Display::None
        } else {
            Display::Flex
        },
    );

    let compact = target_height < 220.0;
    set_if_changed(
        &mut heading.display,
        if compact {
            Display::None
        } else {
            Display::Flex
        },
    );

    let room = layout.store_width() - 2.0 * pad - 20.0;
    let scale = column_scale(room);
    for (cell, optional, mut node) in &mut cells {
        let shown = optional.is_none();
        set_if_changed(
            &mut node.display,
            if shown { Display::Flex } else { Display::None },
        );
        set_if_changed(&mut node.width, px((cell.0 * scale).floor()));
    }
    // The RATE cell's font has to shrink with its box, not just the box: cell
    // width is scaled above, but `NoWrap` text does not reflow to a narrower
    // box on its own, and "TECHNOLOGIST" - the longest word this cell ever
    // shows - visibly overlapped DESCRIPTION on a real 390 px render before
    // this landed. Scaling the font by the same factor as the box preserves
    // the fit ratio exactly: content that fits at `scale = 1.0` still fits at
    // any smaller scale, because both shrink together.
    for (field, mut font) in &mut rate_fonts {
        if matches!(field.stat, UnitStat::Gain) {
            set_if_changed(
                &mut font.font_size,
                FontSize::Px((RATE_FONT_SIZE * scale).max(10.0)),
            );
        }
    }

    let (position, node) = &mut *scroll;
    let overflow = (node.content_size().y - node.size().y).max(0.0);
    position.y = position.y.clamp(0.0, overflow);
    **scroll_cue = if layout.short_landscape() && overflow > 1.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
}

/// Wheel and drag scrolling for the drawer.
///
/// The store is soil-on-soil with no scrollbar, so without this the rows below
/// the fold are unreachable on desktop - and the resting height shows only two
/// of them in landscape.
pub fn scroll_store(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    store: Single<(&ComputedNode, &UiGlobalTransform), With<StoreRoot>>,
    active: Res<ActiveShopTab>,
    menu: Res<MenuState>,
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
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let center = store.1.translation * store.0.inverse_scale_factor;
    let size = store.0.size() * store.0.inverse_scale_factor;
    let hovered = crate::game::contains_inclusive(Rect::from_center_size(center, size), cursor);
    if delta == 0.0 || !hovered || active.0 != ShopTab::Monkeys || *menu != MenuState::Closed {
        return;
    }

    let (position, node) = &mut *scroll;
    let overflow = (node.content_size().y - node.size().y).max(0.0);
    let next = (position.y - delta).clamp(0.0, overflow);
    if position.y != next {
        position.y = next;
    }
}

pub fn handle_store_gesture(
    mut gesture: ResMut<UiTouchGesture>,
    menu: Res<MenuState>,
    mut active: ResMut<ActiveShopTab>,
    mut info: ResMut<InfoOpen>,
    tab_strip: Single<(&ComputedNode, &UiGlobalTransform), With<TabStrip>>,
    mut scroll: Single<(&mut ScrollPosition, &ComputedNode, &UiGlobalTransform), With<StoreScroll>>,
) {
    if *menu != MenuState::Closed || gesture.canceled {
        gesture.consumed |= gesture.canceled;
        return;
    }
    let bounds = |node: &ComputedNode, transform: &UiGlobalTransform| {
        Rect::from_center_size(
            transform.translation * node.inverse_scale_factor,
            node.size() * node.inverse_scale_factor,
        )
    };
    let distance = gesture.position - gesture.start;
    let moved = distance.length() > 10.0;
    let in_tabs = crate::game::contains_inclusive(bounds(tab_strip.0, tab_strip.1), gesture.start);
    let in_list = crate::game::contains_inclusive(bounds(scroll.1, scroll.2), gesture.start);

    if in_tabs && moved && distance.x.abs() > distance.y.abs() * 1.15 {
        gesture.consumed = true;
        if gesture.just_released && distance.x.abs() >= 44.0 {
            active.0 = if distance.x < 0.0 {
                active.0.next()
            } else {
                active.0.previous()
            };
            info.0 = None;
        }
    } else if in_list && moved {
        // A list drag owns the gesture even when it begins slightly diagonal.
        // That is what prevents touch-start over a row from becoming a hire.
        gesture.consumed = true;
        if active.0 == ShopTab::Monkeys && distance.y.abs() >= distance.x.abs() * 0.75 {
            let overflow = (scroll.1.content_size().y - scroll.1.size().y).max(0.0);
            let delta = gesture.position.y - gesture.previous.y;
            scroll.0.y = (scroll.0.y - delta).clamp(0.0, overflow);
        }
    }
}

pub fn sync_shop_tabs(
    active: Res<ActiveShopTab>,
    layout: Res<SceneLayout>,
    mut labels: Query<(&TabLabel, &mut TextColor, &mut TextFont)>,
    mut contents: Query<(&TabContent, &mut Node)>,
    mut scroll: Single<&mut ScrollPosition, With<StoreScroll>>,
) {
    for (label, mut colour, mut font) in &mut labels {
        let wanted = if label.0 == active.0 {
            BROWN
        } else {
            STORE_LABEL
        };
        set_if_changed(&mut colour.0, wanted);
        set_if_changed(
            &mut font.font_size,
            FontSize::Px(if layout.store_width() < 400.0 {
                10.0
            } else {
                14.0
            }),
        );
    }
    for (content, mut node) in &mut contents {
        set_if_changed(
            &mut node.display,
            if content.0 == active.0 {
                Display::Flex
            } else {
                Display::None
            },
        );
    }
    if active.is_changed() {
        scroll.y = 0.0;
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

fn signed_per_minute(rate_per_second: f64) -> String {
    let rate_per_minute = rate_per_second * 60.0;
    let displayed = if rate_per_minute.abs() < 0.05 {
        0.0
    } else {
        rate_per_minute
    };
    format!("{displayed:+.1}/min")
}

fn technologist_research_per_sec(multipliers: Multipliers) -> f64 {
    RESEARCH_PER_TECHNOLOGIST * multipliers.speed
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn sync_readout(
    layout: Res<SceneLayout>,
    treasury: Res<Treasury>,
    _workforce: Res<Workforce>,
    snapshot: Res<EconomySnapshot>,
    feedback: Res<Feedback>,
    mut counter: Single<(&mut Text, &mut TextFont), With<CounterText>>,
    mut panel: Single<&mut Node, (With<RatePanel>, Without<RateLineRow>)>,
    mut rows: Query<(&RateLineRow, &mut Node), Without<RatePanel>>,
    mut lines: Query<(&RateLine, &mut Text, &mut TextFont), Without<CounterText>>,
) {
    let base = if layout.short_landscape() {
        22.0
    } else if layout.viewport.x < NARROW_WIDTH {
        COUNTER_FONT_NARROW
    } else {
        COUNTER_FONT_DESKTOP
    };
    set_if_changed(&mut counter.0.0, treasury.display_string());
    set_if_changed(
        &mut counter.1.font_size,
        FontSize::Px(base + feedback.pulse * base * 0.2),
    );

    set_if_changed(&mut panel.display, Display::Flex);
    set_if_changed(
        &mut panel.padding,
        UiRect::top(px(if layout.short_landscape() { 2.0 } else { 6.0 })),
    );

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
    let rate_font = if layout.short_landscape() {
        9.0
    } else if layout.viewport.x < TINY_WIDTH {
        9.0
    } else if layout.viewport.x < NARROW_WIDTH {
        10.0
    } else {
        11.0
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
        set_if_changed(&mut text.0, signed_per_minute(per_second));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync_shop_new(
    treasury: Res<Treasury>,
    workforce: Res<Workforce>,
    carts: Res<Carts>,
    staff: Res<Staff>,
    fed: Res<FedStaff>,
    research: Res<Research>,
    committed: Res<Committed>,
    multipliers: Res<Multipliers>,
    mut fields: Query<(&UnitField, &mut Text, &mut TextColor)>,
    mut buttons: Query<(
        &HireButton,
        &Interaction,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut Node,
    )>,
    mut plaques: Query<(&LockedPlaque, &mut Node), Without<HireButton>>,
) {
    let world = EconomyState {
        workforce: *workforce,
        carts: *carts,
        staff: *staff,
        fed: *fed,
        research: *research,
        treasury: *treasury,
        committed: committed.0,
        multipliers: *multipliers,
    };
    let cart_locked = research.level() < CART_TECH_REQUIREMENT;
    let (into_level, level_cost) = research.progress();
    let locked = |unit: Unit| matches!(unit, Unit::Cart) && cart_locked;
    let lit: Vec<Unit> = buttons
        .iter()
        .filter(|(_, i, ..)| matches!(i, Interaction::Hovered | Interaction::Pressed))
        .map(|(b, ..)| b.0)
        .collect();
    for (field, mut text, mut colour) in &mut fields {
        let plan = plan_hire(field.unit.kind(), world);
        let affordable = plan.affordable && !locked(field.unit);
        set_if_changed(
            &mut colour.0,
            if locked(field.unit) {
                STORE_LABEL
            } else if matches!(field.stat, UnitStat::Price)
                && affordable
                && lit.contains(&field.unit)
            {
                STORE_SOIL
            } else if matches!(field.stat, UnitStat::Price) && affordable {
                GOLD
            } else if matches!(field.stat, UnitStat::Gain)
                && matches!(field.unit, Unit::Support(SupportRole::Technologist))
            {
                // Checked before the generic negative-gain arm below: a
                // Technologist's own gain_per_min is exactly -wage at every
                // world state (D14) - always negative - even though the row
                // is a perfectly good buy. Gold marks "correctly unranked,"
                // not "underwater."
                GOLD
            } else if matches!(field.stat, UnitStat::Gain) && plan.gain_per_min < 0.0 {
                BROWN_LIGHT
            } else if matches!(field.stat, UnitStat::Price) {
                STORE_LABEL
            } else {
                INK
            },
        );
        let owned = match field.unit {
            Unit::Worker => workforce.count(),
            Unit::Support(role) => staff.count(role),
            Unit::Cart => carts.owned(),
        };
        let value = match (field.stat, field.unit) {
            (UnitStat::Name, _) => field.unit.name().to_string(),
            (UnitStat::Price, Unit::Cart) if cart_locked => String::new(),
            (UnitStat::Price, _) => format!("{:.1}", plan.cost),
            (UnitStat::Owned, Unit::Cart) if cart_locked => String::new(),
            (UnitStat::Owned, _) => format!("OWNED {owned}"),
            (UnitStat::Gain, Unit::Cart) if cart_locked => {
                if staff.count(SupportRole::Technologist) == 0 {
                    "NEEDS A TECHNOLOGIST".to_string()
                } else {
                    format!("RESEARCH {into_level:.0}/{level_cost:.0}")
                }
            }
            (UnitStat::Gain, Unit::Support(SupportRole::Technologist)) => {
                format!("{:.1}\nRES/s", technologist_research_per_sec(*multipliers))
            }
            (UnitStat::Gain, _) => format!("{:+.1}/min", plan.gain_per_min),
            (UnitStat::Feeding, _) => String::new(),
        };
        set_if_changed(&mut text.0, value);
    }
    for (plaque, mut node) in &mut plaques {
        set_if_changed(
            &mut node.display,
            if locked(plaque.0) {
                Display::Flex
            } else {
                Display::None
            },
        );
    }
    for (button, interaction, mut background, mut border, mut node) in &mut buttons {
        let affordable = plan_hire(button.0.kind(), world).affordable && !locked(button.0);
        let (fill, edge) = if !affordable {
            (STORE_CARD, STORE_LABEL)
        } else {
            match interaction {
                Interaction::Pressed | Interaction::Hovered => (GOLD, CREAM),
                Interaction::None => (BROWN_LIGHT, GOLD),
            }
        };
        set_if_changed(&mut *background, BackgroundColor(fill));
        set_if_changed(&mut *border, BorderColor::all(edge));
        set_if_changed(
            &mut node.display,
            if locked(button.0) {
                Display::None
            } else {
                Display::Flex
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync_info(
    info: Res<InfoOpen>,
    workforce: Res<Workforce>,
    carts: Res<Carts>,
    staff: Res<Staff>,
    fed: Res<FedStaff>,
    research: Res<Research>,
    multipliers: Res<Multipliers>,
    mut panel: Single<(&mut Node, &Children), With<InfoPanel>>,
    mut texts: Query<(&InfoText, &mut Text)>,
) {
    let shown = info.0.is_some();
    set_if_changed(
        &mut panel.0.display,
        if shown { Display::Flex } else { Display::None },
    );
    let Some(unit) = info.0 else { return };
    let owned = match unit {
        Unit::Worker => workforce.count(),
        Unit::Support(role) => staff.count(role),
        Unit::Cart => carts.owned(),
    };
    let title = format!("{} INFO", unit.name());
    let body = match unit {
        Unit::Worker | Unit::Cart => {
            let spec = if matches!(unit, Unit::Worker) {
                CycleSpec::WORKER
            } else {
                CycleSpec::CART
            };
            let labels = [
                "TRAVEL TO JUNGLE",
                "HARVEST / LOAD",
                "TRAVEL TO DEPOT",
                "UNLOAD",
                "MEAL",
            ];
            let durations = Segment::ORDER.map(|segment| segment.duration(spec, *multipliers));
            let mut body = format!(
                "OWNED: {owned}\nSALARY: {:.2} bananas/s\nPAYLOAD: {:.0} bananas\n",
                spec.wage, spec.payload
            );
            if matches!(unit, Unit::Cart) {
                body.push_str("CREW: 3 monkeys\n");
            }
            for (label, duration) in labels.into_iter().zip(durations) {
                body.push_str(&format!("{label}: {duration:.1}s\n"));
            }
            body.push_str(&format!(
                "TOTAL CYCLE: {:.1}s",
                cycle_time(spec, *multipliers)
            ));
            body
        }
        Unit::Support(role) => format!(
            "OWNED: {owned}\nSALARY: {:.2} bananas/s\nSHIFT PAYMENT: {:.1} bananas every {:.0}s\nEFFECT: {}",
            role.wage(),
            role.meal(),
            SUPPORT_MEAL_PERIOD,
            unit.description()
        ),
    };
    for (field, mut text) in &mut texts {
        match field {
            InfoText::Title => set_if_changed(&mut text.0, title.clone()),
            InfoText::Body => set_if_changed(&mut text.0, body.clone()),
        }
    }
    let _ = (&carts, &fed, &research); // keep the panel system's live inputs explicit
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
            Without<StoreGrip>,
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
    fn board_and_store_partition_every_supported_viewport() {
        for (width, height) in [
            (320.0, 640.0),
            (390.0, 844.0),
            (844.0, 390.0),
            (1280.0, 720.0),
            (1920.0, 1080.0),
        ] {
            let layout = SceneLayout::for_viewport(Vec2::new(width, height));
            assert!(
                layout.scene_side() >= 260.0,
                "{width}x{height}: board too small"
            );
            assert!(
                layout.store_height() >= 150.0,
                "{width}x{height}: store too short"
            );
            if layout.short_landscape() {
                assert_eq!(layout.scene_side() + layout.store_width(), width);
            } else {
                assert!(
                    (layout.header_height() + layout.scene_side() + layout.store_height() - height)
                        .abs()
                        < 0.01
                );
            }
        }
    }

    #[test]
    fn tabs_cycle_in_both_directions() {
        assert_eq!(ShopTab::Monkeys.next(), ShopTab::Buildings);
        assert_eq!(ShopTab::Buildings.next(), ShopTab::Research);
        assert_eq!(ShopTab::Research.next(), ShopTab::Monkeys);
        assert_eq!(ShopTab::Monkeys.previous(), ShopTab::Research);
        assert_eq!(ShopTab::Research.previous(), ShopTab::Buildings);
        assert_eq!(ShopTab::Buildings.previous(), ShopTab::Monkeys);
    }

    #[test]
    fn displayed_rates_normalize_zero_and_follow_chef_speed() {
        assert_eq!(signed_per_minute(-0.0), "+0.0/min");
        let multipliers = crate::domain::multipliers_for(
            FedStaff {
                chefs: 2,
                ..default()
            },
            Research::default(),
        );
        assert!((technologist_research_per_sec(multipliers) - 1.3).abs() < 1e-9);
    }
}
