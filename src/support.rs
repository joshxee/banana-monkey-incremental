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
    art::{self, Art, Clip, Courier, Facing},
    domain::{
        HarvestCycle, SUPPORT_MEAL_PERIOD, SUPPORT_PHASE_STRIDE, Segment, Staff, SupportCycle,
        SupportRole,
    },
    game::{CREAM, SceneLayout},
    isometric,
    worker::{Lane, Playing, Shadow, Worker},
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
pub(crate) const BODY_HALF_ART_PIXELS: f32 = 32.0 - 9.0;
const BODY_HALF_TEXELS: f32 = BODY_HALF_ART_PIXELS * art::ART_SCALE;

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
        // The unpacker is drawn as a squirrel monkey courier instead, and
        // couriers shuttle rather than stand: see `sync_couriers`.
        if role == SupportRole::Unpacker {
            continue;
        }
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
    for (badge, mut transform, mut visibility, mut sprite) in &mut badges {
        let hired = staff.count(badge.0);
        // Only once the sprites stop being able to carry the count. A "x1" on a
        // lone chef is noise, and "x3" over three visible chefs reads as nine.
        // Per role, because the unpacker's couriers are capped by the traffic
        // they run rather than by the ground beside a station.
        let shown = hired as usize > drawn_for(badge.0, &layout);
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
        //
        // The Unpacker's hangs over the bins instead, because that is where its
        // monkeys are. Its station is only a place on a list now (D31), so a
        // badge there would be an "x9" plate counting nine squirrels over a
        // patch of empty grass while the nine of them worked at the depot.
        let over = if badge.0 == SupportRole::Unpacker {
            layout.town_centre()
        } else {
            layout.support_point(badge.0, Vec2::ZERO)
        };
        transform.translation = layout
            .board_snapped(
                over,
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

// ─────────────────────────────────────────────────────────── squirrel couriers

/// How many squirrel monkeys the depot draws, however many Unpackers are hired.
///
/// Higher than [`AVATARS_PER_ROLE`], and for a reason that does not apply to the
/// other two roles: a courier does not stand in a fan, so what caps it is not
/// how much ground a station has beside it. Six is what the traffic itself
/// affords - the lane between the unloading ring and the bins is a few metres
/// of ground, and a seventh squirrel running it adds a sprite rather than a
/// reading. Past this the count moves to the badge, exactly as it does for a
/// chef.
pub(crate) const COURIER_LIMIT: usize = 6;
// Two or more, because the waiting arc is divided into `COURIER_LIMIT - 1`
// steps. Stated here, next to the value, rather than discovered as a NaN
// bearing and a vanished sprite.
const _: () = assert!(COURIER_LIMIT >= 2);

/// How long one fetch-and-deliver run takes, in seconds.
///
/// Deliberately not tied to the unload it is helping with. `M_unpack` already
/// shortens `Segment::Unload`, so the *harvester* visibly stands at the bins for
/// less time with every Unpacker bought; pinning the shuttle to that as well
/// would slow the squirrels down as the player hired more of them, which is
/// backwards. A fixed, brisk run is what the artist's 400 ms dart loop is drawn
/// for.
const SHUTTLE_SECONDS: f32 = 1.8;

/// How much of the run a courier spends at each end, as a fraction: long enough
/// for the take and the drop to read as moments rather than as a bounce.
const SHUTTLE_PAUSE: f32 = 0.14;

/// How near the ends of its run a courier actually gets, as a fraction of the
/// gap between the monkey and the bins.
///
/// Short of both, and not by accident: a courier that reached 0 would stand
/// inside the bins it is loading, and one that reached 1 would stand inside the
/// harvester it is taking from.
const SHUTTLE_NEAR: f32 = 0.1;
const SHUTTLE_FAR: f32 = 0.9;

/// How far out from the delivery point a courier waits and drops its load, in
/// metres.
///
/// Just inside the ring the harvesters stand on, at the boxes themselves. Wide
/// enough that six of them waiting are a line along the front of the bins
/// rather than a heap on top of them: a squirrel is about 1.9 m of ground
/// across at this scale, and six over a half-turn of a 2.9 m circle leaves
/// roughly a body between neighbours.
const DROP_RADIUS: f32 = 2.9;
/// And on which side of them: towards the viewer, in ground metres, so the
/// couriers work across the front of the boxes rather than behind them.
const DROP_FRONT: f32 = std::f32::consts::FRAC_PI_4;
/// How much of the circle the waiting places are spread over.
const DROP_SWEEP: f32 = std::f32::consts::PI;

/// One drawn squirrel monkey.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct CourierAvatar {
    index: usize,
}

/// A courier's playhead: which clip, which of the eight bearings, how far
/// through the loop, and which harvester it is serving.
///
/// The travelling loops are advanced by distance drawn rather than by the clock,
/// the way a harvester's walk is (see `art::COURIER_STRIDE_TEXELS`), so the
/// squirrel's feet grip the ground at whatever speed the shuttle is running.
/// The idle loop is a clock, because standing still covers no ground.
#[derive(Component, Debug)]
pub(crate) struct CourierPlaying {
    clip: Courier,
    facing: Facing,
    frame: u32,
    elapsed: f32,
    stride: f32,
    last: Option<Vec2>,
    /// The hire index of the harvester it is helping, held until that monkey
    /// stops unloading.
    ///
    /// The whole reason this is remembered rather than recomputed. Picking a
    /// target by position in a list of whoever is currently unloading looks
    /// stable - the list is sorted - but the *list's length* changes every time
    /// a harvester arrives or leaves, which at the shipped cadence is about
    /// once a second, and every change re-maps every courier onto a different
    /// monkey. A squirrel then crosses the whole depot between two frames, with
    /// a step long enough to snap its bearing and spin its stride as well.
    serving: Option<u32>,
    /// Seconds into the current run, so a reassignment starts a fresh one from
    /// the bins rather than dropping the courier mid-lane.
    run: f32,
}

impl CourierPlaying {
    /// Advance the playhead by one frame, having been drawn `step` texels along
    /// the board.
    fn advance(&mut self, step: Vec2, delta: f32) {
        // A courier that has stopped keeps the way it was facing, rather than
        // snapping north on the frame it pauses to take or to drop.
        if step.length_squared() > f32::EPSILON {
            self.facing = Facing::of(step);
        }
        match self.clip {
            Courier::Idle => {
                // Capped before the loop spends it, exactly as `animate_workers`
                // caps a walker's: a frame that arrives after a long stall - a
                // backgrounded tab, a breakpoint - would otherwise be paid out
                // one animation frame at a time.
                self.elapsed = (self.elapsed + delta).min(1.0);
                while self.elapsed >= Courier::idle_hold() {
                    self.elapsed -= Courier::idle_hold();
                    self.frame = (self.frame + 1) % Courier::Idle.frames();
                }
            }
            travelling => {
                self.stride = (self.stride + step.length() / art::COURIER_STRIDE_TEXELS).fract();
                self.frame =
                    (self.stride * travelling.frames() as f32) as u32 % travelling.frames();
            }
        }
    }

    /// Stand still, facing the viewer.
    ///
    /// Squared up rather than left on whatever bearing the last run ended on.
    /// All eight directions are drawn and half of them are a grey back on green
    /// grass; a rank of couriers waiting at the boxes should be looking out of
    /// the screen - and `Facing::SE` is the facing every other standing monkey
    /// in the scene waits on.
    fn wait(&mut self) {
        self.clip = Courier::Idle;
        self.facing = Facing::SE;
    }
}

/// How many monkeys of a role the scene draws before the count moves to a badge.
pub(crate) fn drawn_for(role: SupportRole, layout: &SceneLayout) -> usize {
    match role {
        SupportRole::Unpacker => COURIER_LIMIT,
        _ => avatars_per_role(layout),
    }
}

/// Where a courier is on its run, and which way it is going.
///
/// A triangle with a flat top and a flat bottom: out to the monkey, a pause to
/// take the banana, back to the bins, a pause to drop it. `carrying` is the
/// return leg, which is the one the loaded sheet is drawn for. `moving` is false
/// through the two pauses, where the courier is doing the taking and the
/// dropping and the travelling loops have no distance to advance on.
fn shuttle(phase: f32) -> (f32, bool, bool) {
    let phase = phase.rem_euclid(1.0);
    let travel = (1.0 - 2.0 * SHUTTLE_PAUSE) * 0.5;
    if phase < travel {
        (phase / travel, false, true)
    } else if phase < travel + SHUTTLE_PAUSE {
        (1.0, false, false)
    } else if phase < 2.0 * travel + SHUTTLE_PAUSE {
        (1.0 - (phase - travel - SHUTTLE_PAUSE) / travel, true, true)
    } else {
        (0.0, true, false)
    }
}

/// Where the `index`th courier waits, in metres: its own place along the front
/// of the bins, so six of them are a line rather than a heap.
fn waiting_place(layout: &SceneLayout, index: usize) -> Vec2 {
    let across = DROP_SWEEP * (index as f32 / (COURIER_LIMIT as f32 - 1.0) - 0.5);
    layout.town_centre() + Vec2::from_angle(DROP_FRONT + across) * DROP_RADIUS
}

/// Keep one squirrel monkey drawn per Unpacker hired, up to [`COURIER_LIMIT`].
///
/// Its own system, ahead of the one that poses them, for the reason the
/// harvesters' spawn and pose are two systems: a reconcile that also draws has
/// to carry a "this one is about to go" branch through the drawing, and the
/// branch is where a stale index gets used.
pub(crate) fn spawn_missing_couriers(
    mut commands: Commands,
    art: Res<Art>,
    layout: Res<SceneLayout>,
    staff: Res<Staff>,
    couriers: Query<(Entity, &CourierAvatar)>,
) {
    let wanted = (staff.count(SupportRole::Unpacker) as usize).min(COURIER_LIMIT);
    let drawn = couriers.iter().count();
    for index in drawn..wanted {
        spawn_courier(&mut commands, &art, &layout, index);
    }
    // Handed back by index, so the squirrels that stay keep the harvesters they
    // were helping - a sacking should not reshuffle the survivors.
    for (entity, avatar) in &couriers {
        if avatar.index >= wanted {
            commands.entity(entity).despawn();
        }
    }
}

/// Everything `sync_couriers` touches on one squirrel.
type CourierView<'a> = (
    &'a CourierAvatar,
    Mut<'a, CourierPlaying>,
    Mut<'a, Transform>,
    Mut<'a, Sprite>,
);

/// Run each squirrel monkey between a harvester that has just arrived to unload
/// and the bins it is unloading into.
///
/// The Unpacker is the one support role whose job is a *journey* - it shortens
/// `Segment::Unload`, which is the leg a harvester spends standing at the bins -
/// and standing it still beside a station said none of that. A courier is
/// therefore placed from the harvester it is helping rather than from a station
/// of its own: it is only ever on the line between that monkey and the bins, so
/// what the player sees is the banana being taken off the queue.
///
/// It reads the harvesters' *drawn* positions, which is why it runs here in
/// `Update`, after `worker::position_workers` has written them, and why nothing
/// it computes may reach `FixedUpdate`: a courier is decoration over an unload
/// the economy has already decided the length of.
///
/// An Unpacker with no meal waits instead of working, which is the same thing
/// `sync_support_avatars` says with a greying disc for the other two roles. It
/// is not decoration: an unfed Unpacker really is out of `M_unpack`
/// (`recompute_multipliers`), so the player's unload rate drops, and a drop with
/// nothing on the board to explain it is the one thing the hunger signal exists
/// to prevent.
#[allow(clippy::type_complexity)]
pub(crate) fn sync_couriers(
    art: Res<Art>,
    time: Res<Time>,
    layout: Res<SceneLayout>,
    units: Query<(&SupportRole, &SupportCycle), With<SupportUnit>>,
    harvesters: Query<(&HarvestCycle, &Lane, &Playing), With<Worker>>,
    mut couriers: Query<CourierView>,
) {
    if couriers.is_empty() {
        return;
    }

    // The monkeys standing at the bins with a load to give away, by hire index:
    // the one stable name a harvester has, so a courier can hold on to one.
    let unloading: Vec<(u32, Vec2)> = harvesters
        .iter()
        .filter(|(cycle, ..)| cycle.segment() == Segment::Unload)
        .filter_map(|(_, lane, playing)| playing.standing().map(|at| (lane.index(), at)))
        .collect();

    let hungry = units
        .iter()
        .filter(|(role, cycle)| **role == SupportRole::Unpacker && cycle.is_hungry())
        .count();

    let scale = layout.world_scale();
    // Who is already being helped, so a courier picking a *new* monkey takes one
    // nobody is on before it doubles up.
    let mut taken: Vec<u32> = Vec::new();
    for (avatar, mut playing, mut transform, mut sprite) in &mut couriers {
        let waiting = waiting_place(&layout, avatar.index);
        // Which courier goes hungry is arbitrary and stable enough frame to
        // frame, exactly as it is for a fan of chefs: the *count* drives it.
        let fed = avatar.index >= hungry;
        let served = fed
            .then(|| pick_harvester(&playing, avatar.index, &unloading, &taken))
            .flatten();
        playing.serving = served.map(|(lane, _)| lane);
        if let Some((lane, _)) = served {
            taken.push(lane);
        }

        let point = match served {
            Some((_, monkey)) => {
                playing.run += time.delta_secs();
                let (along, carrying, moving) =
                    shuttle(playing.run / SHUTTLE_SECONDS + avatar.index as f32 * 0.618_034);
                // Standing still at either end is the take and the drop, and a
                // distance-driven travel loop has nothing to advance on there -
                // so it would hold one mid-stride frame for a quarter second.
                // The artist's idle loop is what those beats are drawn for.
                playing.clip = match (moving, carrying) {
                    (false, _) => Courier::Idle,
                    (true, true) => Courier::Carry,
                    (true, false) => Courier::Dart,
                };
                waiting.lerp(monkey, SHUTTLE_NEAR + along * (SHUTTLE_FAR - SHUTTLE_NEAR))
            }
            // Nobody to help, or nothing to eat: it waits at the bins rather
            // than walking off to a station. A courier belongs at the boxes -
            // which is inside the ring the harvesters stand on, where D30
            // forbids a support *station*, and rightly: two chefs stood there
            // once and read as monkeys delivering. A squirrel monkey among
            // spider monkeys cannot be mistaken for one of the queue.
            None => {
                playing.run = 0.0;
                playing.wait();
                waiting
            }
        };

        // The facing comes from the ground actually covered, projected, which
        // is what `Facing::of` wants: the sheets are drawn in screen compass
        // directions, so a ground heading read straight off would face a
        // courier up a diagonal it is not on.
        if let Some(last) = playing.last {
            let step = isometric::project(point - last);
            playing.advance(step, time.delta_secs());
        }
        playing.last = Some(point);

        art.pose_courier(&mut sprite, playing.clip, playing.facing, playing.frame);

        transform.translation = layout.board_snapped(point, 0.0).extend(isometric::stand_z(
            point,
            // A courier runs through the queue, so it needs a tie-break against
            // the harvesters it is weaving between, not just against its peers.
            (avatar.index % 8) as f32 * isometric::NUDGE_STEP,
        ));
        let wanted_scale = Vec3::new(scale, scale, 1.0);
        if transform.scale != wanted_scale {
            transform.scale = wanted_scale;
        }
    }
}

/// Which harvester a courier serves this frame, and where it is standing.
///
/// It keeps the one it already had for as long as that monkey is still
/// unloading, which is what stops a squirrel crossing the depot every time the
/// queue's length changes. Only when its monkey has finished does it take
/// another - preferring one nobody else is on, and otherwise sharing, because
/// three squirrels helping one monkey reads perfectly well and an idle one
/// beside a queue does not.
fn pick_harvester(
    playing: &CourierPlaying,
    index: usize,
    unloading: &[(u32, Vec2)],
    taken: &[u32],
) -> Option<(u32, Vec2)> {
    if let Some(lane) = playing.serving
        && let Some(held) = unloading.iter().find(|(at, _)| *at == lane)
    {
        return Some(*held);
    }
    if unloading.is_empty() {
        return None;
    }
    unloading
        .iter()
        .find(|(lane, _)| !taken.contains(lane))
        .copied()
        .or_else(|| unloading.get(index % unloading.len()).copied())
}

fn spawn_courier(commands: &mut Commands, art: &Art, layout: &SceneLayout, index: usize) {
    let scale = layout.world_scale();
    // Placed where it will wait, not at the origin. A `Transform` left at zero
    // draws one frame in the middle of the screen before the pose system reaches
    // it, and six couriers can be hired in one go.
    let at = waiting_place(layout, index);
    commands
        .spawn((
            CourierAvatar { index },
            CourierPlaying {
                clip: Courier::Idle,
                facing: Facing::SE,
                frame: index as u32 % Courier::Idle.frames(),
                elapsed: 0.0,
                stride: (index as f32 * 0.618_034).fract(),
                last: None,
                serving: None,
                run: 0.0,
            },
            art.courier(Courier::Idle, Facing::SE, index as u32),
            art::SQUIRREL.anchor(),
            Transform::from_translation(
                layout
                    .board_snapped(at, 0.0)
                    .extend(isometric::stand_z(at, 0.0)),
            )
            .with_scale(Vec3::new(scale, scale, 1.0)),
        ))
        .with_child((
            Shadow,
            art.shadow(COURIER_SHADOW_TEXELS, isometric::SHADOW_COLOUR),
            Transform::default(),
        ));
}

/// A courier's contact shadow: two thirds of a harvester's, because the animal
/// under it is two thirds the size.
const COURIER_SHADOW_TEXELS: Vec2 =
    Vec2::new(art::SHADOW_TEXELS.x * 0.66, art::SHADOW_TEXELS.y * 0.66);

/// Keeps a courier's shadow on the ground-mark layer, exactly as a harvester's
/// is kept there.
pub(crate) fn pin_courier_shadows(
    couriers: Query<(&Transform, &Children), With<CourierAvatar>>,
    mut shadows: Query<&mut Transform, (With<Shadow>, Without<CourierAvatar>)>,
) {
    for (transform, children) in &couriers {
        for child in children.iter() {
            if let Ok(mut at) = shadows.get_mut(child) {
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
    fn a_courier_is_only_ever_between_a_monkey_and_the_bins() {
        // The Unpacker shortens `Segment::Unload`, which is the leg a harvester
        // spends standing at the bins - so its monkeys are drawn doing that job
        // and nothing else (D31). What has to hold is that a squirrel is always
        // *on the line* between the monkey it is helping and the boxes: a
        // courier that wanders is a squirrel monkey, not an unpacker.
        let (mut reached_monkey, mut reached_bins) = (false, false);
        let (mut carried_out, mut carried_back) = (false, false);
        let (mut paused_loaded, mut paused_empty) = (false, false);
        let mut previous = shuttle(0.0).0;
        for step in 0..=2000 {
            let phase = step as f32 / 2000.0;
            let (along, carrying, moving) = shuttle(phase);
            assert!(
                (0.0..=1.0).contains(&along),
                "a courier at phase {phase} is {along} of the way along its run"
            );
            // Continuous: it runs the lane rather than blinking along it.
            assert!((along - previous).abs() < 0.02, "the run jumps at {phase}");
            previous = along;
            reached_monkey |= along > 0.999;
            reached_bins |= along < 0.001;
            // The two beats the travelling loops have no distance to play on:
            // taking the banana at the monkey, dropping it at the bins. Both
            // must exist, or a courier holds a mid-stride frame instead.
            if !moving {
                if carrying {
                    paused_empty = true;
                } else {
                    paused_loaded = true;
                }
            }
            // Loaded on the way in, empty on the way out - the two sheets the
            // artist drew for exactly this.
            if along > 0.02 && along < 0.98 {
                if carrying {
                    carried_back = true;
                } else {
                    carried_out = true;
                }
            }
        }
        assert!(reached_monkey, "a courier never reaches the monkey");
        assert!(reached_bins, "a courier never reaches the bins");
        assert!(carried_out && carried_back, "a courier runs one way only");
        assert!(
            paused_loaded && paused_empty,
            "a courier never stops to take or to drop"
        );
        // And it is a loop: the last frame of a run hands over to the first.
        assert!((shuttle(0.9999).0 - shuttle(0.0).0).abs() < 0.01);
    }

    #[test]
    fn couriers_are_capped_by_their_traffic_and_the_rest_by_their_ground() {
        // Two different caps for two different reasons, and the badge follows
        // whichever applies: a chef's fan is bounded by the ground beside its
        // station, a courier by the lane it runs.
        let layout = SceneLayout::for_viewport(Vec2::new(1280.0, 720.0));
        assert_eq!(drawn_for(SupportRole::Unpacker, &layout), COURIER_LIMIT);
        assert_eq!(drawn_for(SupportRole::Chef, &layout), AVATARS_PER_ROLE);
        const { assert!(COURIER_LIMIT > AVATARS_PER_ROLE) };
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

#[cfg(test)]
mod courier_app_tests {
    use super::*;
    use bevy::{asset::AssetPlugin, image::TextureAtlasLayout};

    use crate::{
        domain::{CycleSpec, Multipliers, Research, cycle_time},
        isometric::{Bins, WorldRoot},
        map,
        worker::{self, Lane},
    };

    /// The courier systems on their own, with a real asset server behind them
    /// and the harvester systems they read in front of them.
    ///
    /// Bevy resolves query conflicts when a schedule is *built*, not when it is
    /// compiled, so a system whose parameters overlap panics the first time the
    /// game is opened and never in `cargo test`. These read the harvesters while
    /// writing the squirrels, which is exactly the shape that conflicts.
    ///
    /// `position_workers` is in the chain rather than stubbed, because it is the
    /// system that writes the drawn position `sync_couriers` reads: with it here
    /// the fixture covers the ordering the plugin's chain promises as well as
    /// the conflict.
    fn couriers() -> App {
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
        app.insert_resource(art)
            .insert_resource(map::Village::start())
            .insert_resource(map::WorkedRoute::start())
            .init_resource::<SceneLayout>()
            .init_resource::<Staff>()
            .init_resource::<Multipliers>()
            .init_resource::<Research>()
            .add_systems(
                Update,
                (
                    worker::dress_actors,
                    worker::position_workers,
                    spawn_missing_couriers,
                    sync_couriers,
                    pin_courier_shadows,
                    crate::isometric::sync_cart_bins,
                )
                    .chain(),
            );
        app.world_mut()
            .spawn((WorldRoot, Transform::default(), Visibility::default()));
        app
    }

    fn hire(app: &mut App, role: SupportRole, count: u32) {
        for index in 0..count {
            app.world_mut().resource_mut::<Staff>().hire(role);
            let phase = index as f64 * SUPPORT_MEAL_PERIOD * SUPPORT_PHASE_STRIDE;
            app.world_mut()
                .spawn((SupportUnit, role, SupportCycle::starting(phase)));
        }
    }

    /// A harvester standing at the bins with a load to give away, at `lane`.
    fn spawn_unloading(app: &mut App, lane: u32) {
        let multipliers = Multipliers::default();
        let cycle = (0..4000)
            .map(|step| {
                HarvestCycle::from_phase(
                    cycle_time(CycleSpec::WORKER, multipliers) * f64::from(step) / 4000.0,
                    CycleSpec::WORKER,
                    multipliers,
                )
            })
            .find(|cycle| cycle.segment() == Segment::Unload)
            .expect("a worker cycle passes through Unload");
        app.world_mut()
            .spawn((Worker, cycle, CycleSpec::WORKER, Lane::hire(lane)))
            .with_child((
                worker::CarriedBanana,
                Transform::default(),
                Visibility::Hidden,
            ));
    }

    /// Every courier's drawn ground position.
    fn couriers_on_the_ground(app: &mut App) -> Vec<Vec2> {
        let layout = *app.world().resource::<SceneLayout>();
        app.world_mut()
            .query::<(&CourierAvatar, &Transform)>()
            .iter(app.world())
            .map(|(_, at)| layout.ground(at.translation.truncate()))
            .collect()
    }

    /// Where the harvester at `lane` is drawn, in metres.
    fn harvester_at(app: &mut App, lane: u32) -> Vec2 {
        app.world_mut()
            .query::<(&Lane, &Playing)>()
            .iter(app.world())
            .find(|(at, _)| at.index() == lane)
            .and_then(|(_, playing)| playing.standing())
            .expect("the harvester has been drawn")
    }

    #[test]
    fn a_courier_appears_for_each_unpacker_and_runs_the_lane_to_the_bins() {
        let mut app = couriers();
        app.update();
        assert!(
            couriers_on_the_ground(&mut app).is_empty(),
            "nobody is hired yet"
        );

        hire(&mut app, SupportRole::Unpacker, 2);
        app.update();
        assert_eq!(
            couriers_on_the_ground(&mut app).len(),
            2,
            "one squirrel per unpacker"
        );

        // A harvester standing at the bins with a load to give away. The
        // couriers must end up between it and the delivery point, never out at
        // a station of their own.
        spawn_unloading(&mut app, 0);
        app.update();
        let bins = app.world().resource::<SceneLayout>().town_centre();
        let monkey = harvester_at(&mut app, 0);

        let mut seen: Vec<Vec2> = Vec::new();
        for _ in 0..40 {
            app.update();
            seen.extend(couriers_on_the_ground(&mut app));
        }
        assert!(!seen.is_empty());
        let lane = monkey - bins;
        for at in seen {
            let along = ((at - bins).dot(lane) / lane.length_squared()).clamp(0.0, 1.0);
            let off = at.distance(bins + lane * along);
            assert!(
                off < DROP_RADIUS + 1.0,
                "a courier stands {off} m off the lane between the monkey and the bins"
            );
        }
    }

    #[test]
    fn a_courier_keeps_its_monkey_while_the_queue_changes_around_it() {
        // The defect a sorted list does not fix. Picking a target by *position*
        // in the set of whoever is unloading re-maps every courier the moment
        // that set changes size - about once a second at the shipped cadence -
        // and a squirrel then crosses the depot between two frames.
        let mut app = couriers();
        hire(&mut app, SupportRole::Unpacker, 1);
        spawn_unloading(&mut app, 0);
        spawn_unloading(&mut app, 7);
        for _ in 0..4 {
            app.update();
        }
        let held = app
            .world_mut()
            .query::<&CourierPlaying>()
            .iter(app.world())
            .next()
            .and_then(|playing| playing.serving)
            .expect("the courier took a monkey");

        // Take away the *other* monkey. The courier's own is untouched, so it
        // must not move to the one that is left.
        let other = if held == 0 { 7 } else { 0 };
        let stray = app
            .world_mut()
            .query::<(Entity, &Lane)>()
            .iter(app.world())
            .find(|(_, lane)| lane.index() == other)
            .map(|(entity, _)| entity)
            .expect("both harvesters are there");
        app.world_mut().entity_mut(stray).despawn();

        let before = couriers_on_the_ground(&mut app)[0];
        app.update();
        let after = couriers_on_the_ground(&mut app)[0];
        assert_eq!(
            app.world_mut()
                .query::<&CourierPlaying>()
                .iter(app.world())
                .next()
                .and_then(|playing| playing.serving),
            Some(held),
            "the courier swapped monkeys when the queue changed length"
        );
        assert!(
            before.distance(after) < 0.5,
            "the courier jumped {} m when the queue changed length",
            before.distance(after)
        );
    }

    #[test]
    fn an_unfed_unpacker_stops_working_where_the_player_can_see_it() {
        // The signal `sync_support_avatars` gives the other two roles with a
        // greying disc. An unfed Unpacker is really out of `M_unpack`, so the
        // unload rate drops; without this the drop has nothing on the board to
        // explain it.
        let mut app = couriers();
        hire(&mut app, SupportRole::Unpacker, 2);
        spawn_unloading(&mut app, 0);
        for _ in 0..3 {
            app.update();
        }
        let working = |app: &mut App| {
            app.world_mut()
                .query::<&CourierPlaying>()
                .iter(app.world())
                .filter(|playing| playing.serving.is_some())
                .count()
        };
        assert_eq!(working(&mut app), 2, "both couriers are fed and working");

        // Starve one of them, the way the simulation does: a shift falls due
        // against an empty larder, so the monkey goes unpaid and idle.
        let unit = app
            .world_mut()
            .query_filtered::<Entity, With<SupportUnit>>()
            .iter(app.world())
            .next()
            .expect("an unpacker exists");
        let mut starved = SupportCycle::starting(0.0);
        let mut larder = 0.0;
        starved.advance(SUPPORT_MEAL_PERIOD, 1.0, &mut larder);
        assert!(starved.is_hungry());
        app.world_mut().entity_mut(unit).insert(starved);
        app.update();
        assert_eq!(working(&mut app), 1, "a hungry unpacker kept working");
    }

    #[test]
    fn the_carts_bins_go_up_with_the_research_that_unlocks_the_cart() {
        let mut app = couriers();
        app.update();
        let standing = |app: &mut App| {
            app.world_mut()
                .query::<&Bins>()
                .iter(app.world())
                .filter(|bins| **bins == Bins::Cart)
                .count()
        };
        assert_eq!(standing(&mut app), 0, "no cart, no bins");

        let level = Research::level_cost(0);
        app.world_mut().resource_mut::<Research>().credit(level);
        app.update();
        assert_eq!(
            standing(&mut app),
            1,
            "the level landed and no bins went up"
        );
        app.update();
        assert_eq!(
            standing(&mut app),
            1,
            "a second set went up on the next frame"
        );

        // And a restart takes them down again, the way it takes the cast down.
        app.world_mut().resource_mut::<Research>().restart();
        app.update();
        assert_eq!(standing(&mut app), 0, "the bins outlived the research");
    }
}
