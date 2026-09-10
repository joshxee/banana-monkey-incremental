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
//! Every monkey uses the same outlined lo-fi marker. Roles are told apart by a
//! coloured box worn as a hat, carried as a crate, or used as a desk.

use bevy::prelude::*;

use crate::{
    domain::{SUPPORT_MEAL_PERIOD, SUPPORT_PHASE_STRIDE, Staff, SupportCycle, SupportRole},
    game::{CREAM, GOLD, SceneLayout},
    isometric,
    worker::spawn_monkey_outline,
};

const FRAME_SIZE: u32 = 22;

/// Source texels to a metre of ground, at unit zoom. A monkey is 22 texels tall
/// and stands a shade under two metres, so the fan spacings that were authored
/// in texels keep the spread they were tuned to.
const METRES_TO_TEXELS: f32 = 12.0;

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

/// Half the width of the monkey's actual body, in source texels.
///
/// Not half the 32-texel frame: these sheets centre a much narrower monkey in a
/// square cell, and budgeting the whole frame would declare a collision from
/// two sprites whose transparent margins touch.
const BODY_HALF_TEXELS: f32 = 8.0;

/// Spacing between two monkeys of the same role, in source texels.
///
/// Wider than the widest role box, or three chefs' hats merge into one white
/// rectangle and the role reads as a single object with a strange head. Narrow
/// enough that a role's whole fan still fits between its neighbours - see
/// [`slot_offset_texels`].
const SLOT_STEP_TEXELS: f32 = 13.0;

/// Where a role's `slot`th monkey stands, relative to the role's station, when
/// `drawn` of them are on screen.
///
/// Centred on the station rather than running rightwards from it. Fanning in
/// one direction makes a role's width grow into the next role's space: at three
/// slots the chefs' third monkey landed exactly on the technologist's desk, so
/// buying a third chef appeared to delete the researcher.
pub(crate) fn slot_offset_texels(slot: usize, drawn: usize) -> f32 {
    (slot as f32 - (drawn as f32 - 1.0) * 0.5) * SLOT_STEP_TEXELS
}

/// How many monkeys of one role the scene has room to draw.
///
/// Three stations around one delivery point is a fixed amount of ground, and a
/// fan wide enough to overlap its neighbour turns three roles into one smear.
/// What gives is the *crowd*, not the layout - the count stays exact in the
/// badge and in the shop's OWNED column.
///
/// Both sides of the comparison scale with zoom, so unlike the screen-space
/// version this replaced, the answer does not move with the viewport: it is a
/// property of how far apart `SceneLayout::support_stand` puts the stations.
/// Move a station and this is what silently changes how full the deposit looks,
/// which is why `a_full_role_still_fits_three_monkeys` pins it.
pub(crate) fn avatars_per_role(layout: &SceneLayout) -> usize {
    let scale = layout.world_scale();
    let stations: Vec<Vec2> = SupportRole::ALL
        .iter()
        .map(|role| layout.board(layout.support_stand(*role)))
        .collect();
    // The closest two stations get on screen, which is what a fan of avatars
    // has to stay inside of if the roles are to stay tellable apart.
    let mut gap = f32::INFINITY;
    for (index, station) in stations.iter().enumerate() {
        for other in &stations[index + 1..] {
            gap = gap.min(station.distance(*other));
        }
    }

    (1..=AVATARS_PER_ROLE)
        .rev()
        .find(|drawn| {
            let half = slot_offset_texels(drawn - 1, *drawn) * scale + BODY_HALF_TEXELS * scale;
            half * 2.0 <= gap
        })
        .unwrap_or(1)
}
/// Depth spacing, matched to `worker::LANE_STEP_TEXELS` so support and workers
/// sit on the same ground plane.
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
/// The plate's size at unit zoom. Scaled by [`badge_scale`], along with the
/// font size, so the two stay the same shape at every zoom.
const BADGE_PLATE_TEXELS: Vec2 = Vec2::new(26.0, 14.0);
/// The badge's font size at unit zoom.
const BADGE_FONT: f32 = 10.0;

/// How much bigger a badge is drawn than its unit-zoom size.
///
/// Tracks the world scale so a phone does not get a badge twice its intended
/// size against a 32 px monkey - but only partly, because a label that scaled
/// fully would dominate the monkeys it counts.
fn badge_scale(layout: &SceneLayout) -> f32 {
    (layout.world_scale() * 0.6).max(0.8)
}

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
            let phase = index as f64 * SUPPORT_MEAL_PERIOD * SUPPORT_PHASE_STRIDE;
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
    units: Query<(&SupportRole, &SupportCycle), With<SupportUnit>>,
    mut avatars: Query<(
        Entity,
        &SupportAvatar,
        Option<&mut HireFlash>,
        &mut Transform,
        &mut Sprite,
    )>,
) {
    let per_role = avatars_per_role(&layout);
    for role in SupportRole::ALL {
        let wanted = (staff.count(role) as usize).min(per_role);
        let drawn = avatars
            .iter()
            .filter(|(_, avatar, ..)| avatar.role == role)
            .count();

        for slot in drawn..wanted {
            spawn_avatar(&mut commands, &layout, role, slot);
        }
        // A resize can shrink the fan, so the pool has to give sprites back as
        // well as take them - otherwise rotating a phone leaves a role drawn
        // three-wide in space that now fits one.
        for (entity, avatar, ..) in &avatars {
            if avatar.role == role && avatar.slot >= wanted {
                commands.entity(entity).despawn();
            }
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

    for (entity, avatar, flash, mut transform, mut sprite) in &mut avatars {
        let scale = layout.world_scale();

        // In metres across the ground now, not texels across the screen: a fan
        // of chefs spreads on the plane they are standing on, so the depth rule
        // sorts them against each other for free.
        let spread = slot_offset_texels(avatar.slot, per_role) / METRES_TO_TEXELS;
        let point = layout.support_point(avatar.role, spread);
        let half_height = FRAME_SIZE as f32 * 0.5 * scale;

        transform.translation = layout.board_snapped(point, half_height).extend(
            // Bounded, so a wide fan can never sort in front of a role standing
            // genuinely nearer the viewer.
            isometric::stand_z(point, (avatar.slot % 8) as f32 * isometric::NUDGE_STEP),
        );

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

fn spawn_avatar(commands: &mut Commands, layout: &SceneLayout, role: SupportRole, slot: usize) {
    let scale = layout.world_scale();
    let (size, offset) = role.box_geometry();

    commands
        .spawn((
            SupportAvatar { role, slot },
            HireFlash(HIRE_HIGHLIGHT_SECONDS),
            Sprite::from_color(role.tint(), Vec2::new(13.0, 22.0)),
            Transform::from_scale(Vec3::splat(scale)),
        ))
        .with_children(|avatar| {
            spawn_monkey_outline(avatar, Vec2::new(13.0, 22.0));
            avatar.spawn((
                RoleBox,
                Sprite::from_color(role.box_color(), size),
                Transform::from_xyz(offset.x, offset.y, role.box_z()),
            ));
        });
}

/// Keeps the role boxes on the texel grid when the viewport changes. They are
/// children, so they scale with the parent automatically; only their *sizes* are
/// authored in source texels and need no rescaling at all. This exists to keep
/// the badge honest instead.
pub(crate) fn sync_support_badges(
    mut commands: Commands,
    layout: Res<SceneLayout>,
    staff: Res<Staff>,
    mut badges: Query<(&SupportBadge, &mut Transform, &mut Visibility, &mut Sprite)>,
    mut labels: Query<(&ChildOf, &mut Text2d, &mut TextFont), With<BadgeLabel>>,
) {
    let existing: Vec<SupportRole> = badges.iter().map(|(badge, ..)| badge.0).collect();
    for role in SupportRole::ALL {
        if !existing.contains(&role) {
            commands
                .spawn((
                    SupportBadge(role),
                    BadgePlate,
                    Sprite::from_color(BADGE_PLATE, BADGE_PLATE_TEXELS),
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .with_child((
                    BadgeLabel,
                    Text2d::new(String::new()),
                    TextColor(CREAM),
                    TextFont::from_font_size(BADGE_FONT),
                    // In front of its own plate, and in front of the monkeys.
                    Transform::from_xyz(0.0, 0.0, 0.01),
                ));
        }
    }

    let scale = layout.world_scale();
    let plate = BADGE_PLATE_TEXELS * badge_scale(&layout);
    let per_role = avatars_per_role(&layout);
    for (badge, mut transform, mut visibility, mut sprite) in &mut badges {
        let hired = staff.count(badge.0);
        // Only once the sprites stop being able to carry the count. A "x1" on a
        // lone chef is noise, and "x3" over three visible chefs reads as nine.
        let shown = hired as usize > per_role;
        *visibility = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if !shown {
            continue;
        }
        if sprite.custom_size != Some(plate) {
            sprite.custom_size = Some(plate);
        }

        // `support_point` answers in metres on the ground, so this has to be
        // projected like every other station. Reading it as screen pixels
        // pinned every badge to the same corner of the window whatever the
        // role was doing.
        transform.translation = layout
            .board_snapped(
                layout.support_point(badge.0, 0.0),
                // Centred over the role's fan and lifted clear of it. Placed
                // *beside* the group it covered the outermost monkeys - and at
                // a crowded deposit those were the chefs' hats, which are the
                // only thing telling that role apart.
                (FRAME_SIZE as f32 * 0.5 + 13.0) * scale,
            )
            // A badge counts monkeys rather than standing among them, so it
            // belongs over the board, not in it.
            .extend(isometric::OVERLAY_Z);
        // Scale the *plate*, never the glyph. `Text2d` rasterises at its font
        // size and is then resampled by the transform, so scaling the badge up
        // took a 10 px rendering and smeared it: at the camera's opening zoom
        // "x6" read as "x fi". The font size is set from the same factor
        // instead, in `sync_badge_text`, and this stays at 1.
        transform.scale = Vec3::ONE;
    }

    // Rasterised at the size it is drawn at, so it stays a crisp glyph instead
    // of a resampled 10 px one. Rounded, because a font size of 17.3 px picks a
    // different hinting than 17 and the badge shimmers as the player pinches.
    let font = (BADGE_FONT * badge_scale(&layout)).round().max(BADGE_FONT);
    for (parent, mut text, mut text_font) in &mut labels {
        let Ok((badge, ..)) = badges.get(parent.parent()) else {
            continue;
        };
        let next = format!("x{}", staff.count(badge.0));
        if text.0 != next {
            text.0 = next;
        }
        let sized = bevy::text::FontSize::Px(font);
        if text_font.font_size != sized {
            text_font.font_size = sized;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every avatar of every role, as ground positions in metres.
    fn stations(layout: &SceneLayout) -> Vec<(SupportRole, Vec2)> {
        let drawn = avatars_per_role(layout);
        let mut placed = Vec::new();
        for role in SupportRole::ALL {
            for slot in 0..drawn {
                let spread = slot_offset_texels(slot, drawn) / METRES_TO_TEXELS;
                placed.push((role, layout.support_point(role, spread)));
            }
        }
        placed
    }

    #[test]
    fn every_role_stands_somewhere_distinct_at_every_viewport() {
        for viewport in [
            Vec2::new(320.0, 640.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
            Vec2::new(1920.0, 1080.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);
            let stands: Vec<Vec2> = SupportRole::ALL
                .iter()
                .map(|role| layout.support_stand(*role))
                .collect();

            for (a, b) in [(0, 1), (1, 2), (0, 2)] {
                assert!(
                    stands[a].distance(stands[b]) > 2.0,
                    "{viewport:?}: roles {a} and {b} share a spot at {stands:?}"
                );
            }
        }
    }

    #[test]
    fn a_full_fan_of_one_role_never_reaches_the_next_role() {
        // The bug this pins: fanning slots in one direction made a role's width
        // grow into its neighbour's, and the chefs' third monkey landed on the
        // technologist's desk - so buying a third chef appeared to delete the
        // researcher. Now that stations are ground positions the test can say
        // what it always meant: no two monkeys of different roles overlap.
        let body = BODY_HALF_TEXELS * 2.0 / METRES_TO_TEXELS;
        for viewport in [
            Vec2::new(320.0, 640.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);
            let placed = stations(&layout);
            for (index, (role, at)) in placed.iter().enumerate() {
                for (other_role, other) in &placed[index + 1..] {
                    if role == other_role {
                        continue;
                    }
                    assert!(
                        at.distance(*other) > body,
                        "{viewport:?}: {role:?} at {at:?} overlaps {other_role:?} at {other:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn support_never_stands_on_the_worker_route() {
        // A support monkey inside the walk would be walked through all game.
        // The route is a real polyline now, so this asks the honest question -
        // how far is the station from the line the workers actually cover -
        // rather than comparing screen x against a route that had no width.
        let route = crate::map::WorkedRoute::start();
        let clearance = 2.0;
        for viewport in [
            Vec2::new(320.0, 640.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);
            for (role, at) in stations(&layout) {
                let gap = distance_to_walk(&route.0, at);
                assert!(
                    gap > clearance,
                    "{viewport:?}: {role:?} stands {gap} m from the route"
                );
            }
        }
    }

    /// Shortest distance, in metres, from a ground position to a walk.
    fn distance_to_walk(route: &crate::map::Route, at: Vec2) -> f32 {
        let ground = |p: bevy::math::DVec2| Vec2::new(p.x as f32, p.y as f32);
        route
            .points()
            .windows(2)
            .map(|leg| {
                let (from, to) = (ground(leg[0]), ground(leg[1]));
                let span = to - from;
                let along = if span.length_squared() > 0.0 {
                    ((at - from).dot(span) / span.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                at.distance(from + span * along)
            })
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn support_never_stands_in_the_hand_harvest_target() {
        // The home tree is the node the player picks by hand (D24), and its
        // crown is the drag's start. A station under it puts a research desk
        // inside the thing the player is reaching for - which is where the
        // technologist landed the first time these were moved onto a ring,
        // because the search that placed them only knew about the walk and the
        // stall.
        for viewport in [
            Vec2::new(320.0, 568.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);
            for (role, at) in stations(&layout) {
                let gap = at.distance(layout.home_tree());
                assert!(
                    gap > 4.5,
                    "{viewport:?}: {role:?} stands {gap} m from the home tree"
                );
            }
        }
    }

    #[test]
    fn every_support_avatar_is_on_screen_when_the_game_opens() {
        // Stations are ground positions in metres, so they have to be projected
        // before being compared against anything measured in pixels. Comparing
        // the two directly, as this did when `support_stand` changed units,
        // passes by coincidence and asserts nothing.
        //
        // The bound is the *safe area* rather than a square of the window: with
        // a camera the player drives, "on the board" is no longer a fixed
        // rectangle, and what the player is owed is that the staff they paid
        // for are visible from where the game puts them at the start.
        for viewport in [
            Vec2::new(320.0, 568.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);
            let safe = layout.safe_area();
            for (role, at) in stations(&layout) {
                let screen = layout.board(at);
                assert!(
                    safe.contains(screen),
                    "{viewport:?}: {role:?} opens at {screen:?}, outside the safe area {safe:?}"
                );
            }
        }
    }

    #[test]
    fn a_full_role_still_fits_three_monkeys() {
        // The deposit looking populated is the whole point of drawing a fan at
        // all. Moving a station quietly costs a monkey per role, which is a
        // direct hit on "the field fills up" that nothing else would catch.
        for viewport in [
            Vec2::new(320.0, 640.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(1280.0, 720.0),
            Vec2::new(1920.0, 1080.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);
            assert_eq!(avatars_per_role(&layout), AVATARS_PER_ROLE, "{viewport:?}");
        }
    }
}
