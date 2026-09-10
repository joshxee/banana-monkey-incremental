//! Worker monkey avatars.
//!
//! Position and pose are derived from [`HarvestCycle`] every frame, so an
//! avatar is a pure function of simulation state: it survives a resize, and it
//! cannot drift out of step with the economy.
//!
//! For the isometric lo-fi pass every monkey is a light rectangle with a brown
//! outline. Final art can replace this presentation without changing cycles.

use bevy::prelude::*;

use crate::{
    domain::{
        CART_CREW, Carts, CycleSpec, HarvestCycle, Multipliers, Segment, Workforce, cycle_time,
    },
    game::{Delivery, DeliveryKind, DeliveryQueue, SceneLayout},
    isometric,
    map::{Map, Route, Village, WorkedRoute},
};

const FRAME_SIZE: u32 = 22;
const MONKEY_SIZE: Vec2 = Vec2::new(13.0, 22.0);
const MONKEY_FILL: Color = Color::srgb(0.94, 0.82, 0.67);
const MONKEY_EDGE: Color = Color::srgb(0.34, 0.17, 0.10);

/// How far a monkey drifts ahead of or behind the point the economy has it at,
/// as a fraction of the whole walk.
///
/// This is what makes a crowd rather than a formation. Two monkeys on the same
/// segment fraction used to be a rigid constellation: the same distance apart
/// for the whole trip, every trip, never passing each other. A per-monkey bulge
/// re-times the *drawing* of the journey without touching its length, so the
/// gap between any two of them opens and closes as they walk.
///
/// Six percent of a sixty-metre walk is three and a half metres, which is a
/// monkey and a half - enough to overtake, small enough that nobody appears to
/// be racing.
const SWARM_WOBBLE: f32 = 0.06;

/// Metres a monkey walks ahead of or behind where the wobble put it.
///
/// Continuous, where this used to be five discrete steps of 1.2 m. Five steps
/// times three rows is fifteen distinct positions, so every fifteenth hire was
/// pixel-identical to the first - and at thirty workers the repeat is what the
/// eye locks onto. It reads as formation precisely because it is one.
const SWARM_SCATTER_METRES: f32 = 7.0;

/// How close to the jungle a monkey is willing to walk.
const CORRIDOR_MARGIN_METRES: f64 = 0.9;
/// The narrowest the swarm ever pinches to, in metres either side of the walk.
const SWARM_HALF_MIN: f64 = 1.0;
/// And the widest it blooms, however much open ground there is.
const SWARM_HALF_MAX: f64 = 4.0;

/// The band standing monkeys form around an endpoint, in metres.
///
/// A band, not a disc and not rows. A disc fills in and reads as a mob; three
/// rows read as inventory. An annulus reads as a crowd gathered *around*
/// something, which is what they are - and it leaves the middle clear, so the
/// depot pad and the palm they are gathered at stay visible.
const RING_INNER: f32 = 2.5;
const RING_OUTER: f32 = 6.0;

/// How far off the walk a cart stands, in metres, beyond the swarm's own edge.
///
/// A vehicle is what walkers pass behind, and drawing it in front is also what
/// keeps its long dwell at the depot off the top of the unloading queue. This
/// used to be spelled `Lane(u32::MAX)`, which is not a lane in front of row
/// zero: `u32::MAX % 3` *is* row zero, and its stagger is worker zero's, so the
/// cart and the first monkey hired stood on the same ground.
const CART_CLEARANCE_METRES: f32 = 1.6;

/// How far either side of a walker its direction of travel is measured over.
///
/// Only matters at a corner, and only to the *offsets*: the walker itself
/// always stands on the route. See `walk_step`.
const SMOOTHING_METRES: f64 = 1.5;
#[derive(Component)]
pub struct Worker;

/// A Net Cart on the route.
///
/// Deliberately not given a depth lane. All three worker lanes are spent, and a
/// fourth would lift a sprite 18 texels off the ground line in a 16-texel grass
/// band, standing it on the sky. The cart separates by drawing *in front*
/// instead, which is what a vehicle should do anyway: workers pass behind it
/// rather than through it, and its dwell at the depot is offset along the route
/// so it does not park on top of the unloading queue.
#[derive(Component)]
pub struct Cart;

/// Prevents a random resume phase from creating income or wages before the
/// worker has completed its first post-resume cycle. The carried banana remains
/// presentation state through the existing segment rules.
#[derive(Component)]
pub(crate) struct RestoredCycle;

/// How many carts still need a restored phase, and the source of those phases.
/// Mirrors [`RestoreWorkers`]; see `spawn_missing_carts` for why a cart needs it
/// far more than a worker does.
#[derive(Resource)]
pub struct RestoreCarts {
    remaining: u32,
    rng: fastrand::Rng,
}

impl RestoreCarts {
    pub fn new(remaining: u32) -> Self {
        Self {
            remaining,
            rng: fastrand::Rng::new(),
        }
    }

    /// Reproducible phases, for a scenario that has to place its carts in the
    /// same spots every time it is launched.
    pub fn with_seed(remaining: u32, seed: u64) -> Self {
        Self {
            remaining,
            rng: fastrand::Rng::with_seed(seed),
        }
    }

    pub fn clear(&mut self) {
        self.remaining = 0;
    }
}

impl Default for RestoreCarts {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Default for RestoreWorkers {
    fn default() -> Self {
        Self::new(0)
    }
}

/// The next lane index to hand out. Monotonic, so an index is never reused.
#[derive(Resource, Debug, Default)]
pub struct NextLane(u32);

impl NextLane {
    fn take(&mut self) -> u32 {
        let index = self.0;
        self.0 = self.0.wrapping_add(1);
        index
    }

    pub fn restart(&mut self) {
        self.0 = 0;
    }
}

/// A worker's hire index. Depth row and along-route stagger are both derived
/// from it, so both stay stable across a reload without storing either.
#[derive(Component, Debug, Clone, Copy)]
pub struct Lane(u32);

/// Salts, so one hire index yields four uncorrelated numbers. Arbitrary odd
/// constants; only their difference matters.
const SALT_WOBBLE: u32 = 0x9E37_79B9;
const SALT_ACROSS: u32 = 0x85EB_CA6B;
const SALT_SCATTER: u32 = 0xC2B2_AE35;
const SALT_ANGLE: u32 = 0x27D4_EB2F;
const SALT_RADIUS: u32 = 0x1656_67B1;

impl Lane {
    /// A stable, well-spread number in `0.0..1.0` for this monkey and purpose.
    ///
    /// Hashed from the hire index rather than drawn from an RNG, and that is
    /// what keeps the swarm free: the index is already persisted, so every
    /// offset survives a save and a reload without being stored, and a monkey
    /// comes back standing where it stood. An RNG would need five more numbers
    /// per worker in the save format to say the same thing.
    fn dial(self, salt: u32) -> f32 {
        // A small integer avalanche. The hire indices handed out are 0, 1, 2,
        // ... - the least random input there is - so the mixing has to do all
        // of the work, and a weaker hash shows up immediately as neighbours
        // walking in step.
        let mut bits = self.0.wrapping_mul(0x9E37_79B9) ^ salt;
        bits ^= bits >> 16;
        bits = bits.wrapping_mul(0x7FEB_352D);
        bits ^= bits >> 15;
        bits = bits.wrapping_mul(0x846C_A68B);
        bits ^= bits >> 16;
        // The top 24 bits, so the value is exact in `f32`.
        (bits >> 8) as f32 / f32::from(1u16 << 8).powi(3)
    }

    /// The same, signed: `-1.0..1.0`.
    fn swing(self, salt: u32) -> f32 {
        self.dial(salt).mul_add(2.0, -1.0)
    }

    /// How far ahead of or behind its economic position this monkey is drawn.
    fn wobble(self) -> f32 {
        self.swing(SALT_WOBBLE) * SWARM_WOBBLE
    }

    /// Where across the corridor it walks, as a fraction of the half-width.
    /// Positive is towards the viewer; [`near_side`] turns that into a side.
    fn across(self) -> f32 {
        self.swing(SALT_ACROSS)
    }

    /// Metres along the route from where the wobble put it.
    fn scatter(self) -> f32 {
        self.swing(SALT_SCATTER) * SWARM_SCATTER_METRES
    }

    /// Where it stands when it is not walking: a bearing, and how far out.
    fn ring(self) -> (f32, f32) {
        let angle = self.dial(SALT_ANGLE) * std::f32::consts::TAU;
        let radius = RING_INNER + self.dial(SALT_RADIUS) * (RING_OUTER - RING_INNER);
        (angle, radius)
    }

    /// A stable, bounded separation for two actors that would otherwise sort
    /// identically. Bounded because an epsilon that grows with the hire index
    /// eventually exceeds a real depth difference, and a crowd starts flickering.
    fn nudge(self) -> f32 {
        (self.0 % 8) as f32 * isometric::NUDGE_STEP
    }
}

/// Where a monkey is *drawn* along the walk, given where the economy has it.
///
/// A sine bulge, and the shape is chosen for one property: `sin(0)` and
/// `sin(PI)` are both zero, so the remap is the identity at **both ends of the
/// walk**. A monkey leaves the depot at exactly the tick the economy says and
/// arrives at exactly the tick it says; all that moves is where it is drawn in
/// between. That is what lets the swarm be free without touching D24 — the
/// cycle boundary is bit-identical at any wobble, which
/// `the_swarm_never_moves_an_arrival` pins.
fn swarm_fraction(fraction: f64, wobble: f32) -> f64 {
    fraction + f64::from(wobble) * (std::f64::consts::PI * fraction).sin()
}

/// How far either side of the route the swarm spreads at a point, in metres.
///
/// A fraction of the *local* corridor, so one crowd reads as two different
/// things: shoulder to shoulder where the walk threads a gap in the jungle, and
/// spread wide where it crosses the open town. On the shipped map that is ten
/// metres of clearance for three quarters of the walk and three at the pinch
/// near the grove, so the swarm visibly squeezes and lets go again.
fn corridor_spread(map: &Map, at: Vec2, across: Vec2) -> f32 {
    let wide = map.corridor_half_width(
        bevy::math::DVec2::new(f64::from(at.x), f64::from(at.y)),
        bevy::math::DVec2::new(f64::from(across.x), f64::from(across.y)),
    );
    (wide - CORRIDOR_MARGIN_METRES).clamp(SWARM_HALF_MIN, SWARM_HALF_MAX) as f32
}

/// The banana a worker carries home. Deliberately *not* the `Banana` marker:
/// that one is claimed by several `Single` queries, which silently skip their
/// whole system when more than one entity matches.
#[derive(Component)]
pub struct CarriedBanana;

/// A brief highlight on a freshly hired worker.
///
/// Every hire now walks out of the stall, which is the purchase's own visible
/// consequence, but the stall is also where every other worker is unloading and
/// eating. The flash separates the new one from that traffic. A colour flash
/// rather than a scale pop, so the sprite stays on the texel grid the rest of
/// the scene is drawn on.
#[derive(Component, Debug, Clone, Copy)]
pub struct JustHired {
    remaining: f32,
}

const HIRE_HIGHLIGHT_SECONDS: f32 = 0.6;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pose {
    Idle,
    Run,
}

/// Workers restored from a save get a random elapsed-time phase. Fresh hires
/// still start at the stall so the purchase has an immediate, legible result.
#[derive(Resource)]
pub struct RestoreWorkers {
    remaining: usize,
    rng: fastrand::Rng,
}

impl RestoreWorkers {
    pub fn new(count: u32) -> Self {
        Self {
            remaining: count as usize,
            rng: fastrand::Rng::new(),
        }
    }

    /// Reproducible phases, so a seeded scenario is the same run every time.
    pub fn with_seed(count: u32, seed: u64) -> Self {
        Self {
            remaining: count as usize,
            rng: fastrand::Rng::with_seed(seed),
        }
    }

    pub fn clear(&mut self) {
        self.remaining = 0;
    }
}

/// Give every worker *in the pool* an avatar, whether the pool grew because of a
/// purchase or because a save was loaded.
///
/// The pool is the workforce less everyone aboard a cart, so it can now shrink -
/// boarding takes monkeys off the route. `board_carts` is the only thing that
/// despawns them, and it decrements the same number this reads, so the two
/// cannot disagree about how many should be on screen.
///
#[allow(clippy::too_many_arguments)]
pub fn spawn_missing_workers(
    mut commands: Commands,
    workforce: Res<Workforce>,
    multipliers: Res<Multipliers>,
    mut restored: ResMut<RestoreWorkers>,
    mut next_lane: ResMut<NextLane>,
    carts: Res<Carts>,
    existing: Query<Entity, With<Worker>>,
) {
    let target = workforce.count().saturating_sub(carts.crewed()) as usize;
    let current = existing.iter().count();
    debug_assert!(
        current <= target,
        "the pool shrank without despawning avatars"
    );
    if current >= target {
        return;
    }

    for _ in current..target {
        // A monotonic hand-out, never `existing.count()`. Boarding despawns
        // workers, so a count-derived index gets reused: hire a replacement and
        // it lands in the same lane at the same stagger as the monkey that left,
        // which is exactly the pixel-identical overlap `stagger_texels` exists
        // to prevent.
        let index = next_lane.take();
        let was_restored = restored.remaining > 0;
        let cycle = if was_restored {
            restored.remaining -= 1;
            HarvestCycle::from_phase(
                restored.rng.f64() * cycle_time(CycleSpec::WORKER, *multipliers),
                CycleSpec::WORKER,
                *multipliers,
            )
        } else {
            HarvestCycle::starting(CycleSpec::WORKER)
        };
        let mut worker = commands.spawn((
            Worker,
            cycle,
            CycleSpec::WORKER,
            Lane(index),
            Pose::Run,
            Sprite::from_color(MONKEY_FILL, MONKEY_SIZE),
            Transform::from_xyz(0.0, 0.0, 1.0),
        ));
        if was_restored {
            worker.insert(RestoredCycle);
        } else {
            worker.insert(JustHired {
                remaining: HIRE_HIGHLIGHT_SECONDS,
            });
        }
        worker.with_children(|parent| {
            spawn_monkey_outline(parent, MONKEY_SIZE);
            parent.spawn((
                CarriedBanana,
                Sprite::from_color(Color::srgb(1.0, 0.78, 0.10), Vec2::splat(7.0)),
                Transform::from_xyz(5.5, 7.0, 0.2)
                    .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)),
                Visibility::Hidden,
            ));
        });
    }
}

pub(crate) fn spawn_monkey_outline(parent: &mut ChildSpawnerCommands, size: Vec2) {
    const EDGE: f32 = 2.0;
    for (custom_size, translation) in [
        (
            Vec2::new(size.x + EDGE * 2.0, EDGE),
            Vec2::new(0.0, size.y * 0.5),
        ),
        (
            Vec2::new(size.x + EDGE * 2.0, EDGE),
            Vec2::new(0.0, -size.y * 0.5),
        ),
        (Vec2::new(EDGE, size.y), Vec2::new(-size.x * 0.5, 0.0)),
        (Vec2::new(EDGE, size.y), Vec2::new(size.x * 0.5, 0.0)),
    ] {
        parent.spawn((
            Sprite::from_color(MONKEY_EDGE, custom_size),
            Transform::from_xyz(translation.x, translation.y, 0.1),
        ));
    }
}

#[allow(clippy::type_complexity)]
/// A point on the route, in metres, pushed off the centre line by `across`
/// metres and along it by `along` metres.
///
/// The offsets are presentation and only presentation. Every monkey advances by
/// the same dimensionless fraction, so a wider lane shows as a slightly higher
/// apparent speed - never as a different cycle time. Letting arrival be driven
/// by the drawn position instead is the one construction `map` exists to
/// prevent: a drawn path and a cycle time that are two different journeys.
fn walk_step(route: &Route, fraction: f64, sideways: f32, forward: f32) -> (Vec2, Vec2) {
    let ground = |at: bevy::math::DVec2| Vec2::new(at.x as f32, at.y as f32);
    let at = ground(route.sample(fraction).at);

    // The direction of travel, sampled either side of the point rather than
    // taken from the leg the point happens to sit on. A leg's heading rotates
    // *instantly* at a corner, which would swing the offsets with it and
    // teleport an outer-lane monkey sideways by twice its lane width. Averaging
    // across a couple of metres turns that jump into a turn.
    let span = (SMOOTHING_METRES / route.length().max(f64::EPSILON)).min(0.5);
    let behind = ground(route.sample(fraction - span).at);
    let ahead = ground(route.sample(fraction + span).at);
    let along = (ahead - behind).normalize_or_zero();
    // Ninety degrees off the direction of travel, so a crowd spreads across the
    // route however the route happens to be pointing.
    let across = Vec2::new(-along.y, along.x);

    (at + across * sideways + along * forward, along)
}

/// Where a walking monkey is drawn, and which way it faces.
fn walk_point(map: &Map, route: &Route, fraction: f64, lane: Lane) -> (Vec2, Vec2) {
    let drawn = swarm_fraction(fraction, lane.wobble());
    // The corridor is measured where this monkey actually is, not where the
    // crowd's centre is: at the pinch near the grove the difference between the
    // two is the difference between walking the gap and walking the hedge.
    let (centre, along) = walk_step(route, drawn, 0.0, 0.0);
    let across = Vec2::new(-along.y, along.x);
    let spread = corridor_spread(map, centre, across);
    walk_step(
        route,
        drawn,
        near_side(route) * lane.across() * spread,
        lane.scatter(),
    )
}

/// Where a monkey stands when it is not walking, and which way it faces.
///
/// An endpoint is where a crowd *gathers*, and gathering is a different shape
/// from walking: the lane-and-stagger grid that reads as a swarm in motion
/// reads as a stock list standing still. So a standing monkey takes a bearing
/// and a radius instead, and the group becomes a ring around the thing it is
/// queueing at.
fn stand_point(route: &Route, fraction: f64, lane: Lane) -> (Vec2, Vec2) {
    let step = route.sample(fraction);
    let at = Vec2::new(step.at.x as f32, step.at.y as f32);
    let along = Vec2::new(step.heading.x as f32, step.heading.y as f32);
    let (angle, radius) = lane.ring();
    (at + Vec2::from_angle(angle) * radius, along)
}

/// Which way `walk_step`'s "across" leans towards the viewer, as +1 or -1.
///
/// Every offset across the route — the three worker rows and the cart's bay —
/// is written as *metres towards the viewer* and turned into a side by this.
/// A fixed sign instead puts the front row at the front on this map and at the
/// back on one that runs the other way, so the shade cue reads backwards and a
/// cart parks behind the queue it is supposed to lead.
///
/// Taken from the route's end-to-end bearing rather than from the heading at
/// the sampled point, and that is the important half. Sampled per point, a
/// route that bends through the bearing where "across" is flat on screen would
/// flip the whole crowd to the other side of the path mid-stride.
fn near_side(route: &Route) -> f32 {
    let ground = |at: bevy::math::DVec2| Vec2::new(at.x as f32, at.y as f32);
    let along = ground(route.sample(1.0).at) - ground(route.sample(0.0).at);
    let across = Vec2::new(-along.y, along.x);
    if isometric::depth(across) >= 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// Where a cart stands, and which way it faces.
///
/// Just outside the swarm's own edge rather than at a fixed distance, so it
/// leads the crowd through the pinch instead of parking in the hedge beside it:
/// the ground the swarm has to squeeze through is exactly the ground the cart
/// has to squeeze through.
fn cart_point(map: &Map, route: &Route, fraction: f64) -> (Vec2, Vec2) {
    let (centre, along) = walk_step(route, fraction, 0.0, 0.0);
    let across = Vec2::new(-along.y, along.x);
    let bay = corridor_spread(map, centre, across) + CART_CLEARANCE_METRES;
    walk_step(route, fraction, near_side(route) * bay, 0.0)
}

/// Everything `position_workers` touches on one worker.
type WorkerView<'a> = (
    Entity,
    &'a HarvestCycle,
    &'a Lane,
    Option<Mut<'a, JustHired>>,
    Mut<'a, Transform>,
    Mut<'a, Sprite>,
);

pub fn position_workers(
    time: Res<Time>,
    mut commands: Commands,
    layout: Res<SceneLayout>,
    village: Res<Village>,
    route: Res<WorkedRoute>,
    multipliers: Res<Multipliers>,
    mut workers: Query<WorkerView, With<Worker>>,
) {
    for (entity, cycle, lane, hired, mut transform, mut sprite) in &mut workers {
        let progress = cycle.segment_fraction(CycleSpec::WORKER, *multipliers);
        // How far along the walk, measured from the town centre. The economy
        // decides this and the map decides where it is: a monkey advances by
        // the shared, dimensionless segment fraction, so its lane changes how
        // fast it *appears* to move and never how long its cycle takes.
        let (fraction, outbound) = match cycle.segment() {
            Segment::ToGrove => (progress, true),
            Segment::Pick => (1.0, true),
            Segment::ToDepot => (1.0 - progress, false),
            // Unloading and then eating both happen at the stall, so the
            // monkey stays put and keeps facing it.
            Segment::Unload | Segment::Snack => (0.0, false),
        };
        let standing = matches!(
            cycle.segment(),
            Segment::Pick | Segment::Unload | Segment::Snack
        );

        // Walking and standing are two different shapes. See `stand_point`.
        let (point, along) = if standing {
            stand_point(&route.0, fraction, *lane)
        } else {
            walk_point(&village, &route.0, fraction, *lane)
        };
        let travel = if outbound { along } else { -along };
        let facing_right = isometric::project(travel).x >= 0.0;

        // Depth cue, from where across the corridor it walks rather than from a
        // row index there no longer is: fully forward is unshaded, fully back
        // is a shade darker.
        let back = (1.0 - lane.across()) * 0.5;
        let half_height = FRAME_SIZE as f32 * 0.5 * layout.world_scale();

        let screen = layout.board_snapped(point, half_height);
        let translation = screen.extend(isometric::stand_z(point, lane.nudge()));
        let scale = Vec3::splat(layout.world_scale());
        // Written only on change: a worker stands still through Pick and
        // Unload, and transform propagation is `Changed<Transform>`-driven.
        if transform.translation != translation {
            transform.translation = translation;
        }
        if transform.scale != scale {
            transform.scale = scale;
        }
        if sprite.flip_x != !facing_right {
            sprite.flip_x = !facing_right;
        }

        let shade = 1.0 - 0.18 * back;
        let base = MONKEY_FILL.to_srgba();
        let mut tint = Vec3::new(base.red, base.green, base.blue) * shade;
        // A worker stuck waiting for a banana to eat has stopped producing, and
        // the player has to be able to see why the rate died. Idling at the
        // stall alone is ambiguous - unloading looks the same.
        //
        // A worker no longer has a hungry state to signal. Its meal is reserved
        // out of the delivery it has just made, so neither the player's spending
        // nor the support wage bill can reach it, and the stall it used to show
        // is unreachable by construction. The pulse moved to `support`, where
        // monkeys really do live on somebody else's surplus - see
        // `HarvestCycle::earmarked`.
        if let Some(mut hired) = hired {
            hired.remaining -= time.delta_secs();
            if hired.remaining <= 0.0 {
                commands.entity(entity).remove::<JustHired>();
            } else {
                let strength = (hired.remaining / HIRE_HIGHLIGHT_SECONDS).clamp(0.0, 1.0);
                // Toward gold, and brightening, so the new hire reads against
                // both the ground and the other workers.
                tint = tint.lerp(Vec3::new(2.0, 1.7, 0.6), strength);
            }
        }
        let colour = Color::srgb(tint.x, tint.y, tint.z);
        if sprite.color != colour {
            sprite.color = colour;
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn animate_workers(
    mut workers: Query<(&HarvestCycle, &mut Pose, &Children), With<Worker>>,
    mut carried: Query<&mut Visibility, With<CarriedBanana>>,
) {
    for (cycle, mut pose, children) in &mut workers {
        let segment = cycle.segment();
        let walking = segment.is_walking();
        let next_pose = if walking { Pose::Run } else { Pose::Idle };
        if *pose != next_pose {
            *pose = next_pose;
        }

        for child in children.iter() {
            if let Ok(mut visibility) = carried.get_mut(child) {
                // Held through the snack too: that banana is the meal, and
                // seeing it in hand is what connects the counter's dip to the
                // monkey that caused it.
                let wanted = if segment.holds_banana() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *visibility != wanted {
                    *visibility = wanted;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Map;

    /// A route with a real corner in it. The shipped walk is a single straight
    /// leg, so anything asserted only against that is asserting nothing.
    fn bent_route() -> Route {
        Map::parse(concat!(
            "#########\n",
            "#.......#\n",
            "#.@.#.*.#\n",
            "#...#...#\n",
            "#.......#\n",
            "#########",
        ))
        .expect("the test map parses")
        .reference_route()
    }

    /// The shipped map, for the corridor the swarm is drawn across.
    fn village() -> &'static Map {
        crate::map::start()
    }

    #[test]
    fn the_swarm_never_moves_an_arrival() {
        // The rule the whole increment is built to respect (D24). Every offset
        // is *drawn*, never simulated, and the fraction remap is the one that
        // could break that: it re-times the journey. The sine bulge is chosen
        // because it vanishes at both ends, so however far a monkey drifts in
        // the middle, it leaves the depot and reaches the grove at exactly the
        // fraction the economy says.
        //
        // Asserted at the extremes of the wobble and exactly, not nearly: a
        // boundary that is off by a float's worth is a tick that is off by a
        // float's worth, and the contracts in `sim_tests` assert exact ticks.
        for wobble in [SWARM_WOBBLE, -SWARM_WOBBLE, 0.0] {
            assert_eq!(swarm_fraction(0.0, wobble), 0.0, "wobble {wobble}");
            assert_eq!(swarm_fraction(1.0, wobble), 1.0, "wobble {wobble}");
        }
        // And every monkey the game can hire is inside those extremes.
        for index in 0..5_000u32 {
            let wobble = Lane(index).wobble();
            assert!(
                wobble.abs() <= SWARM_WOBBLE,
                "worker {index} wobbles {wobble}, past the bound"
            );
            assert_eq!(swarm_fraction(0.0, wobble), 0.0);
            assert_eq!(swarm_fraction(1.0, wobble), 1.0);
        }
        // The drawn fraction also stays on the route rather than running off
        // either end of it, which `Route::sample` would otherwise clamp into a
        // pile at the endpoint.
        for step in 0..=1_000 {
            let fraction = f64::from(step) / 1000.0;
            for wobble in [SWARM_WOBBLE, -SWARM_WOBBLE] {
                let drawn = swarm_fraction(fraction, wobble);
                assert!((0.0..=1.0).contains(&drawn), "{fraction} became {drawn}");
            }
        }
    }

    #[test]
    fn monkeys_overtake_each_other_instead_of_holding_formation() {
        // The defect this increment exists to fix. Two monkeys at the same
        // economic fraction used to be a rigid constellation: the same distance
        // apart for the whole trip, every trip, never passing.
        //
        // Neither half of the offset does this alone, which is why both exist.
        // The wobble is a sine bulge, so the gap it opens is proportional to
        // `sin(PI f)` - it grows and shrinks but never changes sign. The
        // scatter is a constant. Added together, the varying term can overtake
        // the constant one, and *that* is where passing comes from.
        let route = WorkedRoute::start().0;
        let length = route.length() as f32;
        // Where a monkey is along the walk, in metres, as it is drawn.
        let along = |fraction: f64, lane: Lane| {
            swarm_fraction(fraction, lane.wobble()) as f32 * length + lane.scatter()
        };

        let mut passes = 0;
        let mut pairs = 0;
        for index in 0..60u32 {
            let (a, b) = (Lane(index), Lane(index + 1));
            pairs += 1;
            // Asymmetric fractions on purpose: `sin(PI f)` takes the same value
            // at 0.15 and 0.85, so a pair sampled either side of the midpoint
            // has an identical gap and any test using them proves nothing.
            let early = along(0.1, a) - along(0.1, b);
            let middle = along(0.5, a) - along(0.5, b);
            assert!(
                (early - middle).abs() > 1e-3,
                "workers {index} and {} hold a constant gap",
                index + 1
            );
            if early.signum() != middle.signum() {
                passes += 1;
            }
        }
        // Not every neighbouring pair swaps order - that would be its own kind
        // of formation - but a real share of them must, or the crowd is still
        // just breathing in and out around fixed places.
        // Every pair in the crowd, not just neighbouring hire indices: two
        // monkeys hired one after another have wildly different scatters, so
        // the pairs that actually swap are the ones whose scatters are close.
        let crowd: Vec<Lane> = (0..60).map(Lane).collect();
        let mut swapped = 0;
        let mut total = 0;
        for (index, a) in crowd.iter().enumerate() {
            for b in &crowd[index + 1..] {
                total += 1;
                let early = along(0.1, *a) - along(0.1, *b);
                let middle = along(0.5, *a) - along(0.5, *b);
                if early.signum() != middle.signum() {
                    swapped += 1;
                }
            }
        }
        // 201 of 1770 as it stands. The bar is set below that rather than at
        // it: the exact figure is a property of the hash, and pinning it would
        // make any future change to the mixing look like a regression in the
        // swarm.
        assert!(
            swapped * 100 >= total * 8,
            "only {swapped} of {total} pairs in the crowd change order \
             ({passes} of {pairs} neighbouring)"
        );
    }

    #[test]
    fn no_two_monkeys_in_a_crowd_are_drawn_alike() {
        // The old lane yielded fifteen distinct offsets - three rows times five
        // stagger steps - so every fifteenth hire was pixel-identical to the
        // first, and at thirty workers the repeat is what the eye locks onto.
        // Sixty is the crowd the `swarm` scenario shows.
        let mut seen: Vec<(u32, u32, u32)> = Vec::new();
        for index in 0..60u32 {
            let lane = Lane(index);
            let print = (
                lane.wobble().to_bits(),
                lane.across().to_bits(),
                lane.scatter().to_bits(),
            );
            assert!(
                !seen.contains(&print),
                "worker {index} is drawn exactly like an earlier one"
            );
            seen.push(print);
        }
        // And the offsets are spread rather than clustered: over a crowd, both
        // sides of the route are used and the middle is not the only place
        // anybody stands.
        let across: Vec<f32> = (0..60).map(|index| Lane(index).across()).collect();
        assert!(across.iter().any(|value| *value < -0.5));
        assert!(across.iter().any(|value| *value > 0.5));
        assert!(across.iter().any(|value| value.abs() < 0.35));
    }

    #[test]
    fn the_swarm_pinches_where_the_jungle_pinches() {
        // One crowd reading as two different things is the point of measuring
        // the corridor: shoulder to shoulder where the walk threads a gap, wide
        // across the open town. If these ever come out equal the swarm has
        // stopped noticing the map and is back to a constant lane width.
        let route = WorkedRoute::start().0;
        let spread_at = |fraction: f64| {
            let (centre, along) = walk_step(&route, fraction, 0.0, 0.0);
            corridor_spread(village(), centre, Vec2::new(-along.y, along.x))
        };
        let open = spread_at(0.4);
        let pinch = spread_at(0.9);
        assert!(
            pinch < open,
            "the swarm is {pinch} m wide at the gap and {open} m in the open"
        );
        // And it never spreads into the jungle it is squeezing past.
        for step in 0..=200 {
            let fraction = f64::from(step) / 200.0;
            let spread = spread_at(fraction);
            assert!(
                (SWARM_HALF_MIN as f32..=SWARM_HALF_MAX as f32).contains(&spread),
                "the swarm is {spread} m wide at {fraction}"
            );
        }
    }

    #[test]
    fn a_standing_crowd_is_a_ring_and_not_a_grid() {
        // Three rows read as inventory and a filled disc reads as a mob. A ring
        // reads as a crowd gathered around something - and it leaves the middle
        // clear, which is what keeps the depot pad and the palm visible under
        // the monkeys queueing at them.
        let route = WorkedRoute::start().0;
        let depot = Vec2::new(route.sample(0.0).at.x as f32, route.sample(0.0).at.y as f32);
        let mut bearings = [false; 8];
        for index in 0..60u32 {
            let (at, _) = stand_point(&route, 0.0, Lane(index));
            let out = at.distance(depot);
            assert!(
                (RING_INNER..=RING_OUTER).contains(&out),
                "worker {index} stands {out} m from the depot"
            );
            let offset = at - depot;
            let sector = ((offset.y.atan2(offset.x) + std::f32::consts::TAU)
                % std::f32::consts::TAU
                / (std::f32::consts::TAU / 8.0)) as usize;
            bearings[sector.min(7)] = true;
        }
        assert!(
            bearings.iter().all(|used| *used),
            "a crowd of sixty left a gap in the ring: {bearings:?}"
        );
    }

    #[test]
    fn the_two_test_routes_lean_opposite_ways() {
        // The guard on the corridor and cart tests: if both routes ever agreed
        // on a side they would go on passing while asserting half of what they
        // say they do.
        assert_eq!(
            near_side(&WorkedRoute::start().0) * near_side(&bent_route()),
            -1.0
        );
    }

    #[test]
    fn a_cart_keeps_its_own_bay_outside_the_swarm() {
        // Spelling this `Lane(u32::MAX)` did not put the cart in front of row
        // zero, it put the cart *in* row zero - `u32::MAX % 3` is 0, and its
        // stagger is worker zero's - so a cart and the first monkey hired stood
        // on the same ground. It now sits outside whatever the swarm's own edge
        // is at that point on the walk, so it leads the crowd through the pinch
        // rather than parking in the hedge beside it.
        let route = WorkedRoute::start().0;
        for step in 1..20 {
            let fraction = f64::from(step) / 20.0;
            let (cart, _) = cart_point(village(), &route, fraction);
            let (centre, along) = walk_step(&route, fraction, 0.0, 0.0);
            let across = Vec2::new(-along.y, along.x);
            let spread = corridor_spread(village(), centre, across);
            for index in 0..60u32 {
                // The lateral question only: a worker genuinely further along
                // the route than the cart is nearer the viewer and *should*
                // draw in front, so comparing whole positions would be asking
                // the bay question and the scatter question at once.
                let (worker, _) = walk_step(
                    &route,
                    fraction,
                    near_side(&route) * Lane(index).across() * spread,
                    0.0,
                );
                assert!(
                    isometric::stand_z(cart, 0.0) > isometric::stand_z(worker, 0.0),
                    "worker {index} draws in front of the cart at {fraction}"
                );
            }
        }
    }

    #[test]
    fn an_offset_changes_apparent_speed_and_never_cycle_time() {
        // Offsets are drawn, not simulated: every monkey is at the same
        // fraction at the same tick, so a monkey on the outside covers slightly
        // more ground in the same time and that is the whole of the difference.
        //
        // Measured on a route that *turns*. On a straight one the offsets are
        // constant and this can only ever pass, which is the trap the first
        // version of this test fell into.
        let walk = |map: &Map, route: &Route, lane: Lane| {
            let mut walked = 0.0;
            let mut previous = walk_point(map, route, 0.0, lane).0;
            for step in 1..=400 {
                let at = walk_point(map, route, f64::from(step) / 400.0, lane).0;
                walked += previous.distance(at);
                previous = at;
            }
            walked
        };

        // On the shipped walk - one straight leg - the drawn journey differs
        // from the nominal one only by the corridor opening and closing under
        // the monkey, which is a few metres over sixty.
        let shipped = WorkedRoute::start().0;
        let nominal = shipped.length() as f32;
        for index in 0..30u32 {
            let walked = walk(village(), &shipped, Lane(index));
            assert!(
                (walked - nominal).abs() / nominal < 0.15,
                "worker {index} walks {walked} m against a nominal {nominal} m"
            );
        }

        // Around a corner an outer offset genuinely covers more ground - that
        // is what the outside of a turn costs, not a defect. What has to hold
        // is that the excess stays bounded by the offsets times the total
        // turning, so it is proportional to the offset rather than running away
        // with the route's shape.
        let bent = bent_route();
        let map = Map::parse(concat!(
            "#########\n",
            "#.......#\n",
            "#.@.#.*.#\n",
            "#...#...#\n",
            "#.......#\n",
            "#########",
        ))
        .expect("the test map parses");
        let nominal = bent.length() as f32;
        for index in 0..30u32 {
            let lane = Lane(index);
            // Every offset counts: a scatter along the route swings with the
            // heading at a corner just as a lateral one does, and the wobble
            // stretches the sampling on top of both.
            let offset = SWARM_HALF_MAX as f32 + lane.scatter().abs();
            let bound = offset * std::f32::consts::PI + nominal * SWARM_WOBBLE + 0.05;
            let excess = (walk(&map, &bent, lane) - nominal).abs();
            assert!(
                excess <= bound,
                "worker {index} is {excess} m over a bent walk, past a bound of {bound}"
            );
        }
    }

    #[test]
    fn an_outer_offset_turns_a_corner_instead_of_teleporting_across_it() {
        // Taking the offset axis from the current leg's heading swings it
        // through the whole turn in one frame, and an outer monkey jumps
        // sideways by twice its offset. Nothing on the shipped map turns yet,
        // so only a bent route can see this.
        let route = bent_route();
        let map = Map::parse(concat!(
            "#########\n",
            "#.......#\n",
            "#.@.#.*.#\n",
            "#...#...#\n",
            "#.......#\n",
            "#########",
        ))
        .expect("the test map parses");
        let lane = Lane(0);
        let mut previous = walk_point(&map, &route, 0.0, lane).0;
        for step in 1..=400 {
            let at = walk_point(&map, &route, f64::from(step) / 400.0, lane).0;
            let jump = previous.distance(at);
            assert!(
                jump < 0.4,
                "an outer offset jumped {jump} m in one four-hundredth of a walk"
            );
            previous = at;
        }
    }
}

// ────────────────────────────────────────────────────────────────── carts

/// Spawn a box for every cart bought, and give the crewed ones a cycle.
///
/// A cart is only advanced once it is full: `Segment::Pick`'s rate is
/// `crew × M_tech / t_pick`, so an empty box has a picking rate of zero, and
/// `HarvestCycle::advance` would divide a remaining distance by nothing. The
/// `Boarding` marker is what keeps it out of the advance query - a clamp would
/// hide the same bug behind a plausible number.
pub fn spawn_missing_carts(
    mut commands: Commands,
    carts: Res<Carts>,
    multipliers: Res<Multipliers>,
    mut restored: ResMut<RestoreCarts>,
    existing: Query<Entity, With<Cart>>,
) {
    let current = existing.iter().count() as u32;
    for index in current..carts.owned() {
        // A restored cart gets a random elapsed phase, exactly as a restored
        // worker does. Starting it at zero would be far worse than for a worker:
        // a cart cycle is about 102 s and 100 bananas, so every reload would discard
        // up to a whole trip, and a player whose session cadence is shorter than
        // a cart cycle would get *zero* cart income, permanently, with nothing
        // on screen to explain it. `RestoredCycle` keeps the placement from
        // creating income on that first partial trip.
        let was_restored = restored.remaining > 0;
        let cycle = if was_restored {
            restored.remaining -= 1;
            HarvestCycle::from_phase(
                restored.rng.f64() * cycle_time(CycleSpec::CART, *multipliers),
                CycleSpec::CART,
                *multipliers,
            )
        } else {
            HarvestCycle::starting(CycleSpec::CART)
        };
        let mut cart = commands.spawn((
            Cart,
            Boarding,
            CycleSpec::CART,
            cycle,
            CartIndex(index),
            Sprite::from_color(CART_BOX, CART_BOX_TEXELS),
            Transform::default(),
        ));
        if was_restored {
            cart.insert(RestoredCycle);
        }
        cart.with_children(|cart| {
            cart.spawn((
                CartLoad,
                Sprite::from_color(CART_LOAD, CART_LOAD_TEXELS),
                // Behind the box's front wall and in front of the riders, so
                // the pile reads as being *in* the cart.
                Transform::from_xyz(0.0, 0.0, 0.001),
                Visibility::Hidden,
            ));
            // Riders are *children* of the box, which buys two things: their
            // position and scale come from the parent for free, and a
            // restart that despawns the cart takes them with it. Spawned
            // separately they outlived it, and the next cart bought found
            // its seats already occupied by ghosts.
            //
            // Three of them, always. A cart is crewed by exactly three
            // monkeys or it does not run, so the sprite count is not a
            // staffing readout - it is a fill gauge while boarding, and the
            // whole crew afterwards.
            for seat in 0..CART_CREW {
                let offset = (seat as f32 - (CART_CREW as f32 - 1.0) * 0.5) * SEAT_STEP_TEXELS;
                let mut seat_entity = cart.spawn((
                    CartSeat { cart: index, seat },
                    Sprite::from_color(MONKEY_FILL, Vec2::new(10.0, 16.0)),
                    // Sitting in the box: feet behind its front wall, heads
                    // clear of the top. Local texels, so the parent's world
                    // scale applies without this having to know it.
                    Transform::from_xyz(offset, FRAME_SIZE as f32 * 0.30, -0.001),
                    Visibility::Hidden,
                ));
                seat_entity.with_children(|seat| {
                    spawn_monkey_outline(seat, Vec2::new(10.0, 16.0));
                });
            }
        });
    }
}

/// A cart that is still filling up.
#[derive(Component)]
pub struct Boarding;

/// The pile of bananas inside a cart, scaled to what it is currently carrying.
///
/// Without it the two segments a cart spends 93% of its life in look identical:
/// a still brown box parked at the grove for 67 s and a still brown box parked
/// at the depot for 100 s. The whitepaper's whole cart argument is "it barely
/// travels and instead sits at the depot being emptied", and the Unpacker
/// purchase only explains itself if the player can see the emptying. Every
/// other actor signals its segment - run pose, carried banana, hunger pulse -
/// and the cart signalled nothing.
#[derive(Component)]
pub struct CartLoad;

/// Which cart this is, so seats can find their box without a parent lookup.
#[derive(Component, Debug, Clone, Copy)]
pub struct CartIndex(pub(crate) u32);

/// One rider's seat on one cart.
#[derive(Component, Debug, Clone, Copy)]
pub struct CartSeat {
    cart: u32,
    seat: u32,
}

/// The box, in source texels. Wide enough to seat three monkeys shoulder to
/// shoulder, low enough that their heads clear the top - the whole read is
/// "three monkeys in a box", so the box must not swallow them.
const CART_BOX_TEXELS: Vec2 = Vec2::new(52.0, 15.0);
const CART_BOX: Color = Color::srgb(0.55, 0.33, 0.14);
/// The bananas piled in the box. Banana-yellow, and the only large yellow mass
/// in the scene, so a loaded cart is distinguishable from the Unpacker's crate
/// at a glance - the two are otherwise both brown rectangles at the depot.
const CART_LOAD: Color = Color::srgb(0.98, 0.82, 0.20);
/// The load, at full payload, in source texels. Inset so the box's own walls
/// still read as walls.
const CART_LOAD_TEXELS: Vec2 = Vec2::new(46.0, 9.0);
/// Seat spacing inside the box.
const SEAT_STEP_TEXELS: f32 = 15.0;

/// Fill the boarding cart from the pool.
///
/// Only workers standing at the stall board, and boarding happens in its own
/// stage *before* the cycles advance - so a boarded monkey never harvests and
/// never eats on the tick it leaves, which is what makes "it costs a trip, not a
/// fee" true by construction rather than by bookkeeping.
///
/// First come, no reservation: the cart takes whoever finishes next. Carts fill
/// in purchase order, so the player never ends up with two carts stuck at 2/3.
pub fn board_carts(
    mut commands: Commands,
    mut carts: ResMut<Carts>,
    mut queue: ResMut<DeliveryQueue>,
    at_stall: Query<(Entity, &HarvestCycle), With<Worker>>,
) {
    let mut berths = carts.berths_open();
    if berths == 0 {
        return;
    }

    for (entity, cycle) in &at_stall {
        if berths == 0 {
            break;
        }
        // Snack is the moment a worker is standing still at the stall - the one
        // point in the cycle where it is carrying nothing and going nowhere.
        if cycle.segment() != Segment::Snack {
            continue;
        }
        // It does still *owe* something: the meal reserved out of the delivery
        // it has just made. Despawning without settling that would hand the
        // monkey a free lunch on its way aboard, which is the one thing D18
        // refuses to allow - and it would leave `Committed` describing a
        // reservation no entity holds.
        if cycle.earmarked() > 0.0 {
            queue.entries.push(Delivery {
                amount: cycle.earmarked(),
                kind: DeliveryKind::Snack,
            });
        }
        commands.entity(entity).despawn();
        carts.board();
        berths -= 1;
    }
}

/// Take the `Boarding` marker off a cart once its crew is complete, so it joins
/// the advance query and starts its first trip.
pub fn launch_crewed_carts(
    mut commands: Commands,
    carts: Res<Carts>,
    boarding: Query<(Entity, &CartIndex), With<Boarding>>,
) {
    for (entity, index) in &boarding {
        if carts.crewed() >= (index.0 + 1) * CART_CREW {
            commands.entity(entity).remove::<Boarding>();
        }
    }
}

/// Every cart on the route, and whether it has launched.
type CartAvatarQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static HarvestCycle,
        Option<&'static Boarding>,
        &'static mut Transform,
    ),
    (With<Cart>, Without<CartLoad>),
>;

/// Position every cart, and reveal the riders that have boarded.
///
/// Only the box is placed: the seats are its children, so their offsets and
/// scale follow for free. All this loop decides is how many of them are visible.
#[allow(clippy::too_many_arguments)]
pub fn position_carts(
    layout: Res<SceneLayout>,
    village: Res<Village>,
    route: Res<WorkedRoute>,
    multipliers: Res<Multipliers>,
    carts_res: Res<Carts>,
    mut carts: CartAvatarQuery,
    mut seats: Query<(&CartSeat, &mut Visibility, &mut Sprite), Without<CartLoad>>,
    mut loads: Query<(&ChildOf, &mut Transform, &mut Visibility, &mut Sprite), With<CartLoad>>,
) {
    let scale = layout.world_scale();
    // Which way the crew faces. A cart heading out to the grove is travelling
    // left, so a permanently flipped rider rides backwards for half the trip.
    let mut facing_left = true;

    let mut carried: Vec<(Entity, f32)> = Vec::new();

    for (entity, cycle, boarding, mut transform) in &mut carts {
        let progress = cycle.segment_fraction(CycleSpec::CART, *multipliers);
        // Its own bay at each end, on the *inside* of the route, so the cart
        // never parks on the unloading queue and never reverses into the palm.
        let dwell = f64::from(layout.cart_offset()) / route.0.length();
        let (grove, stall) = (1.0 - dwell, dwell);
        let fraction = if boarding.is_some() {
            // An unlaunched cart waits at the depot, visibly filling up.
            stall
        } else {
            match cycle.segment() {
                Segment::ToGrove => stall + (grove - stall) * progress,
                Segment::Pick => grove,
                Segment::ToDepot => grove + (stall - grove) * progress,
                Segment::Unload | Segment::Snack => stall,
            }
        };

        // A vehicle draws in *front* of the walkers rather than beside them:
        // workers pass behind it instead of through it. That is one lane's
        // worth of ground towards the viewer, which the depth rule then
        // handles on its own.
        let (point, _) = cart_point(&village, &route.0, fraction);
        transform.translation = layout
            .board_snapped(point, CART_BOX_TEXELS.y * 0.5 * scale)
            .extend(isometric::stand_z(point, isometric::NUDGE_STEP));
        transform.scale = Vec3::splat(scale);
        facing_left = !matches!(cycle.segment(), Segment::ToDepot) || boarding.is_some();

        // How full the box is, 0..=1. Rises as it is picked, stays full for the
        // ride home, drains as it is unloaded, and is empty on the way out.
        let load = if boarding.is_some() {
            0.0
        } else {
            match cycle.segment() {
                Segment::ToGrove => 0.0,
                Segment::Pick => progress,
                Segment::ToDepot => 1.0,
                Segment::Unload => 1.0 - progress,
                Segment::Snack => 0.0,
            }
        };
        carried.push((entity, load as f32));
    }

    for (parent, mut transform, mut visibility, mut sprite) in &mut loads {
        let load = carried
            .iter()
            .find(|(entity, _)| *entity == parent.parent())
            .map_or(0.0, |(_, load)| *load);
        let wanted = if load > 0.01 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
        // Grows from the floor of the box rather than from its centre, so a
        // half-load sits in the bottom half like a pile rather than floating.
        let height = CART_LOAD_TEXELS.y * load;
        let size = Vec2::new(CART_LOAD_TEXELS.x, height);
        if sprite.custom_size != Some(size) {
            sprite.custom_size = Some(size);
        }
        let y = -CART_LOAD_TEXELS.y * 0.5 + height * 0.5;
        if transform.translation.y != y {
            transform.translation.y = y;
        }
    }

    for (seat, mut visibility, mut sprite) in &mut seats {
        if sprite.flip_x != facing_left {
            sprite.flip_x = facing_left;
        }

        // A seat fills only once its monkey has actually climbed aboard, so an
        // empty box visibly gains riders one at a time. That filling is the only
        // feedback during the boarding wait, and the wait is the longest dead
        // stretch in the game.
        let aboard = carts_res
            .crewed()
            .saturating_sub(seat.cart * CART_CREW)
            .min(CART_CREW);
        let wanted = if seat.seat < aboard {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}
