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

use crate::{
    art::{self, Art},
    map::{Map, TILE_METRES, Terrain, Tile},
};

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

/// Where marks laid *on* the ground draw: over the terrain mesh, under anything
/// standing on it.
///
/// A contact shadow belongs to its monkey but must never cover another one, and
/// sorting it with its owner cannot promise that - a shadow a hair behind its
/// monkey is still in front of the monkey a metre further back, and draws over
/// its feet. So every ground mark shares one layer between the mesh at
/// [`GROUND_Z`] and the shallowest depth anything stands at, which is zero.
pub(crate) const MARK_Z: f32 = -0.5;
/// And the deposit glow, under the shadows of the crowd standing in it.
pub(crate) const GLOW_Z: f32 = -0.75;
/// And the treehouse's own ground paint, under the glow: see D30.
const TREEHOUSE_GROUND_Z: f32 = -0.9;

/// Every contact shadow: the green of the art's own baked shadows, a third
/// opaque, so a crowd's shadows pool into a darker patch rather than stacking
/// into black.
pub(crate) const SHADOW_COLOUR: Color = Color::srgba(0.20, 0.30, 0.20, 0.33);

/// A square of ground, `half` metres either side of `centre` along both ground
/// axes: what a tile is, grown. On screen it is a diamond on the 2:1 grid.
///
/// What the player aims a drag at. A place on the ground rather than a box on
/// the screen, so it is exactly as big as the thing it marks at every zoom, and
/// it is hit-tested by taking the pointer *down* to the ground (`unproject`)
/// rather than by bringing the target up to the screen - which is what makes
/// the test exact on a plane the projection folds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Footprint {
    pub(crate) centre: Vec2,
    pub(crate) half: f32,
}

impl Footprint {
    pub(crate) const fn new(centre: Vec2, half: f32) -> Self {
        Self { centre, half }
    }

    /// Whether a ground position, in metres, is on it. Edges included.
    pub(crate) fn contains(self, ground: Vec2) -> bool {
        let off = (ground - self.centre).abs();
        off.x <= self.half && off.y <= self.half
    }

    /// Its four corners, projected at unit zoom: top, right, bottom, left.
    pub(crate) fn diamond(self) -> [Vec2; 4] {
        let h = self.half;
        [
            Vec2::new(-h, -h),
            Vec2::new(h, -h),
            Vec2::new(h, h),
            Vec2::new(-h, h),
        ]
        .map(|corner| project(self.centre + corner))
    }
}

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

// The jungle's depths are canopy seen from above and stay flat; its edge is
// drawn with the jungle plants, which is what has to read as a barrier. The
// open ground is the artist's tiles (see `ground_tile_mesh`).
const JUNGLE_CANOPY: Color = Color::srgb(0.16, 0.34, 0.19);

/// How far the depot reaches from the delivery point, in tiles. The standing
/// ring is sized against this, so the depot contains the crowd that gathers on
/// it, and the trodden plaza drawn round the bins covers all of it.
pub(crate) const DEPOT_EDGE_REACH: i32 = 2;
/// The drop target, in metres either side of the delivery point: the depot's
/// reach, which the plaza and the glow mark, so what the player sees as the
/// depot is what accepts the banana.
pub(crate) const DEPOT_REACH_METRES: f32 = (DEPOT_EDGE_REACH as f32 + 0.5) * METRE;

/// The drawn ground tiles: over the flat underlay, under the treehouse's shade
/// and everything else laid on the ground.
const GROUND_TILE_Z: f32 = -0.95;

/// Board tiles per ground tile, each way. The ground art is drawn at the shared
/// scale like everything standing on it, and at that scale one of its 128 x 64
/// diamonds is exactly two board tiles across.
const GROUND_SPAN: i32 = 2;

/// How far the plaza reaches, in metres along either ground axis, from the
/// ground vertex nearest the delivery point.
///
/// Measured from a vertex rather than from the delivery point, so the plaza is
/// square on the vertex grid; measured from the point, which sits a metre off
/// the grid, it fell flush with the drop target on the side nearest the viewer.
/// Two ground tiles each way puts every tile the target touches wholly on dirt.
const PLAZA_METRES: f32 = 8.0;

/// How near the worked route a ground vertex is trodden to dirt, in metres.
///
/// A little over half the diagonal of one ground tile (2.83 m), so wherever the
/// walk crosses a tile at least one of its corners is dirt, and the trail never
/// breaks into islands. Any narrower and a diagonal route leaves gaps.
const TRAIL_METRES: f32 = 3.0;

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

/// The ground under a board tile, in flat colour: canopy where nothing is
/// drawn over it, and the drawn material's own base colour where a ground tile
/// is.
///
/// The underlay only shows through where the tiles do not, which by design is
/// nowhere - but a float's worth of rounding at a seam, at a zoom the player
/// pinched to, can leave a pixel uncovered, and that pixel should be the
/// ground it is a hole in rather than a dark green speck in the village.
fn underlay_colour(map: &Map, trodden: &Trodden, tile: Tile) -> Color {
    if !drawn(map, tile) {
        return JUNGLE_CANOPY;
    }
    // A board tile touches exactly one ground vertex: the corner of its ground
    // tile it shares.
    let vertex = IVec2::new(
        (tile.x + 1).div_euclid(GROUND_SPAN),
        (tile.y + 1).div_euclid(GROUND_SPAN),
    );
    if trodden.dirt(vertex) {
        art::GROUND_DIRT
    } else {
        art::GROUND_FLOOR
    }
}

/// A mesh under construction, in projected space, with per-vertex colour or
/// texture coordinates.
///
/// Either is what lets a whole ground plane be one draw call: `ColorMaterial`
/// multiplies by the vertex colour and samples its texture at the UVs, so four
/// and a half thousand tiles need one mesh and one material rather than one
/// entity each.
#[derive(Default)]
struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    colours: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn quad(&mut self, corners: [Vec2; 4], colour: Color) {
        let rgba = colour.to_linear().to_f32_array();
        self.colours.extend([rgba; 4]);
        self.corners(corners);
    }

    /// A quad showing the texture between `uv.0` (its top-left) and `uv.1`,
    /// for corners given top-left, top-right, bottom-right, bottom-left.
    fn textured(&mut self, corners: [Vec2; 4], (min, max): (Vec2, Vec2)) {
        self.uvs.extend([
            [min.x, min.y],
            [max.x, min.y],
            [max.x, max.y],
            [min.x, max.y],
        ]);
        self.corners(corners);
    }

    fn corners(&mut self, corners: [Vec2; 4]) {
        let base = self.positions.len() as u32;
        for corner in corners {
            self.positions.push([corner.x, corner.y, 0.0]);
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn build(self) -> Mesh {
        // Every vertex gets a colour or none does, and the same for texture
        // coordinates: a builder that mixed `quad` and `textured` would make a
        // mesh with attributes shorter than its positions.
        debug_assert!(self.colours.is_empty() || self.colours.len() == self.positions.len());
        debug_assert!(self.uvs.is_empty() || self.uvs.len() == self.positions.len());
        // Both worlds, not `RENDER_WORLD` alone: that path *moves* the vertex
        // data out of the main-world asset, so the mesh can never be extracted
        // again. A backgrounded mobile tab that loses its WebGL context would
        // lose the terrain permanently, and `./serve` is the touch playtest.
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        if !self.colours.is_empty() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colours);
        }
        if !self.uvs.is_empty() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        }
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

/// The whole ground plane in flat colour, as one mesh: the canopy, and the
/// underlay beneath the drawn ground.
///
/// Flat, so nothing on it can occlude anything else and it needs no sorting of
/// its own; it simply sits under everything at [`GROUND_Z`]. An entity per tile
/// would be four and a half thousand sprites to cull and sort every frame for a
/// surface that never changes and cannot cover a monkey.
fn ground_mesh(map: &Map, trodden: &Trodden) -> Mesh {
    let mut builder = MeshBuilder::default();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let tile = Tile::new(x, y);
            builder.quad(diamond(tile), underlay_colour(map, trodden, tile));
        }
    }
    builder.build()
}

/// Whether a tile has ground a monkey could stand on beside it.
fn touches_open(map: &Map, tile: Tile) -> bool {
    (-1..=1).any(|dy| {
        (-1..=1).any(|dx| {
            (dx != 0 || dy != 0) && map.terrain(Tile::new(tile.x + dx, tile.y + dy)).passable()
        })
    })
}

/// Whether a board tile has drawn ground over it: open ground, and the jungle's
/// edge, where the plants stand on floor rather than floating over canopy.
fn drawn(map: &Map, tile: Tile) -> bool {
    map.terrain(tile).passable() || touches_open(map, tile)
}

/// A single textured mesh for the deep green jungle floor. Diamond UVs use
/// the whole authored tile; the existing ground remains underneath its edges.
fn jungle_floor_mesh(map: &Map) -> Mesh {
    let mut builder = MeshBuilder::default();
    let mut uv = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let tile = Tile::new(x, y);
            if map.terrain(tile) == Terrain::Jungle && !drawn(map, tile) {
                builder.quad(diamond(tile), Color::WHITE);
                // One 128-art-pixel tile spans two board tiles, preserving
                // ART_SCALE just like every standing asset.
                let u = 0.5 + (x % 2 - y % 2) as f32 * 0.25;
                let v = (x % 2 + y % 2) as f32 * 0.25;
                uv.extend([
                    [u, v],
                    [u + 0.25, v + 0.25],
                    [u, v + 0.5],
                    [u - 0.25, v + 0.25],
                ]);
            }
        }
    }
    let mut mesh = builder.build();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh
}

/// The jungle tiles that show the player a wall.
///
/// Only the ones touching ground a monkey could stand on. This border subset
/// keeps the barrier legible; `scenery_tiles` adds sparse interior clusters.
fn wall_tiles(map: &Map) -> Vec<Tile> {
    let mut tiles = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let tile = Tile::new(x, y);
            if !map.terrain(tile).passable() && touches_open(map, tile) {
                tiles.push(tile);
            }
        }
    }
    tiles
}

/// Where the ground is trodden to dirt: which ground vertices are dirt rather
/// than the jungle's sage floor.
///
/// Dirt is where monkeys walk - the ring path, the plaza round the bins and the
/// worked route between the depot and its grove - and everything else open is
/// floor. The artist drew clay for the town, but a forty-tile town of clay with
/// the same few marks on it reads as a brown sheet ruled into squares; worn
/// into trails it says where the work happens, and the dark monkeys read on
/// both materials.
///
/// Decided at *vertices*, never per tile. A ground tile is chosen by its four
/// corners, and one shared grid of corners is what guarantees neighbouring
/// tiles agree on the edge between them; tiles chosen one at a time do not
/// (see `assets/Ground/README.md`).
struct Trodden {
    columns: i32,
    rows: i32,
    dirt: Vec<bool>,
}

impl Trodden {
    fn of(map: &Map) -> Self {
        // One more vertex than there are ground tiles each way: the far
        // corners of the last row and column.
        let columns = (map.width() + GROUND_SPAN - 1) / GROUND_SPAN + 1;
        let rows = (map.height() + GROUND_SPAN - 1) / GROUND_SPAN + 1;
        let route: Vec<Vec2> = map
            .reference_route()
            .points()
            .iter()
            .map(|point| Vec2::new(point.x as f32, point.y as f32))
            .collect();
        let step = GROUND_SPAN as f32 * METRE;
        let depot = (tile_centre(map.town_centre()) / step).round() * step;
        let mut dirt = Vec::with_capacity((columns * rows) as usize);
        for j in 0..rows {
            for i in 0..columns {
                let at = IVec2::new(i, j).as_vec2() * (GROUND_SPAN as f32 * METRE);
                let (x, y) = (i * GROUND_SPAN, j * GROUND_SPAN);
                // The four board tiles meeting at the vertex.
                let around = [(x - 1, y - 1), (x, y - 1), (x - 1, y), (x, y)]
                    .map(|(x, y)| map.terrain(Tile::new(x, y)));
                // Half the tiles at a vertex. The ring path is two tiles wide,
                // so on two of its sides it straddles one row of vertices and
                // on the other two it touches two: a narrower path on the far
                // sides of the town than on the near ones, which is the parity
                // of a two-tile strip on a two-tile grid.
                let path = around.iter().filter(|t| **t == Terrain::Path).count() >= 2;
                let open = around.iter().filter(|t| t.passable()).count() >= 2;
                let plaza = (at - depot).abs().max_element() <= PLAZA_METRES;
                let trail = route
                    .windows(2)
                    .any(|leg| distance_to_leg(at, leg[0], leg[1]) <= TRAIL_METRES);
                dirt.push(path || (open && (plaza || trail)));
            }
        }
        Self {
            columns,
            rows,
            dirt,
        }
    }

    /// Whether a ground vertex is dirt. Off the grid is floor.
    fn dirt(&self, vertex: IVec2) -> bool {
        (0..self.columns).contains(&vertex.x)
            && (0..self.rows).contains(&vertex.y)
            && self.dirt[(vertex.y * self.columns + vertex.x) as usize]
    }
}

/// How far a ground position is from one straight leg of a route, in metres.
fn distance_to_leg(at: Vec2, from: Vec2, to: Vec2) -> f32 {
    let span = to - from;
    let along = if span.length_squared() > 0.0 {
        ((at - from).dot(span) / span.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    at.distance(from + span * along)
}

/// One drawn ground tile: where it sits in the ground grid, which of the
/// sixteen corner combinations it is, and which of its four detail variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GroundTile {
    at: IVec2,
    mask: u8,
    variant: u8,
}

/// The drawn ground tiles: every one with open ground or the jungle's edge
/// under it. Past that is canopy, which the flat mesh already shows.
fn ground_tiles(map: &Map, trodden: &Trodden) -> Vec<GroundTile> {
    let columns = (map.width() + GROUND_SPAN - 1) / GROUND_SPAN;
    let rows = (map.height() + GROUND_SPAN - 1) / GROUND_SPAN;
    let mut tiles = Vec::new();
    for j in 0..rows {
        for i in 0..columns {
            let shown = (0..GROUND_SPAN).any(|dy| {
                (0..GROUND_SPAN)
                    .any(|dx| drawn(map, Tile::new(i * GROUND_SPAN + dx, j * GROUND_SPAN + dy)))
            });
            if !shown {
                continue;
            }
            // Top, right, bottom, left: the manifest's bits 1, 2, 4 and 8.
            let corners = [
                IVec2::new(i, j),
                IVec2::new(i + 1, j),
                IVec2::new(i + 1, j + 1),
                IVec2::new(i, j + 1),
            ];
            let mask = corners
                .into_iter()
                .enumerate()
                .filter(|&(_, corner)| trodden.dirt(corner))
                .fold(0u8, |mask, (bit, _)| mask | 1 << bit);
            tiles.push(GroundTile {
                at: IVec2::new(i, j),
                mask,
                variant: ground_variant(IVec2::new(i, j)),
            });
        }
    }
    tiles
}

/// Which detail variant a ground tile shows: hashed from its place, for the
/// reason the jungle's planting is - the same ground every time the game
/// opens, with no sequence the eye can lock onto. All four share their edges,
/// so any choice joins its neighbours.
///
/// Two tiles in three are the quiet variant 0. Every marked variant puts its
/// marks at the same place in its diamond, so marked tiles always sit on the
/// ground grid, and an even split - three tiles in four marked - ruled the
/// open ground into a lattice of repeated scuffs.
fn ground_variant(at: IVec2) -> u8 {
    let mut bits =
        (at.x as u32).wrapping_mul(0x27D4_EB2F) ^ (at.y as u32).wrapping_mul(0x1656_67B1);
    bits ^= bits >> 15;
    bits = bits.wrapping_mul(0x846C_A68B);
    bits ^= bits >> 16;
    match bits % 9 {
        0..=5 => 0,
        marked => (marked - 5) as u8,
    }
}

/// Texture coordinates pulled a hair inside their atlas cell.
///
/// The packed atlas has no gutters, and a tile's diamond touches its cell's
/// edge at its four corners. Sampled exactly on the boundary, a fragment at a
/// pinched zoom can read the neighbouring cell's corner pixel - a different
/// mask - and, masked at the same depth, overwrite the tile beside it. A ten
/// thousandth of the atlas is a tenth of an art pixel: no visible shift.
fn inset((min, max): (Vec2, Vec2)) -> (Vec2, Vec2) {
    const INSET: f32 = 1e-4;
    (min + INSET, max - INSET)
}

/// The drawn ground, as one textured mesh.
///
/// One quad per tile, the whole 128 x 64 canvas of each, its transparent
/// corners discarded by an alpha mask: the artist's tiles join edge to edge on
/// their opaque pixels, which a diamond cut through the edge pixels would not
/// promise. Placed by the manifest's rule - a tile's top corner on its top
/// vertex - which is `project` at two board tiles a step, so the art's pixel
/// grid is the board's texel grid and every tile lands on whole texels.
fn ground_tile_mesh(map: &Map, trodden: &Trodden) -> Mesh {
    let size = art::GROUND_TILE * art::ART_SCALE;
    let mut builder = MeshBuilder::default();
    for tile in ground_tiles(map, trodden) {
        let top = project(tile.at.as_vec2() * GROUND_SPAN as f32 * METRE);
        let (left, right) = (top.x - size.x * 0.5, top.x + size.x * 0.5);
        let (upper, lower) = (top.y, top.y - size.y);
        builder.textured(
            [
                Vec2::new(left, upper),
                Vec2::new(right, upper),
                Vec2::new(right, lower),
                Vec2::new(left, lower),
            ],
            inset(art::ground_uv(tile.mask, tile.variant)),
        );
    }
    builder.build()
}

/// Where the treehouse's ground anchor stands, in metres: wherever puts its
/// banana bins on the delivery point (D30).
///
/// The building *is* the depot. Every delivery lands at the town centre, and
/// the artist drew three banana bins at the treehouse's bottom right, so the
/// house is placed by its bins rather than by its middle: the crowd unloads
/// into the bins it can see, and the house rises behind them, on the side the
/// standing crowd leaves open (D27). The walk out to the grove leaves from
/// under the deck, which is where every monkey appears from.
///
/// It used to stand eight metres aside from the delivery point, because a hut
/// centred on it swallowed the arriving queue (D25). That was a hut with no
/// counter; a building whose counter is at its front corner puts the queue in
/// front of it instead.
pub(crate) fn stall_stand(map: &Map) -> Vec2 {
    tile_centre(map.town_centre()) - unproject(art::TOWN_CENTRE.offset_of(art::TOWN_CENTRE_BINS))
}

/// The ground under the middle of the drawn treehouse, in metres: where the
/// opening view is aimed, so the village's landmark opens centred.
pub(crate) fn town_centre_view(map: &Map) -> Vec2 {
    stall_stand(map) + unproject(art::TOWN_CENTRE.offset_of(art::TOWN_CENTRE_MIDDLE))
}

/// Which jungle plant stands on a tile, if any.
///
/// A scatter rather than a hedge. Every jungle tile touching open ground used
/// to be raised into a three-metre block, which read as a wall because it was
/// one; the plants are fourteen metres of crown apiece, so putting one on every
/// tile would be a solid green rampart with no silhouette at all. Roughly two
/// tiles in five carry a plant, and which plant is the tile's own business, so
/// the tree line is broken and uneven and the same every time the game opens.
///
/// Hashed from the tile rather than drawn from an RNG, for the reason every
/// other scatter in this game is: the map is fixed, so its planting should be
/// too, and a save that reopens on a differently-shaped jungle is unsettling in
/// a way nobody can name.
fn planting(tile: Tile) -> Option<usize> {
    let mut bits =
        (tile.x as u32).wrapping_mul(0x9E37_79B9) ^ (tile.y as u32).wrapping_mul(0x85EB_CA6B);
    bits ^= bits >> 16;
    bits = bits.wrapping_mul(0x7FEB_352D);
    bits ^= bits >> 15;
    // Two in five planted, and an even pick between eight silhouettes.
    (bits % 5 < 2).then_some((bits >> 8) as usize % 8)
}

/// Keep the irregular forest edge and add staggered clusters in its depths.
/// Only impassable jungle is planted; routes and the town remain unchanged.
fn scenery_tiles(map: &Map) -> Vec<Tile> {
    let mut tiles = wall_tiles(map);
    for y in (0..map.height()).step_by(3) {
        for x in (y % 6 / 3..map.width()).step_by(3) {
            let tile = Tile::new(x, y);
            if map.terrain(tile) == Terrain::Jungle && !tiles.contains(&tile) {
                tiles.push(tile);
            }
        }
    }
    tiles
}

/// Decorative workspaces around the existing compact support stations. Their
/// footprints stay outside the delivery crowd, home trees and worker route;
/// no new interaction, obstruction or simulation entity is implied.
fn outbuilding_stands(map: &Map) -> [Vec2; 3] {
    [
        Vec2::new(1.0, -21.0),
        Vec2::new(17.0, -5.0),
        // The low, roof-free kitchen frames the existing chefs' station.
        Vec2::new(-1.75, 9.25),
    ]
    .map(|offset| tile_centre(map.town_centre()) + offset)
}

/// Leave a sightline to each building through foreground foliage. This only
/// removes drawn scenery; the jungle's terrain and collision stay unchanged.
fn obscures_outbuilding(map: &Map, at: Vec2) -> bool {
    let feet = project(at);
    let foliage = Rect::from_corners(
        feet + art::PLANT.offset_of(Vec2::ZERO),
        feet + art::PLANT.offset_of(Vec2::new(320.0, 352.0)),
    );
    outbuilding_stands(map).into_iter().any(|building| {
        let anchor = project(building);
        let bounds = Rect::from_corners(
            anchor + Vec2::new(-58.0, -13.0),
            anchor + Vec2::new(58.0, 114.0),
        );
        stand_z(at, 0.0) > stand_z(building, 0.0) && !foliage.intersect(bounds).is_empty()
    })
}

pub(crate) fn spawn_world(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    art: &Art,
    map: &Map,
) {
    // One material for every baked surface: the colour lives in the vertices.
    let painted = materials.add(ColorMaterial::from(Color::WHITE));
    let plant = art::PLANT;
    let trodden = Trodden::of(map);

    commands
        .spawn((WorldRoot, Transform::default(), Visibility::default()))
        .with_children(|root| {
            root.spawn((
                Mesh2d(meshes.add(ground_mesh(map, &trodden))),
                MeshMaterial2d(painted.clone()),
                Transform::from_xyz(0.0, 0.0, GROUND_Z),
            ));
            // The drawn ground over it. Masked rather than blended: the art has
            // binary alpha, and a mask writes depth like the opaque underlay
            // does, so nothing about the layering changes.
            root.spawn((
                Mesh2d(meshes.add(ground_tile_mesh(map, &trodden))),
                MeshMaterial2d(materials.add(ColorMaterial {
                    color: Color::WHITE,
                    texture: Some(art.ground.clone()),
                    alpha_mode: bevy::sprite_render::AlphaMode2d::Mask(0.5),
                    ..default()
                })),
                Transform::from_xyz(0.0, 0.0, GROUND_TILE_Z),
            ));
            root.spawn((
                Mesh2d(meshes.add(jungle_floor_mesh(map))),
                MeshMaterial2d(materials.add(ColorMaterial::from(art.deep_jungle_floor.clone()))),
                Transform::from_xyz(0.0, 0.0, GROUND_Z + 0.01),
            ));

            // Anything with height is its own entity, anchored at the ground it
            // stands on. That is the whole discipline, and it is why the art
            // slots in where the meshes were without touching the layering: a
            // sprite anchored at its feet sorts by `stand_z` exactly as a prism
            // built from its footprint did. The house covers a monkey behind it
            // and not one in front, without any per-frame sorting.
            //
            // A multi-tile footprint can only carry one depth, so each takes its
            // centre's: half a footprint of error either way, rather than a
            // whole one at a corner. Keeping footprints small is what keeps that
            // invisible - and it is why the jungle is planted per *tile* rather
            // than drawn as one wall.
            let jungle = scenery_tiles(map).into_iter().filter_map(|tile| {
                let at = tile_centre(tile);
                planting(tile)
                    .filter(|_| !obscures_outbuilding(map, at))
                    .map(|kind| (at, &art.jungle[kind], plant))
            });

            let village = [
                (stall_stand(map), &art.town_centre, art::TOWN_CENTRE),
                // The worked node still has its bunch on; the home tree has had
                // it cut, and that one banana is the loose one lying at its foot
                // for the player to pick up. Two states of one plant, which is
                // also what finally tells the two nodes apart on sight.
                (
                    tile_centre(map.worked_grove().tile),
                    &art.banana_fruiting,
                    plant,
                ),
            ]
            .into_iter()
            .chain(
                map.home_trees()
                    .iter()
                    .map(|&tile| (tile_centre(tile), &art.banana_harvested, plant)),
            )
            .chain(
                map.groves()
                    .iter()
                    .map(|grove| grove.tile)
                    .filter(|tile| *tile != map.worked_grove().tile)
                    .map(|tile| (tile_centre(tile), &art.banana_fruiting, plant)),
            );

            let buildings = outbuilding_stands(map)
                .into_iter()
                .zip(&art.outbuildings)
                .map(|(at, image)| (at, image, art::OUTBUILDING));
            for (at, image, cell, nudge) in jungle
                .chain(village)
                .map(|(at, image, cell)| (at, image, cell, 0.0))
                .chain(buildings.map(|(at, image, cell)| (at, image, cell, NUDGE_STEP)))
            {
                let anchor = project(at);
                let (sprite, pivot) = art.standing(image, cell);
                root.spawn((
                    sprite,
                    pivot,
                    Transform::from_xyz(anchor.x, anchor.y, stand_z(at, nudge)),
                ));
            }
            for (at, image) in outbuilding_stands(map)
                .into_iter()
                .zip(&art.outbuilding_floors)
            {
                let anchor = project(at);
                let (sprite, pivot) = art.standing(image, art::OUTBUILDING);
                root.spawn((
                    sprite,
                    pivot,
                    Transform::from_xyz(anchor.x, anchor.y, GROUND_TILE_Z + 0.01),
                ));
            }

            // The treehouse's cast shade lies on the ground, so it is drawn
            // there: over the terrain,
            // under the depot glow and the crowd's shadows. Drawn with the
            // house it sorted at the house's depth, far above both, and hid
            // half the drop target and a third of the shadows at the bins.
            let stall = stall_stand(map);
            let anchor = project(stall);
            let (sprite, pivot) = art.standing(&art.town_centre_ground, art::TOWN_CENTRE);
            root.spawn((
                sprite,
                pivot,
                Transform::from_xyz(anchor.x, anchor.y, TREEHOUSE_GROUND_Z),
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dense_forest_adds_all_silhouettes_without_planting_open_ground() {
        let map = crate::map::start();
        let tiles = scenery_tiles(map);
        assert!(tiles.len() > wall_tiles(map).len());
        let mut kinds = [false; 8];
        for tile in tiles {
            assert_eq!(map.terrain(tile), Terrain::Jungle);
            if let Some(kind) =
                planting(tile).filter(|_| !obscures_outbuilding(map, tile_centre(tile)))
            {
                kinds[kind] = true;
            }
        }
        assert!(kinds.into_iter().all(|present| present));
    }

    #[test]
    fn foreground_crowns_leave_the_distribution_center_readable() {
        let map = crate::map::start();
        // A crown rooted beyond the east path reaches back over the shed.
        // Its ground tile need not touch the building to hide its whole front.
        let foreground = tile_centre(Tile::new(56, 35));
        assert_eq!(map.terrain(Tile::new(56, 35)), Terrain::Jungle);
        assert!(obscures_outbuilding(map, foreground));
        assert!(!obscures_outbuilding(map, tile_centre(Tile::new(0, 0))));
    }

    #[test]
    fn outbuildings_leave_delivery_and_support_silhouettes_open() {
        let map = crate::map::start();
        let centre = tile_centre(map.town_centre());
        let layout = crate::game::SceneLayout::for_viewport(Vec2::new(1280.0, 900.0));
        assert_eq!(
            outbuilding_stands(map)[2],
            layout.support_stand(crate::domain::SupportRole::Chef)
        );
        let protected = [
            Vec2::ZERO,
            Vec2::new(7.125, 0.875),
            Vec2::new(-1.75, 9.25),
            Vec2::new(2.75, -6.25),
        ];
        for (index, at) in outbuilding_stands(map).into_iter().enumerate() {
            assert!(
                map.terrain(Tile::new((at.x / METRE) as i32, (at.y / METRE) as i32))
                    .passable()
            );
            // Conservative standing-prop bounds; flat floor paint cannot
            // obscure an actor and is deliberately excluded.
            let anchor = project(at);
            let building = if index == 2 {
                Rect::from_corners(
                    anchor + Vec2::new(-71.0, -4.0),
                    anchor + Vec2::new(75.0, 24.0),
                )
            } else {
                Rect::from_corners(
                    anchor + Vec2::new(-58.0, -2.0),
                    anchor + Vec2::new(58.0, 114.0),
                )
            };
            for offset in protected {
                if index == 2 && offset == Vec2::new(-1.75, 9.25) {
                    continue; // Chefs intentionally stand inside their kitchen.
                }
                let feet = project(centre + offset);
                // Includes the three-avatar fan and full curled tails.
                let crowd =
                    Rect::from_corners(feet + Vec2::new(-27.0, -4.0), feet + Vec2::new(27.0, 38.0));
                assert!(
                    building.intersect(crowd).is_empty(),
                    "building at {at:?} covers {offset:?}"
                );
            }
        }
    }

    #[test]
    fn the_jungle_floor_only_textures_jungle_and_keeps_uvs_inside_the_tile() {
        let map = crate::map::start();
        let mesh = jungle_floor_mesh(map);
        let jungle = (0..map.height())
            .flat_map(|y| (0..map.width()).map(move |x| Tile::new(x, y)))
            .filter(|tile| map.terrain(*tile) == Terrain::Jungle && !drawn(map, *tile))
            .count();
        assert_eq!(mesh.count_vertices(), jungle * 4);
        let bevy::mesh::VertexAttributeValues::Float32x2(uvs) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
        else {
            panic!("UVs missing")
        };
        assert!(uvs.iter().flatten().all(|v| (0.0..=1.0).contains(v)));
    }

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
    fn the_treehouse_stands_with_its_bins_on_the_delivery_point() {
        // A hut centred on the town centre swallows half the unloading queue at
        // the one moment in the cycle the player is watching it.
        let map = crate::map::start();
        let centre = tile_centre(map.town_centre());
        let stall = stall_stand(map);
        // Its bins are on the delivery point, to the pixel: that is where the
        // crowd unloads, so it is where they must be drawn.
        let bins = project(stall) + art::TOWN_CENTRE.offset_of(art::TOWN_CENTRE_BINS);
        assert!(
            bins.distance(project(centre)) < 1e-3,
            "the bins draw at {bins}, the delivery point at {}",
            project(centre)
        );
        // And the house rises behind them, so the queue forms in front of it.
        assert!(stand_z(stall, 0.0) < stand_z(centre, 0.0));
    }

    /// The drawn ground tile a board tile lies under.
    fn under(tiles: &[GroundTile], tile: Tile) -> Option<GroundTile> {
        let at = IVec2::new(
            tile.x.div_euclid(GROUND_SPAN),
            tile.y.div_euclid(GROUND_SPAN),
        );
        tiles.iter().find(|t| t.at == at).copied()
    }

    #[test]
    fn a_ground_tile_covers_exactly_two_board_tiles_each_way() {
        // Drawn at the shared scale, a ground diamond must be exactly the
        // board's diamond over GROUND_SPAN tiles, or the tiles gap or overlap
        // at every seam - and its corners must land on whole texels, or the
        // art's pixels stop lining up with everything standing on it.
        let size = art::GROUND_TILE * art::ART_SCALE;
        let span = GROUND_SPAN as f32 * METRE;
        let right = project(Vec2::new(span, 0.0));
        let left = project(Vec2::new(0.0, span));
        let bottom = project(Vec2::splat(span));
        assert_eq!(size.x, right.x - left.x);
        assert_eq!(size.y, -bottom.y);
        assert_eq!(size, size.round());
        // And the mesh the tiles build is one quad for each of them.
        let map = crate::map::start();
        let trodden = Trodden::of(map);
        let quads = ground_tiles(map, &trodden).len();
        assert_eq!(ground_tile_mesh(map, &trodden).count_vertices(), quads * 4);
    }

    #[test]
    fn neighbouring_ground_tiles_agree_on_every_corner_they_share() {
        let map = crate::map::start();
        let tiles = ground_tiles(map, &Trodden::of(map));
        let at = |i: i32, j: i32| tiles.iter().find(|t| t.at == IVec2::new(i, j));
        let bit = |tile: &GroundTile, bit: u8| tile.mask & bit != 0;
        for tile in &tiles {
            let IVec2 { x: i, y: j } = tile.at;
            // Along i: my right is its top, my bottom its left.
            if let Some(next) = at(i + 1, j) {
                assert_eq!(bit(tile, 2), bit(next, 1), "{:?}", tile.at);
                assert_eq!(bit(tile, 4), bit(next, 8), "{:?}", tile.at);
            }
            // Along j: my left is its top, my bottom its right.
            if let Some(next) = at(i, j + 1) {
                assert_eq!(bit(tile, 8), bit(next, 1), "{:?}", tile.at);
                assert_eq!(bit(tile, 4), bit(next, 2), "{:?}", tile.at);
            }
            assert!(tile.variant < 4);
        }
    }

    #[test]
    fn the_depot_and_the_walk_are_trodden_and_the_rest_is_floor() {
        // A new player is asked to drag a banana to the town centre. Before the
        // depot was trodden in, the town centre drew nothing at all - the same
        // green as the forty tiles around it - so the drag's target was a word.
        let map = crate::map::start();
        let trodden = Trodden::of(map);
        let tiles = ground_tiles(map, &trodden);
        let centre = map.town_centre();
        let depot = under(&tiles, centre).expect("the depot has ground");
        assert_eq!(depot.mask, 15, "the delivery point stands on dirt");
        // The whole drop target is on the plaza, out to its edge, wholly dirt.
        let reach = DEPOT_EDGE_REACH;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let tile = Tile::new(centre.x + dx, centre.y + dy);
                assert_eq!(
                    under(&tiles, tile).unwrap().mask,
                    15,
                    "{tile:?} is off the plaza"
                );
            }
        }
        // The walk to the grove is dirt the whole way: every metre of it has
        // a trodden corner beside it.
        let route = map.reference_route();
        for step in 0..=200 {
            let at = route.sample(f64::from(step) / 200.0).at;
            let tile = Tile::containing(at);
            assert_ne!(
                under(&tiles, tile).unwrap().mask,
                0,
                "the trail breaks at {tile:?}"
            );
        }
        // The ring path is dirt too: one row of vertices on its far sides,
        // two - a whole tile of dirt - on its near ones.
        for tile in [Tile::new(30, 13), Tile::new(30, 14), Tile::new(14, 30)] {
            assert_ne!(under(&tiles, tile).unwrap().mask, 0, "{tile:?}");
        }
        for tile in [Tile::new(30, 54), Tile::new(54, 30)] {
            assert_eq!(under(&tiles, tile).unwrap().mask, 15, "{tile:?}");
        }
        // And the rest of the town, the groves and the jungle edge are floor:
        // the far corner of the town from the walk, the southern grove.
        assert_eq!(under(&tiles, Tile::new(20, 50)).unwrap().mask, 0);
        assert_eq!(under(&tiles, map.groves()[1].tile).unwrap().mask, 0);
        // The underlay under a drawn tile is that ground's colour, never the
        // canopy showing through a seam.
        assert_eq!(underlay_colour(map, &trodden, centre), art::GROUND_DIRT);
        assert_eq!(
            underlay_colour(map, &trodden, Tile::new(20, 50)),
            art::GROUND_FLOOR
        );
        assert_eq!(
            underlay_colour(map, &trodden, Tile::new(0, 0)),
            JUNGLE_CANOPY
        );
    }

    #[test]
    fn all_open_ground_and_the_jungle_edge_are_drawn_and_the_depths_are_not() {
        let map = crate::map::start();
        let tiles = ground_tiles(map, &Trodden::of(map));
        for y in 0..map.height() {
            for x in 0..map.width() {
                let tile = Tile::new(x, y);
                if drawn(map, tile) {
                    assert!(
                        under(&tiles, tile).is_some(),
                        "{tile:?} is open and undrawn"
                    );
                }
            }
        }
        assert!(
            under(&tiles, Tile::new(0, 0)).is_none(),
            "the canopy is drawn over"
        );
    }

    #[test]
    fn most_ground_is_quiet_and_every_variant_is_used() {
        let mut counts = [0u32; 4];
        for j in 0..40 {
            for i in 0..40 {
                counts[ground_variant(IVec2::new(i, j)) as usize] += 1;
            }
        }
        let quiet = counts[0] as f32 / 1600.0;
        assert!((0.6..0.73).contains(&quiet), "{counts:?}");
        assert!(counts[1..].iter().all(|&n| n > 100), "{counts:?}");
    }

    #[test]
    fn the_ground_plane_is_one_quad_per_tile() {
        let map = Map::parse("@.*\n...").expect("parses");
        let mesh = ground_mesh(&map, &Trodden::of(&map));
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
