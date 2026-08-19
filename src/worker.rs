//! Worker monkey avatars.
//!
//! Position and animation are derived from [`HarvestCycle`] every frame, so an
//! avatar is a pure function of simulation state: it survives a resize, and it
//! cannot drift out of step with the economy.
//!
//! Simulation entities are unbounded, but rendering is not: a hundred animated
//! 32x32 sprites on one route is visual mush, and each one costs a change
//! detection hit per frame. The systems here read `(&HarvestCycle, &Lane)` and
//! write `Transform`, so capping avatars at a fixed pool later is a change to
//! *which* entities they iterate, not a rendering rewrite.

use bevy::prelude::*;

use crate::{
    domain::{CycleSpec, HarvestCycle, Multipliers, Segment, Workforce, cycle_time},
    game::SceneLayout,
};

const FRAME_SIZE: u32 = 32;
const IDLE_FRAMES: usize = 18;
const RUN_FRAMES: usize = 8;
const IDLE_FPS: f32 = 12.0;

/// How many run frames elapse over one leg of the journey. Expressed per leg
/// rather than per second so that the foot cadence is identical at every
/// viewport: a worker covers 40 world px/s on desktop but only 12 at 390 px
/// wide, and a fixed frame rate would make it moonwalk on a phone.
const RUN_FRAMES_PER_LEG: f32 = 160.0;

/// Three lanes is enough to keep a crowd legible without turning the route into
/// a parade ground.
const LANES: u32 = 3;
/// Lane spacing in *source* texels; multiplied by the world scale so lanes stay
/// on the pixel grid.
///
/// Small on purpose. The ground is a flat side-on plane with a 16 px grass
/// band, so a lane that steps far *below* the ground line is not "in front of"
/// anything - it is buried in the dirt. Three lanes at three texels keep every
/// worker's feet inside the band.
const LANE_STEP_TEXELS: f32 = 3.0;

#[derive(Component)]
pub struct Worker;

/// Prevents a random resume phase from creating income or wages before the
/// worker has completed its first post-resume cycle. The carried banana remains
/// presentation state through the existing segment rules.
#[derive(Component)]
pub(crate) struct RestoredCycle;

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

/// Cached handles, so the atlas swap does not touch the asset server per frame.
#[derive(Resource)]
pub(crate) struct WorkerArt {
    idle_image: Handle<Image>,
    run_image: Handle<Image>,
    idle_layout: Handle<TextureAtlasLayout>,
    run_layout: Handle<TextureAtlasLayout>,
    banana_image: Handle<Image>,
    banana_layout: Handle<TextureAtlasLayout>,
}

impl WorkerArt {
    /// The idle sheet, shared with `support`: every monkey in the game is this
    /// spritesheet, and the roles are told apart by what they stand next to.
    pub(crate) fn idle_image(&self) -> Handle<Image> {
        self.idle_image.clone()
    }

    pub(crate) fn idle_layout(&self) -> Handle<TextureAtlasLayout> {
        self.idle_layout.clone()
    }

    /// Image, layout and index must move together. Carrying index 12 from the
    /// 18-frame idle sheet into the 8-frame run sheet is out of range.
    fn apply(&self, sprite: &mut Sprite, pose: Pose, index: usize) {
        let (image, layout) = match pose {
            Pose::Idle => (&self.idle_image, &self.idle_layout),
            Pose::Run => (&self.run_image, &self.run_layout),
        };
        sprite.image = image.clone();
        sprite.texture_atlas = Some(TextureAtlas {
            layout: layout.clone(),
            index,
        });
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

    pub fn clear(&mut self) {
        self.remaining = 0;
    }
}

/// Give every hired worker an avatar, whether the workforce grew because of a
/// purchase or because a save was loaded.
///
/// Growth only. Nothing shrinks the workforce except a restart, which despawns
/// every worker outright; if a unit ever becomes sellable, the excess has to be
/// despawned here or the stale avatars keep delivering and eating.
///
#[allow(clippy::too_many_arguments)]
pub fn spawn_missing_workers(
    mut commands: Commands,
    workforce: Res<Workforce>,
    multipliers: Res<Multipliers>,
    art: Option<Res<WorkerArt>>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut restored: ResMut<RestoreWorkers>,
    existing: Query<Entity, With<Worker>>,
) {
    let target = workforce.count() as usize;
    let current = existing.iter().count();
    debug_assert!(
        current <= target,
        "workforce shrank without despawning avatars"
    );
    if current >= target {
        return;
    }

    let art = match art {
        Some(art) => art,
        None => {
            let art = WorkerArt {
                idle_image: asset_server.load("Monkey/Character Spritesheets/1-Idle/Idle.png"),
                run_image: asset_server.load("Monkey/Character Spritesheets/2-Run/Run.png"),
                idle_layout: layouts.add(TextureAtlasLayout::from_grid(
                    UVec2::splat(FRAME_SIZE),
                    IDLE_FRAMES as u32,
                    1,
                    None,
                    None,
                )),
                run_layout: layouts.add(TextureAtlasLayout::from_grid(
                    UVec2::splat(FRAME_SIZE),
                    RUN_FRAMES as u32,
                    1,
                    None,
                    None,
                )),
                banana_image: asset_server.load("Banana/Banana.png"),
                banana_layout: layouts.add(TextureAtlasLayout::from_grid(
                    UVec2::splat(16),
                    12,
                    1,
                    None,
                    None,
                )),
            };
            commands.insert_resource(art);
            // The resource lands at the end of the tick; the freshly hired
            // worker is spawned on the next one, a fixed tick later.
            return;
        }
    };

    for index in current..target {
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
            Lane(index as u32),
            Pose::Run,
            Sprite {
                image: art.run_image.clone(),
                texture_atlas: Some(TextureAtlas {
                    layout: art.run_layout.clone(),
                    index: 0,
                }),
                ..default()
            },
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
            parent.spawn((
                CarriedBanana,
                Sprite {
                    image: art.banana_image.clone(),
                    texture_atlas: Some(TextureAtlas {
                        layout: art.banana_layout.clone(),
                        index: 0,
                    }),
                    custom_size: Some(Vec2::splat(14.0)),
                    ..default()
                },
                // Local space: the parent's scale carries it to world size.
                // The character's head tops out around y 13 of the 32 px frame,
                // so this rides just above it rather than floating clear.
                Transform::from_xyz(1.0, 16.0, 0.1),
                Visibility::Hidden,
            ));
        });
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

        // Depth: lane 0 is the *front* row, feet on the ground line itself, and
        // later lanes step back up the ground plane. Front-first matters more
        // than it sounds - lanes are handed out in hire order, so the common
        // case of a single worker would otherwise spend the whole game in the
        // back row, shaded and visibly hovering 6 texels off the grass. Going
        // the other way, stepping *down* from the ground line, sinks the front
        // row into the dirt.
        let back = lane.row() as f32;
        let feet = layout.ground_top() + back * LANE_STEP_TEXELS * layout.world_scale;
        let half_height = FRAME_SIZE as f32 * 0.5 * layout.world_scale;

        let translation = Vec3::new(
            layout.snap(x + lane.stagger_texels() * layout.world_scale),
            layout.snap(feet + half_height),
            // Nearer lanes draw in front, and every worker sits behind the
            // dragged banana (z 3) and the zone labels (z 2).
            1.0 + (LANES - 1) as f32 * 0.01 - back * 0.01,
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
    time: Res<Time>,
    art: Option<Res<WorkerArt>>,
    multipliers: Res<Multipliers>,
    mut workers: Query<(&HarvestCycle, &mut Pose, &mut Sprite, &Children), With<Worker>>,
    mut carried: Query<&mut Visibility, With<CarriedBanana>>,
) {
    let Some(art) = art else {
        return;
    };
    // f64: past ~10^5 s of session an f32 mantissa coarsens enough to make the
    // idle loop visibly judder.
    let elapsed = time.elapsed_secs_f64();

    for (cycle, mut pose, mut sprite, children) in &mut workers {
        let segment = cycle.segment();
        let walking = segment.is_walking();
        let next_pose = if walking { Pose::Run } else { Pose::Idle };

        let index = if walking {
            (cycle.segment_fraction(CycleSpec::WORKER, *multipliers) as f32 * RUN_FRAMES_PER_LEG)
                as usize
                % RUN_FRAMES
        } else {
            (elapsed * IDLE_FPS as f64) as usize % IDLE_FRAMES
        };

        let pose_changed = *pose != next_pose;
        let index_changed = sprite
            .texture_atlas
            .as_ref()
            .is_none_or(|atlas| atlas.index != index);
        // Only touch the sprite when something actually changed, so idle
        // workers do not trigger a re-extract every frame.
        if pose_changed || index_changed {
            art.apply(&mut sprite, next_pose, index);
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
    fn every_lane_keeps_its_feet_inside_the_ground_band() {
        // The ground is a flat side-on plane with a 16 px grass band. A lane
        // that steps below the ground line is buried in the dirt, and one that
        // steps far above it stands on the sky.
        let layout = SceneLayout::for_viewport(Vec2::new(1280.0, 720.0));
        let band = 16.0;

        for lane in 0..LANES {
            let back = lane as f32;
            let feet = layout.ground_top() + back * LANE_STEP_TEXELS * layout.world_scale;
            let lift = feet - layout.ground_top();

            assert!(lift >= 0.0, "lane {lane} is below the ground line");
            assert!(lift <= band, "lane {lane} floats {lift} above the ground");
        }
        // Lane 0 is the front row: feet exactly on the ground line, unshaded,
        // drawn in front. Lanes are handed out in hire order, so the first
        // worker - which for most of a session is the only worker - must be the
        // one standing on the grass rather than hovering above it.
        let z = |lane: Lane| 1.0 + (LANES - 1) as f32 * 0.01 - lane.row() as f32 * 0.01;
        let shade = |lane: Lane| 1.0 - 0.09 * lane.row() as f32;

        assert_eq!(Lane(0).row(), 0);
        assert_eq!(shade(Lane(0)), 1.0);
        assert!(z(Lane(0)) > z(Lane(LANES - 1)));
        assert!(shade(Lane(0)) > shade(Lane(LANES - 1)));
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

    #[test]
    fn the_run_cycle_covers_whole_frames_across_a_leg() {
        // Every frame index the animation can produce must be inside the run
        // sheet; an out-of-range index renders as a garbled tile.
        for step in 0..=1_000 {
            let fraction = step as f32 / 1_000.0;
            let index = (fraction * RUN_FRAMES_PER_LEG) as usize % RUN_FRAMES;
            assert!(index < RUN_FRAMES);
        }
    }

    #[test]
    fn the_idle_cycle_stays_inside_the_idle_sheet() {
        for tick in 0..5_000 {
            let index = ((tick as f32 * 0.01) * IDLE_FPS) as usize % IDLE_FRAMES;
            assert!(index < IDLE_FRAMES);
        }
    }
}
