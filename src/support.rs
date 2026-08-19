//! Chef, Unpacker and Technologist: the monkeys who never touch a banana tree.
//!
//! Two entity populations live here and they are deliberately not the same one.
//! The **simulation** entities carry `(SupportRole, SupportCycle)` and no sprite
//! at all; there is one per monkey hired, and starving is a per-entity fact
//! because the larder can cover two chefs and not the third. The **avatars** are
//! a small fixed pool - at most [`AVATARS_PER_ROLE`] per role - that reads those
//! entities and draws them.
//!
//! Keeping them apart is what lets the field stay legible while the economy
//! scales: the whitepaper's end state hires 21 support monkeys, and 21 more
//! sprites crowded around one deposit is mush. It also avoids the trap of
//! hanging a role's avatar off "the first entity of that role", which breaks the
//! moment that entity is despawned.
//!
//! No new art. Every monkey here is the worker's idle sheet; the roles are told
//! apart entirely by a coloured box - worn as a chef's hat, carried as a crate,
//! or sat behind as a desk - and by where they stand.

use bevy::prelude::*;

use crate::{
    domain::{SUPPORT_MEAL_PERIOD, Staff, SupportCycle, SupportRole},
    game::{CREAM, GOLD, SceneLayout},
    worker::WorkerArt,
};

const FRAME_SIZE: u32 = 32;
const IDLE_FRAMES: usize = 18;
const IDLE_FPS: f32 = 12.0;

/// Sprites drawn per role before the count moves to a badge.
///
/// Three rather than one. A single sprite per role makes the second purchase of
/// a Chef a no-op on screen, in a genre where "the field fills up" is most of
/// the reward - and this repo already ruled against that effect once, in
/// `worker::Lane::stagger_texels`, when four hires drew as three monkeys. Three
/// covers most of a measured session for Chefs and Technologists while capping
/// the crowd around the deposit at nine.
///
/// It also gives partial starvation somewhere to render. One sprite per role is
/// binary; three can grey independently, so "two of my three chefs are idle" is
/// a thing the player can see rather than infer from a stalled rate.
pub(crate) const AVATARS_PER_ROLE: usize = 3;

/// Spacing between two monkeys of the same role, in source texels.
///
/// Wider than the widest role box, or three chefs' hats merge into one white
/// rectangle and the role reads as a single object with a strange head.
const SLOT_STEP_TEXELS: f32 = 18.0;
/// Depth spacing, matched to `worker::LANE_STEP_TEXELS` so support and workers
/// sit on the same ground plane.
const ROW_STEP_TEXELS: f32 = 3.0;

/// Slow enough to read as distress rather than as a strobe. Shared with the
/// worker's old hunger pulse, which is now unreachable: a harvester's meal is
/// reserved out of its own delivery, so the only monkeys who can go hungry are
/// the ones living on somebody else's surplus.
const HUNGRY_PULSE_HZ: f32 = 0.8;

/// A brief highlight on a freshly hired monkey, matching `worker::JustHired`.
const HIRE_HIGHLIGHT_SECONDS: f32 = 0.6;

/// Marks a simulation entity, as opposed to an avatar.
#[derive(Component)]
pub(crate) struct SupportUnit;

/// One drawn monkey: which role, and which of that role's slots it occupies.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct SupportAvatar {
    role: SupportRole,
    slot: usize,
}

/// The box that tells the roles apart. A child of the avatar, so it inherits
/// position and scale and needs no layout logic of its own.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RoleBox;

/// The `xN` badge beside a role, shown only once the crowd stops being
/// countable. A brown plate carrying cream text: the badge has to stay legible
/// over the sky, over the deposit sign and over a chef's white hat, and no
/// single flat colour does all three.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct SupportBadge(SupportRole);

/// The plate behind a badge's text.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct BadgePlate;

#[derive(Component)]
pub(crate) struct HireFlash(f32);

/// The text inside a badge plate.
#[derive(Component)]
pub(crate) struct BadgeLabel;

/// Soil-dark, so cream text on it stays readable over sky, sign and white hat
/// alike.
const BADGE_PLATE: Color = Color::srgb(0.24, 0.12, 0.06);

impl SupportRole {
    /// Sprite tint, so three identical monkeys are still three distinguishable
    /// monkeys when the boxes overlap at small scale.
    fn tint(self) -> Color {
        match self {
            SupportRole::Chef => Color::srgb(1.0, 0.98, 0.94),
            SupportRole::Unpacker => Color::srgb(0.94, 0.98, 1.0),
            SupportRole::Technologist => Color::srgb(0.98, 0.94, 1.0),
        }
    }

    /// Colour of the role's box.
    fn box_color(self) -> Color {
        match self {
            // A white hat. The only pure-white thing in the scene, which is what
            // makes a 32 px monkey read as a chef at a glance.
            SupportRole::Chef => Color::srgb(0.98, 0.96, 0.90),
            SupportRole::Unpacker => Color::srgb(0.72, 0.45, 0.20),
            SupportRole::Technologist => Color::srgb(0.45, 0.30, 0.55),
        }
    }

    /// Box size in source texels, and its offset from the monkey's centre.
    /// Three shapes, three silhouettes: worn, carried, sat behind.
    fn box_geometry(self) -> (Vec2, Vec2) {
        match self {
            // Narrower than the head and sitting on the crown, so it reads as
            // worn. Wider than the head it becomes a white bar behind a monkey.
            SupportRole::Chef => (Vec2::new(9.0, 7.0), Vec2::new(0.0, 10.0)),
            // Held in front of the chest, breaking the body outline.
            SupportRole::Unpacker => (Vec2::new(12.0, 11.0), Vec2::new(-7.0, -2.0)),
            // A wide, low desk the monkey sits behind.
            SupportRole::Technologist => (Vec2::new(24.0, 12.0), Vec2::new(-4.0, -8.0)),
        }
    }

    /// Every box draws in front of its monkey.
    ///
    /// The hat was briefly behind, on the theory that a hat sits *on* a head.
    /// Drawn behind, the head occludes its middle and all that survives is a
    /// white bar sticking out either side - which reads as scenery, not
    /// millinery. In front, a box narrower than the head reads as worn.
    fn box_z(self) -> f32 {
        0.002
    }
}

/// Stage 2. One simulation entity per hired monkey.
///
/// The shift phase is derived from the hire index rather than randomised: N
/// chefs hired together would otherwise eat on exactly the same tick forever,
/// which turns a smooth 0.10/s drain into a spiky lump every 200 ticks and makes
/// the whole role starve or not starve together. Deterministic offset, so it is
/// reproducible - this is `worker::Lane`'s stagger, not D16's jitter.
pub(crate) fn spawn_missing_support(
    mut commands: Commands,
    staff: Res<Staff>,
    existing: Query<&SupportRole, With<SupportUnit>>,
) {
    for role in SupportRole::ALL {
        let current = existing.iter().filter(|unit| **unit == role).count() as u32;
        let target = staff.count(role);
        debug_assert!(
            current <= target,
            "support shrank without despawning entities"
        );

        for index in current..target {
            let phase = index as f64 * SUPPORT_MEAL_PERIOD / AVATARS_PER_ROLE as f64;
            commands.spawn((SupportUnit, role, SupportCycle::starting(phase)));
        }
    }
}

/// Presentation. Reconciles the avatar pool against the hired count, then poses
/// every avatar from the simulation entities behind it.
pub(crate) fn sync_support_avatars(
    mut commands: Commands,
    time: Res<Time>,
    layout: Res<SceneLayout>,
    staff: Res<Staff>,
    art: Option<Res<WorkerArt>>,
    units: Query<(&SupportRole, &SupportCycle), With<SupportUnit>>,
    mut avatars: Query<(
        Entity,
        &SupportAvatar,
        Option<&mut HireFlash>,
        &mut Transform,
        &mut Sprite,
    )>,
) {
    let Some(art) = art else {
        // The atlas lands at the end of a tick; avatars appear on the next one.
        return;
    };

    for role in SupportRole::ALL {
        let hired = staff.count(role) as usize;
        let wanted = hired.min(AVATARS_PER_ROLE);
        let drawn = avatars
            .iter()
            .filter(|(_, avatar, ..)| avatar.role == role)
            .count();

        for slot in drawn..wanted {
            spawn_avatar(&mut commands, &art, &layout, role, slot);
        }
    }

    // How many of each role are idle. An avatar greys when its *slot index* is
    // within the hungry count, so with two of three chefs unfed exactly two
    // sprites grey - which slot is arbitrary, and stable enough frame to frame
    // because the count is what drives it.
    let mut hungry = [0usize; 3];
    for (role, cycle) in &units {
        if cycle.is_hungry() {
            hungry[role_index(*role)] += 1;
        }
    }

    let frame = ((time.elapsed_secs() * IDLE_FPS) as usize) % IDLE_FRAMES;

    for (entity, avatar, flash, mut transform, mut sprite) in &mut avatars {
        let (x, row) = layout.support_stand(avatar.role);
        let scale = layout.world_scale();
        let slot = avatar.slot as f32;

        // Fan out from the station, away from the deposit's centre line, so
        // the first monkey of a role always stands exactly on its anchor.
        let spread = slot * SLOT_STEP_TEXELS * scale;
        let feet = layout.ground_top() + row as f32 * ROW_STEP_TEXELS * scale;
        let half_height = FRAME_SIZE as f32 * 0.5 * scale;

        transform.translation = Vec3::new(
            layout.snap(x + spread),
            layout.snap(feet + half_height),
            // Behind the dragged banana and the zone labels, and stepped by
            // depth row so a nearer monkey draws over a further one.
            1.0 - row as f32 * 0.01 + slot * 0.001,
        );

        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = frame;
        }
        // Everyone faces left, into the arriving traffic.
        sprite.flip_x = true;

        let starving = avatar.slot < hungry[role_index(avatar.role)];
        let base = avatar.role.tint();
        sprite.color = if starving {
            // Pulse rather than a flat grey: a static dim sprite reads as a
            // rendering bug, a slow pulse reads as distress.
            let pulse = 0.5 + 0.5 * (time.elapsed_secs() * HUNGRY_PULSE_HZ * TAU_F32).sin();
            let dim = 0.35 + 0.25 * pulse;
            Color::srgb(dim, dim * 0.92, dim * 0.88)
        } else {
            base
        };

        if let Some(mut flash) = flash {
            flash.0 -= time.delta_secs();
            if flash.0 <= 0.0 {
                commands.entity(entity).remove::<HireFlash>();
            } else if !starving {
                let t = (flash.0 / HIRE_HIGHLIGHT_SECONDS).clamp(0.0, 1.0);
                sprite.color = base.mix(&GOLD, t);
            }
        }
    }
}

const TAU_F32: f32 = std::f32::consts::TAU;

fn role_index(role: SupportRole) -> usize {
    match role {
        SupportRole::Chef => 0,
        SupportRole::Unpacker => 1,
        SupportRole::Technologist => 2,
    }
}

fn spawn_avatar(
    commands: &mut Commands,
    art: &WorkerArt,
    layout: &SceneLayout,
    role: SupportRole,
    slot: usize,
) {
    let scale = layout.world_scale();
    let (size, offset) = role.box_geometry();

    commands
        .spawn((
            SupportAvatar { role, slot },
            HireFlash(HIRE_HIGHLIGHT_SECONDS),
            Sprite {
                image: art.idle_image(),
                texture_atlas: Some(TextureAtlas {
                    layout: art.idle_layout(),
                    index: 0,
                }),
                color: role.tint(),
                ..default()
            },
            Transform::from_scale(Vec3::splat(scale)),
        ))
        .with_child((
            RoleBox,
            Sprite::from_color(role.box_color(), size),
            Transform::from_xyz(offset.x, offset.y, role.box_z()),
        ));
}

/// Keeps the role boxes on the texel grid when the viewport changes. They are
/// children, so they scale with the parent automatically; only their *sizes* are
/// authored in source texels and need no rescaling at all. This exists to keep
/// the badge honest instead.
pub(crate) fn sync_support_badges(
    mut commands: Commands,
    layout: Res<SceneLayout>,
    staff: Res<Staff>,
    mut badges: Query<(&SupportBadge, &mut Transform, &mut Visibility)>,
    mut labels: Query<(&ChildOf, &mut Text2d), With<BadgeLabel>>,
) {
    let existing: Vec<SupportRole> = badges.iter().map(|(badge, ..)| badge.0).collect();
    for role in SupportRole::ALL {
        if !existing.contains(&role) {
            commands
                .spawn((
                    SupportBadge(role),
                    BadgePlate,
                    Sprite::from_color(BADGE_PLATE, Vec2::new(30.0, 16.0)),
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .with_child((
                    BadgeLabel,
                    Text2d::new(String::new()),
                    TextColor(CREAM),
                    TextFont::from_font_size(11.0),
                    // In front of its own plate, and in front of the monkeys.
                    Transform::from_xyz(0.0, 0.0, 0.01),
                ));
        }
    }

    let scale = layout.world_scale();
    for (badge, mut transform, mut visibility) in &mut badges {
        let hired = staff.count(badge.0);
        // Only once the sprites stop being able to carry the count. A "x1" on a
        // lone chef is noise, and "x3" over three visible chefs reads as nine.
        let shown = hired as usize > AVATARS_PER_ROLE;
        *visibility = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if !shown {
            continue;
        }

        let (x, row) = layout.support_stand(badge.0);
        let feet = layout.ground_top() + row as f32 * ROW_STEP_TEXELS * scale;
        transform.translation = Vec3::new(
            // Past the last drawn monkey of the role, so it labels the group
            // rather than sitting on one of its heads.
            layout.snap(x + AVATARS_PER_ROLE as f32 * SLOT_STEP_TEXELS * scale),
            layout.snap(feet + FRAME_SIZE as f32 * 0.72 * scale),
            // In front of every monkey, including the front depth row.
            2.5,
        );
        // Scene text is authored at a fixed size, so on a phone a desktop-sized
        // badge would be twice its intended size against a 32 px monkey.
        transform.scale = Vec3::splat(scale.max(1.0));
    }

    for (parent, mut text) in &mut labels {
        let Ok((badge, ..)) = badges.get(parent.parent()) else {
            continue;
        };
        let next = format!("x{}", staff.count(badge.0));
        if text.0 != next {
            text.0 = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_stands_somewhere_distinct_at_every_viewport() {
        // Three stations around one deposit is the crowding risk the design
        // brief flagged. Whatever the viewport, no two roles may share an x.
        for viewport in [
            Vec2::new(320.0, 640.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
            Vec2::new(1920.0, 1080.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);
            let stands: Vec<f32> = SupportRole::ALL
                .iter()
                .map(|role| layout.support_stand(*role).0)
                .collect();

            for (a, b) in [(0, 1), (1, 2), (0, 2)] {
                assert!(
                    (stands[a] - stands[b]).abs() > 1.0,
                    "{viewport:?}: roles {a} and {b} overlap at {stands:?}"
                );
            }
        }
    }

    #[test]
    fn support_never_stands_on_the_worker_route() {
        // Workers walk between `grove_stand` and `stall_stand`. A support
        // monkey standing inside that span would be walked through all game.
        for viewport in [
            Vec2::new(320.0, 640.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);
            for role in SupportRole::ALL {
                let (x, _) = layout.support_stand(role);
                assert!(
                    x > layout.stall_stand,
                    "{viewport:?}: {role:?} at {x} is on the route (stall {})",
                    layout.stall_stand
                );
            }
        }
    }

    #[test]
    fn every_support_avatar_keeps_its_feet_inside_the_ground_band() {
        // The same constraint `worker::every_lane_keeps_its_feet_inside_the_ground_band`
        // enforces: the grass band is 16 texels, and a monkey standing above it
        // is standing on the sky.
        for role in SupportRole::ALL {
            let (_, row) = SceneLayout::default().support_stand(role);
            assert!(
                (row as f32 * ROW_STEP_TEXELS) < 16.0,
                "{role:?} sits {row} rows back, outside the grass"
            );
        }
    }
}
