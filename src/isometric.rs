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
    ecs::relationship::RelatedSpawnerCommands,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::{
    art::{self, Art},
    domain::{CART_TECH_REQUIREMENT, Research},
    game::SceneLayout,
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
// drawn with the jungle plants, which is what has to read as a barrier. And the
// village is the brightest ground on the board: the clearing used to be, which
// pulled the eye into an empty corner and away from the only place anything
// happens.
const JUNGLE_CANOPY: Color = Color::srgb(0.16, 0.34, 0.19);
const PATH: Color = Color::srgb(0.84, 0.73, 0.51);
const TOWN: Color = Color::srgb(0.64, 0.79, 0.46);
const CLEARING: Color = Color::srgb(0.60, 0.71, 0.43);

/// The delivery point, trodden into bare earth.
///
/// The town centre used to draw *nothing*. Every delivery in the game lands on
/// it, the hand-harvest drag ends on it, and a new player opening the game was
/// shown a flat green lawn with the word VILLAGE floating over it and asked to
/// drag a banana onto the label. The treehouse's bins stand on it now (D30), but
/// its ground is still what the drop target is measured from, edge to edge.
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
/// And how far the scuffing around it reaches. The standing ring is sized
/// against this, so the pad contains the crowd that gathers on it.
pub(crate) const DEPOT_EDGE_REACH: i32 = 2;
/// The drop target, in metres either side of the delivery point: exactly the
/// ground the depot has trodden bare and scuffed, so what the player sees as the
/// depot is what accepts the banana, edge to edge.
pub(crate) const DEPOT_REACH_METRES: f32 = (DEPOT_EDGE_REACH as f32 + 0.5) * METRE;

#[derive(Component)]
pub(crate) struct WorldRoot;

/// A set of banana bins standing on the ground.
///
/// Two of them exist at most: the harvesters' set, which is the delivery point
/// and is up from the first frame, and the carts' set, which goes up when the
/// research that unlocks the Cart lands (D31). The marker carries which, so
/// `sync_cart_bins` can put the second set up and take it down again without
/// touching the first.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bins {
    Harvest,
    Cart,
}

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
    } else if reach <= DEPOT_EDGE_REACH {
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
    // Two in five planted, and an even pick between the three kinds.
    (bits % 5 < 2).then_some((bits >> 8) as usize % 3)
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

    commands
        .spawn((WorldRoot, Transform::default(), Visibility::default()))
        .with_children(|root| {
            root.spawn((
                Mesh2d(meshes.add(ground_mesh(map))),
                MeshMaterial2d(painted.clone()),
                Transform::from_xyz(0.0, 0.0, GROUND_Z),
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
            let jungle = wall_tiles(map).into_iter().filter_map(|tile| {
                planting(tile).map(|kind| (tile_centre(tile), &art.jungle[kind], plant))
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

            for (at, image, cell) in jungle.chain(village) {
                let anchor = project(at);
                let (sprite, pivot) = art.standing(image, cell);
                root.spawn((
                    sprite,
                    pivot,
                    Transform::from_xyz(anchor.x, anchor.y, stand_z(at, 0.0)),
                ));
            }

            // The harvesters' bins, which are the delivery point itself. Drawn
            // apart from the house they came out of (D31) for one reason: they
            // sort at the ground *they* stand on rather than at the house's
            // depth, so a monkey that has walked round to the front of them is
            // drawn in front of them.
            spawn_bins(root, art, Bins::Harvest, tile_centre(map.town_centre()));

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

/// One set of bins, standing at `at`.
fn spawn_bins(commands: &mut RelatedSpawnerCommands<ChildOf>, art: &Art, which: Bins, at: Vec2) {
    let anchor = project(at);
    // The depot's set is painted into the treehouse and keeps its scale; the
    // carts' set stands on its own and is drawn at the shared one. See
    // `art::CART_BINS`.
    let cell = match which {
        Bins::Harvest => art::BANANA_BINS,
        Bins::Cart => art::CART_BINS,
    };
    let (sprite, pivot) = art.standing(&art.banana_bins, cell);
    commands.spawn((
        which,
        sprite,
        pivot,
        Transform::from_xyz(anchor.x, anchor.y, stand_z(at, 0.0)),
    ));
}

/// Put the carts' bins up when the Cart's research lands, and take them down
/// again if a restart rolls the research back.
///
/// A reconcile rather than a one-shot on the purchase, for the reason every
/// other spawner here is one: a save loaded past the unlock has to open with the
/// bins already standing, and `Restarted` has to be able to remove them without
/// a second code path.
pub(crate) fn sync_cart_bins(
    mut commands: Commands,
    art: Res<Art>,
    layout: Res<SceneLayout>,
    research: Res<Research>,
    root: Query<Entity, With<WorldRoot>>,
    existing: Query<(Entity, &Bins)>,
    mut sign: Query<&mut Visibility, With<crate::game::CartYardLabel>>,
) {
    let wanted = research.level() >= CART_TECH_REQUIREMENT;
    // The sign goes with the bins: named the moment there is something there to
    // name, gone again the moment there is not.
    for mut visibility in &mut sign {
        let shown = if wanted {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != shown {
            *visibility = shown;
        }
    }
    let standing = existing.iter().any(|(_, bins)| *bins == Bins::Cart);
    if wanted == standing {
        return;
    }
    if !wanted {
        for (entity, bins) in &existing {
            if *bins == Bins::Cart {
                commands.entity(entity).despawn();
            }
        }
        return;
    }
    let Ok(root) = root.single() else {
        return;
    };
    let at = layout.cart_bins();
    commands
        .entity(root)
        .with_children(|root| spawn_bins(root, &art, Bins::Cart, at));
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
        let edge = Tile::new(centre.x + DEPOT_EDGE_REACH, centre.y);
        assert_eq!(ground_colour(map, edge), DEPOT_EDGE);
        // And it stops: the town is still the town a few tiles out.
        let away = Tile::new(centre.x + DEPOT_EDGE_REACH + 1, centre.y);
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
