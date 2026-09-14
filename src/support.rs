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
//! Every monkey is the same drawn spider worker the harvesters are. Roles are
//! told apart first by the disc of colour each stands on - the role's own
//! swatch from the shop, so the board and the shop name a role the same way -
//! and second by what it wears, carries or works at: a hat, a crate, a desk.

use bevy::prelude::*;

use crate::{
    art::{self, Art, Clip, Facing},
    domain::{SUPPORT_MEAL_PERIOD, SUPPORT_PHASE_STRIDE, Staff, SupportCycle, SupportRole},
    game::{CREAM, SceneLayout},
    isometric,
};

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
/// Not half the 32-texel cell: the sheets centre a narrower monkey in a
/// square cell, and budgeting the whole cell would declare a collision from two
/// sprites whose transparent margins touch. Measured off the idle frames, whose
/// opaque columns run from 9 to 51 about an anchor at 32 - and the *wider* of
/// the two sides, the tail's, because a fan budgeted on the narrow side lets
/// neighbours overlap by the difference and nothing notices.
const BODY_HALF_TEXELS: f32 = (32.0 - 9.0) * art::ART_SCALE;

/// How far to either side of the front monkey the two behind it stand, in
/// board texels across the screen.
///
/// More than half a body (19 art pixels on the head side), so each shows a snout
/// or a tail past the one in front. The fan used to be a line 13 texels a step
/// along a ground diagonal that recedes from the viewer, and two chefs drawn
/// that way overlapped almost completely: one monkey wearing two white caps.
const FAN_ACROSS_TEXELS: f32 = 22.5 * art::ART_SCALE;
/// How far behind the front monkey the back two stand, in board texels up the
/// screen: enough that a back monkey's head clears the front one's cap.
const FAN_BACK_TEXELS: f32 = 15.0 * art::ART_SCALE;

/// The disc a support monkey stands on, in texels: its contact shadow, and the
/// role's colour. Half again a walker's shadow, so it shows past the feet.
const ROLE_DISC_TEXELS: Vec2 = Vec2::new(52.5 * art::ART_SCALE, 20.0 * art::ART_SCALE);
/// And a lighter ring round it, one texel wide, so a disc reads as a marking
/// rather than as a large shadow - which olive and grey on green grass
/// otherwise do.
const ROLE_RIM_TEXELS: Vec2 = Vec2::new(ROLE_DISC_TEXELS.x + 2.0, ROLE_DISC_TEXELS.y + 2.0);
const ROLE_DISC_ALPHA: f32 = 0.6;
/// The chef's toque above its band, in texels: wider than the band, so the hat
/// has a toque's silhouette rather than being a white block on a head.
const CHEF_PUFF_TEXELS: Vec2 = Vec2::new(20.0 * art::ART_SCALE, 7.5 * art::ART_SCALE);

/// How long a new support monkey takes to walk from the bins to its station,
/// in seconds: about a harvester's pace over the eight metres between them.
const ARRIVE_SECONDS: f32 = 2.5;

/// How long after the game opens a support monkey appears already standing
/// rather than walking out, in seconds.
///
/// The walk-out shows a *hire*. The first reconcile after a load builds the
/// whole crew from the save, and walking all of it out of the house, flashing
/// gold, on every load says the player just bought staff they have had all
/// along - which is why a restored harvester skips its hire flash too.
const OPENING_SECONDS: f32 = 1.0;

/// A support monkey on its way from the bins to its station, and how long it
/// has been walking. Every monkey appears from the treehouse (D30).
#[derive(Component, Debug)]
pub(crate) struct Arriving(f32);

/// Where a role's `slot`th monkey stands relative to the role's station, in
/// board texels on the screen (y up, so up is further back).
///
/// A cluster, not a line: one in front, the next two behind it to either side.
/// A line wide enough to separate three drawn monkeys does not fit - the walk
/// runs up the screen past the depot, so a line across the screen runs straight
/// at it, and the depot sits near the bottom of a small phone's board. A
/// cluster is half as wide for the same three monkeys.
///
/// Centred across the station, never running one way from it: fanning in one
/// direction makes a role's width grow into the next role's space, and at three
/// slots the chefs' third monkey once landed on the technologist's desk, so
/// buying a third chef appeared to delete the researcher. And each slot stands
/// where it stands however many are drawn, so hiring a third does not shuffle
/// the first two.
pub(crate) fn slot_offset(slot: usize) -> Vec2 {
    match slot {
        0 => Vec2::ZERO,
        _ => Vec2::new(
            if slot % 2 == 1 { -1.0 } else { 1.0 } * FAN_ACROSS_TEXELS,
            FAN_BACK_TEXELS,
        ),
    }
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
            let across = (0..*drawn)
                .map(|slot| slot_offset(slot).x.abs())
                .fold(0.0, f32::max);
            let half = (across + BODY_HALF_TEXELS) * scale;
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
/// position and scale; its own x is where it sits on a monkey facing right,
/// mirrored by hand for one facing left - `flip_x` mirrors the texture and not
/// its children, and a chef walking left with its hat still at +x wears it on
/// its tail.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RoleBox {
    base_x: f32,
}

/// The coloured disc a support monkey stands on, and its rim. Children of the
/// avatar with their depth pinned to the ground-mark layer, like a walker's
/// shadow, so a disc never covers the monkey standing behind its owner.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RoleDisc {
    rim: bool,
}

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
    /// The role's colour: the swatch the shop draws beside its row
    /// (`assets/style/hud.css`, `.unit-swatch`), so the board and the shop
    /// name a role the same way.
    ///
    /// This replaced a multiply tint of 94-100% white, which on near-black
    /// monkey art moved nothing a player could see, and prop colours that
    /// contradicted the shop - the unpacker's crate was the worker's orange.
    pub(crate) fn colour(self) -> Color {
        match self {
            SupportRole::Chef => Color::srgb_u8(0x81, 0x94, 0x47),
            SupportRole::Unpacker => Color::srgb_u8(0x6C, 0x81, 0xA1),
            SupportRole::Technologist => Color::srgb_u8(0x87, 0x85, 0x7C),
        }
    }

    /// Colour of the role's box.
    fn box_color(self) -> Color {
        match self {
            // A white hat, and the one prop that keeps its own colour rather
            // than the shop's: a chef's toque is white everywhere, and it is
            // the only pure-white thing in the scene, which is what makes a
            // monkey read as a chef at a glance.
            SupportRole::Chef => Color::srgb(0.98, 0.96, 0.90),
            SupportRole::Unpacker | SupportRole::Technologist => self.colour(),
        }
    }

    /// Box size in source texels, and where its centre sits relative to the
    /// monkey's feet. Three shapes, three silhouettes: worn, carried, sat behind.
    ///
    /// Placed through the art's own coordinates (`Cell::offset_of`), never in
    /// texels guessed off the drawing. The art is a *quadruped* with a low back,
    /// a head forward and a tail that owns the space above it; the cap that was
    /// placed by eye sat over the face, down to the snout.
    fn box_geometry(self) -> (Vec2, Vec2) {
        // Sizes in the worker's own art pixels, so a prop stays the size of
        // the monkey wearing it whatever the art scale.
        let (size, art_centre) = match self {
            // Resting on the crown of the head, overlapping it by a little
            // over an art pixel so it reads as worn rather than floating.
            SupportRole::Chef => (
                Vec2::new(15.0, 10.0),
                Vec2::new(43.0, art::WORKER_CROWN_ROW - 2.0),
            ),
            // Carried in front of the chest, breaking the body outline.
            SupportRole::Unpacker => (Vec2::new(17.5, 15.0), Vec2::new(49.5, 36.0)),
            // A low desk the monkey works over, no wider than it is.
            SupportRole::Technologist => (Vec2::new(32.5, 15.0), Vec2::new(44.5, 48.5)),
        };
        (size * art::ART_SCALE, art::WORKER.offset_of(art_centre))
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

/// Everything `sync_support_avatars` touches on one avatar.
type AvatarView<'a> = (
    Entity,
    &'a SupportAvatar,
    Option<Mut<'a, HireFlash>>,
    Mut<'a, Transform>,
    Mut<'a, Sprite>,
    Option<&'a Children>,
    Option<Mut<'a, Arriving>>,
);

/// Presentation. Reconciles the avatar pool against the hired count, then poses
/// every avatar from the simulation entities behind it.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn sync_support_avatars(
    mut commands: Commands,
    art: Res<Art>,
    time: Res<Time>,
    layout: Res<SceneLayout>,
    staff: Res<Staff>,
    units: Query<(&SupportRole, &SupportCycle), With<SupportUnit>>,
    mut avatars: Query<AvatarView, Without<RoleDisc>>,
    mut discs: Query<(&RoleDisc, &mut Transform, &mut Sprite), Without<SupportAvatar>>,
    mut boxes: Query<(&RoleBox, &mut Transform), (Without<SupportAvatar>, Without<RoleDisc>)>,
) {
    let per_role = avatars_per_role(&layout);
    for role in SupportRole::ALL {
        let wanted = (staff.count(role) as usize).min(per_role);
        let drawn = avatars
            .iter()
            .filter(|(_, avatar, ..)| avatar.role == role)
            .count();

        let walk_out = time.elapsed_secs() > OPENING_SECONDS;
        for slot in drawn..wanted {
            spawn_avatar(&mut commands, &art, &layout, role, slot, walk_out);
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

    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * HUNGRY_PULSE_HZ * TAU_F32).sin();
    for (entity, avatar, flash, mut transform, mut sprite, children, arriving) in &mut avatars {
        // In metres across the ground now, not texels across the screen: a fan
        // of chefs spreads on the plane they are standing on, so the depth rule
        // sorts them against each other for free.
        let station = layout.support_point(avatar.role, slot_offset(avatar.slot));
        let mut point = station;

        // Which side of the monkey its head is on, for the props below.
        let mut heading_left = sprite.flip_x;

        // Walking out of the bins to the station, on the walk row for the way
        // it is going, its feet gripping the ground the same way a
        // harvester's do; then standing.
        if let Some(mut arriving) = arriving {
            arriving.0 += time.delta_secs();
            let from = layout.town_centre();
            let t = (arriving.0 / ARRIVE_SECONDS).clamp(0.0, 1.0);
            point = from.lerp(station, t * t * (3.0 - 2.0 * t));
            let (clip, frame, facing) = if t < 1.0 {
                // Seeded per slot, so a fan hired together does not step in
                // lockstep - the formation read `Playing::starting` avoids.
                let stride = art::walked_texels(point - from) / art::WALK_STRIDE_TEXELS
                    + avatar.slot as f32 * 0.618_034;
                (
                    Clip::Walk,
                    Clip::walk_frame(stride),
                    Facing::of_ground(station - from),
                )
            } else {
                commands.entity(entity).remove::<Arriving>();
                (
                    Clip::Idle,
                    avatar.slot as u32 % Clip::Idle.frames(),
                    Facing::SE,
                )
            };
            art.pose(&mut sprite, clip, facing, frame);
            // A walk is never mirrored - its left-facing rows are drawn - so
            // the head's side comes from the facing rather than the flip.
            heading_left = facing.mirrors_idle();
        }

        // No lift: the art carries its own ground anchor, so the ground
        // position *is* the transform. Keeping the old half-a-monkey lift left
        // the whole support crew hovering eleven texels above the depot.
        transform.translation = layout.board_snapped(point, 0.0).extend(
            // Bounded, so a wide fan can never sort in front of a role standing
            // genuinely nearer the viewer.
            isometric::stand_z(point, (avatar.slot % 8) as f32 * isometric::NUDGE_STEP),
        );
        // Every frame, not only at spawn: a pinch changes the zoom, and an
        // avatar scaled once drew at its hire-time size beside harvesters -
        // the same sprite - drawn at the new one. Unit z scale, so the disc's
        // depth below is a real offset.
        let scale = Vec3::new(layout.world_scale(), layout.world_scale(), 1.0);
        if transform.scale != scale {
            transform.scale = scale;
        }

        // Hunger shows on the disc rather than on the monkey. The art is
        // near-black, so the dimming that used to say it moved nothing; a
        // role's colour draining to grey and back is visible at any zoom.
        let starving = avatar.slot < hungry[role_index(avatar.role)];
        let fill = if starving {
            let grey = Color::srgb(0.45, 0.43, 0.40);
            avatar.role.colour().mix(&grey, 0.4 + 0.6 * pulse)
        } else {
            avatar.role.colour()
        };
        let monkey = match flash {
            Some(mut flash) => {
                flash.0 -= time.delta_secs();
                if flash.0 <= 0.0 {
                    commands.entity(entity).remove::<HireFlash>();
                    Color::WHITE
                } else {
                    // Toward gold and brightening, as a new harvester does.
                    let t = (flash.0 / HIRE_HIGHLIGHT_SECONDS).clamp(0.0, 1.0);
                    Color::WHITE.mix(&Color::srgb(2.0, 1.7, 0.6), t)
                }
            }
            None => Color::WHITE,
        };
        if sprite.color != monkey {
            sprite.color = monkey;
        }

        let facing = if heading_left { -1.0 } else { 1.0 };
        for child in children.into_iter().flatten() {
            if let Ok((role_box, mut at)) = boxes.get_mut(*child) {
                let x = role_box.base_x * facing;
                if at.translation.x != x {
                    at.translation.x = x;
                }
                continue;
            }
            let Ok((disc, mut at, mut disc_sprite)) = discs.get_mut(*child) else {
                continue;
            };
            // The rim a shade under the disc, so the two never tie.
            let z = isometric::MARK_Z - transform.translation.z - if disc.rim { 0.01 } else { 0.0 };
            if at.translation.z != z {
                at.translation.z = z;
            }
            let colour = if disc.rim {
                fill.lighter(0.25).with_alpha(ROLE_DISC_ALPHA)
            } else {
                fill.with_alpha(ROLE_DISC_ALPHA)
            };
            if disc_sprite.color != colour {
                disc_sprite.color = colour;
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
    art: &Art,
    layout: &SceneLayout,
    role: SupportRole,
    slot: usize,
    walk_out: bool,
) {
    let scale = layout.world_scale();
    let (size, offset) = role.box_geometry();

    // The same monkey the harvesters are, standing still. Support staff were
    // the one part of the cast still drawn as a tinted rectangle, and leaving
    // them that way would have put two art styles side by side at the one place
    // the player looks most - the depot, where the harvesters gather.
    //
    // The role is said by the disc underneath, in the shop's colour, which is
    // still legible when sixty harvesters crowd the depot around it. The idle
    // frame is the slot's own, so a fan of three is not one pose repeated.
    let mut spawned = commands.spawn((
        SupportAvatar { role, slot },
        art.worker(Clip::Idle, Facing::SE, slot as u32),
        art::WORKER.anchor(),
        Transform::from_scale(Vec3::new(scale, scale, 1.0)),
    ));
    // A hire walks out of the bins, flashing; a monkey restored with the save
    // is simply there (see `OPENING_SECONDS`).
    if walk_out {
        spawned.insert((HireFlash(HIRE_HIGHLIGHT_SECONDS), Arriving(0.0)));
    }
    spawned.with_children(|avatar| {
        avatar.spawn((
            RoleBox { base_x: offset.x },
            Sprite::from_color(role.box_color(), size),
            Transform::from_xyz(offset.x, offset.y, role.box_z()),
        ));
        if role == SupportRole::Chef {
            // The puff, sitting on the band and overlapping it by half a
            // texel so the two read as one hat.
            let puff = offset.y + (size.y + CHEF_PUFF_TEXELS.y) * 0.5 - 0.5;
            avatar.spawn((
                RoleBox { base_x: offset.x },
                Sprite::from_color(role.box_color(), CHEF_PUFF_TEXELS),
                Transform::from_xyz(offset.x, puff, role.box_z()),
            ));
        }
        for (rim, size) in [(true, ROLE_RIM_TEXELS), (false, ROLE_DISC_TEXELS)] {
            avatar.spawn((
                RoleDisc { rim },
                art.shadow(size, role.colour().with_alpha(ROLE_DISC_ALPHA)),
                // Depth is set every frame; see `sync_support_avatars`.
                Transform::default(),
            ));
        }
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
                layout.support_point(badge.0, Vec2::ZERO),
                // Centred over the role's fan and lifted clear of it. Placed
                // *beside* the group it covered the outermost monkeys - and at
                // a crowded deposit those were the chefs' hats, which are the
                // only thing telling that role apart.
                //
                // Measured from the top of the monkey's drawn silhouette - the
                // tail tip - rather than from a constant or from the canvas,
                // which reaches below the feet. So the badge stays above the
                // tail the day the art or its scale moves.
                (art::WORKER.height_above(art::WORKER_TOP_ROW) + 6.0) * scale,
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
                placed.push((role, layout.support_point(role, slot_offset(slot))));
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
        // what it always meant: no two monkeys of different roles overlap -
        // and overlap is a thing that happens on the *screen*, so the gap is
        // measured projected, in the texels the body width is measured in.
        let body = BODY_HALF_TEXELS * 2.0;
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
                        isometric::project(*at - *other).length() > body,
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
        //
        // Every supported viewport but the two smallest boards: the 320 x 568
        // phone's 320-pixel square and the 844 x 390 landscape phone's 286.
        // The opening view is centred on the treehouse, which is 284 pixels
        // tall (D30), so on those two boards it is nearly all there is, and
        // part of every role's fan opens past an edge of it, a short pan away
        // - as the home tree does on landscape. The
        // owner chose the landmark centred over the crew framed on the
        // smallest boards, and this is where that shows.
        for viewport in [Vec2::new(390.0, 844.0), Vec2::new(1280.0, 720.0)] {
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
    fn support_never_stands_in_the_unloading_crowd() {
        // The harvesters unloading at the bins stand on a ring out to
        // `RING_OUTER`. A support monkey inside it is one of the queue as far as
        // the player can tell - two chefs stood in it once, and their hats read
        // as delivering monkeys - so every monkey of every fan stands clear of
        // it by half a body.
        let clear = crate::worker::RING_OUTER + 0.5;
        for viewport in [
            Vec2::new(320.0, 568.0),
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let layout = SceneLayout::for_viewport(viewport);
            for (role, at) in stations(&layout) {
                let out = at.distance(layout.town_centre());
                assert!(
                    out > clear,
                    "{viewport:?}: {role:?} stands {out} m from the bins, in the queue"
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
