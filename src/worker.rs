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
};

const FRAME_SIZE: u32 = 22;
const MONKEY_SIZE: Vec2 = Vec2::new(13.0, 22.0);
const MONKEY_FILL: Color = Color::srgb(0.94, 0.82, 0.67);
const MONKEY_EDGE: Color = Color::srgb(0.34, 0.17, 0.10);

/// Three lanes is enough to keep a crowd legible without turning the route into
/// a parade ground.
const LANES: u32 = 3;
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

    pub fn clear(&mut self) {
        self.remaining = 0;
    }
}

impl Default for RestoreCarts {
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

impl Lane {
    /// Depth row, front to back.
    fn row(self) -> u32 {
        self.0 % LANES
    }

    /// A small along-route offset, in source texels, so that workers sharing a
    /// row are not pixel-identical.
    ///
    /// Without it, workers 0 and 3 occupy the same lane at the same phase with
    /// the same animation frame and draw exactly on top of each other: hire
    /// four in a burst and the player counts three monkeys while the store
    /// reads OWNED 4. Removing spawn jitter is what exposed this - phases used
    /// to differ, so positions did too.
    fn stagger_texels(self) -> f32 {
        const SPREAD: u32 = 5;
        (self.0 / LANES % SPREAD) as f32 * 4.0 - 8.0
    }
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
pub fn position_workers(
    time: Res<Time>,
    mut commands: Commands,
    layout: Res<SceneLayout>,
    multipliers: Res<Multipliers>,
    mut workers: Query<
        (
            Entity,
            &HarvestCycle,
            &Lane,
            Option<&mut JustHired>,
            &mut Transform,
            &mut Sprite,
        ),
        With<Worker>,
    >,
) {
    for (entity, cycle, lane, hired, mut transform, mut sprite) in &mut workers {
        let progress = cycle.segment_fraction(CycleSpec::WORKER, *multipliers) as f32;
        let (x, facing_right) = match cycle.segment() {
            Segment::ToGrove => (
                layout.stall_stand + (layout.grove_stand - layout.stall_stand) * progress,
                false,
            ),
            Segment::Pick => (layout.grove_stand, false),
            Segment::ToDepot => (
                layout.grove_stand + (layout.stall_stand - layout.grove_stand) * progress,
                true,
            ),
            // Unloading and then eating both happen at the stall, so the
            // monkey stays put and keeps facing it.
            Segment::Unload | Segment::Snack => (layout.stall_stand, true),
        };

        // Project the route and its parallel crowd lanes in one place. Screen
        // y also drives the stable painter's order through `actor_z`.
        let back = lane.row() as f32;
        let point = layout.route_point(x + lane.stagger_texels() * layout.world_scale, back);
        let half_height = FRAME_SIZE as f32 * 0.5 * layout.world_scale;

        let translation = Vec3::new(
            layout.snap(point.x),
            layout.snap(point.y + half_height),
            layout.actor_z(point, lane.0 as f32 * 0.0001),
        );
        let scale = Vec3::splat(layout.world_scale);
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

        // Depth cue: rows further back sit slightly in shade.
        let shade = 1.0 - 0.09 * back;
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

    #[test]
    fn isometric_lanes_are_separated_and_depth_sorted() {
        let layout = SceneLayout::for_viewport(Vec2::new(1280.0, 720.0));
        let x = (layout.grove_stand + layout.stall_stand) * 0.5;
        let points: Vec<Vec2> = (0..LANES)
            .map(|lane| layout.route_point(x, lane as f32))
            .collect();
        for pair in points.windows(2) {
            assert!(pair[1].y < pair[0].y);
            assert!(layout.actor_z(pair[1], 0.0) > layout.actor_z(pair[0], 0.0));
        }
    }

    #[test]
    fn workers_sharing_a_row_do_not_draw_on_top_of_each_other() {
        // Without spawn jitter, same lane plus same phase means pixel-identical
        // sprites: four hires would show three monkeys. Every worker sharing a
        // row within one spread must sit at a different offset.
        let mut seen = std::collections::HashSet::new();
        for index in 0..LANES * 5 {
            let lane = Lane(index);
            assert!(
                seen.insert((lane.row(), lane.stagger_texels().to_bits())),
                "worker {index} collides with an earlier one"
            );
        }
        // And the spread stays inside the route rather than walking off it.
        for index in 0..200u32 {
            assert!(Lane(index).stagger_texels().abs() <= 8.0);
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
pub fn position_carts(
    layout: Res<SceneLayout>,
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
        let progress = cycle.segment_fraction(CycleSpec::CART, *multipliers) as f32;
        // Its own bay at each end, on the *inside* of the route: pushed left at
        // the depot and right at the grove, so the cart never parks on the
        // unloading queue and never reverses into the palm.
        let dwell = layout.cart_offset();
        let (grove, stall) = (layout.grove_stand + dwell, layout.stall_stand - dwell);
        let x = if boarding.is_some() {
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

        let point = layout.route_point(x, -1.0);
        transform.translation = Vec3::new(
            layout.snap(point.x),
            layout.snap(point.y + CART_BOX_TEXELS.y * 0.5 * scale),
            layout.actor_z(point, 0.02),
        );
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
        carried.push((entity, load));
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
