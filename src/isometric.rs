//! The isometric projection, and the one rule that decides what covers what.
//!
//! This module owns only drawing. The economy continues to use distances and
//! segment boundaries from `domain`, and the ground it is all laid out on is
//! `map`; the projected board is a view of both, never an input to either.
//!
//! Two things here are worth keeping straight, because everything drawn on the
//! board depends on them:
//!
//! - [`project`] takes a position **in metres on the ground plane** and returns
//!   a point on the isometric plane. Every actor, building and tree is placed
//!   by that one function, so there is no second opinion about where a metre is.
//! - [`stand_z`] decides the painter's order, from the tile a thing's *feet*
//!   are on. See its own comment: that choice is the whole layering foundation.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::map::{Map, TILE_METRES, Terrain, Tile};

/// Metres per tile, in the `f32` the presentation works in.
const METRE: f32 = TILE_METRES as f32;

/// Half a tile on screen, in pixels, at unit zoom.
///
/// Two to one, the classic pixel-art isometric ratio: it is what makes the
/// ground read as a plane receding rather than a grid seen edge-on, and it
/// keeps every diagonal on a whole-pixel slope.
pub(crate) const TILE_HALF: Vec2 = Vec2::new(16.0, 8.0);

/// Screen pixels per metre of *height*.
///
/// Height is the one axis the projection does not fold, so it needs its own
/// scale. Matching [`TILE_HALF`]`.x` per tile makes a one-tile cube look like a
/// cube rather than a slab.
const HEIGHT_PER_METRE: f32 = TILE_HALF.x / METRE;

/// The baked ground plane, under everything that stands on it.
pub(crate) const GROUND_Z: f32 = -1.0;

/// The floor for anything drawn *over* the board rather than standing on it.
///
/// The scene now occupies z 0 to about 137 — a tile's depth, not a hand-picked
/// layer — which makes the old habit of "a small z means on top" exactly
/// backwards. A baked mesh is opaque and *writes* depth, while a sprite tests
/// against it without writing, so a dragged banana left at z = 4 is not merely
/// mis-sorted: it is behind the hut, the palm and every wall tile, and vanishes
/// at the one moment the player is holding it.
///
/// A dragged banana, a delivery floater and a role badge all belong to the
/// player's hand rather than to the ground, so they go above the whole world.
/// `the_world_never_reaches_the_overlay` is what keeps that true as the map
/// grows.
pub(crate) const OVERLAY_Z: f32 = 500.0;

/// The most [`stand_z`] will shift anything.
///
/// Bounded below by the *depth buffer*, not by `f32`. The camera spans z
/// -1000..1000 into a 32-bit depth target, so one buffer step near the village
/// is about 1.2e-4 in world z; a nudge finer than that is invisible to any
/// comparison against an opaque mesh, however well `f32` resolves it. Bounded
/// above by the smallest separation that must survive: a tenth of a metre
/// between two monkeys is 0.05 in depth units, twelve times this.
const MAX_NUDGE: f32 = 0.004;

/// The step between adjacent nudges. Above one depth-buffer step, so that two
/// things nudged apart really are apart.
pub(crate) const NUDGE_STEP: f32 = 0.0005;

pub(crate) const BOARD_SKY: Color = Color::srgb(0.83, 0.93, 0.84);

// The wall top is deliberately close to the canopy it rises out of. Opening a
// gap between them draws a bright green kerb around the entire jungle boundary,
// which reads as painted trim on a hedge maze rather than as sunlit canopy. And
// the village is the brightest ground on the board: the clearing used to be,
// which pulled the eye into an empty corner and away from the only place
// anything happens.
const JUNGLE_CANOPY: Color = Color::srgb(0.16, 0.34, 0.19);
const JUNGLE_WALL_TOP: Color = Color::srgb(0.20, 0.43, 0.22);
const JUNGLE_WALL_LEFT: Color = Color::srgb(0.10, 0.28, 0.13);
const JUNGLE_WALL_RIGHT: Color = Color::srgb(0.15, 0.38, 0.17);
const PATH: Color = Color::srgb(0.84, 0.73, 0.51);
const TOWN: Color = Color::srgb(0.64, 0.79, 0.46);
const CLEARING: Color = Color::srgb(0.60, 0.71, 0.43);

/// The delivery point, trodden into bare earth.
///
/// The town centre used to draw *nothing*. Every delivery in the game lands on
/// it, the hand-harvest drag ends on it, and a new player opening the game was
/// shown a flat green lawn with the word VILLAGE floating over it and asked to
/// drag a banana onto the label. The stall stands eight metres aside so it does
/// not swallow the arriving queue (D25), and eight metres is off the side of a
/// phone at the camera's opening zoom - so the one thing that could have named
/// the spot was the one thing not on screen.
///
/// Painted into the ground mesh rather than built as a prop, which is what
/// makes it free: it is the tile colour of nine tiles, so it costs no draw
/// call, no sorting, and above all no *span* - it sits exactly at the point the
/// board is aimed at, so it cannot push anything else off the screen.
const DEPOT_PAD: Color = Color::srgb(0.77, 0.66, 0.50);
/// Grass scuffed by traffic, so the pad has an edge rather than a border.
const DEPOT_EDGE: Color = Color::srgb(0.71, 0.73, 0.48);

/// How far the trodden ground reaches from the delivery point, in tiles.
const DEPOT_RADIUS: i32 = 1;
/// And how far the scuffing around it reaches.
const DEPOT_EDGE_RADIUS: i32 = 2;

/// How tall the jungle stands, in metres. Enough to read as a wall a monkey
/// could not step over, which is what the map says it is.
const WALL_HEIGHT: f32 = 3.0;

const HUT_WALL: Color = Color::srgb(0.86, 0.78, 0.62);
const HUT_LEFT: Color = Color::srgb(0.52, 0.36, 0.24);
const HUT_RIGHT: Color = Color::srgb(0.66, 0.47, 0.31);
const HUT_ROOF: Color = Color::srgb(0.78, 0.36, 0.28);
const TRUNK: Color = Color::srgb(0.45, 0.31, 0.20);
const FROND: Color = Color::srgb(0.36, 0.66, 0.29);
const FROND_SHADE: Color = Color::srgb(0.26, 0.52, 0.23);

#[derive(Component)]
pub(crate) struct WorldRoot;

/// Project a ground position, in metres, onto the isometric plane.
///
/// Bevy's y points up the screen, so the `x + y` term is negated: walking
/// "south-east" on the ground moves *down* the screen, towards the viewer.
pub(crate) fn project(world: Vec2) -> Vec2 {
    let tile = world / METRE;
    Vec2::new(
        (tile.x - tile.y) * TILE_HALF.x,
        -(tile.x + tile.y) * TILE_HALF.y,
    )
}

/// Lift a projected point by a height in metres.
pub(crate) fn raise(point: Vec2, metres: f32) -> Vec2 {
    Vec2::new(point.x, point.y + metres * HEIGHT_PER_METRE)
}

/// The ground position, in metres, that [`project`] would put at `point`.
///
/// The projection folds two ground axes onto the screen's, and it is invertible
/// precisely because the ground is a *plane*: there is exactly one metre
/// position under any point on it. That is what lets a drag on the board be
/// read as a grab on the ground rather than as a scroll of a picture — the
/// camera moves so the metre the finger landed on stays under the finger, at
/// any zoom.
pub(crate) fn unproject(point: Vec2) -> Vec2 {
    // `project` is `x - y` across and `-(x + y)` down; recovering the pair from
    // the sum and the difference is the whole inverse.
    let difference = point.x / TILE_HALF.x;
    let sum = -point.y / TILE_HALF.y;
    Vec2::new(sum + difference, sum - difference) * (METRE * 0.5)
}

/// How near the viewer a ground position is. Larger is nearer.
pub(crate) fn depth(world: Vec2) -> f32 {
    (world.x + world.y) / METRE
}

/// Painter's order for something standing on the ground at `world`.
///
/// The whole layering foundation is this one function, and the reason it takes
/// a *ground* position rather than a sprite's centre is the entire point: a
/// thing is sorted by the tile its feet are on, never by where its artwork
/// happens to reach. That is what makes a building cover the monkey behind it
/// while the monkey in front of it walks past unobscured — and because depth is
/// continuous rather than per-tile, a monkey crossing a building's front edge
/// changes order smoothly instead of popping.
///
/// `nudge` separates things that would otherwise sort identically, such as two
/// monkeys idling on the same spot. It is bounded by [`MAX_NUDGE`] so it can
/// never invert a real depth difference — an unbounded per-entity epsilon is
/// exactly how a crowd starts flickering once it is large enough for the
/// epsilons to add up to more than the gaps between its members.
///
/// The clamp is a backstop, not the mechanism: two callers that both exceed the
/// bound do not get an order, they get the *same* z. So an out-of-range nudge is
/// a caller's bug and says so in a debug build, rather than being saturated away
/// quietly for the next one to rediscover.
pub(crate) fn stand_z(world: Vec2, nudge: f32) -> f32 {
    debug_assert!(
        nudge.abs() <= MAX_NUDGE,
        "a nudge of {nudge} is past MAX_NUDGE and would sort arbitrarily"
    );
    depth(world) + nudge.clamp(-MAX_NUDGE, MAX_NUDGE)
}

/// The centre of a tile, in metres. The presentation's `f32` counterpart to
/// [`Tile::centre`].
pub(crate) fn tile_centre(tile: Tile) -> Vec2 {
    Vec2::new((tile.x as f32 + 0.5) * METRE, (tile.y as f32 + 0.5) * METRE)
}

/// The four corners of a tile's diamond, projected: top, right, bottom, left.
fn diamond(tile: Tile) -> [Vec2; 4] {
    let (x, y) = (tile.x as f32 * METRE, tile.y as f32 * METRE);
    [
        project(Vec2::new(x, y)),
        project(Vec2::new(x + METRE, y)),
        project(Vec2::new(x + METRE, y + METRE)),
        project(Vec2::new(x, y + METRE)),
    ]
}

fn terrain_colour(terrain: Terrain) -> Color {
    match terrain {
        Terrain::Jungle => JUNGLE_CANOPY,
        Terrain::Path => PATH,
        Terrain::Town => TOWN,
        Terrain::Grove => CLEARING,
    }
}

/// The colour of one tile of ground, with the depot trodden into it.
///
/// Only walkable ground is trodden: a depot painted over the jungle wall would
/// put bare earth up the side of a three-metre hedge.
fn ground_colour(map: &Map, tile: Tile) -> Color {
    let terrain = map.terrain(tile);
    if !terrain.passable() {
        return terrain_colour(terrain);
    }
    let centre = map.town_centre();
    let reach = (tile.x - centre.x).abs().max((tile.y - centre.y).abs());
    if reach <= DEPOT_RADIUS {
        DEPOT_PAD
    } else if reach <= DEPOT_EDGE_RADIUS {
        DEPOT_EDGE
    } else {
        terrain_colour(terrain)
    }
}

/// A mesh under construction, in projected space with per-vertex colour.
///
/// Vertex colours are what let the entire ground plane be one draw call:
/// `ColorMaterial` multiplies by them, so four and a half thousand tiles of
/// four different terrains need one mesh and one material rather than one
/// entity each.
#[derive(Default)]
struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    colours: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn quad(&mut self, corners: [Vec2; 4], colour: Color) {
        let base = self.positions.len() as u32;
        let rgba = colour.to_linear().to_f32_array();
        for corner in corners {
            self.positions.push([corner.x, corner.y, 0.0]);
            self.colours.push(rgba);
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn build(self) -> Mesh {
        // Both worlds, not `RENDER_WORLD` alone: that path *moves* the vertex
        // data out of the main-world asset, so the mesh can never be extracted
        // again. A backgrounded mobile tab that loses its WebGL context would
        // lose the terrain permanently, and `./serve` is the touch playtest.
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colours);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

/// The whole ground plane, as one mesh.
///
/// Flat, so nothing on it can occlude anything else and it needs no sorting of
/// its own; it simply sits under everything at [`GROUND_Z`]. An entity per tile
/// would be four and a half thousand sprites to cull and sort every frame for a
/// surface that never changes and cannot cover a monkey.
fn ground_mesh(map: &Map) -> Mesh {
    let mut builder = MeshBuilder::default();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let tile = Tile::new(x, y);
            builder.quad(diamond(tile), ground_colour(map, tile));
        }
    }
    builder.build()
}

/// The jungle tiles that show the player a wall.
///
/// Only the ones touching ground a monkey could stand on. Seen from above, the
/// jungle *is* its canopy — flat dark green is the honest look for the depths
/// of it — and what needs height is the edge, where the barrier has to read as
/// something that cannot be walked through.
fn wall_tiles(map: &Map) -> Vec<Tile> {
    let mut tiles = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let tile = Tile::new(x, y);
            if map.terrain(tile).passable() {
                continue;
            }
            let touches_open = (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    (dx != 0 || dy != 0) && map.terrain(Tile::new(x + dx, y + dy)).passable()
                })
            });
            if touches_open {
                tiles.push(tile);
            }
        }
    }
    tiles
}

/// The three shades of one raised box: its top, and the two sides that face the
/// viewer.
#[derive(Clone, Copy)]
struct Palette {
    top: Color,
    left: Color,
    right: Color,
}

const JUNGLE_PALETTE: Palette = Palette {
    top: JUNGLE_WALL_TOP,
    left: JUNGLE_WALL_LEFT,
    right: JUNGLE_WALL_RIGHT,
};

const HUT_PALETTE: Palette = Palette {
    top: HUT_ROOF,
    left: HUT_LEFT,
    right: HUT_RIGHT,
};

const FROND_PALETTE: Palette = Palette {
    top: FROND,
    left: FROND_SHADE,
    right: FROND_SHADE,
};

/// The diamond of a rectangle of ground, projected: top, right, bottom, left.
fn footprint(origin: Vec2, size: Vec2) -> [Vec2; 4] {
    [
        project(origin),
        project(origin + Vec2::new(size.x, 0.0)),
        project(origin + size),
        project(origin + Vec2::new(0.0, size.y)),
    ]
}

/// A box over `size` metres of ground, spanning `base` to `base + height`
/// metres of air.
///
/// `base` is what lets a palm's crown sit on top of its trunk rather than in
/// the grass around it.
fn prism(
    builder: &mut MeshBuilder,
    origin: Vec2,
    size: Vec2,
    base: f32,
    height: f32,
    palette: Palette,
) {
    let [top, right, bottom, left] = footprint(origin, size);
    let floor = Vec2::new(0.0, base * HEIGHT_PER_METRE);
    let ceiling = Vec2::new(0.0, (base + height) * HEIGHT_PER_METRE);
    builder.quad(
        [
            top + ceiling,
            right + ceiling,
            bottom + ceiling,
            left + ceiling,
        ],
        palette.top,
    );
    builder.quad(
        [
            left + ceiling,
            bottom + ceiling,
            bottom + floor,
            left + floor,
        ],
        palette.left,
    );
    builder.quad(
        [
            bottom + ceiling,
            right + ceiling,
            right + floor,
            bottom + floor,
        ],
        palette.right,
    );
}

/// One jungle tile, raised into a wall.
///
/// Built around its own origin and placed by a `Transform`, like the hut and
/// the palms. Baking the tile's position into the vertices instead would give
/// every one of the two hundred wall tiles a distinct mesh asset, and Bevy can
/// only batch consecutive items that share one — two hundred draw calls for a
/// shape that is the same shape two hundred times.
fn wall_mesh() -> Mesh {
    let mut builder = MeshBuilder::default();
    prism(
        &mut builder,
        Vec2::splat(-METRE * 0.5),
        Vec2::splat(METRE),
        0.0,
        WALL_HEIGHT,
        JUNGLE_PALETTE,
    );
    builder.build()
}

/// The hut at the town centre: where every delivery lands, and the first thing
/// on the board tall enough to hide a monkey behind it.
fn hut_mesh() -> Mesh {
    let mut builder = MeshBuilder::default();
    let size = Vec2::splat(METRE * 2.0);
    let origin = Vec2::splat(-METRE);
    prism(&mut builder, origin, size, 0.0, 3.0, HUT_PALETTE);
    // A pale band under the roof, so the hut is not one flat mass.
    let [_, right, bottom, left] = footprint(origin, size);
    let band = Vec2::new(0.0, 1.4 * HEIGHT_PER_METRE);
    builder.quad([left + band, bottom + band, bottom, left], HUT_WALL);
    builder.quad([bottom + band, right + band, right, bottom], HUT_WALL);
    builder.build()
}

/// A banana palm: a trunk, and a crown wider than the tile it stands on.
fn palm_mesh() -> Mesh {
    let mut builder = MeshBuilder::default();
    prism(
        &mut builder,
        Vec2::splat(-0.35),
        Vec2::splat(0.7),
        0.0,
        3.4,
        Palette {
            top: TRUNK,
            left: TRUNK,
            right: TRUNK,
        },
    );
    // The crown sits on top of the trunk, wider than the tile, so a palm reads
    // as something you stand under rather than a bush.
    prism(
        &mut builder,
        Vec2::splat(-METRE * 0.85),
        Vec2::splat(METRE * 1.7),
        3.1,
        0.7,
        FROND_PALETTE,
    );
    builder.build()
}

/// Where the stall stands, in metres: beside the delivery point, never on it.
///
/// The town centre tile *is* where a worker unloads, and the queue spreads a few
/// metres around it. A four-metre hut centred there swallows half the arriving
/// crowd at the one moment in the cycle the player is watching — the counter
/// ticks, the floater fires, and the monkey that earned it is inside a building.
/// So the stall steps aside: square to the walk, so nobody has to route through
/// it, and to whichever side is *further* from the viewer, so the queue forms in
/// front of it rather than behind.
fn stall_stand(map: &Map) -> Vec2 {
    const ASIDE: f32 = 8.0;
    let centre = tile_centre(map.town_centre());
    let outbound = (tile_centre(map.worked_grove().tile) - centre).normalize_or_zero();
    let across = Vec2::new(outbound.y, -outbound.x);
    let aside = if depth(across) <= 0.0 {
        across
    } else {
        -across
    };
    centre + aside * ASIDE
}

pub(crate) fn spawn_world(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    map: &Map,
) {
    // One material for every baked surface: the colour lives in the vertices.
    let painted = materials.add(ColorMaterial::from(Color::WHITE));
    let hut = meshes.add(hut_mesh());
    let palm = meshes.add(palm_mesh());
    let wall = meshes.add(wall_mesh());

    commands
        .spawn((WorldRoot, Transform::default(), Visibility::default()))
        .with_children(|root| {
            root.spawn((
                Mesh2d(meshes.add(ground_mesh(map))),
                MeshMaterial2d(painted.clone()),
                Transform::from_xyz(0.0, 0.0, GROUND_Z),
            ));

            for tile in wall_tiles(map) {
                let at = tile_centre(tile);
                let anchor = project(at);
                root.spawn((
                    Mesh2d(wall.clone()),
                    MeshMaterial2d(painted.clone()),
                    Transform::from_xyz(anchor.x, anchor.y, stand_z(at, 0.0)),
                ));
            }

            // Anything with height is its own entity, anchored at the ground it
            // stands on. That is the whole discipline: the hut covers a monkey
            // behind it and not one in front, without any per-frame sorting.
            //
            // A multi-tile footprint can only carry one depth, so each takes its
            // centre's: half a footprint of error either way, rather than a
            // whole one at a corner. Keeping footprints small is what keeps that
            // invisible.
            let standing = std::iter::once((stall_stand(map), hut)).chain(
                map.groves()
                    .iter()
                    .map(|grove| grove.tile)
                    .chain(map.home_trees().iter().copied())
                    .map(|tile| (tile_centre(tile), palm.clone())),
            );
            for (at, mesh) in standing {
                let anchor = project(at);
                root.spawn((
                    Mesh2d(mesh),
                    MeshMaterial2d(painted.clone()),
                    Transform::from_xyz(anchor.x, anchor.y, stand_z(at, 0.0)),
                ));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_projection_folds_the_ground_plane_the_way_an_isometric_view_does() {
        // Walking east and walking south move opposite ways along the screen's
        // x axis and the same way down it, which is what makes the plane read
        // as receding rather than as a grid seen edge-on.
        let origin = project(Vec2::ZERO);
        let east = project(Vec2::new(METRE, 0.0));
        let south = project(Vec2::new(0.0, METRE));
        assert_eq!(east - origin, Vec2::new(TILE_HALF.x, -TILE_HALF.y));
        assert_eq!(south - origin, Vec2::new(-TILE_HALF.x, -TILE_HALF.y));
        // A tile diagonal is one tile wide and flat on screen.
        assert_eq!(
            project(Vec2::splat(METRE)) - origin,
            Vec2::new(0.0, -TILE_HALF.y * 2.0)
        );
    }

    #[test]
    fn a_point_on_the_board_names_exactly_one_metre_on_the_ground() {
        // The property a drag depends on: grab the board anywhere and the metre
        // under the finger is recoverable, so panning can hold it there.
        for world in [
            Vec2::ZERO,
            Vec2::new(89.0, 61.0),
            Vec2::new(41.0, 25.0),
            Vec2::new(-7.5, 133.25),
        ] {
            let round_trip = unproject(project(world));
            assert!(
                round_trip.distance(world) < 1e-3,
                "{world:?} came back as {round_trip:?}"
            );
        }
        // And in the other direction, which is the one a pinch uses: a screen
        // offset names a ground offset.
        let screen = Vec2::new(96.0, -40.0);
        assert!(project(unproject(screen)).distance(screen) < 1e-3);
    }

    #[test]
    fn feet_decide_the_order_and_a_nudge_can_never_overturn_them() {
        // A monkey a tenth of a metre nearer the viewer draws in front, and no
        // amount of tie-breaking is allowed to say otherwise: this is the rule
        // that stops a crowd swapping who is on top.
        let behind = Vec2::new(10.0, 10.0);
        let front = Vec2::new(10.0, 10.1);
        assert!(stand_z(front, -MAX_NUDGE) > stand_z(behind, MAX_NUDGE));
        // Two things on the same spot are separated, and by more than one step
        // of the depth buffer, which is what makes the separation real rather
        // than merely present in the float.
        assert!(stand_z(behind, NUDGE_STEP) - stand_z(behind, 0.0) >= 1.2e-4);
    }

    #[test]
    fn the_world_never_reaches_the_overlay() {
        // A dragged banana, a floater and a badge are drawn over the board at
        // `OVERLAY_Z`. The board's own z is a tile's *depth*, so it grows with
        // the map - and the day it grows past the overlay, the banana the
        // player is holding disappears behind a tree.
        let map = crate::map::start();
        let deepest = (0..map.height())
            .flat_map(|y| (0..map.width()).map(move |x| Tile::new(x, y)))
            .map(|tile| stand_z(tile_centre(tile), MAX_NUDGE))
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            deepest < OVERLAY_Z,
            "the board reaches z {deepest}, at or past the overlay at {OVERLAY_Z}"
        );
        const { assert!(GROUND_Z < 0.0) };
    }

    #[test]
    fn the_stall_stands_beside_the_delivery_point_and_behind_the_queue() {
        // A hut centred on the town centre swallows half the unloading queue at
        // the one moment in the cycle the player is watching it.
        let map = crate::map::start();
        let centre = tile_centre(map.town_centre());
        let stall = stall_stand(map);
        // Clear of the queue, which spreads a few metres around the centre.
        assert!(stall.distance(centre) > 6.0, "the stall is on the queue");
        // And further from the viewer, so the queue forms in front of it.
        assert!(stand_z(stall, 0.0) < stand_z(centre, 0.0));
        // Square to the walk, so nobody has to route through the building.
        let outbound = (tile_centre(map.worked_grove().tile) - centre).normalize();
        assert!((stall - centre).normalize().dot(outbound).abs() < 1e-3);
    }

    #[test]
    fn the_delivery_point_is_visible_ground_rather_than_bare_lawn() {
        // A new player is asked to drag a banana to the town centre. Before the
        // depot was trodden in, the town centre drew nothing at all - the same
        // green as the forty tiles around it - so the drag's target was a word.
        let map = crate::map::start();
        let centre = map.town_centre();
        assert_eq!(ground_colour(map, centre), DEPOT_PAD);
        assert_ne!(ground_colour(map, centre), terrain_colour(Terrain::Town));
        // With an edge, so it reads as worn rather than as a painted rectangle.
        let edge = Tile::new(centre.x + DEPOT_EDGE_RADIUS, centre.y);
        assert_eq!(ground_colour(map, edge), DEPOT_EDGE);
        // And it stops: the town is still the town a few tiles out.
        let away = Tile::new(centre.x + DEPOT_EDGE_RADIUS + 1, centre.y);
        assert_eq!(ground_colour(map, away), terrain_colour(map.terrain(away)));
        // It never climbs the jungle wall, which is not ground anyone treads.
        for y in 0..map.height() {
            for x in 0..map.width() {
                let tile = Tile::new(x, y);
                if !map.terrain(tile).passable() {
                    assert_eq!(ground_colour(map, tile), terrain_colour(map.terrain(tile)));
                }
            }
        }
    }

    #[test]
    fn the_ground_plane_is_one_quad_per_tile() {
        let map = Map::parse("@.*\n...").expect("parses");
        let mesh = ground_mesh(&map);
        assert_eq!(mesh.count_vertices(), 6 * 4);
    }

    #[test]
    fn only_the_jungle_a_monkey_can_see_over_is_given_height() {
        // The depths of the jungle are canopy seen from above and stay flat;
        // the edge is what has to look like a barrier.
        let map = Map::parse(concat!(
            "#######\n",
            "#######\n",
            "##@...#\n",
            "##...*#\n",
            "#######\n",
            "#######",
        ))
        .expect("parses");
        let walls = wall_tiles(&map);
        assert!(walls.contains(&Tile::new(1, 1)), "the edge is a wall");
        assert!(
            !walls.contains(&Tile::new(0, 0)),
            "jungle with only jungle around it is canopy, not wall"
        );
    }
}
