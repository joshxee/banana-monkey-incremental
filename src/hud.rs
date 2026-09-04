//! The heads-up display and the pause menu.
//!
//! Portrait screens use three stacked bands: economy summary, square village,
//! and a tabbed store. Short landscape screens keep the summary at the top and
//! place the square village beside the store so both remain usable.
//!
//! The bar keeps three cells with equal outer widths, because that is what
//! centres the readout optically. An earlier layout centred it inside a box
//! carrying 110 px of right padding, which put it visibly off-centre against a
//! symmetric scene.
//!
//! Colour, border, radius and interaction state (hover/press/affordability)
//! live in `assets/style/hud.css`, applied through `bevy_flair`'s `Styled`
//! component and toggled with `ClassList`. Viewport-responsive *numbers*
//! (widths, font-size tiers, padding tiers) stay here: they are continuous or
//! compound functions of live geometry (see `SceneLayout`), not the discrete
//! swaps a media query expresses cleanly.
//!
//! The store is a plain scrollable list, not a drawer: it never grows to
//! cover the board, and there is no handle to pull it open.

use bevy::prelude::*;
use bevy_flair::prelude::*;

use crate::{
    domain::{
        CART_TECH_REQUIREMENT, Carts, Committed, CycleSpec, EconomySnapshot, EconomyState,
        FedStaff, Multipliers, RESEARCH_PER_TECHNOLOGIST, Research, SUPPORT_MEAL_PERIOD, Segment,
        Staff, SupportRole, Treasury, UnitKind, Workforce, cycle_time, plan_hire,
    },
    game::{ButtonAction, Feedback, MenuState, SceneLayout, UiTouchGesture},
};

/// Every HUD root loads the same stylesheet. `AssetServer::load` caches by
/// path, so loading it once per root is cheap - each call just clones a
/// handle to the same asset.
const HUD_STYLE: &str = "style/hud.css";

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
pub(crate) struct Banner;

/// The word on the bar's menu button, which shrinks with its cell.
#[derive(Component)]
pub(crate) struct MenuLabel;

#[derive(Component)]
pub(crate) struct CounterText;

#[derive(Component)]
pub(crate) struct RatePanel;

/// The scrolling list of unit cards. The store is always this - a plain list,
/// never a drawer that grows to cover the board.
#[derive(Component)]
pub(crate) struct StoreScroll;

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

/// The second tier of a unit card - the description prose. The only thing a
/// short store panel is allowed to hide: price, rate and owned count stay on
/// the always-visible top tier, because those are what a player checks before
/// every purchase.
#[derive(Component)]
pub(crate) struct UnitDetail;

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

    /// The card's accent colour, one per unit, echoing the isometric scene's
    /// own colourful village blocks. In a text-only shop this is the only
    /// non-verbal cue distinguishing five rows at a glance, so each is a
    /// distinct hue from the natural palette and none of them are close to
    /// the brown/gold chrome the rest of the panel is built from.
    fn swatch_class(self) -> &'static str {
        match self {
            Unit::Worker => "worker",
            Unit::Support(SupportRole::Chef) => "chef",
            Unit::Support(SupportRole::Unpacker) => "unpacker",
            Unit::Support(SupportRole::Technologist) => "technologist",
            Unit::Cart => "cart",
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
    Owned,
    /// The unit's own name, which the Cart greys out while it is locked.
    Name,
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct UnitField {
    pub(crate) unit: Unit,
    pub(crate) stat: UnitStat,
}

/// Marks a unit's hire button so [`sync_shop_new`] can toggle its
/// affordability class without touching colour directly - `hud.css` owns the
/// colour, this only says which state applies.
#[derive(Component)]
pub(crate) struct HireButton(pub(crate) Unit);

/// Stands in for a hire button while its unit is locked.
#[derive(Component, Clone, Copy)]
pub(crate) struct LockedPlaque(pub(crate) Unit);

/// A card's colour swatch, the one non-verbal cue distinguishing rows in a
/// text-only shop - it needs its own `locked` toggle so it dims with the rest
/// of a locked row rather than staying the one saturated, "available"-looking
/// thing on it.
#[derive(Component, Clone, Copy)]
pub(crate) struct UnitSwatch(pub(crate) Unit);

/// The hire button's price label. A marker rather than inferring "this is the
/// price text" from lacking a `ClassList`: an implicit invariant like that is
/// one a future `ClassList` added to this text for an unrelated reason (say,
/// wanting `:hover` styling on the number) would silently break.
#[derive(Component)]
pub(crate) struct PriceText;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuView {
    Scrim,
    Main,
    Restart,
}

pub(crate) fn setup_hud(commands: &mut Commands, asset_server: &AssetServer) {
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
            Styled::new(asset_server.load(HUD_STYLE)),
            ClassList::new("hud-bar"),
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
                            row_gap: px(6),
                            ..default()
                        },
                        ClassList::new("banner"),
                        Banner,
                    ))
                    .with_children(|banner| {
                        // Label above value, rather than "Bananas: 12.3" on one
                        // line: the inline form needs ~170 px, and a 320 px
                        // phone leaves the middle cell only 136.
                        banner.spawn((
                            Text::new("TOTAL BANANAS"),
                            TextFont::from_font_size(11.0),
                            ClassList::new("banner-label"),
                        ));
                        banner.spawn((
                            Text::new("0.0"),
                            TextFont::from_font_size(COUNTER_FONT_DESKTOP),
                            ClassList::new("counter-text"),
                            CounterText,
                        ));

                        banner
                            .spawn((
                                Node {
                                    display: Display::None,
                                    width: percent(100),
                                    padding: UiRect::top(px(6)),
                                    column_gap: px(5),
                                    ..default()
                                },
                                ClassList::new("rate-panel"),
                                RatePanel,
                            ))
                            .with_children(|panel| {
                                // Aligned on the sign so the three lines read
                                // as arithmetic rather than as three unrelated
                                // numbers.
                                spawn_rate_line(panel, RateLine::Farming, "FARMING", "rate-farming");
                                spawn_rate_line(panel, RateLine::Feeding, "FEEDING", "rate-feeding");
                                spawn_rate_line(panel, RateLine::Net, "GROW AVG", "rate-net");
                                spawn_rate_line(panel, RateLine::Hungry, "HUNGRY", "rate-hungry");
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
                        ..default()
                    },
                    ClassList::new("menu-button"),
                ))
                .with_child((
                    Text::new("MENU"),
                    TextFont::from_font_size(20.0),
                    MenuLabel,
                ));
            });
        });

    spawn_store(commands, asset_server);
}

/// The persistent, always-scrollable store. Only the card list scrolls,
/// leaving the tab strip and its 48 px navigation targets pinned in place.
fn spawn_store(commands: &mut Commands, asset_server: &AssetServer) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0),
                left: px(0),
                width: percent(100),
                padding: UiRect::axes(px(14), px(10)),
                flex_direction: FlexDirection::Column,
                // Centred as a block, which keeps every card the same width
                // while matching the symmetry the banner enforces up top.
                // Hard-left it sat in about a thousand pixels of empty soil
                // and read as unfinished rather than as a deliberate origin.
                align_items: AlignItems::Center,
                row_gap: px(6),
                // `max_height` bounds this node's box but not its children,
                // which overflow visibly by default. Clipping is the backstop
                // that makes "the store never covers the grass" true by
                // construction rather than by arithmetic; the card shape and
                // `apply_store_layout` going compact are what stop it being
                // needed.
                overflow: Overflow::clip_y(),
                ..default()
            },
            Styled::new(asset_server.load(HUD_STYLE)),
            ClassList::new("store-panel"),
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
                                ClassList::new("tab-label"),
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
                                ClassList::new("coming-soon"),
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
                ClassList::new("scroll-cue"),
                Visibility::Hidden,
                Pickable::IGNORE,
                children![(Text::new("SCROLL v"), TextFont::from_font_size(10.0))],
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
                row_gap: px(8),
                display: Display::None,
                ..default()
            },
            Styled::new(asset_server.load(HUD_STYLE)),
            ClassList::new("info-panel"),
            GlobalZIndex(20),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("UNIT INFO"),
                TextFont::from_font_size(18.0),
                ClassList::new("info-title"),
                InfoText::Title,
            ));
            panel.spawn((
                Text::new(""),
                TextFont::from_font_size(14.0),
                ClassList::new("info-body"),
                InfoText::Body,
            ));
            panel.spawn((
                Text::new("Tap the i button again to close"),
                TextFont::from_font_size(11.0),
                ClassList::new("info-hint"),
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
            ..default()
        },
        ClassList::new("tab-arrow"),
        children![(Text::new(label), TextFont::from_font_size(30.0))],
    ));
}

/// One unit: one card. A top row that always fits without shrinking - swatch,
/// name, live rate, owned count, price and the info button - and a second row
/// that can be hidden under height pressure, holding only the description
/// prose. Every figure a player checks before a purchase stays on the top
/// row; only the sentence that explains the unit is allowed to disappear.
fn spawn_unit_row(store: &mut ChildSpawnerCommands, unit: Unit) {
    store
        .spawn((
            Node {
                width: percent(100),
                max_width: px(560),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(px(12), px(10)),
                row_gap: px(4),
                ..default()
            },
            ClassList::new("unit-card"),
            unit,
        ))
        .with_children(|card| {
            card.spawn((
                Node {
                    width: percent(100),
                    align_items: AlignItems::Center,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(8),
                    row_gap: px(6),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|top| {
                top.spawn((
                    Node {
                        width: px(16),
                        height: px(16),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ClassList::new(&format!("unit-swatch {}", unit.swatch_class())),
                    UnitSwatch(unit),
                    Pickable::IGNORE,
                ));
                top.spawn((
                    Text::new(unit.name()),
                    TextFont::from_font_size(14.0),
                    ClassList::new("unit-name"),
                    UnitField {
                        unit,
                        stat: UnitStat::Name,
                    },
                ));
                top.spawn((
                    Text::new(""),
                    TextFont::from_font_size(13.0),
                    TextLayout::linebreak(LineBreak::NoWrap),
                    ClassList::new("unit-rate"),
                    UnitField {
                        unit,
                        stat: UnitStat::Gain,
                    },
                ));
                top.spawn((
                    Text::new("x0"),
                    TextFont::from_font_size(12.0),
                    ClassList::new("unit-owned"),
                    UnitField {
                        unit,
                        stat: UnitStat::Owned,
                    },
                ));
                // Pushes the price pill and info button to the row's end
                // without touching the always-left name/rate/owned trio.
                top.spawn((
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                    Pickable::IGNORE,
                ));
                spawn_hire_button(top, unit, unit.kind());
                if matches!(unit, Unit::Cart) {
                    // Shown in the button's place while the Cart is locked. A
                    // dimmed button and a unit that does not exist yet are
                    // different states, and a dimmed button says the first
                    // when it means the second - it also *takes taps*, queues
                    // a hire that is silently dropped, and burns the debounce
                    // doing it.
                    spawn_locked_plaque(top, unit);
                }
                spawn_info_button(top, unit);
            });
            card.spawn((
                UnitDetail,
                Node {
                    width: percent(100),
                    ..default()
                },
                children![(
                    Text::new(unit.description()),
                    TextFont::from_font_size(13.0),
                    ClassList::new("unit-description"),
                    TextLayout::linebreak(LineBreak::WordBoundary),
                )],
            ));
        });
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

/// The hire button's colour, in every state. Set directly from Rust rather
/// than through `ClassList`: see the note on `.hire-button` in `hud.css` -
/// bevy_flair does not re-resolve a `Button` entity's style when only its
/// `ClassList` changes after spawn, and this is the one widget whose colour
/// genuinely needs to change after spawn on every purchase.
const HIRE_DIM_BG: Color = Color::srgb(0.9098, 0.8706, 0.8314);
const HIRE_DIM_BORDER: Color = Color::srgb(0.7373, 0.6784, 0.6235);
const HIRE_DIM_TEXT: Color = Color::srgb(0.5294, 0.5216, 0.4863);
const HIRE_AFFORD_BG: Color = Color::srgb(0.5216, 0.2510, 0.1216);
const HIRE_AFFORD_EDGE: Color = Color::srgb(1.0, 0.7490, 0.0510);
const HIRE_LIT_BG: Color = Color::srgb(1.0, 0.7490, 0.0510);
const HIRE_LIT_BORDER: Color = Color::srgb(1.0, 0.9412, 0.7216);
const HIRE_LIT_TEXT: Color = Color::srgb(0.9961, 0.8824, 0.7216);

fn spawn_hire_button(row: &mut ChildSpawnerCommands, unit: Unit, kind: UnitKind) {
    row.spawn((
        Button,
        ButtonAction::Hire(kind),
        HireButton(unit),
        Node {
            min_width: px(64),
            min_height: px(44),
            padding: UiRect::axes(px(10), px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            border: UiRect::all(px(2)),
            ..default()
        },
        ClassList::new("hire-button"),
        BackgroundColor(HIRE_DIM_BG),
        BorderColor::all(HIRE_DIM_BORDER),
        // Just the price. The tab strip's tab name says what these are, so
        // repeating "HIRE" inside every button costs a word per unit for
        // nothing.
        children![(
            Text::new("4.0"),
            TextFont::from_font_size(17.0),
            TextColor(HIRE_DIM_TEXT),
            PriceText,
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
        Node {
            display: Display::None,
            min_width: px(64),
            min_height: px(44),
            padding: UiRect::axes(px(10), px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        ClassList::new("locked-plaque"),
        children![(Text::new("LOCKED"), TextFont::from_font_size(12.0))],
    ));
}

fn spawn_info_button(row: &mut ChildSpawnerCommands, unit: Unit) {
    row.spawn((
        Button,
        ButtonAction::Info(unit),
        Node {
            width: px(44),
            min_height: px(44),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        ClassList::new("unit-info-button"),
        children![(Text::new("i"), TextFont::from_font_size(18.0))],
    ));
}

fn spawn_rate_line(panel: &mut ChildSpawnerCommands, line: RateLine, label: &str, class: &str) {
    panel.spawn((
        RateLineRow(line),
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            row_gap: px(1),
            ..default()
        },
        ClassList::new(&format!("rate-line {class}")),
        children![
            (Text::new(label), TextFont::from_font_size(9.0)),
            (
                Text::new("+0.0/min"),
                TextFont::from_font_size(14.0),
                // A rate is one token. Allowed to wrap it breaks after the
                // slash, which reads as two numbers.
                TextLayout::linebreak(LineBreak::NoWrap),
                line,
            ),
        ],
    ));
}

pub(crate) fn setup_menu(commands: &mut Commands, asset_server: &AssetServer) {
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                padding: UiRect::all(px(16)),
                ..default()
            },
            Styled::new(asset_server.load(HUD_STYLE)),
            ClassList::new("menu-scrim"),
            GlobalZIndex(100),
            MenuView::Scrim,
        ))
        .with_children(|scrim| {
            scrim
                .spawn((
                    menu_panel_node(),
                    ClassList::new("menu-panel"),
                    MenuView::Main,
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("BANANA BREAK"),
                        TextFont::from_font_size(34.0),
                        ClassList::new("menu-title"),
                    ));
                    panel.spawn((
                        Text::new("CONTROLS"),
                        TextFont::from_font_size(20.0),
                        ClassList::new("menu-subtitle"),
                    ));
                    let controls = if cfg!(target_arch = "wasm32") {
                        "Drag banana from tree to stall\nPress H to harvest\nPress B to hire a worker\nPress L for input logs"
                    } else {
                        "Drag banana from tree to stall\nPress H to harvest\nPress B to hire a worker"
                    };
                    panel.spawn((
                        Text::new(controls),
                        TextFont::from_font_size(19.0),
                        ClassList::new("menu-body"),
                        TextLayout::justify(Justify::Center),
                    ));
                    panel.spawn(menu_button_row()).with_children(|row| {
                        row.spawn(row_menu_button(ButtonAction::Resume, "emphasized"))
                            .with_child(menu_button_text("RESUME"));
                        #[cfg(target_arch = "wasm32")]
                        row.spawn(row_menu_button(ButtonAction::Diagnostics, "secondary"))
                            .with_child(menu_button_text("INPUT LOGS"));
                    });
                    panel
                        .spawn(menu_button(ButtonAction::Restart, ""))
                        .with_child(menu_button_text("RESTART GAME"));
                });

            scrim
                .spawn((
                    Node {
                        display: Display::None,
                        ..menu_panel_node()
                    },
                    ClassList::new("menu-panel"),
                    MenuView::Restart,
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("RESET RUN?"),
                        TextFont::from_font_size(30.0),
                        ClassList::new("menu-title"),
                    ));
                    panel.spawn((
                        Text::new("Reset bananas to 0 and\ndismiss every worker?\nThis cannot be undone."),
                        TextFont::from_font_size(19.0),
                        ClassList::new("menu-body"),
                        TextLayout::justify(Justify::Center),
                    ));
                    panel
                        .spawn(menu_button(ButtonAction::ConfirmRestart, ""))
                        .with_child(menu_button_text("RESET RUN"));
                    panel
                        .spawn(menu_button(ButtonAction::CancelRestart, ""))
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
        ..default()
    }
}

fn menu_button(action: ButtonAction, extra_class: &str) -> impl Bundle {
    (
        Button,
        action,
        Node {
            width: percent(100),
            min_height: px(52),
            padding: UiRect::axes(px(18), px(10)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        ClassList::new(&format!("menu-panel-button {extra_class}")),
    )
}

fn menu_button_row() -> Node {
    Node {
        width: percent(100),
        column_gap: px(10),
        ..default()
    }
}

fn row_menu_button(action: ButtonAction, extra_class: &str) -> impl Bundle {
    (
        Button,
        action,
        Node {
            min_width: px(0),
            min_height: px(52),
            flex_grow: 1.0,
            flex_basis: px(0),
            padding: UiRect::axes(px(8), px(10)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        ClassList::new(&format!("menu-panel-button {extra_class}")),
    )
}

fn menu_button_text(label: &'static str) -> impl Bundle {
    (Text::new(label), TextFont::from_font_size(21.0))
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

/// Same idea, for a class on a [`ClassList`]: `add`/`remove`/`toggle` all mark
/// the component changed unconditionally, which would re-resolve the whole
/// entity's style every frame regardless of whether the class actually moved.
fn set_class(classes: &mut ClassList, class: &'static str, enabled: bool) {
    if enabled {
        if !classes.contains(class) {
            classes.add(class);
        }
    } else if classes.contains(class) {
        classes.remove(class);
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
/// to its right. The store's height is always `layout.store_height()` - it
/// never grows to cover the board.
#[allow(clippy::type_complexity)]
pub fn apply_store_layout(
    layout: Res<SceneLayout>,
    mut store: Single<&mut Node, (With<StoreRoot>, Without<StoreScroll>, Without<UnitDetail>)>,
    mut scroll: Single<
        (&mut ScrollPosition, &ComputedNode),
        (With<StoreScroll>, Without<StoreRoot>, Without<UnitDetail>),
    >,
    mut details: Query<&mut Node, (With<UnitDetail>, Without<StoreRoot>, Without<StoreScroll>)>,
    mut scroll_cue: Single<&mut Visibility, With<StoreScrollCue>>,
) {
    let height = layout.store_height();
    let pad = if layout.store_width() < NARROW_WIDTH {
        8.0
    } else {
        14.0
    };

    set_if_changed(&mut store.width, px(layout.store_width()));
    set_if_changed(&mut store.height, px(height));
    set_if_changed(&mut store.min_height, px(0));
    set_if_changed(&mut store.max_height, px(height));
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

    // A card's description is the one thing a short store panel is allowed to
    // hide - price, rate and owned count stay on the always-visible top row.
    let compact = height < 220.0;
    for mut node in &mut details {
        set_if_changed(
            &mut node.display,
            if compact { Display::None } else { Display::Flex },
        );
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

/// Wheel and drag scrolling for the store list.
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
    mut labels: Query<(&TabLabel, &mut ClassList, &mut TextFont)>,
    mut contents: Query<(&TabContent, &mut Node)>,
    mut scroll: Single<&mut ScrollPosition, With<StoreScroll>>,
) {
    for (label, mut classes, mut font) in &mut labels {
        set_class(&mut classes, "active", label.0 == active.0);
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
    mut fields: Query<(&UnitField, &mut Text, &mut ClassList), Without<PriceText>>,
    mut prices: Query<(&UnitField, &mut Text, &mut TextColor), With<PriceText>>,
    mut buttons: Query<
        (&HireButton, &Interaction, &mut BackgroundColor, &mut BorderColor, &mut Node),
        Without<UnitField>,
    >,
    mut plaques: Query<(&LockedPlaque, &mut Node), Without<HireButton>>,
    mut swatches: Query<(&UnitSwatch, &mut ClassList), Without<UnitField>>,
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

    for (field, mut text, mut classes) in &mut fields {
        let plan = plan_hire(field.unit.kind(), world);
        let is_locked = locked(field.unit);
        set_class(&mut classes, "locked", is_locked);
        // Checked before the generic negative-gain arm: a Technologist's own
        // gain_per_min is exactly -wage at every world state (D14) - always
        // negative - even though the row is a perfectly good buy. Gold marks
        // "correctly unranked," not "underwater."
        let gold = !is_locked
            && matches!(field.stat, UnitStat::Gain)
            && matches!(field.unit, Unit::Support(SupportRole::Technologist));
        let negative = !is_locked
            && !gold
            && matches!(field.stat, UnitStat::Gain)
            && plan.gain_per_min < 0.0;
        set_class(&mut classes, "gold", gold);
        set_class(&mut classes, "negative", negative);

        let owned = match field.unit {
            Unit::Worker => workforce.count(),
            Unit::Support(role) => staff.count(role),
            Unit::Cart => carts.owned(),
        };
        let value = match (field.stat, field.unit) {
            (UnitStat::Name, _) => field.unit.name().to_string(),
            (UnitStat::Price, _) => unreachable!("price text carries `PriceText`, see `prices`"),
            (UnitStat::Owned, Unit::Cart) if cart_locked => String::new(),
            (UnitStat::Owned, _) => format!("x{owned}"),
            (UnitStat::Gain, Unit::Cart) if cart_locked => {
                if staff.count(SupportRole::Technologist) == 0 {
                    "NEEDS A TECHNOLOGIST".to_string()
                } else {
                    format!("RESEARCH {into_level:.0}/{level_cost:.0}")
                }
            }
            (UnitStat::Gain, Unit::Support(SupportRole::Technologist)) => {
                format!("{:.1} RES/s", technologist_research_per_sec(*multipliers))
            }
            (UnitStat::Gain, _) => format!("{:+.1}/min", plan.gain_per_min),
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
    for (swatch, mut classes) in &mut swatches {
        set_class(&mut classes, "locked", locked(swatch.0));
    }
    // One entry per unit, filled in below and consulted by the price text
    // loop: the price text is a sibling of `HireButton`'s own colour, not
    // read through it, since there is no `Children` lookup in this query set.
    let mut price_colors: Vec<(Unit, Color)> = Vec::with_capacity(Unit::ROWS.len());
    for (button, interaction, mut background, mut border, mut node) in &mut buttons {
        let affordable = plan_hire(button.0.kind(), world).affordable && !locked(button.0);
        let lit = affordable && matches!(interaction, Interaction::Hovered | Interaction::Pressed);
        let (bg, edge, text) = if lit {
            (HIRE_LIT_BG, HIRE_LIT_BORDER, HIRE_LIT_TEXT)
        } else if affordable {
            (HIRE_AFFORD_BG, HIRE_AFFORD_EDGE, HIRE_AFFORD_EDGE)
        } else {
            (HIRE_DIM_BG, HIRE_DIM_BORDER, HIRE_DIM_TEXT)
        };
        set_if_changed(&mut *background, BackgroundColor(bg));
        set_if_changed(&mut *border, BorderColor::all(edge));
        price_colors.push((button.0, text));
        set_if_changed(
            &mut node.display,
            if locked(button.0) {
                Display::None
            } else {
                Display::Flex
            },
        );
    }
    for (field, mut text, mut color) in &mut prices {
        let plan = plan_hire(field.unit.kind(), world);
        let value = if locked(field.unit) {
            String::new()
        } else {
            format!("{:.1}", plan.cost)
        };
        set_if_changed(&mut text.0, value);
        if let Some((_, hue)) = price_colors.iter().find(|(unit, _)| *unit == field.unit) {
            set_if_changed(&mut color.0, *hue);
        }
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
