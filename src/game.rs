use std::time::Duration;

use bevy::{diagnostic::FrameCount, input::touch::Touches, prelude::*, window::PrimaryWindow};

use crate::{
    art,
    domain::{
        BANANAS_PER_HARVEST, Carts, Committed, CycleSpec, CycleTerms, EconomySnapshot,
        EconomyState, FedStaff, HarvestCycle, Multipliers, Research, SIM_HZ, Staff, SupportCycle,
        SupportRole, Treasury, UnitKind, Workforce, cart_crew_shortfall, multipliers_for,
        plan_hire, research_per_sec, restart_run,
    },
    hud,
    isometric::{self, Footprint},
    launch::{Launch, View},
    map::{self, Map},
    persistence,
    support::{self, SupportUnit},
    worker::{self, Cart, RestoredCycle, Worker},
};

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

/// The economy: every system that may write simulation state, and nothing
/// that needs a window, a GPU or an asset.
///
/// Runs under `MinimalPlugins` exactly as it runs under `DefaultPlugins`, which
/// is what lets `headless::Headless` step it one 20 Hz tick at a time in a
/// unit test. Inputs arrive through three request resources - [`HireRequests`],
/// [`RestartRequest`] and the [`DeliveryQueue`] - and every consequence leaves
/// through the [`Settled`] message, so a test never has to fake a pointer to
/// exercise the economy, and the presentation never has to be loaded to see
/// what it did.
///
/// The run state itself - `Treasury`, `Workforce`, `Staff`, `Research`, `Carts`
/// and the two restore budgets - is not initialised here. `scenario::install`
/// inserts it, from a save or from a named scenario, before this plugin is
/// added.
pub struct SimulationPlugin;

/// Everything the player sees and touches. Reads simulation state, never
/// writes it, and needs `DefaultPlugins` underneath.
pub struct PresentationPlugin;

/// The simulation, at a fixed 20 Hz. Mirrors stages 1-7 of the architecture
/// doc's §7 schedule; stages 8 and 9 arrive with the units that need them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Sim {
    Purchase,
    Spawn,
    /// Stage 4. Derives `M_speed`/`M_unpack`/`M_tech` from the world, between
    /// spawning and advancing, so every cycle in a tick runs at the same
    /// multipliers and the readout projects the ones that were actually used.
    Multipliers,
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

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Multipliers>()
            .init_resource::<Carts>()
            .init_resource::<worker::NextLane>()
            .init_resource::<worker::RestoreWorkers>()
            .init_resource::<worker::RestoreCarts>()
            .init_resource::<Staff>()
            .init_resource::<Research>()
            .init_resource::<FedStaff>()
            .init_resource::<Committed>()
            .init_resource::<EconomySnapshot>()
            .init_resource::<DeliveryQueue>()
            .init_resource::<HireRequests>()
            .init_resource::<RestartRequest>()
            .init_resource::<PersistenceDirty>()
            .add_message::<Settled>()
            .add_message::<Restarted>()
            .insert_resource(Time::<Fixed>::from_hz(SIM_HZ))
            .configure_sets(
                FixedUpdate,
                (
                    Sim::Purchase,
                    Sim::Spawn,
                    Sim::Multipliers,
                    Sim::Advance,
                    Sim::Settle,
                    Sim::Snapshot,
                )
                    .chain(),
            )
            .add_systems(
                FixedUpdate,
                (
                    (apply_restart, apply_purchases)
                        .chain()
                        .in_set(Sim::Purchase),
                    (
                        // Chained: boarding removes workers from the pool, and
                        // `spawn_missing_workers` reads that pool to decide how
                        // many avatars there should be. Unordered, a boarded
                        // monkey is despawned and respawned in the same tick.
                        worker::board_carts,
                        worker::spawn_missing_workers,
                        worker::spawn_missing_carts,
                        worker::launch_crewed_carts,
                        support::spawn_missing_support,
                    )
                        .chain()
                        .in_set(Sim::Spawn),
                    recompute_multipliers.in_set(Sim::Multipliers),
                    advance_cycles.in_set(Sim::Advance),
                    settle.in_set(Sim::Settle),
                    snapshot_economy.in_set(Sim::Snapshot),
                ),
            );
    }
}

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(map::Village::start())
            .insert_resource(map::WorkedRoute::start())
            .init_resource::<SceneLayout>()
            .init_resource::<BoardCamera>()
            .init_resource::<CameraGesture>()
            .init_resource::<RecentreRequest>()
            .init_resource::<FirstHarvest>()
            .init_resource::<HarvestController>()
            .init_resource::<PendingSettlement>()
            .init_resource::<Feedback>()
            .init_resource::<MenuState>()
            .init_resource::<hud::ActiveShopTab>()
            .init_resource::<hud::InfoOpen>()
            .init_resource::<UiTouchGesture>()
            .init_resource::<PointerGuard>()
            .init_resource::<DiagnosticPointerTrace>()
            .init_resource::<Launch>()
            .init_resource::<persistence::SaveMode>()
            .add_systems(Startup, (setup, apply_launch_speed))
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
                    (
                        track_ui_touch,
                        hud::handle_store_gesture,
                        // Not in the stage view: with no menu drawn, Escape
                        // would open an invisible one that blocks the drag.
                        handle_menu.run_if(|launch: Res<Launch>| launch.view == View::Full),
                        hud::sync_menu_visibility,
                    )
                        .chain(),
                    // After harvest, never before: the two gestures compete
                    // for the same fingers and harvest has right of first
                    // refusal. See `handle_camera_input`.
                    (handle_harvest_input, handle_camera_input).chain(),
                    hud::scroll_store,
                    move_keyboard_harvest,
                    queue_manual_settlement,
                )
                    .chain()
                    .in_set(Present::Input),
            )
            .add_systems(
                Update,
                (
                    // Before anything poses the cast: the simulation spawns
                    // actors with no art on them, and this is what puts it
                    // there. See `worker::dress_actors`. Animation last: it
                    // spends the distance `position_workers` just measured.
                    (
                        worker::dress_actors,
                        worker::position_workers,
                        worker::animate_workers,
                        // After the harvesters are placed, never before: a
                        // courier runs between a monkey's *drawn* position and
                        // the bins, and reading last frame's leaves it a frame
                        // behind the queue it is serving.
                        support::spawn_missing_couriers,
                        support::sync_couriers,
                        support::pin_courier_shadows,
                    )
                        .chain(),
                    worker::position_carts,
                    isometric::sync_cart_bins,
                    support::sync_support_avatars,
                    support::sync_support_badges,
                    animate_banana,
                    animate_collected,
                    place_held_banana,
                    sync_recentre_button,
                    // Before the two systems that consume what it produces, so
                    // a delivery pulses and floats on the frame it settled.
                    (present_settlements, update_feedback, update_floaters).chain(),
                    hud::sync_readout,
                    hud::sync_shop_tabs,
                    hud::sync_shop_new,
                    hud::sync_info,
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

/// A press of the HUD's HOME button, waiting for the camera to act on it.
///
/// A resource rather than a direct write because the two halves live in
/// different systems for good reason: `handle_menu` owns every button, and
/// `handle_camera_input` owns the camera. It is consumed the frame it lands,
/// exactly as the `C` key is.
#[derive(Resource, Debug, Default)]
struct RecentreRequest(bool);

/// The player's banana's shadow: see [`place_held_banana`].
#[derive(Component)]
struct BananaShadow;

/// The harvest end's own diamond, shown faintly until the first hand delivery.
#[derive(Component)]
struct HarvestHint;

/// Whether the player has yet carried a banana home by hand this session.
///
/// Presentation state: it decides only whether the board teaches the drag
/// (see `update_feedback`), and nothing in the economy reads it.
#[derive(Resource, Debug)]
struct FirstHarvest {
    pending: bool,
}

impl Default for FirstHarvest {
    fn default() -> Self {
        Self { pending: true }
    }
}

#[derive(Component)]
struct DepositGlow;

#[derive(Component)]
struct HarvestLabel;

#[derive(Component)]
struct DepositLabel;

/// The sign over the carts' bins. Hidden until they go up.
#[derive(Component)]
pub(crate) struct CartYardLabel;

/// The banana the *player* drags. Several systems reach for it through
/// `Single`, which silently skips the whole system when the query does not
/// match exactly one entity, so nothing else may ever carry this marker.
/// Workers carry [`worker::CarriedBanana`] instead.
#[derive(Component)]
struct Banana;

/// Which of the bunch's clips the player's banana is playing, and where it has
/// got to: see [`animate_banana`].
#[derive(Component)]
struct BananaAnimation {
    clip: art::Bunch,
    head: art::Playhead,
}

/// A bunch the player has just delivered, playing its despawn where it was
/// dropped and then removed. Its own entity, so the next bunch can grow in at
/// the home tree while this one is still leaving.
#[derive(Component)]
struct Collected {
    /// Where it was dropped, in metres on the ground.
    ground: Vec2,
    head: art::Playhead,
}

/// A rising "+n" over the stall. Two sources, two colours, two magnitudes, so
/// a worker's delivery is still legible as someone else's work when it lands
/// during the player's own drag.
#[derive(Component)]
struct Floater {
    elapsed: f32,
    /// Where it was earned, in **metres on the ground**, never in screen
    /// pixels. A floater captured at a screen position detaches from the stall
    /// it came from the moment the board moves under it, and hangs in the
    /// window for the rest of its life.
    anchor: Vec2,
}

#[derive(Component, Clone, Copy)]
enum LayoutElement {
    DepositGlow,
    HarvestHint,
    Banana,
    BananaShadow,
    HarvestLabel,
    DepositLabel,
    CartYardLabel,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonAction {
    OpenMenu,
    /// Take the camera back to the opening view. See [`RecentreRequest`].
    Recentre,
    Hire(UnitKind),
    Info(hud::Unit),
    PreviousShopTab,
    NextShopTab,
    Resume,
    #[cfg(target_arch = "wasm32")]
    Diagnostics,
    Restart,
    ConfirmRestart,
    CancelRestart,
}

#[derive(bevy::ecs::system::SystemParam)]
struct MenuPanels<'w> {
    info_open: ResMut<'w, hud::InfoOpen>,
    active_tab: ResMut<'w, hud::ActiveShopTab>,
}

impl ButtonAction {
    /// Which menu state the button is live in. The touch hit-test iterates
    /// every button regardless of what is actually on screen, so without this
    /// a tap on the scrim would reach the shop card underneath it.
    fn active_in(self, menu: MenuState) -> bool {
        match self {
            ButtonAction::OpenMenu
            | ButtonAction::Recentre
            | ButtonAction::Hire(_)
            | ButtonAction::Info(_)
            | ButtonAction::PreviousShopTab
            | ButtonAction::NextShopTab => menu == MenuState::Closed,
            ButtonAction::Resume | ButtonAction::Restart => menu == MenuState::Open,
            #[cfg(target_arch = "wasm32")]
            ButtonAction::Diagnostics => menu == MenuState::Open,
            ButtonAction::ConfirmRestart | ButtonAction::CancelRestart => {
                menu == MenuState::ConfirmRestart
            }
        }
    }
}

/// The player's view of the board: where they are looking, and how close.
///
/// Pan and zoom are **player state**, not state derived from the window. That
/// is the whole of the camera increment. `SceneLayout` used to recompute its
/// aim from the viewport every frame and point the board at the midpoint of the
/// walk, which D25 recorded as an interim for a board with no controls; the
/// viewport now decides the HUD's reserve and nothing else, and where the board
/// is pointed belongs to the person holding the phone.
///
/// Two numbers, because two numbers are all `SceneLayout::board` needs. Keeping
/// the focus in *metres* rather than as a screen origin is what makes it
/// survive a zoom, a rotation of the device and a resize without drifting: the
/// player is looking at a place, not at a pixel.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub(crate) struct BoardCamera {
    /// The ground position, in metres, held at the centre of the safe area.
    focus: Vec2,
    /// Screen pixels per projected pixel, as drawn this frame.
    ///
    /// Continuous while a pinch is live and whole-numbered the rest of the
    /// time. The ground is a vertex-coloured mesh and takes any scale without
    /// complaint; it is the *sprites* that shimmer off the grid, and a gesture
    /// is the one moment the player is looking at their own fingers rather than
    /// at a monkey's texels. So the pinch tracks continuously and the zoom
    /// settles onto a whole step when the fingers lift.
    zoom: f32,
    /// The whole step `zoom` is settling towards.
    resting_zoom: f32,
}

impl BoardCamera {
    /// The closest the board comes.
    const MAX_ZOOM: f32 = 6.0;

    /// The furthest the board goes, bounded by **how big a monkey is**, never
    /// by how much of the map fits.
    ///
    /// A monkey is 29 texels tall, so this renders one 58 logical pixels — about
    /// a thumbnail, and the point below which the cast stops reading as animals
    /// and starts reading as confetti. Clamping to "fit the 69x69 map" instead
    /// would put a phone near zoom 0.3 and a monkey at six pixels: the whole
    /// board visible and nothing on it worth looking at. The map is explored by
    /// panning (D24), not by zooming out far enough to see it all at once.
    const MIN_ZOOM: f32 = 2.0;

    /// Where the board opens: at the floor, so the board opens at its widest
    /// and the player only ever zooms *in*.
    ///
    /// Two things have to be true of the opening frame and they pull the same
    /// way. The player's first action is a hand-harvest drag from the home tree
    /// to the town centre (D24), five tiles apart, and both ends must sit
    /// inside the safe area or the game opens on a gesture that cannot be made.
    /// The three support stations must be visible too, or staff the player has
    /// paid for draw wages off the side of the screen. On an 844x390 landscape
    /// phone — the tightest safe area the game supports, 286 px square — the
    /// second of those fails at any zoom past this one.
    ///
    /// Since D30 the opening view is centred on the treehouse instead, and on
    /// that phone the home tree and part of the crew open just off the board;
    /// a higher zoom would lose the depot as well.
    ///
    /// So this sits on `MIN_ZOOM` rather than above it, and pinching outwards
    /// from a fresh board does nothing. That is a real cost, and it is the
    /// cheaper one: the alternative is opening below the zoom at which a monkey
    /// reads as a monkey. `the_board_opens_framed_on_the_first_drag` and
    /// `every_support_avatar_is_on_screen_when_the_game_opens` are what keep
    /// both halves honest if someone raises it.
    const DEFAULT_ZOOM: f32 = Self::MIN_ZOOM;

    /// How fast a settling zoom closes on its whole step, per second.
    ///
    /// A hard snap on release is a visible jump of up to half a step - a
    /// quarter of the board at the low end - so it eases instead.
    const SETTLE_RATE: f32 = 14.0;

    /// The view a new player opens on: centred on the treehouse, the village's
    /// landmark and its depot (D30). The hand-harvest drag lies across its
    /// front, so on a portrait phone both ends of it are in view; on a
    /// landscape one the home tree opens nearer the edge than a thumb, and HOME
    /// says so.
    fn opening(map: &Map) -> Self {
        Self {
            focus: isometric::town_centre_view(map),
            zoom: Self::DEFAULT_ZOOM,
            resting_zoom: Self::DEFAULT_ZOOM,
        }
    }

    #[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
    pub(crate) fn zoom(self) -> f32 {
        self.zoom
    }

    #[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
    pub(crate) fn focus(self) -> Vec2 {
        self.focus
    }

    /// Where the projected origin lands on screen, given the centre of the safe
    /// area.
    ///
    /// The single place the camera becomes pixels. `SceneLayout::board` and the
    /// gesture handlers all read the board through this, so there is no second
    /// opinion about where the player is looking.
    fn origin(self, scene_center: Vec2) -> Vec2 {
        scene_center - isometric::project(self.focus) * self.zoom
    }

    /// The ground position, in metres, under a point on the screen.
    fn ground_at(self, scene_center: Vec2, screen: Vec2) -> Vec2 {
        isometric::unproject((screen - self.origin(scene_center)) / self.zoom)
    }

    /// Re-aim so that `world` sits under `screen` at the current zoom.
    ///
    /// This is the whole of both gestures. A drag holds the metre the finger
    /// landed on; a pinch holds the metre between the two fingers while the
    /// zoom changes under it. Neither is expressed as "move the camera by an
    /// amount" - which is how a pinch ends up sliding the ground out from
    /// between the fingers that are supposedly pinching it.
    fn hold(&mut self, scene_center: Vec2, world: Vec2, screen: Vec2) {
        self.focus =
            isometric::unproject(isometric::project(world) + (scene_center - screen) / self.zoom);
    }

    /// Take the zoom to `wanted`, holding the ground under `screen` in place.
    fn zoom_to(&mut self, scene_center: Vec2, screen: Vec2, wanted: f32) {
        let held = self.ground_at(scene_center, screen);
        self.zoom = wanted.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.hold(scene_center, held, screen);
    }

    /// End a pinch: the zoom settles onto the nearest whole step, so the scene
    /// comes to rest with every sprite back on the texel grid.
    fn settle(&mut self) {
        self.resting_zoom = self.zoom.round().clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
    }

    /// Ease a settling zoom towards its whole step, holding `screen`.
    fn ease(&mut self, scene_center: Vec2, screen: Vec2, delta_seconds: f32) {
        if self.zoom == self.resting_zoom {
            return;
        }
        let closed = 1.0 - (-Self::SETTLE_RATE * delta_seconds).exp();
        let next = self.zoom + (self.resting_zoom - self.zoom) * closed;
        // Land exactly rather than approaching forever: a zoom a thousandth off
        // a whole step keeps `snap` off the grid for no visible benefit.
        let wanted = if (next - self.resting_zoom).abs() < 1e-3 {
            self.resting_zoom
        } else {
            next
        };
        self.zoom_to(scene_center, screen, wanted);
    }

    /// Hold the camera over ground the player has a reason to look at.
    ///
    /// What is bounded is how far the focus may stray *from the walk*, and the
    /// bound tightens as the player zooms out — so whatever they do, some of
    /// the ground their monkeys cover is on screen. That is the guarantee, and
    /// it is deliberately not "the village is on screen": looking at the middle
    /// of the route, with neither end in frame, is a thing a player should be
    /// able to do.
    ///
    /// Clamping the focus into the field's bounding *box* is not a weaker
    /// version of this, it is a different and much emptier promise. Half a safe
    /// area is 71 projected pixels at the opening zoom against a box a thousand
    /// across, so a focus legally parked on a corner shows five screens of
    /// nothing — which is exactly what ten drags on a phone produced: a corner
    /// of canopy, a screenful of sky, and no landmark to steer back by.
    fn clamped(self, field: Field, view: Vec2) -> Self {
        Self {
            focus: isometric::unproject(field.hold(
                isometric::project(self.focus),
                view,
                self.zoom,
            )),
            zoom: self.zoom.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM),
            resting_zoom: self.resting_zoom.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM),
        }
    }
}

impl Default for BoardCamera {
    fn default() -> Self {
        Self::opening(map::start())
    }
}

/// Where the board sits on screen, and how the ground plane maps onto it.
///
/// This used to *be* the world: a unit square holding an invented route, scaled
/// to fit. It is now only a projection of the map through a [`BoardCamera`].
/// Every position it reports is the projection of a real position in metres on
/// `map`'s ground plane, which is what lets the drawn village and the walked
/// economy be the same place.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub(crate) struct SceneLayout {
    pub(crate) viewport: Vec2,
    /// The part of the window no HUD surface covers. Everything the player is
    /// meant to look at is framed against *this*, never against the viewport.
    safe: Rect,
    scene_side: f32,
    header_height: f32,
    store_height: f32,
    short_landscape: bool,
    /// The camera resolved into pixels: where the projected origin lands on
    /// screen, and how many screen pixels a projected pixel is worth.
    /// [`Self::board`] is the only road from metres to screen, and these are
    /// all it needs.
    origin: Vec2,
    zoom: f32,
    /// The ground the player has a reason to look at, projected. The pan clamp
    /// lives here so a hit test never re-walks the map.
    field: Field,
    /// Ground anchors, in metres, kept for the same reason.
    town_centre: Vec2,
    grove: Vec2,
    home_tree: Vec2,
}

impl Default for SceneLayout {
    fn default() -> Self {
        Self::for_viewport(Vec2::new(1280.0, 720.0))
    }
}

/// How far the hand-harvest target reaches either side of the home tree, in
/// metres: three tiles square.
///
/// That is its size at the zoom floor, where its diamond's short axis is 96
/// pixels; above the floor it is capped on screen instead (see
/// [`HARVEST_SHORT_AXIS_PX`]). And it stops well short of the depot: the home
/// tree is ten metres out, so three here and five there leave two metres of
/// ground that is neither, and a drag cannot start on its own drop target.
const HARVEST_REACH_METRES: f32 = 3.0;
/// The most screen the grab target's diamond may take, as its short axis in
/// logical pixels: exactly its size at the zoom floor.
///
/// Harvest gets first refusal on every press (D26), so its target is also
/// where the camera cannot be driven from. Three metres at every zoom is a
/// diamond of 576 x 288 pixels at zoom 6 - three quarters of a landscape
/// phone's board - which left a player zoomed in on the village nothing to
/// pan or pinch with but the corners. Zoomed in, the plant grows and the
/// ground its target covers shrinks, never below the banana's own spot.
const HARVEST_SHORT_AXIS_PX: f32 = 96.0;
/// The plant above the banana grabs too, within a narrow column up the trunk:
/// half its width, and its height at most, in logical pixels.
///
/// The palm is the biggest, brightest thing by the banana and it is what a
/// stranger reaches for; a press on its fronds used to be the camera's, and the
/// player's very first try slid the board away from the thing they were
/// reaching for. Narrow, and capped, so that zoomed in - where the crown fills
/// the screen - the board is still the camera's.
const TRUNK_HALF_WIDTH_PX: f32 = 24.0;
const TRUNK_REACH_PX: f32 = 160.0;
/// Where the loose banana lies relative to the home tree's trunk, in metres:
/// towards the viewer, so it sorts in front of the plant and lies on its shadow.
const BANANA_REST_OFFSET: Vec2 = Vec2::new(0.8, 0.8);
/// How far a held banana is drawn above the pointer, in board texels: most of
/// a banana, so a thumb on the glass does not cover it.
const HELD_LIFT_TEXELS: f32 = 14.0;
/// And never less than this, in logical pixels: clear of a thumb's pad, and of
/// the lower edge of the DEPOT sign the banana is being aimed at.
const HELD_LIFT_MIN_PX: f32 = 44.0;
/// How far inside the safe area both ends of the drag must be for it to count
/// as in view, in logical pixels: half a thumb.
const DRAG_VIEW_MARGIN: f32 = 22.0;

/// Where the carts' bins stand, in metres from the delivery point. See
/// [`SceneLayout::cart_bins`].
const CART_BINS_OFFSET: Vec2 = Vec2::new(-4.0, 12.0);
/// How far in front of those bins a cart parks, in metres.
///
/// Far enough in front that the *boxes still show over the rank*. A cart's box
/// is 18.75 texels tall and its riders sit above that, so a rank parked a
/// body's length away simply erases the thing it is unloading into; the bins
/// reach 19 texels above their own ground line. Measured up the screen this
/// standoff is 31 texels of lift, and
/// `the_cart_rank_never_hides_the_bins_it_unloads_into` is what holds the
/// margin that leaves.
const CART_PARK_STANDOFF: f32 = 6.5;
/// And how far apart two bays are, in metres along the ground's x axis.
///
/// Along a ground axis rather than across the screen, which is half the point:
/// a rank laid out across the screen puts every cart on one line of screen y,
/// and brown boxes wider than the gap between them read as one long brown bar.
/// A ground axis steps each bay both sideways *and* nearer the viewer, so they
/// overlap the way a rank of parked vehicles seen from above overlaps, and the
/// depth rule sorts them front to back for free.
///
/// The other half is the size of the step, which was 2.2 m: 17.6 texels across
/// the screen against a cart 65 texels wide, so four bays spanned less screen
/// than *one* cart and each rear box showed a 27% sliver. Four metres is a
/// whole cart's width of separation, which is what lets the yellow load band -
/// the only readout of how full a hundred-banana box is - be visible on more
/// than the front cart.
const CART_BAY_STEP: f32 = 4.0;
/// How many bays there are across the front of the yard, and how many rows
/// deep it goes.
///
/// Six all told, rather than four, because four is below the fleet the game's
/// own economy settles at: the whitepaper's reference run owns six carts by
/// twenty-four minutes, so a four-bay yard is permanently double-parked past
/// ten minutes - and because a later bay is drawn nearer the viewer, each new
/// cart would hide an older one, so buying a cart made the yard look emptier.
///
/// Three across and two deep rather than six across, because six bays a cart's
/// width apart is twenty metres of rank: the outer bays swing past the bins
/// they are meant to be parked at, and on a landscape phone they leave the
/// board entirely. A second row costs depth, which the isometric view has to
/// spare, and reads as a yard filling up rather than a queue sprawling.
const CART_BAY_COLUMNS: u32 = 3;
const CART_BAY_ROWS: u32 = 2;
pub(crate) const CART_BAYS: u32 = CART_BAY_COLUMNS * CART_BAY_ROWS;
/// How far behind the front row the second one stands, in metres.
const CART_ROW_STEP: f32 = 3.5;

/// How far past the ground they work the player may pan, in metres.
///
/// Six tiles: enough that the village is never pinned against the edge of the
/// screen, and short enough that the player can always see something of theirs.
const FIELD_MARGIN: f32 = 12.0;

/// The ground the camera is allowed to look at: the walk, projected, with a
/// margin around it.
///
/// A *polyline*, not a bounding box, and that is the whole difference between a
/// guarantee and a slogan. The box around the home tree, the town centre and
/// the grove has corners that are two hundred projected pixels from any of the
/// three — off the walk, off the path, on ground nobody has ever been to — and
/// a focus is perfectly entitled to sit on one. Measuring from the walk itself
/// means the slack is slack *from something*.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Field {
    /// The walk, projected, in the order it is travelled: the home tree the
    /// player picks by hand, the town centre everything is delivered to, and
    /// the grove the workers walk out to.
    walk: [Vec2; 3],
    /// How far off the walk the camera may stray, in projected pixels.
    margin: f32,
}

impl Field {
    /// The point on the walk nearest `at`.
    fn nearest(self, at: Vec2) -> Vec2 {
        self.walk
            .windows(2)
            .map(|leg| {
                let (from, span) = (leg[0], leg[1] - leg[0]);
                let along = if span.length_squared() > f32::EPSILON {
                    ((at - from).dot(span) / span.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                from + span * along
            })
            .min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at)))
            .unwrap_or(self.walk[0])
    }

    /// Where a focus of `at` is allowed to settle, given how much of the board
    /// a screenful covers.
    ///
    /// The slack tightens as the player zooms *in*, because a screenful then
    /// covers less ground: it is capped at whatever keeps the nearest point of
    /// the walk inside the *short* side of the safe area, so the guarantee
    /// holds in portrait and landscape alike. Zoomed out, the margin is what
    /// binds instead.
    fn hold(self, at: Vec2, view: Vec2, zoom: f32) -> Vec2 {
        // Nine tenths of the half-extent, not all of it. At exactly half, the
        // nearest point of the walk lands *on* the edge of the safe area, where
        // a rounded origin and a float comparison decide whether it is on
        // screen or a pixel outside it. The tenth is what makes the guarantee
        // survive being asserted.
        let reach = view.min_element() * 0.45 / zoom;
        let slack = self.margin.min(reach);
        let near = self.nearest(at);
        near + (at - near).clamp_length_max(slack)
    }

    /// How long the walk is, projected. Only the tests care, and what they care
    /// about is that it grows with the ground the player works.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn span(self) -> f32 {
        self.walk
            .windows(2)
            .map(|leg| leg[0].distance(leg[1]))
            .sum()
    }
}

impl SceneLayout {
    pub(crate) fn for_viewport(viewport: Vec2) -> Self {
        Self::for_view(viewport, View::Full)
    }

    pub(crate) fn for_view(viewport: Vec2, view: View) -> Self {
        Self::for_map(viewport, view, map::start(), BoardCamera::default())
    }

    /// The window decides the chrome; the camera decides the aim.
    ///
    /// That split is the point. Everything above `origin` here is a function of
    /// the viewport alone — the banner's strip, the store's panel, and the
    /// square the two of them leave — and is what has to change when the device
    /// rotates. Where the board is *pointed* is not one of those things.
    ///
    /// The stage view has no banner and no store to make room for, so the board
    /// takes the largest square the window holds, centred.
    pub(crate) fn for_map(viewport: Vec2, view: View, map: &Map, camera: BoardCamera) -> Self {
        let width = viewport.x.max(320.0);
        let height = viewport.y.max(320.0);
        let stage = view == View::Stage;
        let short_landscape = !stage && width > height * 1.35 && height < 560.0;
        let header_height = if stage { 0.0 } else { 104.0 };
        let scene_side = if stage {
            width.min(height)
        } else if short_landscape {
            (height - header_height).min(width * 0.48)
        } else {
            width.min(height * 0.58)
        };
        let store_height = if stage {
            0.0
        } else if short_landscape {
            height - header_height
        } else {
            (height - header_height - scene_side).max(0.0)
        };

        // What is left of the window once the chrome has taken its reserve.
        // Framing against the viewport instead is what put the hut under the
        // store panel and the grove behind the banner: dead centre of the
        // window is dead centre of *nothing the player can see*.
        let half = Vec2::new(width, height) * 0.5;
        let safe = if stage {
            Rect::from_corners(-half, half)
        } else if short_landscape {
            let store_width = width - scene_side;
            Rect::from_corners(
                Vec2::new(-half.x, -half.y),
                Vec2::new(half.x - store_width, half.y - header_height),
            )
        } else {
            Rect::from_corners(
                Vec2::new(-half.x, -half.y + store_height),
                Vec2::new(half.x, half.y - header_height),
            )
        };

        let town_centre = isometric::tile_centre(map.town_centre());
        let grove = isometric::tile_centre(map.worked_grove().tile);
        let home_tree = map
            .home_trees()
            .first()
            .map_or(town_centre, |&tile| isometric::tile_centre(tile));

        // The ground the player has a reason to look at: the walk, plus a
        // margin. The *worked* grove, not every grove on the map — the second
        // node sits 117 m south and is never assigned a worker in the MVP, so
        // folding it in stretched the leash half again as far for ground nobody
        // has ever been to. It joins the walk the day it is worked, which is
        // what "the leash lengthens with the run" was always supposed to mean.
        //
        // The margin is converted to projected pixels through the *widest* the
        // fold ever stretches a metre, so twelve metres of slack is at least
        // twelve metres in every direction rather than twelve along one axis
        // and six along the other.
        let field = Field {
            walk: [home_tree, town_centre, grove].map(isometric::project),
            margin: FIELD_MARGIN * isometric::TILE_HALF.x / map::TILE_METRES as f32,
        };

        // Rounded to whole pixels, because this is the corner the texel grid is
        // measured from (see `board_snapped`). A board sitting on half a pixel
        // rasterises every sprite in the scene against a half-pixel offset —
        // and under a pan, against a *different* half-pixel offset every frame.
        let origin = camera.origin(safe.center()).round();

        Self {
            viewport: Vec2::new(width, height),
            safe,
            scene_side,
            header_height,
            store_height,
            short_landscape,
            origin,
            zoom: camera.zoom,
            field,
            town_centre,
            grove,
            home_tree,
        }
    }

    /// The part of the window no HUD surface covers.
    #[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
    pub(crate) fn safe_area(self) -> Rect {
        self.safe
    }

    /// The ground the camera's focus is held near, projected.
    pub(crate) fn field(self) -> Field {
        self.field
    }

    /// Screen position of a ground position, in metres.
    ///
    /// The one road from the world to the screen. `WorldRoot` carries the same
    /// two numbers as a `Transform`, so a baked mesh under it lands exactly
    /// where this says it would.
    pub(crate) fn board(self, world: Vec2) -> Vec2 {
        self.origin + isometric::project(world) * self.zoom
    }

    /// Screen position of something standing `metres` tall at `world`.
    pub(crate) fn board_raised(self, world: Vec2, metres: f32) -> Vec2 {
        self.origin + isometric::raise(isometric::project(world), metres) * self.zoom
    }

    pub(crate) fn world_root(self) -> Transform {
        Transform::from_translation(self.origin.extend(0.0))
            .with_scale(Vec3::new(self.zoom, self.zoom, 1.0))
    }

    pub(crate) fn town_centre(self) -> Vec2 {
        self.town_centre
    }

    pub(crate) fn grove(self) -> Vec2 {
        self.grove
    }

    pub(crate) fn home_tree(self) -> Vec2 {
        self.home_tree
    }

    /// The middle of the ground the player can actually see. The board's aim
    /// lands here, not in the middle of the window.
    pub(crate) fn scene_center(self) -> Vec2 {
        self.safe.center()
    }

    pub(crate) fn scene_side(self) -> f32 {
        self.scene_side
    }

    pub(crate) fn header_height(self) -> f32 {
        self.header_height
    }

    pub(crate) fn short_landscape(self) -> bool {
        self.short_landscape
    }

    pub(crate) fn store_width(self) -> f32 {
        if self.short_landscape {
            self.viewport.x - self.scene_side
        } else {
            self.viewport.x
        }
    }

    pub(crate) fn store_height(self) -> f32 {
        self.store_height
    }

    /// Sprite scale. Every texel in the scene is the same size, so an actor
    /// scales with the board and with nothing else.
    pub(crate) fn world_scale(self) -> f32 {
        self.zoom
    }

    /// Where the loose banana lies, in metres: on the ground at the home tree's
    /// foot, which is the node the player picks by hand (D24), a step towards
    /// the viewer so the plant can never draw over it.
    ///
    /// On the ground, not in the plant. It used to hang 1.4 m up, a height
    /// chosen for the old palm mesh's crown, and against the drawn plant that
    /// is across the trunk: a yellow band tied round the tree rather than
    /// something lying there to be picked up.
    pub(crate) fn banana_ground(self) -> Vec2 {
        self.home_tree + BANANA_REST_OFFSET
    }

    /// Where the loose banana is drawn when nobody is holding it: its feet.
    pub(crate) fn banana_home(self) -> Vec2 {
        self.board(self.banana_ground())
    }

    /// Where a held banana is drawn, for a pointer at `screen`.
    ///
    /// Lifted clear of the pointer, so on a phone the thumb covers the
    /// banana's shadow - which marks the ground the drop is tested against -
    /// and not the banana it is carrying.
    pub(crate) fn held_banana(self, screen: Vec2) -> Vec2 {
        screen + Vec2::new(0.0, (HELD_LIFT_TEXELS * self.zoom).max(HELD_LIFT_MIN_PX))
    }

    /// The ground position, in metres, under a point on the screen: the exact
    /// inverse of [`Self::board`].
    pub(crate) fn ground(self, screen: Vec2) -> Vec2 {
        isometric::unproject((screen - self.origin) / self.zoom)
    }

    /// Where a hand harvest starts: a square of ground round the home tree.
    ///
    /// Three metres at the zoom floor, and no more than
    /// [`HARVEST_SHORT_AXIS_PX`] of screen above it.
    pub(crate) fn harvest_target(self) -> Footprint {
        // The diamond's short axis per metre of half, at unit zoom.
        let per_metre = 2.0 * isometric::project(Vec2::splat(-1.0)).y;
        let half = HARVEST_REACH_METRES.min(HARVEST_SHORT_AXIS_PX / (per_metre * self.zoom));
        Footprint::new(self.home_tree, half)
    }

    /// Where it ends: the depot's trodden ground, edge to edge.
    pub(crate) fn deposit_target(self) -> Footprint {
        Footprint::new(self.town_centre, isometric::DEPOT_REACH_METRES)
    }

    /// Whether a point on the screen is over the harvest target.
    ///
    /// Taken down to the ground and tested there. The target used to be a
    /// square of screen, `scene_side * 0.18`, sized from the chrome rather than
    /// from the board: zoom in and the plant grew while the box stayed put,
    /// zoom out and the box swallowed the village, and at any zoom it was the
    /// one shape on screen off the 2:1 grid.
    pub(crate) fn on_harvest(self, screen: Vec2) -> bool {
        self.harvest_target().contains(self.ground(screen)) || self.on_trunk(screen)
    }

    /// Whether a point on the screen is on the home plant, up its trunk into
    /// the lower crown. See [`TRUNK_HALF_WIDTH_PX`].
    fn on_trunk(self, screen: Vec2) -> bool {
        let foot = self.board(self.home_tree);
        let crown = (art::PLANT.height_above(art::PLANT_CROWN_ROW) * self.zoom).min(TRUNK_REACH_PX);
        (screen.x - foot.x).abs() <= TRUNK_HALF_WIDTH_PX
            && screen.y >= foot.y
            && screen.y <= foot.y + crown
    }

    /// Whether a point on the screen is over the depot.
    pub(crate) fn on_deposit(self, screen: Vec2) -> bool {
        self.deposit_target().contains(self.ground(screen))
    }

    /// The box round a footprint's diamond, on screen. For the diagnostics
    /// log and the browser suite, which want a rectangle; never for hit
    /// testing, which is what `on_harvest` and `on_deposit` are for.
    pub(crate) fn screen_box(self, footprint: Footprint) -> Rect {
        let [top, right, bottom, left] = footprint.diamond().map(|p| self.origin + p * self.zoom);
        Rect::from_corners(Vec2::new(left.x, bottom.y), Vec2::new(right.x, top.y))
    }

    pub(crate) fn harvest_bounds(self) -> Rect {
        self.screen_box(self.harvest_target())
    }

    pub(crate) fn deposit_bounds(self) -> Rect {
        self.screen_box(self.deposit_target())
    }

    /// Whether both ends of the hand-harvest drag are in the safe area.
    ///
    /// The pan clamp promises that *some of the walk* is always on screen, not
    /// that the home tree is: pan to the grove end and the one gesture a new
    /// player is taught becomes impossible, silently. When this is false the
    /// HUD offers a way home (see `sync_recentre_button`), which on a phone is
    /// the only one there is.
    pub(crate) fn drag_in_view(self) -> bool {
        // Half a thumb inside the edge, not merely inside it: a home tree
        // whose centre is a pixel inside the safe area has most of its target
        // under the store panel, and the drag is as good as gone.
        let inner = self.safe.inflate(-DRAG_VIEW_MARGIN);
        [self.home_tree, self.town_centre]
            .into_iter()
            .all(|at| contains_inclusive(inner, self.board(at)))
    }

    pub(crate) fn stall_glow_anchor(self) -> Vec2 {
        self.board_raised(self.town_centre, 1.0)
    }

    /// How far along the route a cart stands from where the walking monkeys do,
    /// in metres. The cart uses its own bay at the grove so its long pick does
    /// not cover the harvesters working it.
    pub(crate) fn cart_offset(self) -> f32 {
        4.0
    }

    /// Where the carts' own banana bins stand, in metres.
    ///
    /// A second set of bins, put up the moment the Technologist's first
    /// research level lands and the Cart stops being a greyed-out shop row
    /// (D31). Carts are a hundred bananas a trip against a harvester's five, and
    /// they used to unload onto the same nine tiles the walking crowd stands
    /// on: a vehicle the length of three monkeys, parked across the queue for a
    /// hundred seconds at a time.
    ///
    /// Giving the freight its own bins makes the depot two places doing two
    /// jobs, and it is the visible reward for the research that unlocked them.
    ///
    /// Out in front of the village, past the kitchen, on the near side of the
    /// walk home - and that side is the whole reason it is there rather than in
    /// the empty quarter to the right of the depot, which is where it went
    /// first. The road comes in from the up-left. With the yard on the right the
    /// delivery point sits *between* the road and the bay, so a cart driving to
    /// its bay crosses the ground the harvesters unload on; on the near side it
    /// never does. See `worker::drive_into_bay`, and
    /// `a_cart_drives_round_the_unloading_crowd_rather_than_through_it`, which
    /// is what holds it.
    ///
    /// Far enough out that the whole yard stands clear of the unloading ring and
    /// of every support monkey - `the_cart_park_stands_clear_of_the_village` -
    /// and near enough that it is a short pan from where the board opens, which
    /// is `the_carts_corner_is_a_short_pan_from_the_opening_view`.
    pub(crate) fn cart_bins(self) -> Vec2 {
        self.town_centre + CART_BINS_OFFSET
    }

    /// Where the `index`th cart parks, in metres.
    ///
    /// A rank drawn up across the *front* of the cart bins - the viewer's side,
    /// which on the ground is the direction of increasing depth. So a parked
    /// cart draws in front of the boxes it is unloading into rather than behind
    /// them, and the rank stands on open grass rather than in the tree line the
    /// bins back onto.
    ///
    /// Evenly spaced rather than hashed, unlike a harvester's spot. There are
    /// never many carts, and a handful of vehicles at even spacing reads as
    /// *parked*; the same handful scattered reads as abandoned. Past
    /// [`CART_BAYS`] the bays repeat and carts overlap, which is the owner's
    /// call: a cart that cannot find a clear bay parks on top of one rather
    /// than queueing out of sight.
    pub(crate) fn cart_park(self, index: u32) -> Vec2 {
        // Towards the viewer, in ground metres: both axes increasing.
        const FRONT: Vec2 = Vec2::new(
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        );
        let bay = index % CART_BAYS;
        // The near row fills first, so the first cart bought is the one in
        // front: a yard fills towards the viewer rather than away from them.
        let row = (CART_BAY_ROWS - 1 - bay / CART_BAY_COLUMNS) as f32;
        let column = (bay % CART_BAY_COLUMNS) as f32 - (CART_BAY_COLUMNS as f32 - 1.0) * 0.5;
        self.cart_bins()
            + FRONT * (CART_PARK_STANDOFF + row * CART_ROW_STEP)
            + Vec2::X * column * CART_BAY_STEP
    }

    /// Where each support role stands, in metres, relative to the town centre.
    ///
    /// A **ring** around the delivery point, not a line out from it. The three
    /// used to be strung along one bearing at five, eleven and five metres,
    /// which separated them on screen only because the far one was twice as far
    /// out as the near one. That works on a board framed to fit the window and
    /// fails the moment the player is zoomed in: at the camera's opening zoom
    /// the eleven-metre station is off the side of a phone, so a chef the
    /// player paid for is drawing wages somewhere they cannot see.
    ///
    /// Since the treehouse became the depot (D30) the three stand round its
    /// bins rather than on an 8.2 m ring, and every monkey of every fan clear
    /// of the ring the unloading crowd stands on (4.8 m) by half a body, so no
    /// role is mistaken for the queue: the unpacker to the right of the bins it
    /// empties, where the old ring put it behind the house; the chef in front
    /// and to the left, towards the stairs; the technologist further left,
    /// clear of the home tree. `support_never_stands_in_the_unloading_crowd`
    /// holds the first of those. The closest two are still more than a full fan apart
    /// on screen, every avatar of every fan is inside the safe area at the
    /// opening camera on every viewport but the two smallest boards - the
    /// 320 x 568 phone and the 844 x 390 landscape one (D30) - and all
    /// three stand clear of the walk, of the house and of the home tree. That
    /// last one is a real constraint, not a courtesy: the home tree carries the
    /// hand-harvest drag target, so a station under its crown puts a monkey
    /// inside the thing the player is trying to grab.
    /// `every_support_avatar_is_on_screen_when_the_game_opens`,
    /// `support_never_stands_on_the_worker_route` and
    /// `support_never_stands_in_the_hand_harvest_target` hold those, and they
    /// check the *fan* rather than just the station: it is the outermost chef
    /// that leaves the screen first.
    pub(crate) fn support_stand(self, role: SupportRole) -> Vec2 {
        let offset = match role {
            // Beside the bins it empties, level with them on screen and to
            // their right. Since D31 nothing of the Unpacker's *stands* here:
            // its monkeys are squirrel couriers, and they are either running
            // between an arriving harvester and the bins or waiting at the bins
            // themselves. What is left here is where the `xN` badge hangs when
            // there are more couriers than sprites - which is why this is the
            // one station the carts' bins are allowed to stand near.
            SupportRole::Unpacker => Vec2::new(4.0, -4.6),
            // Nearest the viewer, at the front of the village: being fed is the
            // most-watched thing that happens at the stall, and what the player
            // is looking for when the banner reads HUNGRY.
            SupportRole::Chef => Vec2::new(1.0, 7.0),
            // Off to one side, clear of the ground between the depot and the
            // kitchen: research is the one job with no traffic of its own. Its
            // bearing is also the one the home tree constrains - swung further
            // round, the research desk stands underneath the tree the player
            // hand-harvests from, inside the drag target.
            SupportRole::Technologist => Vec2::new(-6.2, 5.0),
        };
        self.town_centre + offset
    }

    /// Where one member of a role's fan stands, in metres, placed `offset`
    /// board texels from the station *on the screen* (see
    /// `support::slot_offset`).
    ///
    /// A fan is a thing the player sees, so it is laid out in screen terms and
    /// taken back to the ground through `unproject`. It used to be a step along
    /// the ground diagonal `(1, 0.5)`, which projects to about six texels
    /// across and nine down per metre: the monkeys stood in a queue receding
    /// from the viewer, each mostly hidden behind the one in front, and no
    /// spacing along that line could separate them.
    pub(crate) fn support_point(self, role: SupportRole, offset: Vec2) -> Vec2 {
        self.support_stand(role) + isometric::unproject(offset)
    }

    /// Snap a distance *from the board's origin* to the world's texel grid, so
    /// pixel-art detail does not crawl at the low speeds a walking monkey moves
    /// at.
    ///
    /// A distance, never an absolute screen coordinate: see
    /// [`Self::board_snapped`] for why the difference is the whole point.
    ///
    /// Private, and that is the point. Every caller outside this module wants
    /// `board_snapped`; the one that reached for this instead spent a release
    /// quantising an absolute screen position, and a second one was still doing
    /// it after the first was fixed. There is no correct use of this from
    /// another module, so there is no way to reach it from one.
    fn snap(self, offset: f32) -> f32 {
        (offset / self.zoom).round() * self.zoom
    }

    /// Where a sprite standing at `world` draws, lifted `lift` screen pixels so
    /// its feet land on the ground, and quantised to the world's texel grid.
    ///
    /// The quantising is applied to the *offset from the origin*, not to the
    /// screen position. Rounding the sum instead measures the grid from the
    /// corner of the window rather than from the board, and `origin` is not a
    /// multiple of `zoom` — so every actor is biased by a fraction of a texel
    /// that changes as the camera moves. With a fixed board that bias is a
    /// constant nobody can see; the moment the player can pan, the terrain
    /// slides smoothly while the entire cast jumps in zoom-sized steps against
    /// it, which is precisely what snapping exists to prevent.
    pub(crate) fn board_snapped(self, world: Vec2, lift: f32) -> Vec2 {
        let offset = isometric::project(world) * self.zoom + Vec2::new(0.0, lift);
        self.origin + Vec2::new(self.snap(offset.x), self.snap(offset.y))
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
pub(crate) struct DeliveryQueue {
    pub(crate) entries: Vec<Delivery>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Delivery {
    /// Always positive. [`DeliveryKind::Snack`] is the one that subtracts.
    pub(crate) amount: f64,
    pub(crate) kind: DeliveryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeliveryKind {
    /// The player dragged a banana to the stall.
    Manual,
    /// A worker unloaded its payload.
    Worker,
    /// A cart unloaded its payload - two hundred bananas at once.
    Cart,
    /// A worker ate its wage, out of the delivery it just made.
    Snack,
    /// A support monkey ate its wage, out of whatever surplus there was.
    Wage,
}

impl DeliveryKind {
    pub(crate) fn is_income(self) -> bool {
        !matches!(self, DeliveryKind::Snack | DeliveryKind::Wage)
    }
}

/// One delivery that has just moved the treasury, in the order it did so.
///
/// The simulation's only output besides its resources. Presentation turns each
/// one into a pulse and a rising "+n"; a headless test reads them back as the
/// ledger of what the economy did and when. Written in `FixedUpdate`, so a
/// frame that ran several ticks carries several of these.
/// The run went back to nothing. Presentation clears whatever was still in
/// the air over the old one.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Restarted;

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settled {
    pub(crate) delivery: Delivery,
    /// The treasury immediately after this delivery was applied.
    pub(crate) balance: f64,
}

/// Counted rather than a flag, so clicking twice between two fixed ticks does
/// not silently drop a purchase.
#[derive(Resource, Debug, Default)]
pub(crate) struct HireRequests(pub(crate) Vec<UnitKind>);

#[derive(Resource, Debug, Default)]
pub(crate) struct RestartRequest(pub(crate) bool);

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

/// One touch gesture shared by the UI systems.
///
/// Bevy marks a button pressed at touch-start. Mobile scrolling needs to wait
/// until intent is known, so touch buttons are dispatched manually on release
/// while mouse buttons continue through `Interaction`.
#[derive(Resource, Debug, Default)]
pub(crate) struct UiTouchGesture {
    pub(crate) id: Option<u64>,
    pub(crate) start: Vec2,
    pub(crate) position: Vec2,
    pub(crate) previous: Vec2,
    pub(crate) just_pressed: bool,
    pub(crate) just_released: bool,
    pub(crate) canceled: bool,
    pub(crate) consumed: bool,
    pub(crate) started_in_ui: bool,
    candidate_action: Option<ButtonAction>,
}

impl UiTouchGesture {
    fn is_touch_frame(&self) -> bool {
        self.id.is_some() || self.just_pressed || self.just_released || self.canceled
    }
}

/// Every UI surface a pointer can land on instead of the board.
type UiRegions<'w, 's> = Query<
    'w,
    's,
    (&'static ComputedNode, &'static UiGlobalTransform),
    Or<(
        With<hud::HudRoot>,
        With<hud::StoreRoot>,
        With<hud::InfoPanel>,
    )>,
>;

#[derive(bevy::ecs::system::SystemParam)]
struct UiInput<'w, 's> {
    touch: Res<'w, UiTouchGesture>,
    regions: UiRegions<'w, 's>,
}

fn track_ui_touch(
    touches: Res<Touches>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui_regions: UiRegions,
    mut gesture: ResMut<UiTouchGesture>,
) {
    gesture.just_pressed = false;
    gesture.just_released = false;
    gesture.canceled = false;

    if gesture.id.is_none()
        && let Some(touch) = touches.iter_just_pressed().next()
    {
        let position =
            pointer_in_camera_space(touch.position(), window.resolution.base_scale_factor());
        gesture.id = Some(touch.id());
        gesture.start = position;
        gesture.position = position;
        gesture.previous = position;
        gesture.consumed = false;
        gesture.started_in_ui = ui_regions
            .iter()
            .any(|(node, transform)| ui_node_contains(node, transform, position));
        gesture.candidate_action = None;
        gesture.just_pressed = true;
    }

    let Some(id) = gesture.id else { return };
    if let Some(touch) = touches.get_pressed(id) {
        gesture.previous = gesture.position;
        gesture.position =
            pointer_in_camera_space(touch.position(), window.resolution.base_scale_factor());
    }
    if let Some(touch) = touches.iter_just_released().find(|touch| touch.id() == id) {
        gesture.previous = gesture.position;
        gesture.position =
            pointer_in_camera_space(touch.position(), window.resolution.base_scale_factor());
        gesture.id = None;
        gesture.just_released = true;
    } else if touches.iter_just_canceled().any(|touch| touch.id() == id) {
        gesture.id = None;
        gesture.canceled = true;
        gesture.consumed = true;
        gesture.candidate_action = None;
    }
}

fn ui_node_contains(node: &ComputedNode, transform: &UiGlobalTransform, position: Vec2) -> bool {
    let center = transform.translation * node.inverse_scale_factor;
    let size = node.size() * node.inverse_scale_factor;
    contains_inclusive(Rect::from_center_size(center, size), position)
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

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    asset_server: Res<AssetServer>,
    mut atlas_layouts: ResMut<Assets<bevy::image::TextureAtlasLayout>>,
    mut images: ResMut<Assets<Image>>,
    launch: Res<Launch>,
    village: Res<map::Village>,
) {
    commands.spawn((Camera2d, MainCamera));

    let art = art::Art::load(&asset_server, &mut atlas_layouts, &mut images);
    isometric::spawn_world(&mut commands, &mut meshes, &mut materials, &art, &village);
    commands.insert_resource(art.clone());

    // The depot's own diamond, laid on the ground under the crowd standing in
    // it. It was an axis-aligned square of light hung a metre up, sized from
    // the chrome: on a phone the single most dominant object on the board, and
    // the only thing on it off the 2:1 grid. Built round the origin and placed
    // by its transform, so it pans and zooms with the ground it marks.
    let [top, right, bottom, left] =
        Footprint::new(Vec2::ZERO, isometric::DEPOT_REACH_METRES).diamond();
    commands.spawn((
        Mesh2d(meshes.add(Rhombus::new(right.x - left.x, top.y - bottom.y))),
        MeshMaterial2d(materials.add(ColorMaterial {
            color: GLOW.with_alpha(0.0),
            alpha_mode: bevy::sprite_render::AlphaMode2d::Blend,
            ..default()
        })),
        Transform::default(),
        DepositGlow,
        LayoutElement::DepositGlow,
    ));

    // The harvest end's own diamond, faint, until the player has made one
    // delivery by hand: see `update_feedback`. Built at a unit half and scaled
    // to whatever the target is at the current zoom.
    let [top, right, bottom, left] = Footprint::new(Vec2::ZERO, 1.0).diamond();
    commands.spawn((
        Mesh2d(meshes.add(Rhombus::new(right.x - left.x, top.y - bottom.y))),
        MeshMaterial2d(materials.add(ColorMaterial {
            color: GLOW.with_alpha(0.0),
            alpha_mode: bevy::sprite_render::AlphaMode2d::Blend,
            ..default()
        })),
        Transform::default(),
        HarvestHint,
        LayoutElement::HarvestHint,
    ));

    // The drawn bunch, glinting where it lies: the one thing on the board
    // that moves for no reason but to be noticed, which is what a pickup is.
    // Anchored where the artist put its ground, so it lies *on* the ground at
    // its feet, in its own drawn shadow.
    commands.spawn((
        art.bunch(art::Bunch::Idle, 0),
        art::BUNCH.anchor(),
        Transform::default(),
        Banana,
        LayoutElement::Banana,
        BananaAnimation {
            clip: art::Bunch::Idle,
            head: art::Playhead::default(),
        },
    ));
    // And its shadow, which stays on the ground while the banana is carried:
    // under the pointer, marking the exact ground the drop is tested against.
    commands.spawn((
        art.shadow(art::SHADOW_TEXELS, isometric::SHADOW_COLOUR),
        Transform::default(),
        BananaShadow,
        LayoutElement::BananaShadow,
    ));

    spawn_place_label(
        &mut commands,
        "JUNGLE",
        (HarvestLabel, LayoutElement::HarvestLabel),
    );
    // "DEPOT", not "VILLAGE": the label names the drop target, and the village
    // is the terrain it stands on. A new player reads this as an instruction
    // about where the banana goes, which is what it is.
    spawn_place_label(
        &mut commands,
        "DEPOT",
        (DepositLabel, LayoutElement::DepositLabel),
    );
    // And the freight's own yard, which the Cart's research puts up (D31). The
    // board already names the two places a banana can be, and an unlabelled
    // second set of blue bins appearing on the grass is a duplicate rather than
    // a reward: this is what says the new one is the carts'. Spawned with the
    // rest and hidden until the bins are there, like every other reconciled
    // piece of the scene.
    spawn_place_label(
        &mut commands,
        "CART YARD",
        (
            CartYardLabel,
            LayoutElement::CartYardLabel,
            Visibility::Hidden,
        ),
    );

    // The stage view is the board and its actors with nothing in front of
    // them: a playtest of "one monkey walks to the grove" does not need the
    // store, the banner or the menu drawn over it. Every HUD system reaches
    // its nodes through `Single`, so with none spawned they simply skip.
    if launch.view == View::Full {
        hud::setup_hud(&mut commands, &asset_server);
        hud::setup_menu(&mut commands, &asset_server);
    }
}

/// Where the DEPOT sign stands relative to the delivery point, in metres, and
/// how high: towards the viewer's right of the bins, at their lip.
const DEPOT_SIGN_OFFSET: Vec2 = Vec2::new(1.2, -1.2);
const DEPOT_SIGN_RAISE: f32 = 2.4;

/// And where the CART YARD sign stands over the carts' own bins. Its own pair
/// because that set is drawn at the shared scale rather than the treehouse's
/// (see `art::CART_BINS`), so it is twice as tall and twice as wide as the
/// depot's: the same sign placed by the same numbers would sit on the boxes
/// rather than over them.
const CART_SIGN_OFFSET: Vec2 = Vec2::new(2.4, -2.4);
const CART_SIGN_RAISE: f32 = 5.0;

/// Where delivery floaters rise from, relative to the delivery point, in
/// metres: beside the bins, to the right, rather than through the sign.
const FLOATER_ORIGIN: Vec2 = Vec2::new(2.0, -2.0);

/// The type size a place label is drawn at.
const PLACE_LABEL_FONT: f32 = 16.0;

/// Where a place label sits in the overlay.
///
/// Under the delivery floaters, which land on exactly the spot the depot label
/// names and matter more when they do: a floater is feedback about something
/// that just happened, and the label is a standing sign that will still be
/// there afterwards.
const PLACE_LABEL_Z: f32 = isometric::OVERLAY_Z - 1.0;

/// Name a place on the board, on a plate that survives a crowd standing on it.
///
/// Cream on a dark plate rather than ink on grass. At sixty workers the depot
/// label was being cut into pieces by the monkey outlines running through the
/// letterforms, and a delivery floater sat on top of the remains — the two
/// pieces of text that exist to tell a new player where bananas go, illegible
/// at exactly the moment there is most going on.
///
/// The plate is the pattern the role badges already use, and those are the one
/// piece of text that survives a crowd intact today, so this is borrowing a
/// solution rather than inventing one.
fn spawn_place_label(commands: &mut Commands, name: &str, markers: impl Bundle) {
    // Sized from the string: the font is fixed-width at this size, so a plate
    // measured per character fits every label without laying the text out.
    const PER_CHARACTER: f32 = 9.6;
    const PADDING: Vec2 = Vec2::new(14.0, 7.0);
    let plate = Vec2::new(name.len() as f32 * PER_CHARACTER, PLACE_LABEL_FONT) + PADDING * 2.0;

    commands
        .spawn((
            Text2d::new(name.to_owned()),
            TextFont::from_font_size(PLACE_LABEL_FONT),
            TextColor(CREAM),
            Transform::from_xyz(0.0, 0.0, 2.0),
            markers,
        ))
        .with_child((
            Sprite::from_color(BROWN, plate),
            // Behind its own text, and with it above the board.
            Transform::from_xyz(0.0, 0.0, -0.01),
        ));
}

fn refresh_layout(
    window: Single<&Window, With<PrimaryWindow>>,
    launch: Res<Launch>,
    camera: Res<BoardCamera>,
    village: Res<map::Village>,
    mut layout: ResMut<SceneLayout>,
) {
    let next = SceneLayout::for_map(
        Vec2::new(window.width(), window.height()),
        launch.view,
        &village,
        *camera,
    );
    // The whole layout, not just the viewport it was derived from. The view is
    // the second input now, and a stage launch at exactly the default 1280x720
    // resolution produced an identical viewport - so a viewport-only guard left
    // the board sitting in the HUD's reserve with three quarters of the window
    // empty, which is precisely what the stage view exists to avoid.
    //
    // With a camera the player drives, this differs on every frame of a drag,
    // so the guard now catches only the still frames - which is most of them,
    // and it costs fifteen floats to check. What it is really protecting is
    // downstream change detection, not the arithmetic.
    if next != *layout {
        *layout = next;
    }
}

#[allow(clippy::type_complexity)]
fn apply_layout(
    layout: Res<SceneLayout>,
    mut world: Single<&mut Transform, With<isometric::WorldRoot>>,
    mut elements: Query<
        (&LayoutElement, Option<&mut Sprite>, &mut Transform),
        Without<isometric::WorldRoot>,
    >,
) {
    // The baked terrain is one child of this root, so moving the root is the
    // whole of panning and zooming - and it carries exactly the two numbers
    // `SceneLayout::board` uses, so meshes and actors cannot drift apart.
    let root = layout.world_root();
    if world.translation != root.translation {
        world.translation = root.translation;
    }
    if world.scale != root.scale {
        world.scale = root.scale;
    }

    // Board texels to screen pixels, and a unit z scale so a depth offset
    // stays a depth offset.
    let board_scale = Vec3::new(layout.world_scale(), layout.world_scale(), 1.0);
    for (element, _, mut transform) in &mut elements {
        match element {
            LayoutElement::HarvestHint => {
                // Built at a unit half; scaled to whatever the target is at
                // this zoom, so the hint is exactly what grabs.
                let half = layout.harvest_target().half;
                transform.translation = layout.board(layout.home_tree()).extend(isometric::GLOW_Z);
                transform.scale = Vec3::new(board_scale.x * half, board_scale.y * half, 1.0);
            }
            LayoutElement::DepositGlow => {
                transform.translation =
                    layout.board(layout.town_centre()).extend(isometric::GLOW_Z);
                transform.scale = board_scale;
            }
            LayoutElement::Banana => {
                // Placed by `place_held_banana`, after input. Drawn at the
                // board's own scale, held or not, like every other piece of
                // art: a bunch in the hand is the size of the bunch it was
                // lying on the ground.
                transform.scale = board_scale;
            }
            LayoutElement::BananaShadow => {
                transform.scale = board_scale;
            }
            LayoutElement::HarvestLabel => {
                transform.translation = layout
                    .board_raised(layout.grove(), 6.0)
                    .extend(PLACE_LABEL_Z);
                transform.scale = Vec3::splat(layout.world_scale().clamp(0.8, 1.35));
            }
            LayoutElement::CartYardLabel => {
                // Over its own bins, at the same height above them the DEPOT
                // sign sits above the delivery point's, so the two read as the
                // same kind of sign rather than as two different things.
                transform.translation = layout
                    .board_raised(layout.cart_bins() + CART_SIGN_OFFSET, CART_SIGN_RAISE)
                    .extend(PLACE_LABEL_Z);
                transform.scale = Vec3::splat(layout.world_scale().clamp(0.8, 1.35));
            }
            LayoutElement::DepositLabel => {
                // Just clear of a monkey's head, so it reads as a sign *on* the
                // depot rather than as a word floating in the sky. At six
                // metres it hung a hundred pixels above the pad it names, which
                // is what let it sit over bare grass for so long without
                // anyone noticing there was nothing under it.
                // On the bins it names, just above their lip, and off the
                // porch and door above them - the drawing's best detail, which
                // a sign centred on the delivery point covered.
                transform.translation = layout
                    .board_raised(layout.town_centre() + DEPOT_SIGN_OFFSET, DEPOT_SIGN_RAISE)
                    .extend(PLACE_LABEL_Z);
                transform.scale = Vec3::splat(layout.world_scale().clamp(0.8, 1.35));
            }
        }
    }
}

/// Scale the clock for playtests and browser tests.
///
/// A worker's cycle is 50 *simulated* seconds, and a playtest that watches a
/// delivery therefore takes 50 real ones. Scaling `Time<Virtual>` is the honest
/// way to change that: `Time<Fixed>` is driven by virtual time, so
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
/// The speed itself only ever reaches [`Launch`] under the `test-hooks`
/// feature, so the released build has no such switch. See `launch.rs`.
///
/// `max_delta` is deliberately left alone. It clamps the **raw** delta before
/// the scale is applied (`bevy_time::virt`), not after, so a 60 Hz frame's
/// 16.7 ms is already 15x under the 250 ms default and the clamp never fires at
/// any scale the launcher allows. Raising it "to match" would only widen the
/// spiral-of-death guard: at 60x, a 30-second stall - a breakpoint, a
/// backgrounded tab, a lost wgpu device - would become 1800 virtual seconds and
/// 36,000 fixed ticks in a single frame.
fn apply_launch_speed(launch: Res<Launch>, mut virtual_time: ResMut<Time<Virtual>>) {
    let Some(scale) = launch.speed else {
        return;
    };
    virtual_time.set_relative_speed_f64(scale);
    bevy::log::info!("launch: simulation running at {scale}x");
}

// ────────────────────────────────────────────────── simulation, at 20 Hz

/// Stage 1. [`Treasury`] is written here and in [`settle`], both inside
/// `FixedUpdate`; no presentation system may write it.
#[allow(clippy::too_many_arguments)]
fn apply_purchases(
    mut requests: ResMut<HireRequests>,
    mut treasury: ResMut<Treasury>,
    mut workforce: ResMut<Workforce>,
    mut carts: ResMut<Carts>,
    mut staff: ResMut<Staff>,
    mut dirty: ResMut<PersistenceDirty>,
    research: Res<Research>,
    fed: Res<FedStaff>,
    committed: Res<Committed>,
    multipliers: Res<Multipliers>,
) {
    for kind in std::mem::take(&mut requests.0) {
        let plan = plan_hire(
            kind,
            EconomyState {
                workforce: *workforce,
                carts: *carts,
                staff: *staff,
                fed: *fed,
                research: *research,
                treasury: *treasury,
                committed: committed.0,
                multipliers: *multipliers,
            },
        );
        // `continue`, not `break`: the requests are heterogeneous now, so an
        // unaffordable chef says nothing about the worker queued behind it.
        if !plan.affordable {
            continue;
        }
        treasury.charge(plan.cost);
        match kind {
            UnitKind::Worker => workforce.hire(),
            UnitKind::Support(role) => staff.hire(role),
            UnitKind::Cart => {
                // Any crew the player was short of is hired with the cart and
                // boards immediately - they are standing at the stall already.
                // The rest of the berths fill from the pool as workers finish
                // their trips, which is what `board_carts` does.
                let hires = cart_crew_shortfall(*workforce, *carts);
                for _ in 0..hires {
                    workforce.hire();
                }
                carts.buy(hires);
            }
        }
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
    mut carts: ResMut<Carts>,
    mut staff: ResMut<Staff>,
    mut research: ResMut<Research>,
    mut queue: ResMut<DeliveryQueue>,
    mut requests: ResMut<HireRequests>,
    mut dirty: ResMut<PersistenceDirty>,
    // One parameter, because `apply_restart` is at Bevy's sixteen-parameter
    // cap and these three are the same concern: where freshly spawned avatars
    // are placed, and which indices they get.
    mut placement: (
        ResMut<worker::RestoreWorkers>,
        ResMut<worker::RestoreCarts>,
        ResMut<worker::NextLane>,
    ),
    workers: Query<Entity, With<Worker>>,
    support: Query<Entity, With<SupportUnit>>,
    vehicles: Query<Entity, With<Cart>>,
    mut restarted: MessageWriter<Restarted>,
) {
    if !std::mem::take(&mut request.0) {
        return;
    }

    restart_run(
        &mut treasury,
        &mut workforce,
        &mut carts,
        &mut staff,
        &mut research,
    );
    let (restored, restored_carts, next_lane) = &mut placement;
    restored.clear();
    restored_carts.clear();
    next_lane.restart();
    queue.entries.clear();
    requests.0.clear();
    for entity in workers.iter().chain(&support).chain(&vehicles) {
        commands.entity(entity).despawn();
    }
    restarted.write(Restarted);
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
/// Every harvester on foot: the pool, and whether it is a restored placement.
type PoolQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut HarvestCycle,
        &'static CycleSpec,
        Option<&'static RestoredCycle>,
    ),
    (With<Worker>, Without<Cart>),
>;

/// Every cart that is actually running. `Without` on both sides is what makes
/// Bevy accept two mutable `HarvestCycle` queries in one system: it cannot
/// prove `With<Worker>` and `With<Cart>` are disjoint on its own.
type CartQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut HarvestCycle,
        &'static CycleSpec,
        Option<&'static RestoredCycle>,
    ),
    (With<Cart>, Without<Worker>, Without<worker::Boarding>),
>;

#[allow(clippy::too_many_arguments)]
fn advance_cycles(
    time: Res<Time<Fixed>>,
    multipliers: Res<Multipliers>,
    fed: Res<FedStaff>,
    mut research: ResMut<Research>,
    treasury: Res<Treasury>,
    committed: Res<Committed>,
    mut queue: ResMut<DeliveryQueue>,
    mut workers: PoolQuery,
    mut carts: CartQuery,
    mut support: Query<(&SupportRole, &mut SupportCycle)>,
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
    // Less every meal a harvester has already reserved. Without this the
    // reservation holds for exactly one tick: `settle` credits the whole
    // payload to the treasury, and the next tick re-derives the larder from
    // that balance - which now contains the very meal that was set aside. A
    // worker holds its earmark for 50 ticks and a cart for 758, so support was
    // eating reserved bananas ~98% of the time it mattered, `Treasury::charge`
    // was overdrawing, and release builds were silently forgiving the meal.
    //
    // `Committed` is one tick stale, exactly as `apply_purchases` reads it.
    // That staleness is conservative in the right direction: it can only be too
    // large, immediately after a delivery.
    let mut larder = (treasury.bananas() + pending - committed.0).max(0.0);

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

    // Carts run the same cycle with different constants, and are paid the same
    // way: their meal is reserved out of the two hundred bananas they have just
    // delivered. Only fully crewed carts are in this query - an empty box has a
    // picking rate of zero.
    for (entity, mut cycle, spec, restored) in &mut carts {
        // The same placement guard a restored worker gets: a cart dropped on
        // its return leg is drawn full of bananas it never picked, and that
        // load must not be credited. See `spawn_missing_carts`.
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
                kind: DeliveryKind::Cart,
            });
        }
        if output.eaten > 0.0 {
            queue.entries.push(Delivery {
                amount: output.eaten,
                kind: DeliveryKind::Snack,
            });
        }
    }

    // Research accrues from the technologists that were fed *last* tick, which
    // is what `FedStaff` holds when `recompute_multipliers` runs ahead of this
    // system. A technologist that starves this tick stops researching next one.
    if fed.technologists > 0 {
        research.credit(research_per_sec(*fed, *multipliers) * dt);
    }

    // Support staff eat last, and only out of what the harvesters did not
    // reserve for themselves. `larder` already excludes every earmarked meal,
    // so no amount of hiring can starve a monkey that has just delivered.
    //
    // Within support, the order is `SupportRole::FEEDING_ORDER` rather than
    // query order: Bevy's iteration order is stable within a run but changes
    // whenever an archetype moves, and an economy whose feeding priority
    // depended on that would be silently non-reproducible.
    for role in SupportRole::FEEDING_ORDER {
        for (unit, mut cycle) in &mut support {
            if *unit != role {
                continue;
            }
            let eaten = cycle.advance(dt, role.meal(), &mut larder);
            if eaten > 0.0 {
                queue.entries.push(Delivery {
                    amount: eaten,
                    kind: DeliveryKind::Wage,
                });
            }
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
    mut settled: MessageWriter<Settled>,
) {
    for delivery in queue.entries.drain(..) {
        match delivery.kind {
            DeliveryKind::Manual => {
                treasury.credit(delivery.amount);
                dirty.mark_immediate();
            }
            DeliveryKind::Worker | DeliveryKind::Cart => {
                treasury.credit(delivery.amount);
                // Not immediate: these arrive `W / 50` times a second, so the
                // immediate path would put the save write rate back on the
                // treadmill the throttle exists to stop, and would reset the
                // retry backoff every time.
                dirty.mark_pending();
            }
            DeliveryKind::Snack | DeliveryKind::Wage => {
                treasury.charge(delivery.amount);
                dirty.mark_pending();
            }
        }

        settled.write(Settled {
            delivery,
            balance: treasury.bananas(),
        });
    }
}

/// Every settlement becomes a pulse on the stall and a rising "+n". Reads the
/// message rather than sitting inside [`settle`], so the simulation can run
/// with no scene at all.
fn present_settlements(
    mut settled: MessageReader<Settled>,
    mut restarted: MessageReader<Restarted>,
    mut feedback: ResMut<Feedback>,
    mut commands: Commands,
    layout: Res<SceneLayout>,
    floaters: Query<Entity, With<Floater>>,
    mut first: ResMut<FirstHarvest>,
) {
    // Otherwise up to 0.9 s of "+5" keeps rising over a stall that just went
    // back to zero.
    if restarted.read().next().is_some() {
        for entity in &floaters {
            commands.entity(entity).despawn();
        }
    }
    for Settled { delivery, .. } in settled.read() {
        if delivery.kind == DeliveryKind::Manual {
            first.pending = false;
        }
        if delivery.kind.is_income() {
            feedback.success = Some(Timer::new(
                Duration::from_secs_f32(match delivery.kind {
                    DeliveryKind::Worker | DeliveryKind::Cart => DELIVERY_PULSE_SECONDS,
                    _ => SUCCESS_PULSE_SECONDS,
                }),
                TimerMode::Once,
            ));
        }
        spawn_floater(&mut commands, &layout, *delivery);
    }
}

/// Stage 4. Fed-ness is read off the world, never off the head count: an idle
/// chef shortens nobody's trip, so it must not appear in `M_speed`.
///
/// Placed *before* [`advance_cycles`] rather than after [`settle`], which is
/// where the fed flags are actually written. The one-tick lag that buys is
/// deliberate and costs 50 ms; the alternative has `advance_cycles` and
/// [`snapshot_economy`] disagreeing about the multipliers *within* one tick,
/// which would put the readout's projected rate at odds with the rate the
/// simulation just ran - exactly the drift I3' exists to prevent.
fn recompute_multipliers(
    support: Query<(&SupportRole, &SupportCycle)>,
    research: Res<Research>,
    mut fed: ResMut<FedStaff>,
    mut multipliers: ResMut<Multipliers>,
) {
    let mut counted = FedStaff::default();
    for (role, cycle) in &support {
        if !cycle.is_hungry() {
            counted.add(*role);
        }
    }
    fed.set_if_neq(counted);
    let next = multipliers_for(counted, *research);
    multipliers.set_if_neq(next);
}

/// Stage 7. Counted from the world rather than from the resources, because a
/// hired monkey is not necessarily a working one, and reporting a rate the
/// world is not producing is what the readout used to get wrong.
#[allow(clippy::too_many_arguments)]
fn snapshot_economy(
    workforce: Res<Workforce>,
    carts: Res<Carts>,
    staff: Res<Staff>,
    fed: Res<FedStaff>,
    multipliers: Res<Multipliers>,
    cycles: Query<&HarvestCycle>,
    mut committed: ResMut<Committed>,
    mut snapshot: ResMut<EconomySnapshot>,
) {
    // Summed from the world every tick rather than accumulated, so it cannot
    // drift out of step with the meals actually outstanding (I3').
    committed.0 = cycles.iter().map(|cycle| cycle.earmarked()).sum();
    *snapshot = EconomySnapshot::project(workforce.count(), *carts, *staff, *fed, *multipliers);
}

/// Three sources, three sizes, three colours - readable without reading. The
/// snack is the smallest and the dimmest on purpose: it is the cost of doing
/// business, not an event the player has to act on.
/// One of the dark copies drawn behind a floater to give it an edge.
#[derive(Component)]
struct FloaterOutline;

/// Where those copies sit, in texels around the glyph.
///
/// Four, not eight: on a 22-to-44 pixel glyph the diagonals add nothing a
/// player can see and cost four more text layouts per delivery.
/// The font size the offsets below are measured at.
const FLOATER_OUTLINE_AT: f32 = 22.0;

const FLOATER_OUTLINE: [Vec2; 4] = [
    Vec2::new(-2.0, 0.0),
    Vec2::new(2.0, 0.0),
    Vec2::new(0.0, -2.0),
    Vec2::new(0.0, 2.0),
];

/// How far a floater is nudged off the stall, in metres, so consecutive ones
/// do not stack.
const FLOATER_SPREAD_METRES: f32 = 2.2;

fn spawn_floater(commands: &mut Commands, layout: &SceneLayout, delivery: Delivery) {
    // Scattered around the stall rather than all launched from one point. At
    // the swarm scenario's rate a delivery lands about twice a second against a
    // floater that lives most of one, so two or three are on screen at any
    // moment - and stacked on the same pixel they overprint into mush, on the
    // one piece of feedback that tells the player the economy is working.
    //
    // Hashed from the delivery's own figures rather than from a counter, so it
    // needs no state and two identical deliveries in a row still separate:
    // the amount differs, or the kind does, or it is genuinely the same event.
    let spin = {
        let bits = delivery.amount.to_bits() ^ ((delivery.kind as u64) << 57);
        let mixed = (bits ^ (bits >> 29)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        // The top 24 bits over 2^24, exact in `f32` and inside `0.0..1.0`.
        ((mixed >> 40) as u32) as f32 / 16_777_216.0
    };
    let angle = spin * std::f32::consts::TAU;
    let anchor =
        layout.town_centre() + FLOATER_ORIGIN + Vec2::from_angle(angle) * FLOATER_SPREAD_METRES;
    let (label, size, colour) = match delivery.kind {
        DeliveryKind::Worker => (format!("+{:.0}", delivery.amount), 34.0, GOLD),
        // Bigger, because it is forty times the size and lands once every three
        // minutes. A cart delivery is the loudest event in the economy.
        DeliveryKind::Cart => (format!("+{:.0}", delivery.amount), 44.0, GOLD),
        DeliveryKind::Manual => (format!("+{:.0}", delivery.amount), 26.0, CREAM),
        // Both wages read the same, because to the player they are the same
        // event: a monkey being paid. The snack happens at the stall and the
        // support wage happens beside it, so they even share an origin.
        DeliveryKind::Snack | DeliveryKind::Wage => {
            (format!("-{:.1}", delivery.amount), 22.0, MUTED)
        }
    };
    // Every floater colour fails contrast against the ground it lands on, and
    // this is the game's primary reward feedback. Town floor is #A3C975 at
    // luminance 0.508; GOLD is 0.586, which is 1.14:1 - below the 3:1 floor for
    // large text before the alpha fade even starts. The palette was chosen
    // against the cream HUD, not against grass. An INK edge takes GOLD to
    // 8.4:1 and fixes all four colours at once, without repainting a palette
    // that is right everywhere else.
    commands
        .spawn((
            Text2d::new(label.clone()),
            TextFont::from_font_size(size),
            TextColor(colour),
            Transform::from_translation(layout.stall_glow_anchor().extend(isometric::OVERLAY_Z)),
            Floater {
                elapsed: 0.0,
                anchor,
            },
        ))
        .with_children(|floater| {
            // Proportional to the glyph, not a fixed two pixels: a cart's "+100"
            // is twice the height of a snack's "-1.5", and one edge width for
            // both leaves the big one looking smudged and the small one bare.
            let width = size / FLOATER_OUTLINE_AT;
            for offset in FLOATER_OUTLINE {
                floater.spawn((
                    FloaterOutline,
                    Text2d::new(label.clone()),
                    TextFont::from_font_size(size),
                    TextColor(INK),
                    // Behind its own glyph, and still above the whole board.
                    Transform::from_xyz(offset.x * width, offset.y * width, -0.01),
                ));
            }
        });
}

// ─────────────────────────────────────────────────────────────── input

#[allow(clippy::too_many_arguments)]
fn handle_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut interactions: Query<(&Interaction, &ButtonAction), Changed<Interaction>>,
    buttons: Query<(&ButtonAction, &ComputedNode, &UiGlobalTransform)>,
    mut ui_touch: ResMut<UiTouchGesture>,
    mut menu: ResMut<MenuState>,
    mut controller: ResMut<HarvestController>,
    mut pending: ResMut<PendingSettlement>,
    mut restart: ResMut<RestartRequest>,
    mut hire_requests: ResMut<HireRequests>,
    mut panels: MenuPanels,
    mut pointer_guard: ResMut<PointerGuard>,
    mut feedback: ResMut<Feedback>,
    time: Res<Time>,
    layout: Res<SceneLayout>,
    mut banana: Single<&mut Transform, With<Banana>>,
    mut recentre: ResMut<RecentreRequest>,
) {
    pointer_guard.suppress_hire_for =
        (pointer_guard.suppress_hire_for - time.delta_secs()).max(0.0);
    let mut requested = None;
    let mut pressed_actions = Vec::new();
    #[cfg(target_arch = "wasm32")]
    if *menu == MenuState::Open && keys.just_pressed(KeyCode::KeyL) {
        open_web_diagnostics();
    }
    if keys.just_pressed(KeyCode::Escape) && panels.info_open.0.is_some() {
        panels.info_open.0 = None;
    } else if keys.just_pressed(KeyCode::Escape) {
        requested = Some(match *menu {
            MenuState::Closed => MenuState::Open,
            MenuState::Open => MenuState::Closed,
            MenuState::ConfirmRestart => MenuState::Open,
        });
    }
    if *menu == MenuState::Closed && keys.just_pressed(KeyCode::ArrowLeft) {
        pressed_actions.push(ButtonAction::PreviousShopTab);
    }
    if *menu == MenuState::Closed && keys.just_pressed(KeyCode::ArrowRight) {
        pressed_actions.push(ButtonAction::NextShopTab);
    }

    if !ui_touch.is_touch_frame() && pointer_guard.suppress_mouse_for == 0.0 {
        for (interaction, action) in &mut interactions {
            if *interaction == Interaction::Pressed {
                pressed_actions.push(*action);
            }
        }
    }

    if ui_touch.just_pressed {
        ui_touch.candidate_action = buttons
            .iter()
            .find(|(action, node, transform)| {
                action.active_in(*menu) && ui_node_contains(node, transform, ui_touch.start)
            })
            .map(|(action, ..)| *action);
    }

    if ui_touch.just_released {
        const TAP_SLOP: f32 = 10.0;
        if ui_touch.position.distance(ui_touch.start) > TAP_SLOP {
            ui_touch.consumed = true;
        }
        if !ui_touch.consumed
            && let Some(candidate) = ui_touch.candidate_action
            && buttons.iter().any(|(action, node, transform)| {
                *action == candidate
                    && action.active_in(*menu)
                    && ui_node_contains(node, transform, ui_touch.position)
            })
            && !pressed_actions.contains(&candidate)
        {
            pressed_actions.push(candidate);
        }
        ui_touch.candidate_action = None;
    }

    for action in pressed_actions {
        match action {
            ButtonAction::OpenMenu if *menu == MenuState::Closed => {
                requested = Some(MenuState::Open);
            }
            ButtonAction::Recentre if *menu == MenuState::Closed => {
                recentre.0 = true;
            }
            ButtonAction::Hire(kind)
                if *menu == MenuState::Closed
                    && panels.active_tab.0 == hud::ShopTab::Monkeys
                    && panels.info_open.0.is_none()
                    && pointer_guard.suppress_hire_for == 0.0 =>
            {
                hire_requests.0.push(kind);
                pointer_guard.suppress_hire_for = HIRE_DEBOUNCE_SECONDS;
            }
            ButtonAction::PreviousShopTab if *menu == MenuState::Closed => {
                panels.active_tab.0 = panels.active_tab.0.previous();
                panels.info_open.0 = None;
            }
            ButtonAction::NextShopTab if *menu == MenuState::Closed => {
                panels.active_tab.0 = panels.active_tab.0.next();
                panels.info_open.0 = None;
            }
            ButtonAction::Info(unit)
                if *menu == MenuState::Closed && panels.active_tab.0 == hud::ShopTab::Monkeys =>
            {
                panels.info_open.0 = if panels.info_open.0 == Some(unit) {
                    None
                } else {
                    Some(unit)
                };
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
                banana.translation = layout.banana_home().extend(isometric::OVERLAY_Z);
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
            banana.translation = layout.banana_home().extend(isometric::OVERLAY_Z);
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
    ui_input: UiInput,
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
            banana.translation = layout.banana_home().extend(isometric::OVERLAY_Z);
        }
        return;
    }

    if *menu != MenuState::Closed {
        return;
    }

    if keys.just_pressed(KeyCode::KeyB) {
        hire_requests.0.push(UnitKind::Worker);
    }

    let interaction = controller.interaction;
    match interaction {
        HarvestInteraction::Idle => {
            let mut touch_start = None;
            for touch in touches.iter_just_pressed() {
                if ui_input.touch.id == Some(touch.id()) && ui_input.touch.started_in_ui {
                    continue;
                }
                let raw = touch.position();
                let camera_position =
                    pointer_in_camera_space(raw, window.resolution.base_scale_factor());
                let world = screen_to_world(camera_position, &camera);
                let accepted = world.is_some_and(|position| layout.on_harvest(position));
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
                    layout.harvest_bounds().min.x,
                    layout.harvest_bounds().min.y,
                    layout.harvest_bounds().max.x,
                    layout.harvest_bounds().max.y,
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
                banana.translation = layout.held_banana(position).extend(isometric::OVERLAY_Z);
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
                let started_in_ui = raw.is_some_and(|position| {
                    ui_input
                        .regions
                        .iter()
                        .any(|(node, transform)| ui_node_contains(node, transform, position))
                });
                let camera_position = raw.map(|position| {
                    pointer_in_camera_space(position, window.resolution.base_scale_factor())
                });
                let world = camera_position.and_then(|position| screen_to_world(position, &camera));
                let in_harvest = world.is_some_and(|position| layout.on_harvest(position));
                let accepted =
                    pointer_guard.suppress_mouse_for == 0.0 && in_harvest && !started_in_ui;
                diagnostic_log!(
                    frame_count,
                    "mouse_start",
                    Some(PointerId::Mouse);
                    "raw_valid={} raw=({:.2},{:.2}) camera=({:.2},{:.2}) world_valid={} world=({:.2},{:.2}) in_harvest={} started_in_ui={} suppression_seconds={:.3} accepted={}",
                    raw.is_some(),
                    raw.map_or(f32::NAN, |position| position.x),
                    raw.map_or(f32::NAN, |position| position.y),
                    camera_position.map_or(f32::NAN, |position| position.x),
                    camera_position.map_or(f32::NAN, |position| position.y),
                    world.is_some(),
                    world.map_or(f32::NAN, |position| position.x),
                    world.map_or(f32::NAN, |position| position.y),
                    in_harvest,
                    started_in_ui,
                    pointer_guard.suppress_mouse_for,
                    accepted,
                );
                if accepted && let (Some(raw), Some(position)) = (raw, world) {
                    controller.interaction = HarvestInteraction::Dragging {
                        pointer: PointerId::Mouse,
                        position,
                    };
                    diagnostic_trace.begin(PointerId::Mouse, raw);
                    banana.translation = layout.held_banana(position).extend(isometric::OVERLAY_Z);
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
                banana.translation = layout.banana_home().extend(isometric::OVERLAY_Z);
            } else if let Some(touch) = touches.get_released(id) {
                let raw = touch.position();
                let camera_position =
                    pointer_in_camera_space(raw, window.resolution.base_scale_factor());
                let position = screen_to_world(camera_position, &camera);
                let in_deposit = position.is_some_and(|position| layout.on_deposit(position));
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
                    layout.deposit_bounds().min.x,
                    layout.deposit_bounds().min.y,
                    layout.deposit_bounds().max.x,
                    layout.deposit_bounds().max.y,
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
                    banana.translation = layout.held_banana(position).extend(isometric::OVERLAY_Z);
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
                let in_deposit = position.is_some_and(|position| layout.on_deposit(position));
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
                    banana.translation = layout.held_banana(position).extend(isometric::OVERLAY_Z);
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
                banana.translation = layout.banana_home().extend(isometric::OVERLAY_Z);
            }
        }
        HarvestInteraction::KeyboardHarvest { .. } => {}
    }
}

/// The board gesture in flight, and the pointers it is allowed to use.
///
/// Kept separate from [`HarvestController`] because the two compete for the
/// same fingers and the tie-break has to be stated somewhere: harvest gets
/// right of first refusal on a pointer, and the camera takes what is left.
#[derive(Resource, Debug, Default)]
struct CameraGesture {
    /// Touches that began on open ground — outside every HUD surface, and not
    /// claimed by a harvest drag — in the order they landed.
    ///
    /// Eligibility is decided **once, at touch-down**, and never revisited. A
    /// finger that starts on the store panel and slides onto the grass is still
    /// scrolling the store, and a finger that starts on the banana is still
    /// harvesting even after it leaves the node's hit box.
    on_board: Vec<u64>,
    motion: CameraMotion,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum CameraMotion {
    #[default]
    Idle,
    /// One pointer dragging the ground. `last` is where it was, in camera
    /// space, so a pan is a delta rather than an absolute grab — which is what
    /// keeps it correct when the finger picks up a new metre after a zoom.
    Pan { pointer: PointerId, last: Vec2 },
    /// Two touches, and the distance between them last frame.
    Pinch { a: u64, b: u64, span: f32 },
}

/// A window position as a point in the space `SceneLayout` works in.
///
/// Not [`pointer_in_camera_space`], despite the name of that one: it hands back
/// the raw window position because what consumes it is Bevy's
/// `viewport_to_world_2d`, which wants viewport coordinates. The board's own
/// geometry is centred with y running *up*, so a pan fed raw window coordinates
/// drags the ground the right way horizontally and the wrong way vertically —
/// which is exactly what it did until this existed.
fn window_to_camera(window: &Window, raw: Vec2) -> Vec2 {
    Vec2::new(raw.x - window.width() * 0.5, window.height() * 0.5 - raw.y)
}

/// How fast the keyboard pans the board, in metres of ground per second.
///
/// Desktop only, and it exists for two reasons: `./play` is a keyboard
/// playtest, and a drag is the one gesture that cannot be exercised without a
/// pointing device. A monkey walks at 3 m/s, so this is a brisk jog — fast
/// enough to cross the field in a few seconds, which is what makes it usable
/// for looking at something rather than for travelling.
///
/// Approximate by up to a factor of root two, because the keys pan along the
/// *screen* axes while the fold stretches a metre differently along each ground
/// axis. That is the right trade: the board should move the way the keys point,
/// not the way the ground happens to run.
const KEY_PAN_METRES: f32 = 24.0;

/// A wheel notch, in whole zoom steps.
const WHEEL_ZOOM_STEP: f32 = 1.0;

/// Pan, pinch and zoom the board.
///
/// Runs **after** `handle_harvest_input`, and the order is the design rather
/// than an accident. Manual harvest is a drag that starts on a banana node, and
/// the camera is a drag that starts anywhere else; the only way to tell them
/// apart is to let harvest look first and have the camera take what it did not
/// want. That is also what keeps the drag-to-harvest of the next increment
/// possible: adding a node adds a place harvest claims, and the camera gives it
/// up without knowing the node exists.
///
/// A pinch needs *two* unclaimed touches for the same reason. Putting a second
/// finger down mid-harvest must not tear the banana out of the player's hand.
#[allow(clippy::too_many_arguments)]
fn handle_camera_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    window: Single<&Window, With<PrimaryWindow>>,
    menu: Res<MenuState>,
    controller: Res<HarvestController>,
    pointer_guard: Res<PointerGuard>,
    ui_input: UiInput,
    layout: Res<SceneLayout>,
    village: Res<map::Village>,
    mut gesture: ResMut<CameraGesture>,
    mut board: ResMut<BoardCamera>,
    mut recentre: ResMut<RecentreRequest>,
) {
    let centre = layout.scene_center();
    let mut next = *board;

    // Whatever else happens this frame, a settling zoom keeps settling: the
    // fingers that started it have already left the glass.
    next.ease(centre, centre, time.delta_secs());

    let blocked = *menu != MenuState::Closed || web_diagnostics_panel_open();
    if blocked {
        gesture.on_board.clear();
        gesture.motion = CameraMotion::Idle;
    } else {
        // A pointer harvest owns is not the camera's, on the frame it is
        // claimed or on any frame after: `handle_harvest_input` runs first, so
        // by now the claim is already in the controller.
        let claimed = match controller.interaction {
            HarvestInteraction::Dragging { pointer, .. } => Some(pointer),
            _ => None,
        };
        let in_ui = |raw: Vec2| {
            ui_input
                .regions
                .iter()
                .any(|(node, transform)| ui_node_contains(node, transform, raw))
        };

        gesture
            .on_board
            .retain(|id| touches.get_pressed(*id).is_some());
        for touch in touches.iter_just_pressed() {
            let id = touch.id();
            if claimed == Some(PointerId::Touch(id)) || in_ui(touch.position()) {
                continue;
            }
            if !gesture.on_board.contains(&id) {
                gesture.on_board.push(id);
            }
        }

        let at = |id: u64| {
            touches
                .get_pressed(id)
                .map(|touch| window_to_camera(&window, touch.position()))
        };

        gesture.motion = match (gesture.on_board.as_slice(), gesture.motion) {
            // Two fingers on the ground is always a pinch, however it started.
            ([a, b, ..], was) => {
                let (a, b) = (*a, *b);
                // The `else` should be unreachable, since `retain` has already
                // dropped every id that is no longer pressed. It falls back
                // rather than bailing out of the system, because a bail-out
                // would also skip the settle and the clamp below: a dropped
                // frame of easing is a worse bug than a dropped frame of pinch.
                match (at(a), at(b)) {
                    (Some(first), Some(second)) => {
                        let span = first.distance(second);
                        let middle = first.midpoint(second);
                        if let CameraMotion::Pinch {
                            a: was_a,
                            b: was_b,
                            span: before,
                        } = was
                            && (was_a, was_b) == (a, b)
                            && before > 1.0
                            && span > 1.0
                        {
                            // Scale by the ratio the fingers moved, and hold
                            // the ground between them: a pinch that only
                            // multiplies the zoom slides the world out from
                            // between the two fingers pinching it.
                            next.zoom_to(centre, middle, next.zoom * span / before);
                            next.resting_zoom = next.zoom;
                        }
                        CameraMotion::Pinch { a, b, span }
                    }
                    _ => {
                        next.settle();
                        CameraMotion::Idle
                    }
                }
            }
            ([id], _) => match at(*id) {
                Some(now) => {
                    let pointer = PointerId::Touch(*id);
                    match gesture.motion {
                        CameraMotion::Pan { pointer: was, last } if was == pointer => {
                            drag(&mut next, centre, now - last);
                        }
                        // Includes the frame a pinch drops back to one finger:
                        // the survivor re-grabs from where it is rather than
                        // from where the pinch's midpoint was.
                        _ => next.settle(),
                    }
                    CameraMotion::Pan { pointer, last: now }
                }
                None => CameraMotion::Idle,
            },
            ([], _) => {
                if matches!(gesture.motion, CameraMotion::Pinch { .. }) {
                    next.settle();
                }
                mouse_motion(
                    &mut next,
                    centre,
                    gesture.motion,
                    &window,
                    &mouse,
                    &pointer_guard,
                    claimed,
                    &in_ui,
                )
            }
        };

        // Desktop: the wheel steps whole zooms about the cursor, and the
        // keyboard walks the board. Neither is reachable on a phone, and both
        // are how a playtest without a touchscreen reaches the camera at all.
        // Only over the board. `hud::scroll_store` reads the same wheel from
        // its own cursor, and every reader gets every message — so without
        // this, a notch over the shop scrolls the shop *and* zooms the village
        // behind it.
        let raw_cursor = window.cursor_position().filter(|raw| !in_ui(*raw));
        let cursor = raw_cursor.map(|raw| window_to_camera(&window, raw));
        let over_board = raw_cursor.is_some();
        let mut notches: f32 = wheel
            .read()
            .map(|event| if over_board { event.y.signum() } else { 0.0 })
            .sum();
        // `-` and `=` are the wheel's keyboard spelling, for a playtest with no
        // pointing device - and `=` rather than `+` so it needs no shift.
        for (key, step) in [
            (KeyCode::Equal, 1.0),
            (KeyCode::NumpadAdd, 1.0),
            (KeyCode::Minus, -1.0),
            (KeyCode::NumpadSubtract, -1.0),
        ] {
            if keys.just_pressed(key) {
                notches += step;
            }
        }
        if notches != 0.0 {
            let wanted = (next.resting_zoom + notches * WHEEL_ZOOM_STEP)
                .round()
                .clamp(BoardCamera::MIN_ZOOM, BoardCamera::MAX_ZOOM);
            next.resting_zoom = wanted;
            next.zoom_to(centre, cursor.unwrap_or(centre), wanted);
        }

        // WASD, not the arrows: `handle_menu` already binds ArrowLeft and
        // ArrowRight to the shop's tabs, so an arrow-key camera panned the
        // board *and* jumped the shop to another tab on the same press.
        let mut walk = Vec2::ZERO;
        for (key, step) in [
            (KeyCode::KeyA, Vec2::NEG_X),
            (KeyCode::KeyD, Vec2::X),
            (KeyCode::KeyW, Vec2::Y),
            (KeyCode::KeyS, Vec2::NEG_Y),
        ] {
            if keys.pressed(key) {
                walk += step;
            }
        }
        if walk != Vec2::ZERO {
            // Screen pixels, so the arrow keys move the board the way the
            // arrows point rather than the way the ground's axes happen to run.
            // Through the projection, because `drag` takes a *screen* delta and
            // divides it back out by the zoom. Without the conversion the
            // constant is projected pixels per second, which is eight times
            // slower than its own name and takes a minute of held key to cross
            // the field.
            let metres_to_board = isometric::TILE_HALF.x / map::TILE_METRES as f32;
            let step = -walk.normalize()
                * KEY_PAN_METRES
                * metres_to_board
                * next.zoom
                * time.delta_secs();
            drag(&mut next, centre, step);
        }
        // `C` on a keyboard, HOME on the HUD - the only way back on a phone.
        // Taken before the test, so a HOME press landing on the same frame as
        // `C` is spent rather than firing again next frame.
        let home = std::mem::take(&mut recentre.0);
        if keys.just_pressed(KeyCode::KeyC) || home {
            // The zoom as well as the aim. Recentring a player who is lost at
            // maximum zoom onto a three-tile keyhole leaves them exactly as
            // lost, facing the right way. The zoom eases rather than jumping,
            // because `resting_zoom` is what `ease` chases.
            let opening = BoardCamera::opening(&village);
            next.focus = opening.focus;
            next.resting_zoom = opening.zoom;
        }
    }

    let next = next.clamped(layout.field(), layout.safe_area().size());
    if next != *board {
        *board = next;
    }
}

/// Move the board with a pointer: the metre the finger grabbed stays under it.
fn drag(camera: &mut BoardCamera, scene_center: Vec2, screen_delta: Vec2) {
    let held = camera.ground_at(scene_center, scene_center);
    camera.hold(scene_center, held, scene_center + screen_delta);
}

/// The mouse half of the gesture, kept apart so the touch path above reads as
/// one decision. A left drag on open ground pans; everything else is idle.
#[allow(clippy::too_many_arguments)]
fn mouse_motion(
    camera: &mut BoardCamera,
    scene_center: Vec2,
    was: CameraMotion,
    window: &Window,
    mouse: &ButtonInput<MouseButton>,
    pointer_guard: &PointerGuard,
    claimed: Option<PointerId>,
    in_ui: &dyn Fn(Vec2) -> bool,
) -> CameraMotion {
    let Some(raw) = window.cursor_position() else {
        return CameraMotion::Idle;
    };
    let now = window_to_camera(window, raw);
    if !mouse.pressed(MouseButton::Left) {
        return CameraMotion::Idle;
    }
    match was {
        CameraMotion::Pan {
            pointer: PointerId::Mouse,
            last,
        } => {
            drag(camera, scene_center, now - last);
            CameraMotion::Pan {
                pointer: PointerId::Mouse,
                last: now,
            }
        }
        // A press that began on the HUD, on the banana, or in the shadow of a
        // touch is not a pan, and must not become one by being held.
        _ if !mouse.just_pressed(MouseButton::Left)
            || claimed == Some(PointerId::Mouse)
            || in_ui(raw)
            || pointer_guard.suppress_mouse_for > 0.0 =>
        {
            CameraMotion::Idle
        }
        _ => CameraMotion::Pan {
            pointer: PointerId::Mouse,
            last: now,
        },
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
    if position.is_some_and(|position| layout.on_deposit(position)) {
        pending.0 = Some(SettlementSource::Pointer(pointer));
    } else {
        cancel_harvest(controller, pending);
        banana.translation = layout.banana_home().extend(isometric::OVERLAY_Z);
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
    let eased = ease(progress);
    let mut position = layout
        .banana_home()
        .lerp(layout.board(layout.town_centre()), eased);
    position.y += (std::f32::consts::PI * progress).sin() * 72.0;
    banana.translation = position.extend(isometric::OVERLAY_Z);

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
#[allow(clippy::too_many_arguments)]
fn queue_manual_settlement(
    mut commands: Commands,
    art: Res<art::Art>,
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
        banana.translation = layout.banana_home().extend(isometric::OVERLAY_Z);
        // The delivered bunch is collected where it was dropped - lifted,
        // shrunk and gone - while the next grows in at the home tree.
        let dropped = match interaction_before {
            HarvestInteraction::Dragging { position, .. } => layout.ground(position),
            _ => layout.town_centre(),
        };
        commands.spawn((
            art.bunch(art::Bunch::Despawn, 0),
            art::BUNCH.anchor(),
            Transform::from_translation(
                layout
                    .board(dropped)
                    .extend(isometric::stand_z(dropped, 0.0)),
            )
            .with_scale(Vec3::new(layout.world_scale(), layout.world_scale(), 1.0)),
            Collected {
                ground: dropped,
                head: art::Playhead::default(),
            },
        ));
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

#[allow(clippy::too_many_arguments)]
fn persist_changes(
    time: Res<Time>,
    mode: Res<persistence::SaveMode>,
    mut dirty: ResMut<PersistenceDirty>,
    treasury: Res<Treasury>,
    workforce: Res<Workforce>,
    staff: Res<Staff>,
    research: Res<Research>,
    carts: Res<Carts>,
) {
    dirty.since_last_save += time.delta_secs();
    if !dirty.pending || *mode == persistence::SaveMode::Off {
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
        staff: *staff,
        research: *research,
        carts: *carts,
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

/// Show HOME only while the hand-harvest drag is out of view.
///
/// Shown always, it is a permanent button for a problem most players never
/// have; hidden always, a player who has panned to the grove on a phone has no
/// way back and no hint that the banana still exists. Appearing exactly when
/// the drag leaves the screen makes it the feedback as well as the fix.
fn sync_recentre_button(
    layout: Res<SceneLayout>,
    mut buttons: Query<&mut Node, With<hud::RecentreButton>>,
) {
    let display = if layout.drag_in_view() {
        Display::None
    } else {
        Display::Flex
    };
    for mut node in &mut buttons {
        if node.display != display {
            node.display = display;
        }
    }
}

/// Smoothstep: a motion that starts and ends at rest.
fn ease(progress: f32) -> f32 {
    progress * progress * (3.0 - 2.0 * progress)
}

/// The drop point's shadow: where it is and whether it is shown.
///
/// Named rather than spelled out in the signature, the way every other query
/// with more than a component or two in this crate is - `Without<Banana>` is
/// half of what stops it aliasing the banana's own transform, and that is worth
/// a line of its own rather than a fourth clause on a parameter.
type HeldBananaShadow<'w, 's> = Single<
    'w,
    's,
    (&'static mut Transform, &'static mut Visibility),
    (With<BananaShadow>, Without<Banana>),
>;

/// Where the player's banana and its shadow are drawn this frame, from what
/// the hand is doing.
///
/// One system, after input, for both. They used to be placed a schedule
/// apart, the shadow in layout from last frame's interaction and the banana by
/// input this frame, so the shadow trailed the banana by a frame on every drag,
/// stayed under the tree for the whole keyboard arc, and a banana put back
/// drew over the plant's visitors for a frame. The shadow is the drop point;
/// it cannot be a frame stale.
fn place_held_banana(
    layout: Res<SceneLayout>,
    controller: Res<HarvestController>,
    mut banana: Single<&mut Transform, (With<Banana>, Without<BananaShadow>)>,
    mut shadow: HeldBananaShadow<'_, '_>,
) {
    let ground = match controller.interaction {
        // Lying where it rests, sorted with the world: in front of the plant
        // it lies before, behind a monkey walking past in front of it.
        HarvestInteraction::Idle => {
            banana.translation = layout
                .banana_home()
                .extend(isometric::stand_z(layout.banana_ground(), 0.0));
            layout.banana_ground()
        }
        // In the hand, over everything; its shadow under the pointer.
        HarvestInteraction::Dragging { position, .. } => {
            banana.translation = layout.held_banana(position).extend(isometric::OVERLAY_Z);
            layout.ground(position)
        }
        // The keyboard flies the banana on an arc (`move_keyboard_harvest`);
        // its shadow crosses the ground under it on the same easing.
        HarvestInteraction::KeyboardHarvest { elapsed, .. } => {
            let progress = (elapsed / KEYBOARD_HARVEST_SECONDS).clamp(0.0, 1.0);
            layout
                .banana_ground()
                .lerp(layout.town_centre(), ease(progress))
        }
    };
    let (at, visibility) = &mut *shadow;
    at.translation = layout.board(ground).extend(isometric::MARK_Z);
    // Lying, the bunch stands in the shadow the artist drew under it. The cast
    // one marks where a *lifted* bunch would land, so it is shown only then.
    let shown = if matches!(controller.interaction, HarvestInteraction::Idle) {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    if **visibility != shown {
        **visibility = shown;
    }
}

/// The player's bunch: glinting where it lies, still in the hand, and growing
/// back into place whenever it comes home - delivered, or put back.
///
/// Spawn and despawn are the artist's one-shots and play once at their own
/// pace; the idle loops. Held, it is the still, because a glint crossing a
/// bunch that is swinging under a thumb reads as a flicker.
fn animate_banana(
    time: Res<Time>,
    art: Res<art::Art>,
    controller: Res<HarvestController>,
    mut banana: Single<(&mut BananaAnimation, &mut Sprite), With<Banana>>,
) {
    let (animation, sprite) = &mut *banana;
    let lying = matches!(controller.interaction, HarvestInteraction::Idle);
    let wanted = match (lying, animation.clip) {
        (false, _) => art::Bunch::Still,
        (true, art::Bunch::Still) => art::Bunch::Spawn,
        (true, clip) => clip,
    };
    if animation.clip != wanted {
        animation.clip = wanted;
        animation.head = art::Playhead::default();
    }
    let clip = animation.clip;
    let finished = animation
        .head
        .advance(clip.durations(), clip.loops(), time.delta_secs());
    if finished && clip == art::Bunch::Spawn {
        animation.clip = art::Bunch::Idle;
        animation.head = art::Playhead::default();
    }
    art.pose_bunch(sprite, animation.clip, animation.head.frame);
}

/// Play every delivered bunch's despawn where it was dropped, and remove it
/// once it has gone. Placed every frame from the ground it was dropped on, so
/// it stays there through a pan or a pinch.
fn animate_collected(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<art::Art>,
    layout: Res<SceneLayout>,
    mut bunches: Query<(Entity, &mut Collected, &mut Transform, &mut Sprite)>,
) {
    let scale = Vec3::new(layout.world_scale(), layout.world_scale(), 1.0);
    for (entity, mut collected, mut transform, mut sprite) in &mut bunches {
        let ground = collected.ground;
        transform.translation = layout.board(ground).extend(isometric::stand_z(ground, 0.0));
        transform.scale = scale;
        let clip = art::Bunch::Despawn;
        if collected
            .head
            .advance(clip.durations(), clip.loops(), time.delta_secs())
        {
            commands.entity(entity).despawn();
            continue;
        }
        art.pose_bunch(&mut sprite, clip, collected.head.frame);
    }
}

#[allow(clippy::too_many_arguments)]
fn update_feedback(
    time: Res<Time>,
    controller: Res<HarvestController>,
    layout: Res<SceneLayout>,
    first: Res<FirstHarvest>,
    workforce: Res<Workforce>,
    mut feedback: ResMut<Feedback>,
    glow: Single<&MeshMaterial2d<ColorMaterial>, With<DepositGlow>>,
    hint: Single<&MeshMaterial2d<ColorMaterial>, With<HarvestHint>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let drag_highlight = matches!(
        controller.interaction,
        HarvestInteraction::Dragging { position, .. }
            if layout.on_deposit(position)
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

    // Until the player has carried one banana home, both ends of the drag are
    // marked - a faint diamond under the banana, a slow breath on the depot -
    // so the board says "from here to there" without a word of text. Only on
    // a board nobody else is working: once monkeys are hired the player has
    // learned the drag, or skipped it for a scenario, and a tutorial glow
    // under sixty workers is noise.
    let teaching = first.pending
        && workforce.count() == 0
        && !matches!(controller.interaction, HarvestInteraction::Dragging { .. });
    let breath = if teaching {
        let phase = time.elapsed_secs() * std::f32::consts::TAU * HINT_BREATH_HZ;
        0.06 + 0.08 * (0.5 + 0.5 * phase.sin())
    } else {
        0.0
    };
    let alpha = if drag_highlight {
        0.34
    } else {
        (pulse * 0.46).max(breath)
    };
    set_colour(&mut materials, &glow.0, GLOW.with_alpha(alpha));
    let hint_alpha = if teaching { 0.12 } else { 0.0 };
    set_colour(&mut materials, &hint.0, GLOW.with_alpha(hint_alpha));
}

/// How fast the untaught depot breathes: slow enough to read as an invitation
/// rather than an alarm.
const HINT_BREATH_HZ: f32 = 0.5;

/// Set a material's colour, reading it first. `get_mut` marks a material
/// changed, and a changed material is re-uploaded - every frame, for glows that
/// are dark almost all of the time.
fn set_colour(
    materials: &mut Assets<ColorMaterial>,
    handle: &Handle<ColorMaterial>,
    colour: Color,
) {
    if materials
        .get(handle)
        .is_some_and(|material| material.color != colour)
        && let Some(mut material) = materials.get_mut(handle)
    {
        material.color = colour;
    }
}

/// The depot's glow: banana gold, over the pad while a drag is over it and in
/// a pulse when a delivery lands.
const GLOW: Color = Color::srgb(1.0, 0.78, 0.08);

fn update_floaters(
    time: Res<Time>,
    layout: Res<SceneLayout>,
    mut commands: Commands,
    mut floaters: Query<
        (Entity, &mut Floater, &mut Transform, &mut TextColor),
        Without<FloaterOutline>,
    >,
    mut outlines: Query<(&ChildOf, &mut TextColor), With<FloaterOutline>>,
) {
    // Collected rather than read back through the parent, because a second
    // query reading `Floater` would conflict with this one's `&mut` on it.
    let mut fading: Vec<(Entity, f32)> = Vec::new();

    for (entity, mut floater, mut transform, mut color) in &mut floaters {
        floater.elapsed += time.delta_secs();
        let progress = (floater.elapsed / FLOATER_SECONDS).clamp(0.0, 1.0);
        // Ease out, so it leaps off the stall and settles as it fades.
        let eased = 1.0 - (1.0 - progress) * (1.0 - progress);
        // Re-projected every frame rather than replayed from a captured
        // position, so a "+5" stays over the stall that earned it while the
        // player pans.
        transform.translation = (layout.board_raised(floater.anchor, 1.0)
            + Vec2::new(0.0, FLOATER_RISE * eased))
        .extend(isometric::OVERLAY_Z);
        color.0.set_alpha(1.0 - eased);
        fading.push((entity, 1.0 - eased));

        if progress >= 1.0 {
            // Despawns its outline with it: an orphaned outline is four black
            // glyphs left standing over the stall.
            commands.entity(entity).despawn();
        }
    }

    // The edge has to fade with the glyph it edges, or the last thing the
    // player sees of a delivery is its outline.
    for (parent, mut color) in &mut outlines {
        if let Some((_, alpha)) = fading.iter().find(|(entity, _)| *entity == parent.parent()) {
            color.0.set_alpha(*alpha);
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
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TestButtons {
    menu: TestPoint,
    hire_worker: TestPoint,
    hire_chef: TestPoint,
    hire_unpacker: TestPoint,
    hire_technologist: TestPoint,
    hire_cart: TestPoint,
    previous_tab: TestPoint,
    next_tab: TestPoint,
    resume: TestPoint,
    logs: TestPoint,
    restart: TestPoint,
    confirm_restart: TestPoint,
    cancel_restart: TestPoint,
}

/// One support role's state, as the e2e suite sees it.
#[cfg(target_arch = "wasm32")]
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TestStaff {
    role: &'static str,
    owned: u32,
    hungry: u32,
    next_cost: f64,
    gain_per_min: f64,
    can_hire: bool,
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
    /// The camera, so a browser test can drive a pan or a pinch and assert
    /// where it ended up rather than eyeballing a screenshot.
    camera_zoom: f32,
    /// Where the camera is looking, in **metres on the ground** - the same
    /// units `map` and the economy use.
    camera_focus: TestPoint,
    /// The window less the HUD's reserve. Anything the player is meant to look
    /// at should be inside this, and a spec can now say so.
    safe_bounds: TestBounds,
    monkeys: Vec<TestWorker>,
    staff: Vec<TestStaff>,
    /// `support::SupportAvatar` entities on screen, summed across every role.
    /// Guards against the resource-ordering bug where a support monkey could
    /// be fully hired and drawing wages with no sprite at all: this is zero
    /// only when nothing is hired, never when something is.
    avatars_drawn: u32,
    /// Bananas reserved by harvesters against meals they have earned but not
    /// yet eaten, and which therefore nothing else may spend.
    committed: f64,
    research: f64,
    research_level: u32,
    research_per_sec: f64,
    carts: u32,
    /// Monkeys aboard, across every cart.
    crewed: u32,
    /// Carts with a full crew, and therefore actually running.
    carts_running: u32,
    cart_cost: f64,
    cart_can_hire: bool,
    /// Workers still walking the route: hired, less everyone aboard a cart.
    pool: u32,
    selected_tab: &'static str,
    store_scroll: f32,
    buttons: TestButtons,
}

/// Every resource the economy is made of, in one parameter.
///
/// Bevy caps a system at sixteen parameters, and the exporter passed it once
/// the payroll grew. Bundling is also the fix the shape wanted anyway: these
/// eight always travel together, and a reader that took seven of them would be
/// reporting a partial economy.
#[cfg(target_arch = "wasm32")]
#[derive(bevy::ecs::system::SystemParam)]
struct EconomyView<'w> {
    treasury: Res<'w, Treasury>,
    workforce: Res<'w, Workforce>,
    staff: Res<'w, Staff>,
    fed: Res<'w, FedStaff>,
    committed: Res<'w, Committed>,
    multipliers: Res<'w, Multipliers>,
    snapshot: Res<'w, EconomySnapshot>,
    research: Res<'w, Research>,
    carts: Res<'w, Carts>,
    active_tab: Res<'w, hud::ActiveShopTab>,
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
fn sync_web_test_state(
    economy: EconomyView,
    controller: Res<HarvestController>,
    menu: Res<MenuState>,
    layout: Res<SceneLayout>,
    camera: Res<BoardCamera>,
    primary_window: Single<&Window, With<PrimaryWindow>>,
    touches: Res<Touches>,
    banana_transform: Single<&Transform, With<Banana>>,
    workers: Query<(&HarvestCycle, &Transform), With<Worker>>,
    support: Query<(&SupportRole, &SupportCycle), With<SupportUnit>>,
    avatars: Query<Entity, With<support::SupportAvatar>>,
    buttons: Query<(&ButtonAction, &ComputedNode, &UiGlobalTransform)>,
    // Absent in the stage view, which has no store; the export must still
    // appear, or a spec pointed at `?view=stage` waits for it forever.
    store_scroll: Option<Single<&ScrollPosition, With<hud::StoreScroll>>>,
    mut warmup_frames: Local<u8>,
) {
    use crate::domain::Segment;
    use wasm_bindgen::JsValue;

    let EconomyView {
        treasury,
        workforce,
        staff,
        fed,
        committed,
        multipliers,
        snapshot,
        research,
        carts,
        active_tab,
    } = economy;

    if *warmup_frames < 3 {
        *warmup_frames += 1;
        return;
    }

    let point = |value: Vec2| TestPoint {
        x: value.x,
        y: value.y,
    };
    let screen = |value: Vec2| point(layout.world_to_screen(value));

    // One slot per button, so four hire buttons cannot collapse onto each
    // other and leave the suite clicking whichever row happened to be last.
    let mut button_centers = [Vec2::ZERO; 13];
    for (action, node, transform) in &buttons {
        let index = match action {
            ButtonAction::OpenMenu => 0,
            // Neither is a row the suite clicks by slot: INFO is reached from
            // the row it belongs to, and HOME is driven through
            // `sync_recentre_button`. They have no slot rather than a slot
            // nothing writes.
            ButtonAction::Info(_) | ButtonAction::Recentre => continue,
            ButtonAction::Hire(UnitKind::Worker) => 1,
            ButtonAction::Hire(UnitKind::Support(SupportRole::Chef)) => 2,
            ButtonAction::Hire(UnitKind::Support(SupportRole::Unpacker)) => 3,
            ButtonAction::Hire(UnitKind::Support(SupportRole::Technologist)) => 4,
            ButtonAction::Resume => 5,
            ButtonAction::Diagnostics => 6,
            ButtonAction::Restart => 7,
            ButtonAction::ConfirmRestart => 8,
            ButtonAction::CancelRestart => 9,
            ButtonAction::Hire(UnitKind::Cart) => 10,
            ButtonAction::PreviousShopTab => 11,
            ButtonAction::NextShopTab => 12,
        };
        button_centers[index] = transform.translation * node.inverse_scale_factor;
    }

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
    let plan_for = |kind| plan_hire(kind, world);
    let plan = plan_for(UnitKind::Worker);
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
        harvest: screen(layout.harvest_bounds().center()),
        harvest_bounds: TestBounds {
            min: screen(Vec2::new(
                layout.harvest_bounds().min.x,
                layout.harvest_bounds().max.y,
            )),
            max: screen(Vec2::new(
                layout.harvest_bounds().max.x,
                layout.harvest_bounds().min.y,
            )),
        },
        deposit: screen(layout.board(layout.town_centre())),
        camera_zoom: camera.zoom(),
        camera_focus: point(camera.focus()),
        safe_bounds: TestBounds {
            min: screen(Vec2::new(
                layout.safe_area().min.x,
                layout.safe_area().max.y,
            )),
            max: screen(Vec2::new(
                layout.safe_area().max.x,
                layout.safe_area().min.y,
            )),
        },
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
                }
            })
            .collect(),
        staff: SupportRole::ALL
            .iter()
            .map(|role| {
                let plan = plan_for(UnitKind::Support(*role));
                TestStaff {
                    role: role.name(),
                    owned: staff.count(*role),
                    hungry: support
                        .iter()
                        .filter(|(unit, cycle)| *unit == role && cycle.is_hungry())
                        .count() as u32,
                    next_cost: plan.cost,
                    gain_per_min: plan.gain_per_min,
                    can_hire: plan.affordable,
                }
            })
            .collect(),
        avatars_drawn: avatars.iter().count() as u32,
        committed: committed.0,
        research: research.points(),
        research_level: research.level(),
        research_per_sec: research_per_sec(*fed, *multipliers),
        carts: carts.owned(),
        crewed: carts.crewed(),
        carts_running: carts.running(),
        cart_cost: plan_for(UnitKind::Cart).cost,
        cart_can_hire: plan_for(UnitKind::Cart).affordable,
        pool: workforce.count().saturating_sub(carts.crewed()),
        selected_tab: active_tab.0.label(),
        store_scroll: store_scroll.map_or(0.0, |scroll| scroll.y),
        buttons: TestButtons {
            menu: point(button_centers[0]),
            hire_worker: point(button_centers[1]),
            hire_chef: point(button_centers[2]),
            hire_unpacker: point(button_centers[3]),
            hire_technologist: point(button_centers[4]),
            previous_tab: point(button_centers[11]),
            next_tab: point(button_centers[12]),
            hire_cart: point(button_centers[10]),
            resume: point(button_centers[5]),
            logs: point(button_centers[6]),
            restart: point(button_centers[7]),
            confirm_restart: point(button_centers[8]),
            cancel_restart: point(button_centers[9]),
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
    fn the_cart_park_stands_clear_of_the_village() {
        // The freight's own corner (D31). A park that reaches back into the
        // unloading ring puts a vehicle the length of three monkeys across the
        // queue - which is exactly the thing giving the carts their own bins was
        // meant to stop - and one that lands on a support station hides a monkey
        // the player is paying for behind a box.
        let layout = SceneLayout::for_viewport(Vec2::new(1280.0, 720.0));
        let depot = layout.town_centre();
        let bins = layout.cart_bins();
        assert!(
            bins.distance(depot) > crate::worker::RING_OUTER + 0.5,
            "the cart bins stand {} m from the delivery point",
            bins.distance(depot)
        );
        for index in 0..CART_BAYS * 2 {
            let park = layout.cart_park(index);
            let out = park.distance(depot);
            assert!(
                out > crate::worker::RING_OUTER + 0.5,
                "bay {index} parks {out} m from the delivery point, in the queue"
            );
            // And in front of the boxes, on the viewer's side, so a cart draws
            // over the bins it is unloading into rather than behind them.
            assert!(
                isometric::depth(park - bins) > 0.0,
                "bay {index} parks behind the bins"
            );
        }
        // Clear of the home tree's drag target and of the walk, like every
        // other thing standing in the village.
        assert!(bins.distance(layout.home_tree()) > 6.0);

        // Clear of every support monkey on screen, measured in the texels a
        // sprite's width is measured in: the bins are a wide object, so what has
        // to fit is half of them plus half a monkey. The bins' half-width
        // arrives already in texels - it is drawn at a quarter of a texel per
        // art pixel, not a half, and the first cut of this test converted it
        // with `ART_SCALE` and so demanded 49 texels where 30 was the honest
        // figure. That doubled bar was the only thing the Unpacker failed, and
        // excluding it was the wrong repair.
        let clearance =
            art::CART_BINS_HALF_WIDTH_TEXELS + support::BODY_HALF_ART_PIXELS * art::ART_SCALE;
        for role in SupportRole::ALL {
            for slot in 0..support::AVATARS_PER_ROLE {
                let at = layout.support_point(role, support::slot_offset(slot));
                let gap = isometric::project(at - bins).length();
                assert!(
                    gap > clearance,
                    "{role:?} slot {slot} stands {gap} texels from the cart bins, \
                     inside the {clearance} they need"
                );
            }
        }
    }

    #[test]
    fn the_cart_rank_never_hides_the_bins_it_unloads_into() {
        // The reason `CART_PARK_STANDOFF` has the value it has, asserted rather
        // than asserted-in-a-comment. A cart is a box with a rider's head and
        // tail above it; park the rank a body's length in front of the boxes and
        // six of them erase the thing the yard is *for*, which is also the only
        // thing that says this corner is a place and not a car park.
        let layout = SceneLayout::for_viewport(Vec2::new(1280.0, 720.0));
        let bins = layout.board(layout.cart_bins());
        // The top of the bins' art, above their own ground line.
        let crown = art::CART_BINS.height_above(479.0) * layout.world_scale();
        for index in 0..CART_BAYS {
            let park = layout.board(layout.cart_park(index));
            // The tallest drawn pixel of a parked cart: the box's lid, plus the
            // rider sitting in it, measured through the art the way every other
            // prop on a monkey is.
            let rider = worker::cart_crown() * layout.world_scale();
            assert!(
                park.y + rider < bins.y + crown,
                "bay {index} reaches {} px up the board against the bins' {}",
                park.y + rider,
                bins.y + crown
            );
        }
    }

    #[test]
    fn the_carts_corner_is_a_short_pan_from_the_opening_view() {
        // The bins go up mid-session, at the moment the player has just been
        // told the Cart is theirs to buy, and a reward they cannot find is not
        // one. What they are owed is that it is *findable*, not that it is
        // already framed: the opening camera is centred on the treehouse (D30),
        // which leaves barely a hundred pixels of board below the depot on a
        // desktop and none at all on a phone - and the freight's corner is in
        // front of the depot by construction, since a rank of carts has to
        // stand between the viewer and the boxes it is unloading into.
        //
        // So the bar is half a screenful, held on the two boards D30 holds the
        // support crew on. The 844 x 390 landscape phone is the exception there
        // too: its board is 286 pixels, the treehouse is 284 of them, and the
        // home tree already opens eleven pixels inside its edge - a yard in
        // front of the depot is simply past it, and the owner's call in D30 was
        // the landmark centred over the village framed on that board.
        for viewport in [Vec2::new(390.0, 844.0), Vec2::new(1280.0, 720.0)] {
            let layout = SceneLayout::for_viewport(viewport);
            let pan = layout.safe_area().size().min_element() * 0.5;
            let reach = layout.safe_area().inflate(pan);
            for index in 0..CART_BAYS {
                let screen = layout.board(layout.cart_park(index));
                assert!(
                    reach.contains(screen),
                    "{viewport:?}: bay {index} opens at {screen:?}, more than a \
                     {pan} px pan outside {:?}",
                    layout.safe_area()
                );
            }
            assert!(reach.contains(layout.board(layout.cart_bins())));
        }
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
    fn the_stage_view_gives_the_board_the_whole_window() {
        // The regression this pins: the layout is a function of the viewport
        // *and* the view, and a stage launch at the default resolution has the
        // same viewport as a full one. A guard that compared only the viewport
        // left the stage board at HUD size, which is the one thing the view is
        // for. `refresh_layout` therefore compares whole layouts, so these two
        // must differ.
        let viewport = Vec2::new(1280.0, 720.0);
        let full = SceneLayout::for_view(viewport, View::Full);
        let stage = SceneLayout::for_view(viewport, View::Stage);

        assert_ne!(full, stage);
        assert_eq!(stage.scene_side(), 720.0, "the largest square that fits");
        assert!(stage.scene_side() > full.scene_side() * 1.5);
        assert_eq!(stage.scene_center(), Vec2::ZERO, "centred, with no header");
        assert_eq!(stage.header_height(), 0.0);
        assert!(
            !stage.short_landscape(),
            "no store to place beside the board"
        );
        // And the board still fits: a square window is the tight case.
        for viewport in [Vec2::new(720.0, 720.0), Vec2::new(390.0, 844.0)] {
            let stage = SceneLayout::for_view(viewport, View::Stage);
            assert_eq!(stage.scene_side(), viewport.x.min(viewport.y).max(320.0));
        }
    }

    #[test]
    fn responsive_layout_preserves_isometric_flow_and_minimum_targets() {
        for viewport in [
            Vec2::new(320.0, 640.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(475.0, 653.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);

            assert!(layout.board(layout.grove()).x < layout.board(layout.town_centre()).x);
            assert!(layout.board(layout.grove()).y > layout.board(layout.town_centre()).y);
            // The drawn banana is inside the thing that grabs it, and the
            // depot's own point is inside the thing that takes it.
            assert!(
                layout.on_harvest(layout.banana_home()),
                "{viewport:?}: the banana lies outside its own target"
            );
            // A thumb's worth on the diamond's *short* axis, at the zoom the
            // board opens at - the smallest it is ever drawn.
            assert!(
                layout.harvest_bounds().height() >= 80.0,
                "{viewport:?}: the harvest target is {} px tall",
                layout.harvest_bounds().height()
            );
            // And the two ends of the drag are never the same ground, so a
            // drag cannot start on its own drop target.
            let apart = (layout.home_tree() - layout.town_centre())
                .abs()
                .max_element();
            assert!(apart > layout.harvest_target().half + layout.deposit_target().half);
        }
    }

    #[test]
    fn the_drag_targets_are_places_on_the_ground_at_every_zoom() {
        // What they replaced were squares of *screen*, sized from the chrome:
        // zoom in and the plant grew while its target stayed put, zoom out and
        // the target swallowed the village. So the property, at every zoom
        // the camera allows and wherever it is aimed: whether a point on the
        // screen grabs the banana depends only on the ground under it, and the
        // target is drawn at a size set by the zoom and by nothing else.
        let map = map::start();
        let opening = BoardCamera::opening(map);
        for viewport in VIEWPORTS {
            for zoom in [2.0, 3.0, 4.0, 5.0, 6.0] {
                for pan in [Vec2::ZERO, Vec2::new(3.0, -2.0), Vec2::new(-4.5, 5.5)] {
                    let camera = BoardCamera {
                        focus: opening.focus + pan,
                        zoom,
                        resting_zoom: zoom,
                    };
                    let layout = SceneLayout::for_map(viewport, View::Full, map, camera);
                    // Thumb-sized at every zoom: the ground it covers shrinks
                    // as the plant grows, never to less than a thumb.
                    let short = layout.harvest_bounds().height();
                    assert!(
                        short >= 80.0,
                        "{viewport:?} zoom {zoom}: the grab target is {short} px across"
                    );
                    // The drawn banana always lies inside what grabs it.
                    assert!(
                        layout.harvest_target().contains(layout.banana_ground()),
                        "{viewport:?} zoom {zoom}: the banana lies outside its own target"
                    );
                    // A press up the trunk grabs the plant; a press a thumb and
                    // a half clear of it is the camera's.
                    let foot = layout.board(layout.home_tree());
                    assert!(layout.on_harvest(foot + Vec2::new(0.0, 60.0)));
                    for off in [
                        Vec2::new(200.0, 0.0),
                        Vec2::new(-200.0, 0.0),
                        Vec2::new(0.0, -120.0),
                        Vec2::new(100.0, 120.0),
                    ] {
                        assert!(
                            !layout.on_harvest(foot + off),
                            "{viewport:?} zoom {zoom}: a press {off} off the plant grabs"
                        );
                    }
                    // A grid round each end of the drag, scaled to that
                    // target so it reaches half as far again outside it - a
                    // fixed step that stops inside the larger target can
                    // never see it accept too much - and never lands exactly
                    // on an edge (steps of 0.15 of the half).
                    for x in -10..=10 {
                        for y in -10..=10 {
                            let grid = Vec2::new(x as f32, y as f32) * 0.15;
                            // The harvest end also grabs up the plant's trunk,
                            // which is a column of *screen*, not ground; the
                            // depot is its footprint and nothing else.
                            for (target, hit, trunk) in [
                                (
                                    layout.harvest_target(),
                                    SceneLayout::on_harvest as fn(SceneLayout, Vec2) -> bool,
                                    true,
                                ),
                                (layout.deposit_target(), SceneLayout::on_deposit, false),
                            ] {
                                let step = grid * target.half;
                                let screen = layout.board(target.centre + step);
                                let on = hit(layout, screen);
                                assert_eq!(
                                    on,
                                    target.contains(target.centre + step)
                                        || (trunk && layout.on_trunk(screen)),
                                    "{viewport:?} zoom {zoom} pan {pan}: {step} m off {:?}",
                                    target.centre
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn harvest_never_takes_the_board_from_the_camera() {
        // Harvest has first refusal on every press, so what it covers is
        // ground the camera cannot be driven from. Sampled over the whole
        // board, at every zoom, with the camera on the home tree - the worst
        // case, where the most of the target is on screen - it must leave the
        // player at least three quarters of the board to pan and pinch on.
        let map = map::start();
        for viewport in [
            Vec2::new(320.0, 568.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
        ] {
            for zoom in [2.0, 3.0, 4.0, 5.0, 6.0] {
                let camera = BoardCamera {
                    focus: isometric::tile_centre(map.home_trees()[0]),
                    zoom,
                    resting_zoom: zoom,
                };
                let layout = SceneLayout::for_map(viewport, View::Full, map, camera);
                let safe = layout.safe_area();
                let (mut grabbed, mut total) = (0u32, 0u32);
                let mut y = safe.min.y;
                while y < safe.max.y {
                    let mut x = safe.min.x;
                    while x < safe.max.x {
                        total += 1;
                        grabbed += u32::from(layout.on_harvest(Vec2::new(x, y)));
                        x += 4.0;
                    }
                    y += 4.0;
                }
                let share = grabbed as f32 / total as f32;
                assert!(
                    share < 0.25,
                    "{viewport:?} at zoom {zoom}: harvest claims {share} of the board"
                );
            }
        }
    }

    #[test]
    fn the_way_home_shows_exactly_when_the_drag_leaves_the_screen() {
        // The pan clamp keeps *some of the walk* on screen, not the home tree,
        // so a player can pan to where hand harvest is impossible. On a phone
        // HOME is the only way back, and its appearing is the only sign that
        // anything is out of reach - so it must be hidden at the opening view
        // and shown once the player has panned as far grove-wards as the clamp
        // lets them.
        let map = map::start();
        for viewport in [
            Vec2::new(320.0, 568.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
        ] {
            let opening = BoardCamera::opening(map);
            let home = SceneLayout::for_map(viewport, View::Full, map, opening);
            // Portrait phones and desktops open with the whole drag in view.
            // The short landscape phone opens centred on the treehouse, which
            // fills its board, with the home tree nearer the edge than a thumb
            // - so there HOME shows from the first frame (D30).
            let landscape = home.short_landscape();
            assert_eq!(
                home.drag_in_view(),
                !landscape,
                "{viewport:?}: HOME is wrong on a fresh board"
            );

            let wandered = BoardCamera {
                focus: home.grove(),
                ..opening
            }
            .clamped(home.field(), home.safe_area().size());
            let away = SceneLayout::for_map(viewport, View::Full, map, wandered);
            assert!(
                !away.drag_in_view(),
                "{viewport:?}: panned to the grove, the drag still counts as in view"
            );
        }
    }

    /// Every viewport the game is expected to survive, including the two the
    /// e2e matrix drives and the 320-wide floor `for_map` clamps to.
    const VIEWPORTS: [Vec2; 5] = [
        Vec2::new(320.0, 568.0),
        Vec2::new(390.0, 844.0),
        Vec2::new(844.0, 390.0),
        Vec2::new(1280.0, 720.0),
        Vec2::new(1920.0, 1080.0),
    ];

    #[test]
    fn the_board_opens_framed_on_the_first_drag() {
        // The opening act is a drag from the home tree to the town centre
        // (D24). Both ends have to be inside the *safe area* - not merely
        // inside the window - or the game opens on a gesture whose start or
        // finish is behind the banner or under the store panel, which is the
        // same as not drawing it.
        //
        // This is what pins `DEFAULT_ZOOM`. Zoom 4 loses the town centre on an
        // 844x390 landscape phone; if someone raises the default, this fails
        // rather than the framing quietly getting worse.
        for viewport in VIEWPORTS {
            let layout = SceneLayout::for_viewport(viewport);
            // Half a thumb inside the edge, as HOME measures it, so the two
            // tests cannot disagree about whether the drag is in view.
            let safe = layout.safe_area().inflate(-DRAG_VIEW_MARGIN);
            for (name, at) in [
                ("the home tree", layout.home_tree()),
                ("the town centre", layout.town_centre()),
            ] {
                // D30: centred on the treehouse, the landscape phone opens with
                // the home tree nearer its edge than a thumb, and HOME showing
                // - see `the_way_home_shows_exactly_when_the_drag_leaves_the_screen`.
                // The depot is still held, which is what pins `DEFAULT_ZOOM`.
                if name == "the home tree" && layout.short_landscape() {
                    continue;
                }
                let on_screen = layout.board(at);
                assert!(
                    contains_inclusive(safe, on_screen),
                    "{viewport:?}: {name} opens at {on_screen:?}, outside the safe area {safe:?}"
                );
            }
        }
    }

    #[test]
    fn the_safe_area_is_the_window_less_the_chrome() {
        for viewport in VIEWPORTS {
            let layout = SceneLayout::for_viewport(viewport);
            let safe = layout.safe_area();
            let half = layout.viewport * 0.5;
            assert!(
                safe.min.x >= -half.x && safe.max.x <= half.x,
                "{viewport:?}"
            );
            assert!(
                safe.min.y >= -half.y && safe.max.y <= half.y,
                "{viewport:?}"
            );
            // The banner's strip is reserved at the top, and the store's panel
            // on whichever side it took.
            assert!(
                half.y - safe.max.y >= layout.header_height(),
                "{viewport:?}: the safe area reaches into the banner"
            );
            let reserved = if layout.short_landscape() {
                half.x - safe.max.x
            } else {
                safe.min.y + half.y
            };
            assert!(
                reserved >= layout.store_height().min(layout.store_width()) - 1e-3
                    || layout.store_height() == 0.0,
                "{viewport:?}: the safe area reaches into the store"
            );
            // And the board is aimed at the middle of it, never at the middle
            // of the window.
            assert_eq!(layout.scene_center(), safe.center());
        }
    }

    #[test]
    fn the_root_transform_and_board_are_two_spellings_of_the_camera() {
        // A `WorldRoot` transform and `board` are two spellings of the same two
        // numbers; if they drift, the baked terrain slides out from under the
        // monkeys walking on it. Now exercised at a non-unit zoom, which the
        // old fit-to-viewport board never reached at any real viewport.
        for viewport in VIEWPORTS {
            let layout = SceneLayout::for_viewport(viewport);
            assert!(layout.world_scale() > 1.0, "{viewport:?}: zoom is still 1");
            let root = layout.world_root();
            for at in [layout.grove(), layout.town_centre(), layout.home_tree()] {
                let projected = isometric::project(at);
                let through_root = root.transform_point(projected.extend(0.0)).truncate();
                assert!(
                    (through_root - layout.board(at)).length() < 1e-3,
                    "{viewport:?}"
                );
            }
        }
    }

    /// A layout at a chosen camera, for the gesture tests.
    fn viewed(camera: BoardCamera) -> SceneLayout {
        SceneLayout::for_map(Vec2::new(1280.0, 720.0), View::Full, map::start(), camera)
    }

    #[test]
    fn a_window_position_becomes_a_centred_y_up_point() {
        // The bug this exists for shipped once and no other test could see it:
        // every camera unit test works in board space and passes whichever way
        // the window's y runs, so an unflipped pointer drags the ground the
        // right way across and the wrong way down. It took driving a real mouse
        // at a real window to catch, which is why the conversion is pinned here
        // rather than left implicit at the call site.
        let mut window = Window {
            resolution: bevy::window::WindowResolution::new(800, 600),
            ..default()
        };
        window.resolution.set_scale_factor_override(Some(1.0));

        // The window's origin is its top-left corner; the board's is its middle.
        assert_eq!(
            window_to_camera(&window, Vec2::new(400.0, 300.0)),
            Vec2::ZERO
        );
        // Down the window is *down* the board, not up it.
        let lower = window_to_camera(&window, Vec2::new(400.0, 500.0));
        assert!(
            lower.y < 0.0,
            "a pointer below the middle came back above it"
        );
        assert_eq!(lower, Vec2::new(0.0, -200.0));
        // And across is across, unchanged.
        assert_eq!(
            window_to_camera(&window, Vec2::new(700.0, 300.0)),
            Vec2::new(300.0, 0.0)
        );
    }

    #[test]
    fn a_drag_keeps_the_ground_under_the_finger() {
        // The property that makes a pan feel like moving a map rather than
        // scrolling a picture: whatever metre the finger came down on is still
        // beneath it after the drag.
        let mut camera = BoardCamera::default();
        let layout = viewed(camera);
        let centre = layout.scene_center();
        let grabbed = camera.ground_at(centre, centre);

        for delta in [
            Vec2::new(120.0, 0.0),
            Vec2::new(-37.0, 64.0),
            Vec2::new(0.0, -210.0),
        ] {
            let before = camera.ground_at(centre, centre);
            drag(&mut camera, centre, delta);
            let landed = camera.ground_at(centre, centre + delta);
            assert!(
                landed.distance(before) < 1e-2,
                "a drag of {delta:?} slid the ground by {}",
                landed.distance(before)
            );
        }
        // And a drag is a pan, never a zoom.
        assert_eq!(camera.zoom(), BoardCamera::DEFAULT_ZOOM);
        assert!(camera.focus != grabbed || grabbed == camera.focus);
    }

    #[test]
    fn a_pinch_keeps_the_ground_between_the_fingers() {
        // The same property, for the gesture that changes scale: the metre
        // between the two fingers must not slide out from between them.
        let mut camera = BoardCamera::default();
        let layout = viewed(camera);
        let centre = layout.scene_center();

        for anchor in [centre, centre + Vec2::new(180.0, -90.0)] {
            for wanted in [2.0, 4.5, 6.0, 3.0] {
                let held = camera.ground_at(centre, anchor);
                camera.zoom_to(centre, anchor, wanted);
                assert_eq!(camera.zoom(), wanted);
                let still = camera.ground_at(centre, anchor);
                assert!(
                    still.distance(held) < 1e-2,
                    "zooming to {wanted} at {anchor:?} moved the ground by {}",
                    still.distance(held)
                );
            }
        }
    }

    #[test]
    fn the_zoom_is_bounded_by_how_big_a_monkey_is() {
        // Not by how much of the map fits. A monkey is 58 art pixels tall,
        // and the floor keeps it a readable 44 logical pixels or more.
        const MONKEY_TEXELS: f32 = 58.0 * art::ART_SCALE;
        const { assert!(MONKEY_TEXELS * BoardCamera::MIN_ZOOM >= 44.0) };
        // The counter-case, stated so nobody "fixes" the floor by fitting the
        // map: the whole 69-tile board on a phone needs a zoom that renders a
        // monkey unreadable.
        let across = map::start().width() as f32 * isometric::TILE_HALF.x * 2.0;
        let to_fit = 390.0 / across;
        assert!(
            to_fit < 0.5 && MONKEY_TEXELS * to_fit < 12.0,
            "fitting the map would draw a monkey {} pixels tall",
            MONKEY_TEXELS * to_fit
        );
        const { assert!(BoardCamera::MIN_ZOOM <= BoardCamera::DEFAULT_ZOOM) };
        const { assert!(BoardCamera::DEFAULT_ZOOM < BoardCamera::MAX_ZOOM) };
    }

    #[test]
    fn a_pinch_settles_the_zoom_back_onto_a_whole_step() {
        // Continuous while the fingers are down, whole-numbered once they lift:
        // the ground mesh takes any scale, but a sprite off the texel grid
        // crawls, and rest is when that shows.
        let layout = viewed(BoardCamera::default());
        let centre = layout.scene_center();
        for landed_on in [2.4, 3.7, 5.5, 1.2, 9.0] {
            let mut camera = BoardCamera::default();
            camera.zoom_to(centre, centre, landed_on);
            camera.settle();
            for _ in 0..600 {
                camera.ease(centre, centre, 1.0 / 60.0);
            }
            assert_eq!(
                camera.zoom(),
                camera.zoom().round(),
                "settled on a fractional zoom from {landed_on}"
            );
            assert!((BoardCamera::MIN_ZOOM..=BoardCamera::MAX_ZOOM).contains(&camera.zoom()));
        }
    }

    #[test]
    fn the_player_cannot_pan_the_village_away() {
        // Shove the camera as hard as any gesture could, from every direction,
        // at every zoom the player can reach, and something of theirs is still
        // on screen when it comes to rest.
        //
        // The assertion this replaces was a tautology: it checked that the
        // clamped focus was nearer to a landmark than the *diagonal of the
        // field*, which is true by construction and cannot fail. It passed
        // while ten drags on a phone landed on a corner of canopy and a
        // screenful of empty sky, because clamping the bare focus lets the
        // visible rectangle hang entirely outside the ground being clamped to.
        for viewport in VIEWPORTS {
            for step in 0..=4 {
                let zoom = BoardCamera::MIN_ZOOM
                    + step as f32 * (BoardCamera::MAX_ZOOM - BoardCamera::MIN_ZOOM) / 4.0;
                for angle in 0..16 {
                    let radians = angle as f32 * std::f32::consts::TAU / 16.0;
                    let mut camera = BoardCamera::default();
                    let seed = SceneLayout::for_map(viewport, View::Full, map::start(), camera);
                    camera.zoom_to(seed.scene_center(), seed.scene_center(), zoom);
                    drag(
                        &mut camera,
                        seed.scene_center(),
                        Vec2::from_angle(radians) * 100_000.0,
                    );
                    let camera = camera.clamped(seed.field(), seed.safe_area().size());

                    let layout = SceneLayout::for_map(viewport, View::Full, map::start(), camera);
                    let safe = layout.safe_area();
                    // Sampled along the walk in *metres*, so this asks whether
                    // the player can see ground their monkeys cover rather than
                    // re-deriving the clamp's own arithmetic and agreeing with
                    // it. The endpoints alone are too strict: looking at the
                    // middle of the route with neither end in frame is a thing
                    // a player should be able to do.
                    let legs = [
                        (layout.home_tree(), layout.town_centre()),
                        (layout.town_centre(), layout.grove()),
                    ];
                    let visible = legs.iter().any(|(from, to)| {
                        (0..=64).any(|step| {
                            let at = from.lerp(*to, step as f32 / 64.0);
                            contains_inclusive(safe, layout.board(at))
                        })
                    });
                    assert!(
                        visible,
                        "{viewport:?} at zoom {zoom}: a flick at {radians} rad left no part of \
                         the walk on screen - the ends landed at {:?} in a safe area of {safe:?}",
                        [
                            layout.board(layout.home_tree()),
                            layout.board(layout.town_centre()),
                            layout.board(layout.grove()),
                        ],
                    );
                }
            }
        }
    }

    #[test]
    fn the_leash_tightens_as_the_player_zooms_in() {
        // A screenful covers less ground the closer the board comes, so the
        // focus has to be held nearer the walk to keep any of it in frame. A
        // bounding-box clamp cannot express this at all: its bound is a
        // property of the ground and takes no notice of the zoom.
        let layout = viewed(BoardCamera::default());
        let field = layout.field();
        let strays: Vec<f32> = [
            BoardCamera::MIN_ZOOM,
            BoardCamera::MAX_ZOOM * 0.5,
            BoardCamera::MAX_ZOOM,
        ]
        .into_iter()
        .map(|zoom| {
            let mut camera = BoardCamera::default();
            camera.zoom_to(layout.scene_center(), layout.scene_center(), zoom);
            drag(&mut camera, layout.scene_center(), Vec2::splat(100_000.0));
            let held = isometric::project(camera.clamped(field, layout.safe_area().size()).focus());
            held.distance(field.nearest(held))
        })
        .collect();

        for pair in strays.windows(2) {
            assert!(
                pair[1] <= pair[0] + 1e-3,
                "zooming in was allowed further from the walk: {strays:?}"
            );
        }
        assert!(
            strays[0] > strays[2] + 1.0,
            "the leash did not tighten at all across the zoom range: {strays:?}"
        );
    }

    #[test]
    fn the_field_grows_with_the_ground_the_player_works() {
        // The leash lengthens with the run rather than fencing the opening
        // village in: a map with a second node further out has a wider field.
        let near = Map::parse(concat!(
            "#######\n",
            "#.....#\n",
            "#.@.*.#\n",
            "#.T...#\n",
            "#######",
        ))
        .expect("parses");
        let far = Map::parse(concat!(
            "###########\n",
            "#.........#\n",
            "#.@.....*.#\n",
            "#.T.......#\n",
            "###########",
        ))
        .expect("parses");
        let field = |map: &Map| {
            SceneLayout::for_map(
                Vec2::new(1280.0, 720.0),
                View::Full,
                map,
                BoardCamera::opening(map),
            )
            .field()
        };
        assert!(field(&far).span() > field(&near).span());
    }

    #[test]
    fn world_positions_snap_to_a_whole_texel_grid() {
        for viewport in VIEWPORTS {
            let layout = SceneLayout::for_viewport(viewport);
            let snapped = layout.snap(11.4);
            let grid_units = snapped / layout.world_scale();
            assert!((grid_units - grid_units.round()).abs() < 0.001);
        }
    }

    #[test]
    fn a_pan_moves_every_sprite_by_the_same_whole_texels() {
        // What `board_snapped` is for. The grid is measured from the board's
        // own origin, so a pan shifts the whole cast together; measured from
        // the corner of the window instead, each actor rounds against a
        // different fraction of a texel and the crowd shimmers against ground
        // that is sliding smoothly underneath it.
        let mut camera = BoardCamera::default();
        let before = viewed(camera);
        let cast = [
            before.town_centre(),
            before.grove(),
            before.home_tree(),
            before.town_centre() + Vec2::new(1.7, -0.3),
            before.town_centre() + Vec2::new(-4.25, 9.1),
        ];

        for nudge in 1..=12 {
            drag(
                &mut camera,
                before.scene_center(),
                Vec2::new(nudge as f32 * 0.37, 0.0),
            );
            let after = viewed(camera);
            let mut moves = cast
                .iter()
                .map(|at| after.board_snapped(*at, 11.0) - before.board_snapped(*at, 11.0));
            let first = moves.next().expect("the cast is not empty");
            for step in moves {
                assert_eq!(
                    step, first,
                    "a pan moved one sprite by {step:?} and another by {first:?}"
                );
            }
        }
    }

    #[test]
    fn the_walk_to_the_grove_recedes_from_the_viewer() {
        // The worked node is north-west of the town, which in this projection
        // is *away*: a monkey setting out has to pass behind everything it
        // started in front of, and arrive drawn behind the stall it left.
        let layout = SceneLayout::default();
        let route = map::WorkedRoute::start();
        let leaving = route.0.sample(0.0).at;
        let arriving = route.0.sample(1.0).at;
        let ground = |at: bevy::math::DVec2| Vec2::new(at.x as f32, at.y as f32);

        assert!(
            isometric::stand_z(ground(arriving), 0.0) < isometric::stand_z(ground(leaving), 0.0)
        );
        // And it climbs the screen the whole way, rather than doubling back.
        let mut previous = layout.board(ground(leaving));
        for step in 1..=20 {
            let at = layout.board(ground(route.0.sample(f64::from(step) / 20.0).at));
            assert!(at.y > previous.y, "the walk doubled back at step {step}");
            previous = at;
        }
    }

    #[test]
    fn buttons_are_only_live_in_the_view_that_shows_them() {
        // A tap on the menu scrim must never reach the shop card underneath.
        let hire = ButtonAction::Hire(UnitKind::Worker);
        assert!(hire.active_in(MenuState::Closed));
        assert!(!hire.active_in(MenuState::Open));
        assert!(!hire.active_in(MenuState::ConfirmRestart));
        assert!(ButtonAction::OpenMenu.active_in(MenuState::Closed));
        assert!(ButtonAction::Resume.active_in(MenuState::Open));
        assert!(!ButtonAction::Resume.active_in(MenuState::Closed));
        assert!(ButtonAction::ConfirmRestart.active_in(MenuState::ConfirmRestart));
        assert!(!ButtonAction::ConfirmRestart.active_in(MenuState::Open));
    }
}
