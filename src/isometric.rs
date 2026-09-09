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

/// The most [`stand_z`] will shift anything.
///
/// Small enough that it can never reorder two things genuinely at different
/// depths: a tenth of a metre of separation is already 0.05 in depth units,
/// fifty times this.
const MAX_NUDGE: f32 = 0.001;

pub(crate) const BOARD_SKY: Color = Color::srgb(0.83, 0.93, 0.84);

const JUNGLE_CANOPY: Color = Color::srgb(0.16, 0.34, 0.19);
const JUNGLE_WALL_TOP: Color = Color::srgb(0.24, 0.55, 0.24);
const JUNGLE_WALL_LEFT: Color = Color::srgb(0.10, 0.28, 0.13);
const JUNGLE_WALL_RIGHT: Color = Color::srgb(0.15, 0.38, 0.17);
const PATH: Color = Color::srgb(0.84, 0.73, 0.51);
const TOWN: Color = Color::srgb(0.61, 0.76, 0.43);
const CLEARING: Color = Color::srgb(0.72, 0.82, 0.52);

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
/// monkeys idling on the same spot. It is clamped to [`MAX_NUDGE`] so it can
/// never invert a real depth difference — an unbounded per-entity epsilon is
/// exactly how a crowd starts flickering once it is large enough for the
/// epsilons to add up to more than the gaps between its members.
pub(crate) fn stand_z(world: Vec2, nudge: f32) -> f32 {
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
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
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
            builder.quad(diamond(tile), terrain_colour(map.terrain(tile)));
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
fn wall_mesh(tile: Tile) -> Mesh {
    let mut builder = MeshBuilder::default();
    let origin = Vec2::new(tile.x as f32 * METRE, tile.y as f32 * METRE);
    prism(
        &mut builder,
        origin,
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
                root.spawn((
                    Mesh2d(meshes.add(wall_mesh(tile))),
                    MeshMaterial2d(painted.clone()),
                    Transform::from_xyz(0.0, 0.0, stand_z(at, 0.0)),
                ));
            }

            // Anything with height is its own entity, anchored at the ground it
            // stands on. That is the whole discipline: the hut covers a monkey
            // behind it and not one in front, without any per-frame sorting.
            let standing = std::iter::once((map.town_centre(), hut)).chain(
                map.groves()
                    .iter()
                    .map(|grove| grove.tile)
                    .chain(map.home_trees().iter().copied())
                    .map(|tile| (tile, palm.clone())),
            );
            for (tile, mesh) in standing {
                let at = tile_centre(tile);
                let anchor = project(at);
                root.spawn((
                    Mesh2d(mesh),
                    MeshMaterial2d(painted.clone()),
                    // A multi-tile footprint can only carry one depth, so it
                    // takes its centre's: half a footprint of error either way,
                    // rather than a whole one at a corner. Keeping footprints
                    // small is what keeps that invisible.
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
    fn feet_decide_the_order_and_a_nudge_can_never_overturn_them() {
        // A monkey a tenth of a metre nearer the viewer draws in front, and no
        // amount of tie-breaking is allowed to say otherwise: this is the rule
        // that stops a crowd swapping who is on top.
        let behind = Vec2::new(10.0, 10.0);
        let front = Vec2::new(10.0, 10.1);
        assert!(stand_z(front, -1.0) > stand_z(behind, 1.0));
        // Two things on the same spot are separated, and stably.
        assert!(stand_z(behind, 0.0005) > stand_z(behind, 0.0));
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
