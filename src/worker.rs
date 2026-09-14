//! Worker monkey avatars.
//!
//! Position and pose are derived from [`HarvestCycle`] every frame, so an
//! avatar is a pure function of simulation state: it survives a resize, and it
//! cannot drift out of step with the economy.
//!
//! The simulation spawns a worker with no art on it; [`dress_actors`] gives it
//! a sprite, a playhead, a shadow and a banana to carry, and the presentation
//! systems pose all four from the cycle every frame.

use bevy::prelude::*;

use crate::{
    art::{self, Art, CartClip, CartSheets, Clip, Facing},
    domain::{
        CART_CREW, Carts, CycleSpec, HarvestCycle, Multipliers, Segment, Workforce, cycle_time,
    },
    game::{Delivery, DeliveryKind, DeliveryQueue, SceneLayout},
    isometric,
    map::{Map, Route, Village, WorkedRoute},
};

/// A walker's contact shadow: the green of the art's own baked shadows, a
/// third opaque, so a crowd's shadows pool into a darker patch rather than
/// stacking into black.
const SHADOW: Color = isometric::SHADOW_COLOUR;

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
///
/// The outer radius is inside the depot's own trodden ground (`DEPOT_RADIUS`
/// plus its scuffed ring is 5 m from the centre), so the pad contains the crowd
/// that stands on it instead of the crowd spilling onto plain grass around it.
const RING_INNER: f32 = 2.2;
pub(crate) const RING_OUTER: f32 = 4.8;

/// How much of the ring is left open at the back, as a fraction of the circle.
///
/// A full annulus is symmetric on the ground and *asymmetric on screen*: every
/// sprite grows upward from its feet, so the far arc's bodies pile over the
/// middle while the near arc's feet leave the near half bare. The result reads
/// as a heap beside the landmark rather than a crowd around it.
///
/// Leaving the up-screen arc empty fixes both halves at once - the pad and the
/// trunk stay visible, and a crowd at a counter stands in front of it anyway,
/// not behind it.
const RING_OPEN_BACK: f32 = 0.34;

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

    /// Hand out indices from `base` instead of from zero.
    ///
    /// Only a test wants this, and the test it exists for is the one that
    /// proves the index cannot reach the economy: run the same world from two
    /// different bases and every tick, delivery and banana has to match to the
    /// bit. Every swarm offset is a hash of this number, so if any of them ever
    /// crossed into `FixedUpdate`, this is what would notice.
    #[cfg(test)]
    pub fn set_base(&mut self, base: u32) {
        self.0 = base;
    }
}

/// A worker's hire index. Every offset that makes this monkey draw differently
/// from the one beside it is derived from it, so all of them stay stable across
/// a reload without any of them being stored.
#[derive(Component, Debug, Clone, Copy)]
pub struct Lane(u32);

/// Salts, so one hire index yields six uncorrelated numbers.
///
/// Arbitrary odd constants, but not *entirely* arbitrary: the avalanche below
/// has zero as a fixed point, so `dial(index, salt) == 0` exactly at the index
/// that multiplies to the salt. Using the mixer's own first multiplier as a
/// salt put that index at **1** — the second monkey ever hired sat at exactly
/// the extreme of its range, every game, forever. These are chosen so the zero
/// index of each stream is past any reachable hire count.
const SALT_WOBBLE: u32 = 0xBF58_476D;
const SALT_ACROSS: u32 = 0x85EB_CA6B;
/// The second dial summed into `across`: see [`Lane::across`].
const SALT_ACROSS_SPREAD: u32 = 0x94D0_49BB;
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
        // The top 24 bits over 2^24: the numerator is below 2^24, so the
        // quotient is exact in `f32` and lands in `0.0..1.0`.
        (bits >> 8) as f32 / 16_777_216.0
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
    ///
    /// Triangular, not uniform: the sum of two dials, less one. A uniform draw
    /// fills the corridor at one density right up to its edges, so a crowd of
    /// sixty reads as a strip with ruled sides - a road painted brown rather
    /// than monkeys walking one. A triangle is densest on the route's spine
    /// and thins linearly to nothing at the edge, so the crowd has a core and
    /// a ragged fringe. Same range, `-1.0..1.0`, so nothing that bounds a
    /// monkey inside the corridor has to change; only where in it they tend
    /// to walk.
    fn across(self) -> f32 {
        self.dial(SALT_ACROSS) + self.dial(SALT_ACROSS_SPREAD) - 1.0
    }

    /// Metres along the route from where the wobble put it.
    fn scatter(self) -> f32 {
        self.swing(SALT_SCATTER) * SWARM_SCATTER_METRES
    }

    /// Where it stands when it is not walking: a bearing, and how far out.
    ///
    /// The bearing is drawn from the open part of the circle only, then rotated
    /// so the gap faces *away* from the viewer. On the isometric plane that is
    /// the direction of increasing depth, which is a constant - the ring is
    /// built around a point, not along a route, so it has no bearing of its own
    /// to measure against.
    fn ring(self) -> (f32, f32) {
        /// Up-screen, in ground metres: the direction a sprite's body covers.
        const AWAY: f32 = -std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4;
        let sweep = std::f32::consts::TAU * (1.0 - RING_OPEN_BACK);
        let angle =
            AWAY + std::f32::consts::TAU * RING_OPEN_BACK * 0.5 + self.dial(SALT_ANGLE) * sweep;
        let radius = RING_INNER + self.dial(SALT_RADIUS) * (RING_OUTER - RING_INNER);
        (angle, radius)
    }

    /// The spot this monkey stands on, in metres from whatever it is gathered
    /// around.
    ///
    /// Picked once, from the hire index, and therefore *known before the monkey
    /// sets out* - which is the whole of what D31 needs. Nothing reserves it,
    /// so two monkeys can share one; the alternative is a claim that has to be
    /// released when a monkey boards a cart or the game restarts, for a crowd
    /// the player reads as a crowd either way.
    fn spot(self) -> Vec2 {
        let (angle, radius) = self.ring();
        Vec2::from_angle(angle) * radius
    }

    /// A stable, bounded separation for two actors that would otherwise sort
    /// identically. Bounded because an epsilon that grows with the hire index
    /// eventually exceeds a real depth difference, and a crowd starts flickering.
    fn nudge(self) -> f32 {
        (self.0 % 8) as f32 * isometric::NUDGE_STEP
    }

    /// The hire index itself. Only for ordering one monkey against another -
    /// a courier has to pick the same harvester to help two frames running, and
    /// the hire index is the one stable name a harvester has.
    pub(crate) fn index(self) -> u32 {
        self.0
    }

    /// A lane with a given hire index, for a test that has to stand a named
    /// harvester somewhere.
    #[cfg(test)]
    pub(crate) fn hire(index: u32) -> Self {
        Self(index)
    }
}

/// Where a monkey is *drawn* along the walk, given where the economy has it.
///
/// A sine bulge, chosen for one property: the remap is the identity at **both
/// ends of the walk**, so a monkey is drawn leaving the depot and reaching the
/// grove at exactly the fractions the economy has it at, and all that moves is
/// where it is drawn in between.
///
/// That identity is a floating-point fact rather than an algebraic one, and it
/// is worth being exact about it. `sin(PI)` in `f64` is 1.22e-16, not zero, so
/// `f + a·sin(PI·f)` is only *exactly* 1.0 at f = 1 while `|a|·1.22e-16` stays
/// under half an ulp — which holds for `|a| < 0.907`. There is fifteen thousand
/// times that margin at the shipped 0.06, and
/// `the_swarm_never_moves_an_arrival` asserts the equality rather than trusting
/// the algebra.
///
/// Two further bounds on `a`, neither of them the aesthetic one:
/// - **Monotonicity** needs `|a| < 1/PI = 0.318`, since `d/df = 1 + a·PI·cos(PI·f)`
///   has minimum `1 - |a|·PI`. Past it a monkey would visibly walk backwards.
///   Monotonic plus fixed endpoints is also what makes the range exactly [0,1],
///   so no separate range argument is needed.
/// - **Apparent speed** is the one that actually binds. The drawn speed is
///   `v·(1 ± a·PI)`, so the shipped 0.06 already means a monkey leaves 19% fast
///   and arrives 19% slow. That is the budget to spend, long before 1/PI.
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

/// A contact shadow under an actor. A child, so it follows for free; its depth
/// is pinned to [`isometric::MARK_Z`] every frame instead, so it can never draw
/// over the monkey behind its owner.
#[derive(Component)]
pub(crate) struct Shadow;

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
    /// Seconds since the hire, for the walk out of the bins (see [`emerge`]).
    age: f32,
}

const HIRE_HIGHLIGHT_SECONDS: f32 = 0.6;

/// How fast a fresh hire walks out of the bins at its quickest, in metres a
/// second: a harvester's own pace.
const EMERGE_SPEED: f32 = 3.0;

/// Where a fresh hire is drawn, `age` seconds after it was hired, on its way
/// from the bins at `from` to the place in the crowd it is headed for at `to` -
/// and whether it has got there.
///
/// Every monkey comes out of the treehouse (D30). The walk is timed by the
/// distance, not by the hire flash: a smoothstep peaks at one and a half times
/// its average speed, so a duration of `1.5 d / EMERGE_SPEED` holds the peak
/// to a walking pace. Spent over the 0.6 s flash instead, a monkey bound for
/// the outer ring left the bins at twelve metres a second.
fn emerge(from: Vec2, to: Vec2, age: f32) -> (Vec2, bool) {
    let duration = 1.5 * from.distance(to) / EMERGE_SPEED;
    if duration <= 0.0 || age >= duration {
        return (to, true);
    }
    let t = age / duration;
    (from.lerp(to, t * t * (3.0 - 2.0 * t)), false)
}

/// Which loop a monkey is playing, and where it has got to.
///
/// Presentation state, so the presentation makes it: [`dress_actors`] inserts
/// it, and the simulation's spawns know nothing about animation. It is also the
/// only record of whether a monkey is walking - a separate `Pose` component
/// used to be computed from the same test in the same loop and was never read.
///
/// A playhead per monkey rather than one shared clock, and that is the point:
/// it is seeded from the hire index, so sixty monkeys spawned on the same tick
/// are already spread across the stride. A shared clock would have every one of
/// them plant the same foot at the same moment, which is the formation read the
/// swarm offsets exist to break - reintroduced in the one channel those offsets
/// cannot reach.
#[derive(Component, Debug)]
pub(crate) struct Playing {
    clip: Clip,
    frame: u32,
    /// Which way it faces. Decided where the monkey is placed, from the way it
    /// is going, and drawn where it is posed.
    facing: Facing,
    /// Seconds into the current idle frame.
    elapsed: f32,
    /// How far through one walk loop, as a fraction of a stride. Advanced by
    /// distance drawn, never by time: see [`art::WALK_STRIDE_TEXELS`].
    stride: f32,
    /// Texels walked since the playhead last looked, on the unit-zoom board.
    travelled: f32,
    /// Where the monkey was last drawn standing, in metres.
    last: Option<Vec2>,
}

impl Playing {
    fn starting(clip: Clip, index: u32) -> Self {
        // The golden ratio walks the unit interval without ever repeating a
        // pattern a crowd could line up on, which `index % frames` does every
        // twelfth hire.
        let stride = (index as f32 * 0.618_034).fract();
        let frame = if clip.is_walk() {
            Clip::walk_frame(stride)
        } else {
            index % Clip::Idle.frames()
        };
        Self {
            clip,
            frame,
            facing: Facing::SE,
            elapsed: 0.0,
            stride,
            travelled: 0.0,
            last: None,
        }
    }

    /// Where the monkey was last drawn standing, in metres, once it has been
    /// drawn at all.
    ///
    /// The one way anything outside this module learns where a harvester
    /// actually *is*: its `Transform` is a screen position at the board's zoom,
    /// and a courier that has to run between a monkey and the bins needs the
    /// ground under it. Nothing in `FixedUpdate` may read this - it is a drawn
    /// position, and the economy is not allowed to depend on one.
    pub(crate) fn standing(&self) -> Option<Vec2> {
        self.last
    }

    /// Record where the monkey is drawn this frame.
    fn walk_to(&mut self, point: Vec2) {
        if let Some(last) = self.last {
            // Along the ground, in the texels the stride was measured in:
            // the same step whichever of the eight ways the monkey faces.
            self.travelled += art::walked_texels(point - last);
        }
        self.last = Some(point);
    }
}

/// The loop a segment is drawn on: walking or standing, and whether a walking
/// monkey has the banana in its hand.
fn clip_for(segment: Segment) -> Clip {
    match (segment.is_walking(), segment.holds_banana()) {
        (false, _) => Clip::Idle,
        (true, false) => Clip::Walk,
        (true, true) => Clip::CarryWalk,
    }
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
            Transform::from_xyz(0.0, 0.0, 1.0),
        ));
        if was_restored {
            worker.insert(RestoredCycle);
        } else {
            worker.insert(JustHired {
                remaining: HIRE_HIGHLIGHT_SECONDS,
                age: 0.0,
            });
        }
        // The banana it carries home. Spawned bare, like the monkey: it is a
        // simulation fact that the worker has a banana to hold, and the
        // presentation's business what one looks like and where it rides.
        worker.with_child((CarriedBanana, Transform::default(), Visibility::Hidden));
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
///
/// `swarm` scales the offsets that make a crowd a crowd - the scatter along the
/// route and the spread across it - without touching where on the route the
/// monkey is. One as it walks, and closing to zero as it takes up its standing
/// spot: the spot is a place, so a monkey settling onto one has to stop being
/// scattered around the route as well as arrive at it.
///
/// It is the *along* half of that taper that matters, and it is not cosmetic.
/// The scatter throws a monkey up to seven metres past the end of the route,
/// and a spot reaches only 4.8 m from it, so without the taper a monkey that
/// overshot had to be dragged back up the route to settle - a backwards slide,
/// at the moment D31 promises there is none.
fn walk_point(map: &Map, route: &Route, fraction: f64, lane: Lane, swarm: f32) -> (Vec2, Vec2) {
    let drawn = swarm_fraction(fraction, lane.wobble());
    let scatter = lane.scatter() * swarm;
    // The corridor is measured **after** the along-route scatter, at the point
    // the monkey is actually standing. Measuring it at the unscattered point
    // and then displacing by up to seven metres asks the width of one place and
    // spends it at another - and near the grove, where the gap closes over a
    // few metres, that put monkeys a metre and a half inside the jungle wall.
    let (scattered, along) = walk_step(route, drawn, 0.0, scatter);
    let across = Vec2::new(-along.y, along.x);
    let spread = corridor_spread(map, scattered, across);
    walk_step(
        route,
        drawn,
        near_side(route) * lane.across() * spread * swarm,
        scatter,
    )
}

/// How far out from an endpoint a monkey is already walking to its own spot,
/// in metres.
///
/// The whole of D31's pathing change is this number. A monkey used to walk the
/// route to its end and *then* step aside onto the standing ring - up to ten
/// metres of sidestep, spent in the first third of `Pick` or `Unload`, at the
/// one moment in the cycle the player is watching. It now picks its spot before
/// it sets out and closes on it over the last stretch of the walk, so it
/// *arrives* at the spot instead of arriving beside it and sliding in.
///
/// Sixteen metres, and the number is measured rather than chosen. Two things
/// bind it, both of them properties of the *drawn* path that
/// `a_monkey_never_walks_backwards_on_its_way_in_or_out` and
/// `the_approach_is_walked_rather_than_sidestepped` hold:
///
/// - **Forwards.** The swarm offsets close over the same ramp the spot opens
///   on, and the faster that happens the more of the closing shows as motion
///   against the walk. Ten metres left a third of the crowd being dragged
///   *backwards* up the route - as much as 2.5 m of it - because the scatter
///   throws a monkey up to seven metres past the end of the walk while a spot
///   reaches only 4.8 m from it. At sixteen there is no reverse at all, on any
///   lane, on either leg.
/// - **Sideways.** The spot is up to `RING_OUTER` off the route, and covering
///   it has to read as walking a curve rather than as drifting. The worst lane
///   spends 0.44 m across the route per metre along it over its approach, under
///   the half a metre the second test holds.
///
/// A quarter of a sixty-metre leg, so a monkey walks the middle half of every
/// walk on the corridor offsets and the crowd still reads as a crowd.
const APPROACH_METRES: f64 = 16.0;

/// Where a monkey is drawn, given where the economy has it and how much of its
/// standing spot it has taken up.
///
/// An endpoint is where a crowd *gathers*, and gathering is a different shape
/// from walking: the offsets that read as a swarm in motion read as a stock
/// list standing still, so a standing monkey takes a bearing and a radius and
/// the group becomes a ring around the bins it is queueing at.
///
/// The spot is added as an **offset that fades in**, not lerped to as a
/// position, and that is the whole of what makes the approach monotone. Lerping
/// between the walking point and a fixed spot mixes two things that move at
/// different speeds: the route point keeps advancing while the weight is still
/// short of one, so the drawn point overshoots the spot and is pulled back onto
/// it. Measured on the shipped route that was up to 2.5 m of backwards drag on
/// a third of the crowd - a slide, in the along axis, at exactly the moment the
/// player is watching. Added as an offset there is nothing to overshoot: the
/// route advances, the swarm offsets close, the spot opens, and every term is
/// monotone.
///
/// No anchor is passed in, and none is needed. At `weight` 1 the fraction *is*
/// an endpoint - that is what `Phase::of` guarantees - so `route.sample` is
/// already the point the spot is measured from.
fn stand_point(
    map: &Map,
    route: &Route,
    fraction: f64,
    lane: Lane,
    ring_weight: f32,
) -> (Vec2, Vec2) {
    let (walking, along) = walk_point(map, route, fraction, lane, 1.0 - ring_weight);
    (walking + lane.spot() * ring_weight, along)
}

/// Smoothstep, so an approach starts and ends at rest rather than snapping
/// into motion.
fn ease(progress: f32) -> f32 {
    let eased = progress.clamp(0.0, 1.0);
    eased * eased * (3.0 - 2.0 * eased)
}

/// How much of one leg of `route` the approach to a spot takes up.
///
/// Capped at half the leg, so the peel-off and the closing-in never overlap
/// however short a map's walk is - at exactly half, a monkey leaves one spot
/// and crosses straight to the next with no corridor in between. And held off
/// zero at the other end: `travel_ring` divides by this, and a leg long enough
/// to round it to nothing would hand the ramps a 0/0 rather than a weight.
fn approach_fraction(route: &Route) -> f64 {
    (APPROACH_METRES / route.length().max(f64::EPSILON)).clamp(1e-3, 0.5)
}

/// How much of its standing spot a monkey has taken up `progress` of the way
/// along a travelling segment.
///
/// One function for both ends, because a leg has two: the monkey peels off the
/// spot it left over the first `approach`, walks the middle on the corridor
/// offsets, and closes on the spot it is bound for over the last `approach`.
/// Whichever weight is higher wins, so the two can never pull against each
/// other: on a leg shorter than two approaches they hand straight over at the
/// middle, and a monkey crosses from one spot to the next with no corridor in
/// between.
///
/// Which *end* the weight refers to needs no answer here. A weight of one only
/// ever happens at `progress` 0 or 1, where the route fraction is already the
/// endpoint the spot is measured from - so `stand_point` reads it off the route
/// rather than being told.
fn travel_ring(progress: f64, approach: f64) -> f32 {
    leaving_ring(progress, approach).max(arriving_ring(progress, approach))
}

/// How much of the spot it started on a traveller still has, `progress` into a
/// leg: 1 as it sets out, 0 once it is `approach` clear of it.
fn leaving_ring(progress: f64, approach: f64) -> f32 {
    1.0 - ease((progress / approach) as f32)
}

/// And how much of the spot it is bound for it has taken up: 0 until the last
/// `approach` of the leg, 1 exactly as it arrives.
fn arriving_ring(progress: f64, approach: f64) -> f32 {
    ease(((progress - (1.0 - approach)) / approach) as f32)
}

/// Where a monkey is drawn, given only where its cycle has it.
///
/// The whole of the cycle's shape, in one place, so the continuity across a
/// segment boundary is a property of a function a test can walk rather than of
/// two `match` arms that happen to agree.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Phase {
    /// How far along the walk, measured from the town centre.
    fraction: f64,
    /// Which way it is facing.
    outbound: bool,
    /// How far between the walking offsets and the standing spot.
    ring_weight: f32,
}

impl Phase {
    /// `approach` is how much of this leg the walk spends closing on a spot, as
    /// a fraction of it: see [`APPROACH_METRES`].
    fn of(segment: Segment, progress: f64, approach: f64) -> Self {
        // The economy decides the progress and the map decides where that is: a
        // monkey advances by the shared, dimensionless segment fraction, so its
        // offsets change how fast it *appears* to move and never how long its
        // cycle takes.
        //
        // A travelling segment leaves one spot and closes on the next; the
        // three standing segments are *on* a spot throughout. So every boundary
        // hands over at weight 1, which is what
        // `a_whole_cycle_is_drawn_without_a_single_jump` walks end to end - and
        // a monkey standing still now never moves at all, where it used to
        // spend the first third of `Pick` and `Unload` sidestepping.
        let (fraction, outbound, ring_weight) = match segment {
            Segment::ToGrove => (progress, true, travel_ring(progress, approach)),
            Segment::Pick => (1.0, true, 1.0),
            Segment::ToDepot => (1.0 - progress, false, travel_ring(progress, approach)),
            // Unloading and then eating both happen at the bins, so the monkey
            // stays put and keeps facing them.
            Segment::Unload => (0.0, false, 1.0),
            Segment::Snack => (0.0, false, 1.0),
        };
        Self {
            fraction,
            outbound,
            ring_weight,
        }
    }
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
    // Outside the swarm, but never outside the corridor: the clearance the
    // cart is given is a metre and a half past the crowd's own edge, and on a
    // map with a tighter neck than this one that sum would put a vehicle in
    // the hedge.
    let clearance = corridor_spread(map, centre, across) + CART_CLEARANCE_METRES;
    let room = map.corridor_half_width(
        bevy::math::DVec2::new(f64::from(centre.x), f64::from(centre.y)),
        bevy::math::DVec2::new(f64::from(across.x), f64::from(across.y)),
    ) - CORRIDOR_MARGIN_METRES;
    let bay = clearance.min(room.max(0.0) as f32);
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
    Mut<'a, Playing>,
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
    let approach = approach_fraction(&route.0);
    for (entity, cycle, lane, hired, mut transform, mut sprite, mut playing) in &mut workers {
        let progress = cycle.segment_fraction(CycleSpec::WORKER, *multipliers);
        let Phase {
            fraction,
            outbound,
            ring_weight,
        } = Phase::of(cycle.segment(), progress, approach);

        // Walking and standing are two different shapes, blended rather than
        // switched between, and the blend is spent *walking in* rather than on
        // arrival. See `stand_point`.
        let (mut point, along) = stand_point(&village, &route.0, fraction, *lane, ring_weight);
        let travel = if outbound { along } else { -along };
        let mut facing = Facing::of_ground(travel);
        // A fresh hire walks out of the bins - the treehouse, which is where
        // every monkey appears from (D30) - to its place in the crowd, facing
        // the way it is going rather than the way the route will take it.
        let mut emerged = true;
        if let Some(hired) = hired.as_ref() {
            let bins = isometric::tile_centre(village.town_centre());
            let target = point;
            let (drawn, arrived) = emerge(bins, target, hired.age);
            if !arrived {
                point = drawn;
                facing = Facing::of_ground(target - bins);
            }
            emerged = arrived;
        }

        // Depth cue, taken from where the monkey is actually drawn rather than
        // from `across`, which describes only the walking shape: a monkey on
        // the near arc of a standing ring is placed by `ring()` and would
        // otherwise be shaded by an offset it is not using, so the front of a
        // crowd could come out darkest.
        let centre = route.0.sample(fraction).at;
        let from_route = point - Vec2::new(centre.x as f32, centre.y as f32);
        let back = (0.5 - isometric::depth(from_route) * 0.25).clamp(0.0, 1.0);
        // No lift: the art is anchored at the monkey's feet, so the ground
        // position *is* the transform.
        let screen = layout.board_snapped(point, 0.0);
        let translation = screen.extend(isometric::stand_z(point, lane.nudge()));
        // Unit z scale, so a child's local z is a real depth offset: the
        // shadow is pinned to the ground-mark layer by subtracting this
        // monkey's depth, and a z scale of the zoom would multiply that back.
        let scale = Vec3::new(layout.world_scale(), layout.world_scale(), 1.0);
        playing.walk_to(point);
        // Written only on change: a worker stands still through Pick and
        // Unload, and transform propagation is `Changed<Transform>`-driven.
        if transform.translation != translation {
            transform.translation = translation;
        }
        if transform.scale != scale {
            transform.scale = scale;
        }
        // Drawn by `animate_workers`, which picks the row or the mirror.
        playing.facing = facing;

        // The art's own colours are the colours; `sprite.color` carries only
        // what the game says on top of them - this depth shade, and the flash.
        let shade = 1.0 - 0.18 * back;
        let mut tint = Vec3::splat(shade);
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
            hired.age += time.delta_secs();
            if hired.remaining <= 0.0 && emerged {
                commands.entity(entity).remove::<JustHired>();
            } else if hired.remaining > 0.0 {
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
/// Give every actor the simulation has spawned its sprite.
///
/// The spawning belongs to `SimulationPlugin`, which runs headless with no
/// asset server and no art at all - a contract test steps a thousand ticks of a
/// sixty-monkey economy without a window. So the simulation makes the monkey
/// and the presentation dresses it, one frame later, and the seam holds.
///
/// It also removes a smell that predated the art: the spawn systems used to
/// build `Sprite`s of their own, which worked only because a coloured rectangle
/// needs no resource to make. The first sprite that needed one broke every
/// headless contract at once.
///
/// Runs first in the chain that poses the cast, in the same frame the
/// simulation spawned the actor (`FixedUpdate` runs before `Update`), so there
/// is no frame on which a monkey is drawn undressed or on the wrong loop.
pub fn dress_actors(
    mut commands: Commands,
    art: Res<Art>,
    assets: Res<AssetServer>,
    sheets: Option<Res<CartSheets>>,
    workers: Query<(Entity, &Lane, &HarvestCycle), (With<Worker>, Without<Sprite>)>,
    bananas: Query<Entity, (With<CarriedBanana>, Without<Sprite>)>,
    carts: Query<Entity, (With<Cart>, Without<Sprite>)>,
) {
    for (entity, lane, cycle) in &workers {
        // On the loop its segment wants from the first frame: a monkey
        // restored mid-unload used to open on a walk frame and swap a frame
        // later.
        let clip = clip_for(cycle.segment());
        let playing = Playing::starting(clip, lane.0);
        commands
            .entity(entity)
            .insert((
                art.worker(clip, playing.facing, playing.frame),
                art::WORKER.anchor(),
                playing,
            ))
            .with_child((
                Shadow,
                art.shadow(art::SHADOW_TEXELS, SHADOW),
                Transform::default(),
            ));
    }
    for entity in &bananas {
        commands.entity(entity).insert(art.carried_banana());
    }
    if !carts.is_empty() {
        // The cart's sheets arrive with the first cart rather than at startup:
        // see `CartSheets`.
        let sheets = match sheets {
            Some(sheets) => CartSheets::clone(&sheets),
            None => {
                let loaded = CartSheets::load(&assets);
                commands.insert_resource(loaded.clone());
                loaded
            }
        };
        for entity in &carts {
            commands.entity(entity).insert((
                art.cart(&sheets, CartClip::TravelEmpty, Facing::SE, 0),
                art::CART.anchor(),
            ));
        }
    }
}

/// What `animate_workers` touches on a worker's children.
type CarriedView<'a> = (Mut<'a, Transform>, Mut<'a, Visibility>);

#[allow(clippy::type_complexity)]
pub fn animate_workers(
    time: Res<Time>,
    art: Res<Art>,
    mut workers: Query<
        (
            &HarvestCycle,
            &Transform,
            &mut Playing,
            &mut Sprite,
            &Children,
        ),
        With<Worker>,
    >,
    mut carried: Query<CarriedView, (With<CarriedBanana>, Without<Worker>, Without<Shadow>)>,
    mut shadows: Query<&mut Transform, (With<Shadow>, Without<Worker>, Without<CarriedBanana>)>,
) {
    // Where the banana rides, relative to the feet, for a monkey facing right.
    let back = art::WORKER.offset_of(art::WORKER_BACK);

    for (cycle, transform, mut playing, mut sprite, children) in &mut workers {
        let segment = cycle.segment();

        // Walking or standing, and whether the banana is in hand, which is the
        // whole of what the three loops have to say. Switching clips keeps the
        // frame index rather than resetting it, so a crowd that all stops at
        // once does not all restart its idle on frame zero together.
        let wanted = clip_for(segment);
        if playing.clip != wanted {
            playing.clip = wanted;
            playing.frame %= wanted.frames();
            playing.elapsed = 0.0;
        }

        let travelled = std::mem::take(&mut playing.travelled);
        if playing.clip.is_walk() {
            // The feet grip the ground: one loop per stride actually drawn,
            // whatever the speed. See `art::WALK_STRIDE_TEXELS`. The empty
            // and carrying walks are drawn in step, so the stride carries
            // across a switch between them.
            playing.stride = (playing.stride + travelled / art::WALK_STRIDE_TEXELS).fract();
            playing.frame = Clip::walk_frame(playing.stride);
        } else {
            // Capped before the loop below spends it: a frame that arrives
            // after a long stall - a backgrounded tab, a breakpoint - would
            // otherwise be paid out one animation frame at a time.
            playing.elapsed = (playing.elapsed + time.delta_secs()).min(1.0);
            while playing.elapsed >= Clip::hold(playing.frame) {
                playing.elapsed -= Clip::hold(playing.frame);
                playing.frame = (playing.frame + 1) % Clip::Idle.frames();
            }
        }
        art.pose(&mut sprite, playing.clip, playing.facing, playing.frame);

        // `flip_x` mirrors the texture and not the children, so anything
        // placed against the art has to be mirrored by hand. The art is not
        // symmetric - head forward, tail back - and an unmirrored banana on a
        // monkey facing left rides on its tail.
        let facing = if sprite.flip_x { -1.0 } else { 1.0 };
        for child in children.iter() {
            if let Ok((mut at, mut visibility)) = carried.get_mut(child) {
                // Held through the snack too: that banana is the meal, and
                // seeing it is what connects the counter's dip to the monkey
                // that caused it. Only while standing: walking, it is in the
                // monkey's hand, drawn into the carry sheet.
                let shown = if segment.holds_banana() && !playing.clip.is_walk() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *visibility != shown {
                    *visibility = shown;
                }
                let place = Vec3::new(back.x * facing, back.y, 0.2);
                if at.translation != place {
                    at.translation = place;
                }
            }
            if let Ok(mut at) = shadows.get_mut(child) {
                // The parent's z scale is one, so this lands the shadow at
                // exactly `MARK_Z` in the world.
                let z = isometric::MARK_Z - transform.translation.z;
                if at.translation.z != z {
                    at.translation.z = z;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{game::SceneLayout, map::Map};

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
        // Relative along-route position is `db*sin(PI f) + ds`, where `db` is
        // the difference of two wobbles over the walk and `ds` the difference
        // of two scatters. `sin` is non-negative with zeros at both ends, so a
        // pair swaps somewhere in the open walk **iff the two differences have
        // opposite signs and the varying one is the larger** - which is a
        // closed form, not something to sample. Sampling two fractions and
        // comparing signs, as this first did, misses every pair that crosses
        // outside the window and reports about a third fewer swaps.
        let route = WorkedRoute::start().0;
        let length = route.length() as f32;
        let crowd: Vec<Lane> = (0..60).map(Lane).collect();

        let mut swapped = 0;
        let mut total = 0;
        for (index, a) in crowd.iter().enumerate() {
            for b in &crowd[index + 1..] {
                total += 1;
                let bulge = (a.wobble() - b.wobble()) * length;
                let scatter = a.scatter() - b.scatter();
                if bulge * scatter < 0.0 && bulge.abs() > scatter.abs() {
                    swapped += 1;
                }
            }
        }
        // Independent uniform draws of the two offsets put this at 14.95%; the
        // shipped crowd measures 18.8%. The bar is set below the *model* rather
        // than below the measurement, so it tests the mechanism and not the
        // hash - any future change to the mixing is free to land anywhere in
        // the distribution without looking like a regression.
        assert!(
            swapped * 100 >= total * 10,
            "only {swapped} of {total} pairs in the crowd ever change order"
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
    }

    #[test]
    fn no_monkey_is_ever_drawn_inside_the_jungle() {
        // What the bounds check here used to claim and could not see. It
        // asserted that `corridor_spread` returned a value inside the range
        // `corridor_spread` clamps to, which is true by construction and cannot
        // fail - while four of the sixty workers in the `swarm` scenario were
        // being drawn up to a metre and a half *inside* the jungle wall near
        // the grove, because the corridor was measured before the along-route
        // scatter and spent after it.
        let route = WorkedRoute::start().0;
        for index in 0..120u32 {
            for step in 0..=400 {
                let fraction = f64::from(step) / 400.0;
                let (at, _) = walk_point(village(), &route, fraction, Lane(index), 1.0);
                let tile = crate::map::Tile::containing(bevy::math::DVec2::new(
                    f64::from(at.x),
                    f64::from(at.y),
                ));
                assert!(
                    village().terrain(tile).passable(),
                    "worker {index} is drawn in the jungle at {fraction}: {at:?} is {:?}",
                    village().terrain(tile)
                );
            }
        }
    }

    #[test]
    fn the_corridor_is_continuous_rather_than_stepped() {
        // The defect the ray-cast replaced: a sampled march answers in whole
        // steps, so the width jumps by a step as the ray creeps forward and the
        // crowd snaps narrower and wider as it walks.
        //
        // Continuity is what is asserted, and it is asserted the way continuity
        // actually shows: halve the sampling interval and the largest jump has
        // to halve with it. A stepped signal does not care how finely it is
        // sampled - its jump stays the size of its step - so this fails the
        // moment anyone puts the march back, and cannot be satisfied by
        // choosing a lenient constant.
        let route = WorkedRoute::start().0;
        let spread_at = |fraction: f64| {
            let (centre, along) = walk_step(&route, fraction, 0.0, 0.0);
            corridor_spread(village(), centre, Vec2::new(-along.y, along.x))
        };
        let worst_jump = |steps: u32| {
            let mut worst: f32 = 0.0;
            let mut previous = spread_at(0.0);
            for step in 1..=steps {
                let spread = spread_at(f64::from(step) / f64::from(steps));
                worst = worst.max((spread - previous).abs());
                previous = spread;
            }
            worst
        };
        let coarse = worst_jump(1_000);
        let fine = worst_jump(2_000);
        assert!(
            fine <= coarse * 0.6,
            "halving the step only took the largest jump from {coarse} to {fine}: \
             the corridor is quantised, not continuous"
        );
    }

    /// Where a monkey is drawn, from segment and progress alone.
    fn drawn_at(route: &Route, lane: Lane, segment: Segment, progress: f64) -> Vec2 {
        let phase = Phase::of(segment, progress, approach_fraction(route));
        stand_point(village(), route, phase.fraction, lane, phase.ring_weight).0
    }

    #[test]
    fn a_monkey_arrives_on_its_spot_rather_than_sliding_onto_it() {
        // D31's whole point. A monkey used to reach the end of the route and
        // *then* step aside onto the ring, spending the first third of `Pick`
        // and of `Unload` walking sideways - at the two moments in the cycle
        // the player is actually watching. It now picks the spot before it
        // leaves and closes on it while it walks, so the three standing
        // segments are standing still.
        let route = WorkedRoute::start().0;
        let depot = Vec2::new(route.sample(0.0).at.x as f32, route.sample(0.0).at.y as f32);
        let grove = Vec2::new(route.sample(1.0).at.x as f32, route.sample(1.0).at.y as f32);
        for index in 0..40u32 {
            let lane = Lane(index);
            // The spot is a bearing and a radius the hire index alone decides,
            // so it is known before the monkey sets out.
            let at_bins = depot + lane.spot();
            let at_grove = grove + lane.spot();

            // Every standing segment is spent on that spot, from its first
            // frame to its last, without moving a millimetre.
            for (segment, spot) in [
                (Segment::Unload, at_bins),
                (Segment::Snack, at_bins),
                (Segment::Pick, at_grove),
            ] {
                for step in 0..=20 {
                    let at = drawn_at(&route, lane, segment, f64::from(step) / 20.0);
                    assert!(
                        at.distance(spot) < 1e-3,
                        "worker {index} is {} m off its spot {step}/20 through {segment:?}",
                        at.distance(spot)
                    );
                }
            }

            // And the walks hand over onto it exactly, at both ends and in both
            // directions: no jump at a boundary, and no step aside after one.
            assert!(drawn_at(&route, lane, Segment::ToGrove, 0.0).distance(at_bins) < 1e-3);
            assert!(drawn_at(&route, lane, Segment::ToGrove, 1.0).distance(at_grove) < 1e-3);
            assert!(drawn_at(&route, lane, Segment::ToDepot, 0.0).distance(at_grove) < 1e-3);
            assert!(drawn_at(&route, lane, Segment::ToDepot, 1.0).distance(at_bins) < 1e-3);
        }
    }

    /// The route's own straight-line direction of travel, and the axis across
    /// it: the shipped walk is one leg, so these are constant along it.
    fn axes(route: &Route) -> (Vec2, Vec2) {
        let end = |at: f64| {
            let p = route.sample(at).at;
            Vec2::new(p.x as f32, p.y as f32)
        };
        let along = (end(0.0) - end(1.0)).normalize();
        (along, Vec2::new(-along.y, along.x))
    }

    #[test]
    fn a_monkey_never_walks_backwards_on_its_way_in_or_out() {
        // The failure the offset form of `stand_point` exists to remove, and
        // the one its predecessor hid. Lerping a *position* between the walking
        // point and a fixed spot mixes two things moving at different speeds:
        // the route point runs past the spot while the weight is still short of
        // one, so a monkey overshoots and is dragged back onto it - up to 2.5 m
        // of reverse, on a third of the crowd, in the along axis, at the moment
        // the player is watching a delivery land.
        //
        // Stated on the axis it went wrong on. Sideways drift towards a spot is
        // the whole point of an approach; going *backwards up the route* is
        // never right, and no amount of it is small enough to allow, so the bar
        // is a float epsilon rather than a tolerance.
        let route = WorkedRoute::start().0;
        let (home, _) = axes(&route);
        for index in 0..60u32 {
            let lane = Lane(index);
            for (segment, heading) in [(Segment::ToDepot, home), (Segment::ToGrove, -home)] {
                let mut previous = drawn_at(&route, lane, segment, 0.0);
                let mut worst = 0.0f32;
                for step in 1..=2000 {
                    let at = drawn_at(&route, lane, segment, f64::from(step) / 2000.0);
                    worst = worst.min((at - previous).dot(heading)).min(worst);
                    previous = at;
                }
                assert!(
                    worst > -1e-4,
                    "worker {index} is dragged {} m backwards during {segment:?}",
                    -worst
                );
            }
        }
    }

    #[test]
    fn the_approach_is_walked_rather_than_sidestepped() {
        // The spot is up to `RING_OUTER` off the route, and that has to be
        // covered *while walking forward*, not as a lateral step.
        //
        // Measured over the approach itself, which is the thing the budget is
        // about. Integrating over the whole half-leg instead - as this did when
        // it was first written - buys three times the forward travel the
        // approach actually has and dilutes the ratio by the same factor: it
        // read a worst case of 0.22 against a bar of 0.5 while a fifth of the
        // crowd was over 0.5 inside the approach, and two lanes were over 2.
        let route = WorkedRoute::start().0;
        let approach = approach_fraction(&route);
        let (home, across) = axes(&route);
        for index in 0..60u32 {
            let lane = Lane(index);
            let (mut forward, mut sideways) = (0.0f32, 0.0f32);
            let from = 1.0 - approach;
            let mut previous = drawn_at(&route, lane, Segment::ToDepot, from);
            for step in 1..=500 {
                let progress = from + approach * f64::from(step) / 500.0;
                let at = drawn_at(&route, lane, Segment::ToDepot, progress);
                let step = at - previous;
                forward += step.dot(home);
                sideways += step.dot(across).abs();
                previous = at;
            }
            assert!(
                forward > 0.0,
                "worker {index} covers {forward} m of route on its way in"
            );
            assert!(
                sideways < forward * 0.5,
                "worker {index} covers {sideways} m across the route against {forward} m \
                 along it *over the approach*: it is sliding, not walking"
            );
        }
    }

    #[test]
    fn an_approach_can_never_outrun_the_leg_it_is_spent_on() {
        // Leaving one spot and closing on the next are the same ramp at either
        // end of a leg, and on a short enough walk they would overlap and fight
        // - the weight would fall towards the spot behind while rising towards
        // the spot ahead, and the monkey would be drawn somewhere neither.
        const STEPS: u32 = 10_000;
        for approach in [0.05, 0.2, 0.5] {
            let mut previous = travel_ring(0.0, approach);
            let mut lowest = previous;
            assert_eq!(previous, 1.0, "a leg starts on the spot it is leaving");
            for step in 1..=STEPS {
                let progress = f64::from(step) / f64::from(STEPS);
                let now = travel_ring(progress, approach);
                // The two ramps never overlap: at any point one of them is the
                // whole weight and the other is nothing, so the weight names
                // exactly one spot and reaches it without being pulled towards
                // the other. Asked of the two halves the shipped function is
                // built from, rather than of copies of their algebra.
                let (leaving, arriving) = (
                    leaving_ring(progress, approach),
                    arriving_ring(progress, approach),
                );
                assert!(leaving <= 1e-6 || arriving <= 1e-6, "the two ramps overlap");
                assert_eq!(now, leaving.max(arriving));
                // And it is continuous as it hands over: a jump here is a jump
                // on the board, on every monkey, at the same moment.
                assert!(
                    (now - previous).abs() < 0.01,
                    "the weight jumps from {previous} to {now} at {step}"
                );
                lowest = lowest.min(now);
                previous = now;
            }
            assert_eq!(previous, 1.0, "a leg ends on the spot it closed on");
            assert_eq!(lowest, 0.0, "the middle of the leg is walked, not drifted");
        }
    }

    #[test]
    fn a_whole_cycle_is_drawn_without_a_single_jump() {
        // The continuity bar used to stop at the edge of the walking segments,
        // and the increment that introduced the standing ring walked straight
        // through the gap: walking and standing shared no term, so a monkey
        // teleported a mean of 5.9 m - three tiles, ten at worst - four times
        // per cycle, at the exact moment the player is watching a delivery
        // land. Thirty times the jump the walking path itself is held to.
        //
        // So the bar now covers the whole cycle, boundaries included, and it is
        // stated in the units that matter: metres per rendered frame. A monkey
        // walks 3 m/s, so a frame of walking is 0.05 m; the sidestep onto the
        // ring is deliberately brisker than walking, and this is what bounds
        // how much brisker.
        const CYCLE_SECONDS: f64 = 50.0;
        const FRAMES: u32 = (CYCLE_SECONDS * 60.0) as u32;
        let route = WorkedRoute::start().0;
        // In the order the cycle touches them, with each segment's share of the
        // cycle: walk out, pick, walk home, unload, eat.
        let cycle = [
            (Segment::ToGrove, 20.0),
            (Segment::Pick, 5.0),
            (Segment::ToDepot, 20.0),
            (Segment::Unload, 2.5),
            (Segment::Snack, 2.5),
        ];

        for index in 0..30u32 {
            let lane = Lane(index);
            let mut drawn = Vec::new();
            for (segment, seconds) in cycle {
                let steps = (FRAMES as f64 * seconds / CYCLE_SECONDS) as u32;
                for step in 0..steps {
                    let phase = Phase::of(
                        segment,
                        f64::from(step) / f64::from(steps),
                        approach_fraction(&route),
                    );
                    drawn.push(
                        stand_point(village(), &route, phase.fraction, lane, phase.ring_weight).0,
                    );
                }
            }
            // Round the cycle, so the seam between the last frame of `Snack`
            // and the first of `ToGrove` is measured like every other.
            let mut worst = 0.0f32;
            let mut at = 0usize;
            for step in 0..drawn.len() {
                let jump = drawn[step].distance(drawn[(step + 1) % drawn.len()]);
                if jump > worst {
                    worst = jump;
                    at = step;
                }
            }
            // As a multiple of the walk's own step rather than in absolute
            // metres, which is what makes the bar mean something: metres per
            // frame scale with `M_speed`, so a bar in metres is only the bar
            // the test was written at, and every Chef hired loosens it.
            //
            // Two nominal steps. The measured worst is 1.76 of them, all of it
            // in the approach, where a monkey is covering its own sideways
            // offset as well as the route. The bar this replaced was 0.30 m -
            // six nominal steps - because it was calibrated against the
            // *sidestep* D31 removed, a ten-metre lateral move smoothstepped
            // over 0.875 s, which peaks at 0.286. It would still pass with that
            // artefact back in.
            let nominal = route.length() as f32 / 20.0 / 60.0;
            assert!(
                worst < 2.0 * nominal,
                "worker {index} moves {worst} m in one frame against a walking step of \
                 {nominal} m, {} of the way through its cycle",
                at as f32 / drawn.len() as f32
            );
        }
    }

    #[test]
    fn every_hash_stream_is_zero_only_past_any_reachable_hire() {
        // The avalanche has zero as a fixed point, so each stream is exactly
        // 0.0 at the one index that multiplies to its salt - and that monkey
        // sits at the extreme of its range, in every game, forever. Spelling
        // the wobble's salt as the mixer's own multiplier put that index at 1:
        // the second monkey ever hired.
        let reachable = crate::domain::MAX_WORKERS;
        for salt in [
            SALT_WOBBLE,
            SALT_ACROSS,
            SALT_ACROSS_SPREAD,
            SALT_SCATTER,
            SALT_ANGLE,
            SALT_RADIUS,
        ] {
            for index in 0..reachable * 4 {
                assert!(
                    Lane(index).dial(salt) != 0.0,
                    "salt {salt:#x} is pinned to zero at hire {index}, inside a \
                     reachable {reachable}"
                );
            }
        }
    }

    #[test]
    fn a_fresh_hire_walks_out_of_the_bins_at_a_walking_pace() {
        // Held to the same continuity bar as the rest of the cycle: at sixty
        // frames a second no step of the walk out is more than a tenth of a
        // metre, even to the far edge of the standing ring - and it arrives,
        // exactly where it was going, rather than snapping there at the end.
        const FRAME: f32 = 1.0 / 60.0;
        let from = Vec2::ZERO;
        for bearing in 0..12 {
            let to = Vec2::from_angle(bearing as f32 * std::f32::consts::TAU / 12.0) * RING_OUTER;
            let (mut last, mut age, mut arrived) = (from, 0.0, false);
            let mut worst: f32 = 0.0;
            while !arrived {
                age += FRAME;
                let (at, done) = emerge(from, to, age);
                worst = worst.max(at.distance(last));
                last = at;
                arrived = done;
                assert!(age < 10.0, "the walk out never arrives");
            }
            assert!(worst < 0.10, "a hire steps {worst} m in one frame");
            assert_eq!(last, to);
        }
    }

    #[test]
    fn a_walking_crowd_has_a_dense_spine_and_a_ragged_edge() {
        // The property `across` exists for, measured on the crowd rather than
        // read off its formula. A triangle on -1..1 has mean |x| of 1/3 and
        // puts 4% beyond 0.8; a uniform draw - what this replaced - gives 1/2
        // and 20%. Each bar sits between the two models rather than just
        // under a measurement, so the distribution is free to move within the
        // triangle's neighbourhood and a regression to a flat strip cannot
        // pass. At a thousand hires the sampling error is under a hundredth.
        let lanes: Vec<f32> = (0..1000).map(|index| Lane(index).across()).collect();
        let mean = lanes.iter().map(|x| x.abs()).sum::<f32>() / lanes.len() as f32;
        assert!(mean < 0.42, "mean |across| is {mean}: the crowd is flat");
        let fringe = lanes.iter().filter(|x| x.abs() > 0.8).count() as f32 / lanes.len() as f32;
        assert!(
            fringe < 0.10,
            "{fringe} of the crowd walks at the corridor's edge"
        );
        // And the two sides stay balanced. This cannot see correlation between
        // the two dials - correlation changes the spread, never the mean - so
        // it is the two bars above that would catch a correlated pair.
        let bias = lanes.iter().sum::<f32>() / lanes.len() as f32;
        assert!(bias.abs() < 0.05, "the crowd leans {bias} to one side");
    }

    #[test]
    fn a_standing_crowd_is_a_horseshoe_open_away_from_the_viewer() {
        // Three rows read as inventory and a filled disc reads as a mob. A band
        // reads as a crowd gathered around something - but a *full* band is
        // symmetric on the ground and asymmetric on screen, because every
        // sprite grows upward from its feet: the far arc's bodies pile over the
        // middle while the near arc's feet leave the near half bare, and the
        // whole thing reads as a heap beside the landmark.
        //
        // So the arc facing away from the viewer is left empty, and these are
        // the two halves of that: the crowd still wraps most of the way round,
        // and the part it does not wrap is the part that would cover the thing
        // they are gathered at.
        let route = WorkedRoute::start().0;
        let depot = Vec2::new(route.sample(0.0).at.x as f32, route.sample(0.0).at.y as f32);

        let mut used = [false; 8];
        for index in 0..60u32 {
            let (at, _) = stand_point(village(), &route, 0.0, Lane(index), 1.0);
            let offset = at - depot;
            let out = offset.length();
            assert!(
                (RING_INNER..=RING_OUTER).contains(&out),
                "worker {index} stands {out} m from the depot"
            );
            // Never in the arc that would cover the landmark. Measured as an
            // angle from the open direction rather than against a guessed
            // depth: the gap is defined as a slice of the circle, so that is
            // what to assert.
            const AWAY: f32 = -std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4;
            let from_gap = {
                let raw = (offset.y.atan2(offset.x) - AWAY).abs() % std::f32::consts::TAU;
                raw.min(std::f32::consts::TAU - raw)
            };
            let half_gap = std::f32::consts::TAU * RING_OPEN_BACK * 0.5;
            assert!(
                from_gap >= half_gap - 1e-3,
                "worker {index} stands {from_gap} rad from the open arc, inside \
                 a gap of {half_gap} either side"
            );
            let bearing =
                (offset.y.atan2(offset.x) + std::f32::consts::TAU) % std::f32::consts::TAU;
            used[((bearing / (std::f32::consts::TAU / 8.0)) as usize).min(7)] = true;
        }
        assert!(
            used.iter().filter(|seen| **seen).count() >= 5,
            "a crowd of sixty huddled into one corner rather than wrapping: {used:?}"
        );
        // And it leans towards the viewer overall, which is the whole reason
        // the gap is where it is.
        let leaning: f32 = (0..60)
            .map(|index| {
                let (at, _) = stand_point(village(), &route, 0.0, Lane(index), 1.0);
                isometric::depth(at - depot)
            })
            .sum();
        assert!(leaning > 0.0, "the crowd leans away from the viewer");
    }

    #[test]
    fn the_standing_crowd_fits_on_the_ground_it_stands_on() {
        // The depot is nine tiles of trodden earth with a scuffed ring around
        // it, five metres from the centre. A crowd drawn wider than that stands
        // on plain grass beside the pad it is supposed to be queueing at, and
        // the pad stops containing its own crowd.
        // The scuffed ring covers every tile within `DEPOT_EDGE_REACH` of the
        // centre tile, so the trodden ground reaches the *far edge* of that
        // tile - half a tile past its centre.
        let reach = (f64::from(crate::isometric::DEPOT_EDGE_REACH) + 0.5) * crate::map::TILE_METRES;
        assert!(
            f64::from(RING_OUTER) <= reach,
            "the ring reaches {RING_OUTER} m against trodden ground {reach} m across"
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
    fn a_cart_leaves_the_route_for_its_bay_rather_than_unloading_on_it() {
        // A cart carries a hundred bananas and spends a hundred seconds giving
        // them up, and it used to do that from a dwell point on the walk - a
        // box the length of three monkeys parked across the queue for twice as
        // long as a harvester's whole trip. Since D31 the freight has its own
        // bins to park around (see `SceneLayout::cart_park`), so the two ends of
        // the cycle are two places.
        let route = WorkedRoute::start().0;
        let approach = approach_fraction(&route);
        let bay = (0.1, 0.9);
        // Parked for the whole of both standing segments, and for the wait
        // before it is crewed: the box fills, empties and boards in its bay.
        for segment in [Segment::Unload, Segment::Snack] {
            for step in 0..=10 {
                let (_, parked) = cart_phase(segment, f64::from(step) / 10.0, false, bay, approach);
                assert_eq!(parked, 1.0, "{segment:?} is spent off the bay");
            }
        }
        assert_eq!(
            cart_phase(Segment::ToGrove, 0.0, true, bay, approach).1,
            1.0
        );
        // And clear of it for the whole of the picking at the far end, which is
        // the one dwell that still happens on the route.
        assert_eq!(cart_phase(Segment::Pick, 0.5, false, bay, approach).1, 0.0);
        // The two legs hand over at the bay exactly, in both directions, so
        // there is no jump at a boundary.
        assert_eq!(
            cart_phase(Segment::ToDepot, 1.0, false, bay, approach).1,
            1.0
        );
        assert_eq!(
            cart_phase(Segment::ToGrove, 0.0, false, bay, approach).1,
            1.0
        );
        assert_eq!(
            cart_phase(Segment::ToDepot, 0.0, false, bay, approach).1,
            0.0
        );
        assert_eq!(
            cart_phase(Segment::ToGrove, 1.0, false, bay, approach).1,
            0.0
        );
        // And the route fraction still runs bay to bay, so the drive itself is
        // unchanged: it is only the last stretch of it that peels off.
        assert!((cart_phase(Segment::ToDepot, 1.0, false, bay, approach).0 - 0.1).abs() < 1e-12);
        assert!((cart_phase(Segment::ToGrove, 1.0, false, bay, approach).0 - 0.9).abs() < 1e-12);
    }

    /// Where a cart is drawn, from its segment and progress alone, on the
    /// shipped layout.
    fn cart_drawn_at(
        layout: &SceneLayout,
        route: &Route,
        bay: u32,
        segment: Segment,
        progress: f64,
    ) -> Vec2 {
        let dwell = f64::from(layout.cart_offset()) / route.length();
        let approach = (CART_APPROACH_METRES / route.length()).clamp(1e-3, 0.5);
        let (fraction, parked) =
            cart_phase(segment, progress, false, (dwell, 1.0 - dwell), approach);
        let (on_route, _) = cart_point(village(), route, fraction);
        drive_into_bay(on_route, layout.cart_park(bay), parked)
    }

    #[test]
    fn a_cart_drives_round_the_unloading_crowd_rather_than_through_it() {
        // The whole of D31's cart argument, and the half of it the first cut
        // got wrong. Giving the freight its own bins takes a hundred-banana box
        // off the queue's ground for the hundred seconds it spends unloading -
        // and then, if the drive is a straight interpolation, puts it back
        // across that ground twice a trip at three times road speed. The road
        // arrives from the up-left and the bins are down-right, so the delivery
        // point is *between* them: the chord went 0.70 m from it, inside
        // `RING_INNER`, over the bins sprite.
        //
        // Both directions, because a cart leaves the same way it arrives.
        let layout = SceneLayout::for_viewport(Vec2::new(1280.0, 720.0));
        let route = WorkedRoute::start().0;
        let depot = layout.town_centre();
        for bay in 0..crate::game::CART_BAYS {
            for segment in [Segment::ToDepot, Segment::ToGrove] {
                let mut nearest = f32::INFINITY;
                let mut worst_step = 0.0f32;
                let mut previous = cart_drawn_at(&layout, &route, bay, segment, 0.0);
                for step in 1..=4000 {
                    let at = cart_drawn_at(&layout, &route, bay, segment, f64::from(step) / 4000.0);
                    nearest = nearest.min(at.distance(depot));
                    worst_step = worst_step.max(at.distance(previous));
                    previous = at;
                }
                assert!(
                    nearest > RING_OUTER,
                    "bay {bay} passes {nearest} m from the delivery point during \
                     {segment:?}, inside the {RING_OUTER} m the crowd stands on"
                );
                // And it drives rather than slides. The drive into the bay is
                // longer than the road it leaves, so some overshoot is
                // inherent - but not the three and a half times that borrowing
                // the harvesters' approach produced for a bay twenty metres off
                // the route.
                let leg_frames = 4000.0_f32;
                let nominal = route.length() as f32 / leg_frames;
                let steps = worst_step / nominal;
                assert!(
                    steps < 2.5,
                    "bay {bay} moves {steps} times its own road step in a frame during \
                     {segment:?}"
                );
            }
        }
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
            let mut previous = walk_point(map, route, 0.0, lane, 1.0).0;
            for step in 1..=400 {
                let at = walk_point(map, route, f64::from(step) / 400.0, lane, 1.0).0;
                walked += previous.distance(at);
                previous = at;
            }
            walked
        };

        // On the shipped walk - one straight leg - the drawn journey differs
        // from the nominal one only by the corridor opening and closing under
        // the monkey. The wobble contributes exactly nothing here: it is a
        // monotone reparametrisation, which adds no arc length on a straight
        // route. Measured worst over thirty workers is 1.95%, so this bar has
        // two times' headroom; it was briefly set at 15%, which would have
        // passed a crowd running nine metres long on a sixty-metre leg.
        let shipped = WorkedRoute::start().0;
        let nominal = shipped.length() as f32;
        for index in 0..30u32 {
            let walked = walk(village(), &shipped, Lane(index));
            assert!(
                (walked - nominal).abs() / nominal < 0.04,
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
            // No wobble term: a monotone reparametrisation of the same
            // polyline traverses the same arc length whatever it does to the
            // timing, so the only excess is what the offsets sweep at a corner.
            let bound = offset * std::f32::consts::PI + 0.05;
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
        let mut previous = walk_point(&map, &route, 0.0, lane, 1.0).0;
        for step in 1..=400 {
            let at = walk_point(&map, &route, f64::from(step) / 400.0, lane, 1.0).0;
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
            // Spawned bare, like a worker: `dress_actors` gives it its art.
            Transform::default(),
        ));
        if was_restored {
            cart.insert(RestoredCycle);
        }
    }
}

/// A cart that is still filling up.
#[derive(Component)]
pub struct Boarding;

/// Which cart this is, in purchase order: carts fill in that order, so it is
/// also how many of the crewed monkeys are this cart's.
#[derive(Component, Debug, Clone, Copy)]
pub struct CartIndex(pub(crate) u32);

/// How opaque an empty cart waiting for its crew is drawn.
///
/// The crew is drawn into the cart's art - a driver and two pedallers, sitting
/// in every one of its frames - so the three riders that used to appear one at
/// a time as a fill gauge have nothing to appear on. The fade says the same two
/// things on the one sprite there is: not running yet, and how close. Each
/// monkey that climbs aboard firms it up by a third of the way to solid.
const BOARDING_ALPHA: f32 = 0.35;

/// How far above its ground point a parked cart reaches, in world texels: the
/// tip of its crew's tails, over the load.
///
/// Off the art's own measured top row rather than its canvas, which carries a
/// margin of empty rows above the drawing. What reads this is the clearance
/// between a parked rank and the bins behind it, so an over-stated height
/// spreads the yard over ground it does not need.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn cart_crown() -> f32 {
    art::CART.height_above(art::CART_TOP_ROW)
}

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

/// Where a cart is along the route, and how far into its bay at the cart bins.
///
/// The bay is a *place*, not an offset along the walk. A cart used to unload
/// from a dwell point on the route itself, which put a vehicle the length of
/// three monkeys across the arriving queue for the hundred seconds an unload
/// takes; since D31 it leaves the route over the last stretch of the way home
/// and parks in the rank at the carts' own bins.
///
/// Which end of the leg the bay is at is decided here rather than by a flag
/// coming back out of the ramp: on the way out the cart is *leaving* the bay,
/// on the way home it is *arriving* at it, and the grove end is a dwell on the
/// route with no bay at all.
fn cart_phase(
    segment: Segment,
    progress: f64,
    boarding: bool,
    (stall, grove): (f64, f64),
    approach: f64,
) -> (f64, f32) {
    if boarding {
        // An unlaunched cart waits in its bay, visibly filling up: it is being
        // crewed for a trip it has not left on.
        return (stall, 1.0);
    }
    match segment {
        Segment::ToGrove => (
            stall + (grove - stall) * progress,
            leaving_ring(progress, approach),
        ),
        Segment::Pick => (grove, 0.0),
        Segment::ToDepot => (
            grove + (stall - grove) * progress,
            arriving_ring(progress, approach),
        ),
        Segment::Unload | Segment::Snack => (stall, 1.0),
    }
}

/// How far out from its bay a cart is already leaving the road, in metres of
/// route.
///
/// Its own number rather than the harvesters' [`APPROACH_METRES`], because the
/// distance being covered is its own: a monkey steps at most 4.8 m off the
/// route to its spot, a cart drives more than twenty to its bay, and rounds the
/// crowd on the way. Spent over the harvesters' sixteen metres that came out at
/// twice road speed, which on a vehicle reads as a slide rather than a drive.
const CART_APPROACH_METRES: f64 = 30.0;

/// Where a cart is drawn between the road and its bay: straight between the
/// two, over the last [`CART_APPROACH_METRES`] of the way home.
///
/// A straight line is only good enough because of where the bins are put. The
/// carts' bins are on the *same side of the delivery point as the road* - out
/// in front of the village, on the near side of the walk home - so the line
/// from the road to a bay never crosses the ground the harvesters unload on: it
/// passes 7.8 m clear of the delivery point at its nearest, against a ring of
/// 4.8. Put the yard on the far side instead, with the delivery point between
/// the road and the bay, and the same straight line cuts the chord: the first
/// cut of D31 did exactly that and drove a hundred-banana box through the
/// middle of the queue at three times road speed, twice a cycle. That is what
/// `a_cart_drives_round_the_unloading_crowd_rather_than_through_it` holds, and
/// it is the reason the yard is where it is rather than in the empty quarter to
/// the right.
fn drive_into_bay(on_route: Vec2, bay: Vec2, parked: f32) -> Vec2 {
    on_route.lerp(bay, parked)
}

/// Which way a cart is drawn moving, in metres on the ground, or nothing at all
/// if it is standing still.
///
/// A finite difference over the *drawn* path, taken a whisker back along the
/// segment. The path is a blend of the road and the bay, so the alternative is
/// differentiating both terms and the ramp between them and hoping the algebra
/// still matches the drawing; this cannot disagree with what is drawn, because
/// it asks the same function.
#[allow(clippy::too_many_arguments)]
fn drawn_travel(
    map: &Map,
    route: &Route,
    bay_fractions: (f64, f64),
    bay: Vec2,
    segment: Segment,
    progress: f64,
    boarding: bool,
    approach: f64,
) -> Option<Vec2> {
    /// A thousandth of a leg: short enough to be a direction rather than an
    /// average, long enough to stay well clear of `f32`'s noise floor at the
    /// metre magnitudes the board works in.
    const STEP: f64 = 1e-3;
    let at = |progress: f64| {
        let (fraction, parked) = cart_phase(segment, progress, boarding, bay_fractions, approach);
        drive_into_bay(cart_point(map, route, fraction).0, bay, parked)
    };
    let behind = at((progress - STEP).max(0.0));
    let ahead = at((progress + STEP).min(1.0));
    (ahead - behind).try_normalize()
}

/// Every dressed cart on the route, and whether it has launched.
type CartAvatarQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static HarvestCycle,
        &'static CartIndex,
        Option<&'static Boarding>,
        &'static mut Transform,
        &'static mut Sprite,
    ),
    With<Cart>,
>;

/// What a cart is showing: which clip, which frame of it, and whether it faces
/// the grove.
///
/// Travel plays a whole number of loops over each leg, so the wheels and pedals
/// set off on their first frame and arrive on it. Parked, a cart holds that
/// frame, which is exactly where the fill and the offload begin - the art's
/// seams are exact (`the_cart_clips_meet_pixel_for_pixel`), and this is what
/// lands on them.
///
/// Each one-shot plays once at its authored pace and is timed to *finish* as
/// its segment does. The fill ends as the cart pulls away full, and the offload
/// tips the last bunch out as the +100 lands, which is when the economy credits
/// it. Started on arrival instead, a cart would show an empty bed for the
/// whole of a hundred-second unload it had not finished, and pay out long
/// after its bananas had visibly gone. A segment shorter than its clip - a
/// crowd of Unpackers gets the unload there - plays the clip faster rather
/// than cutting it.
fn cart_pose(
    segment: Segment,
    boarding: bool,
    progress: f64,
    seconds: f64,
) -> (CartClip, u32, bool) {
    if boarding {
        return (CartClip::TravelEmpty, 0, true);
    }
    let rolling = |clip: CartClip| {
        let laps = (seconds / f64::from(clip.seconds())).round().max(1.0);
        let phase = (progress * laps).fract();
        ((phase * f64::from(clip.frames())) as u32).min(clip.frames() - 1)
    };
    let finishing = |clip: CartClip, parked: CartClip| {
        let window = f64::from(clip.seconds()).min(seconds);
        let left = (1.0 - progress) * seconds;
        if window <= 0.0 || left > window {
            (parked, 0)
        } else {
            let into = (window - left) / window;
            let frame = ((into * f64::from(clip.frames())) as u32).min(clip.frames() - 1);
            (clip, frame)
        }
    };
    match segment {
        Segment::ToGrove => (CartClip::TravelEmpty, rolling(CartClip::TravelEmpty), true),
        Segment::Pick => {
            let (clip, frame) = finishing(CartClip::Fill, CartClip::TravelEmpty);
            (clip, frame, true)
        }
        Segment::ToDepot => (CartClip::TravelFull, rolling(CartClip::TravelFull), false),
        Segment::Unload => {
            let (clip, frame) = finishing(CartClip::Offload, CartClip::TravelFull);
            (clip, frame, false)
        }
        // Empty and closed at the depot, where the offload left it, until it
        // turns for the grove.
        Segment::Snack => (CartClip::TravelEmpty, 0, false),
    }
}

/// Place and pose every cart.
///
/// The cart is drawn whole by the artist - bed, wheels, cargo and its crew of
/// three - in all eight directions, so everything the old box, pile and three
/// seated riders said is now said by which clip is playing: an empty bed
/// rolling out, the fill at the grove, a full bed rolling home, and the bed
/// tipping out at the depot.
#[allow(clippy::too_many_arguments)]
pub fn position_carts(
    art: Res<Art>,
    sheets: Option<Res<CartSheets>>,
    layout: Res<SceneLayout>,
    village: Res<Village>,
    route: Res<WorkedRoute>,
    multipliers: Res<Multipliers>,
    carts_res: Res<Carts>,
    mut carts: CartAvatarQuery,
) {
    // No sheets means no cart has been dressed yet, so there is nothing to pose.
    let Some(sheets) = sheets else {
        return;
    };
    let scale = Vec3::new(layout.world_scale(), layout.world_scale(), 1.0);

    let approach = (CART_APPROACH_METRES / route.0.length().max(f64::EPSILON)).clamp(1e-3, 0.5);
    for (cycle, index, boarding, mut transform, mut sprite) in &mut carts {
        let segment = cycle.segment();
        let progress = cycle.segment_fraction(CycleSpec::CART, *multipliers);
        // Its own bay at the grove, on the *inside* of the route, so the cart
        // never reverses into the palm the harvesters are working.
        let dwell = f64::from(layout.cart_offset()) / route.0.length();
        let (grove, stall) = (1.0 - dwell, dwell);
        let (fraction, parked) = cart_phase(
            cycle.segment(),
            progress,
            boarding.is_some(),
            (stall, grove),
            approach,
        );

        // A vehicle draws in *front* of the walkers rather than beside them:
        // workers pass behind it instead of through it. That is one lane's
        // worth of ground towards the viewer, which the depth rule then
        // handles on its own.
        //
        // Except at the depot end, where it leaves the route altogether for its
        // bay at the cart bins (D31). A cart used to run the route to a dwell
        // offset and unload from there, parked across the queue for the hundred
        // seconds an unload takes.
        let bay = layout.cart_park(index.0);
        let (on_route, along) = cart_point(&village, &route.0, fraction);
        let point = drive_into_bay(on_route, bay, parked);
        // No lift: the art is anchored where its wheels meet the ground.
        let translation = layout
            .board_snapped(point, 0.0)
            .extend(isometric::stand_z(point, isometric::NUDGE_STEP));
        if transform.translation != translation {
            transform.translation = translation;
        }
        if transform.scale != scale {
            transform.scale = scale;
        }

        let seconds = segment.duration(CycleSpec::CART, *multipliers);
        let (clip, frame, outbound) = cart_pose(segment, boarding.is_some(), progress, seconds);
        // Which way the cart is pointing is the way it is *drawn* moving, not
        // the way the route runs: over the last stretch of the way home it is
        // leaving the road for its bay, and a cart still facing down the road
        // while it pulls into the yard drives sideways.
        let travel = drawn_travel(
            &village,
            &route.0,
            (stall, grove),
            bay,
            segment,
            progress,
            boarding.is_some(),
            approach,
        );
        let facing = Facing::of_ground(travel.unwrap_or(if outbound { along } else { -along }));
        art.pose_cart(&mut sprite, &sheets, clip, facing, frame);

        // Faint until its crew is aboard: see `BOARDING_ALPHA`. The boarding
        // wait is the longest dead stretch in the game, and each monkey that
        // climbs on is the only thing that happens in it.
        let alpha = if boarding.is_some() {
            let aboard = carts_res
                .crewed()
                .saturating_sub(index.0 * CART_CREW)
                .min(CART_CREW);
            BOARDING_ALPHA + (1.0 - BOARDING_ALPHA) * aboard as f32 / CART_CREW as f32
        } else {
            1.0
        };
        let colour = Color::WHITE.with_alpha(alpha);
        if sprite.color != colour {
            sprite.color = colour;
        }
    }
}

#[cfg(test)]
mod dressing_tests {
    use super::*;
    use bevy::{asset::AssetPlugin, image::TextureAtlasLayout, sprite::Anchor};

    /// The presentation's dressing pass on its own, with a real asset server
    /// behind it, so `Art` is the resource the game actually builds.
    fn dressing() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            ImagePlugin::default(),
        ));
        app.init_asset::<TextureAtlasLayout>();
        let world = app.world_mut();
        let art = world.resource_scope(|world, mut layouts: Mut<Assets<TextureAtlasLayout>>| {
            world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
                Art::load(world.resource::<AssetServer>(), &mut layouts, &mut images)
            })
        });
        app.insert_resource(art);
        app.add_systems(Update, dress_actors);
        app
    }

    /// A worker as `spawn_missing_workers` makes one: no sprite, no playhead,
    /// and a bare banana to carry.
    fn spawn_bare(app: &mut App, cycle: HarvestCycle, lane: u32) -> Entity {
        app.world_mut()
            .spawn((
                Worker,
                cycle,
                CycleSpec::WORKER,
                Lane(lane),
                Transform::default(),
            ))
            .with_child((CarriedBanana, Transform::default(), Visibility::Hidden))
            .id()
    }

    fn children_with<T: Component>(app: &mut App, parent: Entity) -> usize {
        let children: Vec<Entity> = app
            .world()
            .get::<Children>(parent)
            .map(|children| children.iter().collect())
            .unwrap_or_default();
        children
            .into_iter()
            .filter(|child| app.world().get::<T>(*child).is_some())
            .count()
    }

    #[test]
    fn an_actor_spawned_after_startup_is_dressed_once_on_the_loop_it_needs() {
        // The seam D28 rests on: the simulation spawns a monkey with nothing
        // to draw, and the presentation gives it everything, once. A second
        // pass must not stack a second shadow on it, and a monkey restored
        // mid-unload must open standing, not on a walk frame.
        let mut app = dressing();
        app.update();

        let walker = spawn_bare(&mut app, HarvestCycle::starting(CycleSpec::WORKER), 3);
        let multipliers = Multipliers::default();
        let cycle = cycle_time(CycleSpec::WORKER, multipliers);
        let unloading = (0..1000)
            .map(|step| {
                HarvestCycle::from_phase(
                    cycle * f64::from(step) / 1000.0,
                    CycleSpec::WORKER,
                    multipliers,
                )
            })
            .find(|cycle| cycle.segment() == Segment::Unload)
            .expect("a worker's cycle has an unload in it");
        let unloader = spawn_bare(&mut app, unloading, 4);

        app.update();
        app.update();

        let art = app.world().resource::<Art>().clone();
        for (entity, clip) in [(walker, Clip::Walk), (unloader, Clip::Idle)] {
            let world = app.world();
            assert!(world.get::<Anchor>(entity).is_some(), "{clip:?}: no anchor");
            let playing = world.get::<Playing>(entity).expect("no playhead");
            assert_eq!(playing.clip, clip, "opened on the wrong loop");
            let sprite = world.get::<Sprite>(entity).expect("no sprite");
            assert_eq!(sprite.image, art.clip(clip).0, "{clip:?}: wrong sheet");
            assert_eq!(children_with::<Shadow>(&mut app, entity), 1, "{clip:?}");
            let banana = app
                .world()
                .get::<Children>(entity)
                .unwrap()
                .iter()
                .find(|child| app.world().get::<CarriedBanana>(*child).is_some())
                .unwrap();
            assert!(
                app.world().get::<Sprite>(banana).is_some(),
                "{clip:?}: the carried banana was never dressed"
            );
        }
    }

    /// The picture a cart pose shows, naming each one-shot's first and last
    /// frames by the travel frame they are pixel-identical to.
    fn picture(clip: CartClip, frame: u32) -> (CartClip, u32) {
        match (clip, frame) {
            (CartClip::Fill, 0) => (CartClip::TravelEmpty, 0),
            (CartClip::Fill, 23) => (CartClip::TravelFull, 0),
            (CartClip::Offload, 0) => (CartClip::TravelFull, 0),
            (CartClip::Offload, 23) => (CartClip::TravelEmpty, 0),
            other => other,
        }
    }

    #[test]
    fn a_cart_changes_clip_only_where_the_art_meets_itself() {
        // Every segment boundary, at the shipped multipliers and with an
        // unload squeezed shorter than its clip: the last picture of one
        // segment is the first of the next, so no state change pops.
        let shipped = |segment: Segment| segment.duration(CycleSpec::CART, Multipliers::default());
        let squeezed = |segment: Segment| match segment {
            Segment::Unload | Segment::Pick => 0.7,
            other => shipped(other),
        };
        for seconds in [&shipped as &dyn Fn(Segment) -> f64, &squeezed] {
            for segment in Segment::ORDER {
                let next = segment.next();
                let (end, last, _) = cart_pose(segment, false, 1.0, seconds(segment));
                let (start, first, _) = cart_pose(next, false, 0.0, seconds(next));
                assert_eq!(
                    picture(end, last),
                    picture(start, first),
                    "{segment:?} ends on a different picture than {next:?} starts"
                );
            }
        }
        // And a cart launching off the boarding bay rolls out of the pose it
        // waited in.
        let (clip, frame, outbound) = cart_pose(Segment::ToGrove, true, 0.4, 13.0);
        assert_eq!((clip, frame, outbound), (CartClip::TravelEmpty, 0, true));
        let (clip, frame, _) = cart_pose(Segment::ToGrove, false, 0.0, 13.0);
        assert_eq!((clip, frame), (CartClip::TravelEmpty, 0));
    }

    #[test]
    fn a_one_shot_plays_every_frame_at_its_pace_as_its_segment_ends() {
        // Three seconds out, the fill has not begun; 1.2 s out it is half way
        // through its 2.4 s; at the end it is on its last frame.
        let seconds = 60.0;
        let at = |left: f64| cart_pose(Segment::Pick, false, 1.0 - left / seconds, seconds);
        assert_eq!(at(3.0), (CartClip::TravelEmpty, 0, true));
        assert_eq!(at(1.2), (CartClip::Fill, 12, true));
        assert_eq!(at(0.0), (CartClip::Fill, 23, true));
        // A segment shorter than the clip still shows all of it.
        for (segment, clip) in [
            (Segment::Pick, CartClip::Fill),
            (Segment::Unload, CartClip::Offload),
        ] {
            let shown: std::collections::BTreeSet<u32> = (0..=2000)
                .map(|step| cart_pose(segment, false, f64::from(step) / 2000.0, 0.5))
                .filter(|(playing, ..)| *playing == clip)
                .map(|(_, frame, _)| frame)
                .collect();
            assert_eq!(shown.len() as u32, clip.frames(), "{clip:?} skipped frames");
        }
    }

    #[test]
    fn a_rolling_cart_turns_its_wheels_at_the_authored_pace() {
        // A whole number of laps over the leg, so the pace is the manifest's
        // 80 ms a frame to within half a lap spread over the whole trip.
        let seconds = 13.3;
        let steps = 13_300;
        let mut changes = 0;
        let mut last = cart_pose(Segment::ToDepot, false, 0.0, seconds).1;
        for step in 1..=steps {
            let frame = cart_pose(
                Segment::ToDepot,
                false,
                f64::from(step) / f64::from(steps),
                seconds,
            )
            .1;
            if frame != last {
                changes += 1;
                last = frame;
            }
        }
        let paced = seconds / f64::from(CartClip::TravelFull.frame_seconds());
        assert!(
            (f64::from(changes) - paced).abs() <= 6.0,
            "{changes} frames over a leg paced for {paced}"
        );
    }
}
