//! The village map, and the routes across it.
//!
//! Pure and Bevy-light, like `domain`: a grid of terrain and an A* that answers
//! "how do I get from here to there, and how far is it".
//!
//! Both halves of the game need that answer, for different reasons. The travel
//! leg in `domain` is a *length* measured here, and a monkey's drawn walk is
//! the *polyline*. Today the economy holds that length as the `GROVE_DISTANCE`
//! constant and [`agrees_with_travel_leg`] asserts the two still say the same
//! thing - so the map is what the constant is *checked against*, not what
//! supplies it at runtime. That changes when a second node goes live and travel
//! stops being one number (D24). Serving both from one module is what makes
//! sure that when it does, a monkey's drawn path and its cycle time cannot
//! become two different journeys.
//!
//! The map is compiled in with `include_str!` rather than loaded as an asset,
//! because the headless economy needs it: `cargo test` has no asset server and
//! no window.

use std::{cmp::Ordering, collections::BinaryHeap, fmt, sync::OnceLock};

use bevy::math::DVec2;

/// A tile's edge, in metres.
///
/// The grid is a pathfinding granularity, not the resolution anything is drawn
/// at: monkeys move continuously along the smoothed polyline [`Map::route`]
/// returns. Two metres keeps a 69-tile map hand-authorable in a text editor
/// while leaving a monkey comfortably smaller than the square it stands on.
///
/// It is also, since D24, a *balance parameter*. It scales every route this
/// module measures and so scales `GROVE_DISTANCE` itself: halving it for
/// smoother routing would halve the travel leg and move D17's cart advantage,
/// with no speed change to compensate. It moves with the speeds or not at all.
pub const TILE_METRES: f64 = 2.0;

/// A* step costs, scaled so the octile metric is exact integer arithmetic.
///
/// Accumulating `1.0` and `SQRT_2` in `f64` leaves two paths of mathematically
/// equal cost differing by a few ULP, and which one wins then depends on the
/// order the additions happened in rather than on any rule this module states.
/// Integers make equal costs *equal*, which is what lets the cell-index
/// tie-break in [`Candidate`] actually govern - and the headless suite asserts
/// exact ticks on the strength of that.
const STEP: u64 = 1_000_000;
/// `STEP * sqrt(2)`, rounded up.
///
/// The rounding belongs to the search's cost model alone. A route's reported
/// length is measured on the finished polyline in [`Route::new`], never
/// accumulated here, so this constant cannot move a travel leg.
const DIAGONAL_STEP: u64 = 1_414_214;

/// How close two boundary crossings have to be to count as the same corner.
///
/// [`Map::clear_line`] compares its two crossing distances. On a map of side
/// `n` their smallest true non-zero separation is `1/(2·run·rise)` - about
/// 1.1e-4 at 69 tiles - while float drift at a *genuine* corner stays under
/// 3e-15. This sits five orders clear of both, and stays correct while
/// `2·run·rise` is below `1/GRAZE`: roughly 22 000 tiles a side.
const GRAZE: f64 = 1e-9;

const NEIGHBOURS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Whether a measured walk still stands behind the constant the economy uses.
///
/// A relative picometre: any real redraw of the map fails this by a dozen
/// orders of magnitude, while a designer is never asked to transcribe a
/// seventeen-digit literal to the last bit. The exactness the shipped map does
/// have is claimed separately, by asserting the worked route equals its own
/// straight line.
pub fn agrees_with_travel_leg(walked: f64, leg: f64) -> bool {
    (walked - leg).abs() <= leg.abs() * 1e-12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terrain {
    /// The hard barrier. Nothing routes through it, and it is what will make
    /// the ring path worth walking - though not yet: the shipped town is open
    /// enough that both worked routes cross it in a straight line.
    Jungle,
    /// The ring around the town.
    Path,
    /// Town ground. Buildings stand on it and monkeys walk *through* them, so
    /// the town is one open floor rather than a set of obstacles.
    Town,
    /// A clearing cut into the jungle around a banana node.
    Grove,
}

impl Terrain {
    pub const fn passable(self) -> bool {
        !matches!(self, Self::Jungle)
    }
}

/// Signed, so a tile off the west or north edge can be *expressed*.
///
/// Converting a pointer position to a tile is how drag-and-drop harvesting will
/// work, and a drag past those edges produces a negative. In `u32` that
/// saturates silently to `(0, 0)` - a real tile, in the far corner of the map -
/// instead of reading as the miss it is. [`Map::terrain`] answers `Jungle`
/// for anything off the map, in either direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    pub x: i32,
    pub y: i32,
}

impl Tile {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The tile's centre, in metres. Routes are built from centres, so a route
    /// between two tiles in the same row is exactly their separation.
    pub fn centre(self) -> DVec2 {
        DVec2::new(
            (f64::from(self.x) + 0.5) * TILE_METRES,
            (f64::from(self.y) + 0.5) * TILE_METRES,
        )
    }

    /// The tile a position in metres falls on.
    ///
    /// Floored rather than truncated, so a position west or north of the origin
    /// lands on the negative tile it is actually on instead of folding onto
    /// tile zero — the same reason [`Tile`] is signed at all.
    pub fn containing(at: DVec2) -> Self {
        Self::new(
            (at.x / TILE_METRES).floor() as i32,
            (at.y / TILE_METRES).floor() as i32,
        )
    }
}

/// A banana node worked by monkeys, with its walk already measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grove {
    pub tile: Tile,
    /// Metres from the town centre, along the route [`Map::route`] returns.
    pub walk: f64,
}

/// A walk, as a polyline in metres.
///
/// Straightened, so the staircase A* produces on a grid is not what anybody
/// walks or what the economy is charged for: on open ground a route between two
/// points is the single segment between them.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    points: Vec<DVec2>,
    length: f64,
}

/// A place on a walk, and the direction of travel there.
///
/// The heading is what a monkey faces, and — turned ninety degrees — the axis a
/// swarm offset is measured along, so the two always come from the same place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    pub at: DVec2,
    /// Unit length, or zero on a route that goes nowhere.
    pub heading: DVec2,
}

impl Route {
    fn new(points: Vec<DVec2>) -> Self {
        let length = points.windows(2).map(|leg| leg[0].distance(leg[1])).sum();
        Self { points, length }
    }

    /// The corners, in order, starting at the origin tile's centre.
    pub fn points(&self) -> &[DVec2] {
        &self.points
    }

    /// Metres, walked.
    pub fn length(&self) -> f64 {
        self.length
    }

    /// Where a walker a given fraction of the way along stands, and which way
    /// it faces.
    ///
    /// By *arc length*, not by leg index: a walk with a long leg and a short
    /// one is still covered at one speed, which is the only reading under which
    /// the drawn journey and the economy's `2d/v` are the same journey.
    ///
    /// A linear scan. Routes on this map have one or two legs, and the scan is
    /// what keeps `Route` a plain polyline rather than something that has to
    /// maintain prefix sums; revisit it if a route ever grows to many corners.
    pub fn sample(&self, fraction: f64) -> Step {
        let target = self.length * fraction.clamp(0.0, 1.0);
        let mut walked = 0.0;
        for leg in self.points.windows(2) {
            let span = leg[0].distance(leg[1]);
            if walked + span >= target {
                let along = if span > 0.0 {
                    (target - walked) / span
                } else {
                    0.0
                };
                return Step {
                    at: leg[0].lerp(leg[1], along),
                    heading: (leg[1] - leg[0]).normalize_or_zero(),
                };
            }
            walked += span;
        }
        // A route of one point, or a fraction that floating point walked off
        // the end of: both stand at the far end, facing the way they came.
        let at = *self.points.last().expect("a route has at least one point");
        let heading = self
            .points
            .len()
            .checked_sub(2)
            .map_or(DVec2::ZERO, |previous| {
                (at - self.points[previous]).normalize_or_zero()
            });
        Step { at, heading }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    Empty,
    Ragged {
        row: i32,
    },
    Unknown {
        row: i32,
        column: i32,
        found: char,
    },
    NoTownCentre,
    TwoTownCentres,
    NoGrove,
    /// A node the town centre cannot reach. Caught at parse time because the
    /// alternative is a worker that walks out and never delivers.
    Unreachable(Tile),
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the map is empty"),
            Self::Ragged { row } => write!(f, "row {row} is not the width of row 0"),
            Self::Unknown { row, column, found } => {
                write!(f, "unknown glyph `{found}` at row {row}, column {column}")
            }
            Self::NoTownCentre => write!(f, "the map has no town centre (`@`)"),
            Self::TwoTownCentres => write!(f, "the map has more than one town centre (`@`)"),
            Self::NoGrove => write!(f, "the map has no banana node (`*`)"),
            Self::Unreachable(tile) => write!(
                f,
                "the banana node at ({}, {}) cannot be reached from the town centre",
                tile.x, tile.y
            ),
        }
    }
}

pub struct Map {
    width: i32,
    height: i32,
    terrain: Vec<Terrain>,
    town_centre: Tile,
    /// Nearest first. See [`Map::parse`].
    groves: Vec<Grove>,
    home_trees: Vec<Tile>,
}

/// The map, as a resource.
///
/// The map is compiled in, so this holds a reference to a static rather than
/// owned data. It exists anyway, and is worth the newtype, because a system
/// that reads the map should *say so in its signature*: reaching for
/// [`start()`] from inside a system makes the map an ambient dependency, and
/// the readability of `SimulationPlugin` and `PresentationPlugin` rests on
/// their systems declaring what they touch.
#[derive(bevy::prelude::Resource, Clone, Copy)]
pub struct Village(&'static Map);

impl Village {
    pub fn start() -> Self {
        Self(start())
    }
}

impl std::ops::Deref for Village {
    type Target = Map;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

/// The walk the workforce is on, computed once.
///
/// [`Map::route`] allocates and clears three vectors the size of the map on
/// every call, so it is emphatically not something to ask per frame. The route
/// is fixed for as long as the worked node is, so it is measured at startup and
/// read from here.
#[derive(bevy::prelude::Resource)]
pub struct WorkedRoute(pub Route);

impl WorkedRoute {
    pub fn start() -> Self {
        Self(start().reference_route())
    }
}

/// The map every run begins on.
pub fn start() -> &'static Map {
    static START: OnceLock<Map> = OnceLock::new();
    START.get_or_init(|| {
        Map::parse(include_str!("../assets/maps/start.txt"))
            .unwrap_or_else(|error| panic!("the compiled-in starting map is invalid: {error}"))
    })
}

impl Map {
    /// One character per tile:
    ///
    /// ```text
    /// #  jungle, impassable      *  a clearing holding a banana node
    /// +  the ring path           T  town ground holding a home tree
    /// .  town ground             @  town ground holding the town centre
    /// o  a clearing in the jungle
    /// ```
    ///
    /// More than a lexer: this also *searches*. Every node is routed from the
    /// town centre, which proves it reachable and orders the nodes by walk, so
    /// both of those are type invariants rather than things a caller has to
    /// remember to check.
    pub fn parse(source: &str) -> Result<Self, MapError> {
        let rows: Vec<&str> = source.lines().collect();
        let width = rows.first().map_or(0, |row| row.chars().count());
        if rows.is_empty() || width == 0 {
            return Err(MapError::Empty);
        }

        let mut terrain = Vec::with_capacity(rows.len() * width);
        let mut town_centre = None;
        let mut nodes = Vec::new();
        let mut home_trees = Vec::new();
        for (y, row) in rows.iter().enumerate() {
            if row.chars().count() != width {
                return Err(MapError::Ragged { row: y as i32 });
            }
            for (x, glyph) in row.chars().enumerate() {
                let tile = Tile::new(x as i32, y as i32);
                terrain.push(match glyph {
                    '#' => Terrain::Jungle,
                    '+' => Terrain::Path,
                    '.' => Terrain::Town,
                    'o' => Terrain::Grove,
                    '*' => {
                        nodes.push(tile);
                        Terrain::Grove
                    }
                    'T' => {
                        home_trees.push(tile);
                        Terrain::Town
                    }
                    '@' => {
                        if town_centre.replace(tile).is_some() {
                            return Err(MapError::TwoTownCentres);
                        }
                        Terrain::Town
                    }
                    found => {
                        return Err(MapError::Unknown {
                            row: y as i32,
                            column: x as i32,
                            found,
                        });
                    }
                });
            }
        }

        if nodes.is_empty() {
            return Err(MapError::NoGrove);
        }
        let mut map = Self {
            width: width as i32,
            height: rows.len() as i32,
            terrain,
            town_centre: town_centre.ok_or(MapError::NoTownCentre)?,
            groves: Vec::new(),
            home_trees,
        };

        // Nearest first, so "the node the workforce works" is a lookup rather
        // than a search, and so it does not depend on where in the file a node
        // happens to be written. Ties break on position, for a stable order
        // whatever the author's layout. The walk is kept rather than
        // recomputed, so what gets reported is what the sort actually used.
        for tile in nodes {
            let walk = map
                .route(map.town_centre, tile)
                .ok_or(MapError::Unreachable(tile))?
                .length();
            map.groves.push(Grove { tile, walk });
        }
        map.groves.sort_by(|a, b| {
            a.walk
                .partial_cmp(&b.walk)
                .expect("no NaN in a route length")
                .then_with(|| (a.tile.y, a.tile.x).cmp(&(b.tile.y, b.tile.x)))
        });
        Ok(map)
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    /// Off-map reads as jungle, which is what it is: the barrier does not stop
    /// at the edge of the file.
    pub fn terrain(&self, tile: Tile) -> Terrain {
        if tile.x < 0 || tile.y < 0 || tile.x >= self.width || tile.y >= self.height {
            return Terrain::Jungle;
        }
        self.terrain[tile.y as usize * self.width as usize + tile.x as usize]
    }

    pub fn town_centre(&self) -> Tile {
        self.town_centre
    }

    /// Every worked banana node, nearest first.
    pub fn groves(&self) -> &[Grove] {
        &self.groves
    }

    /// The node the workforce harvests: the nearest, and for the MVP the only
    /// one worked. A second node becomes live when workers can be assigned,
    /// which is the point at which travel stops being one number.
    pub fn worked_grove(&self) -> Grove {
        self.groves[0]
    }

    /// Banana trees the player picks by hand and no monkey is ever sent to.
    ///
    /// A home tree stands a few tiles from the town centre so that the harvest
    /// drag is a short flick with both ends on screen at once. The worked nodes
    /// cannot serve that purpose: at a zoom where a monkey is legible a phone
    /// holds well under twenty tiles, and the nearest worked node is thirty
    /// away. Kept out of [`Map::groves`] for a second reason - being nearer
    /// than the worked node, a home tree counted among them would take over as
    /// [`Map::worked_grove`] and silently move the travel leg.
    pub fn home_trees(&self) -> &[Tile] {
        &self.home_trees
    }

    /// The walk the economy is balanced against.
    pub fn reference_route(&self) -> Route {
        self.route(self.town_centre, self.worked_grove().tile)
            .expect("every node was proved reachable at parse time")
    }

    /// How much open ground there is either side of `at`, along `across`.
    ///
    /// The swarm is drawn across this rather than across a constant, which is
    /// what lets one crowd read as two different things: shoulder to shoulder
    /// where the walk threads a gap, and spread wide where it crosses the open
    /// town. A fixed lane width has to be narrow enough for the tightest point
    /// on the route, so it is that narrow everywhere.
    ///
    /// Symmetric — the *smaller* of the two sides — so a swarm centred on the
    /// route stays inside the corridor rather than leaning into whichever wall
    /// is further away. Cast along the line, because the alternative is a
    /// distance field over 4761 tiles for a question asked about one ray.
    pub fn corridor_half_width(&self, at: DVec2, across: DVec2) -> f64 {
        /// Beyond this the answer stops mattering: the swarm has its own cap.
        const REACH: f64 = 10.0;

        let across = across.normalize_or_zero();
        if across == DVec2::ZERO || !self.terrain(Tile::containing(at)).passable() {
            return 0.0;
        }
        self.clearance(at, across, REACH)
            .min(self.clearance(at, -across, REACH))
    }

    /// How far a ray from `at` travels before it enters impassable ground.
    ///
    /// A grid traversal rather than a sampled march, and the difference is not
    /// precision but *continuity*. Probing at fixed intervals answers in whole
    /// steps, so the width jumps by a step as the ray creeps forward - and a
    /// swarm drawn across that width snaps narrower and wider as it walks,
    /// which reads as the crowd flinching. This returns the exact distance to
    /// the wall, so the width is a continuous function of where the monkey is.
    fn clearance(&self, at: DVec2, direction: DVec2, reach: f64) -> f64 {
        let mut tile = Tile::containing(at);
        let mut crossing = DVec2::INFINITY;
        let mut stride = DVec2::INFINITY;
        let mut step = (0, 0);

        if direction.x != 0.0 {
            let ahead = if direction.x > 0.0 { 1.0 } else { 0.0 };
            crossing.x = ((f64::from(tile.x) + ahead) * TILE_METRES - at.x) / direction.x;
            stride.x = TILE_METRES / direction.x.abs();
            step.0 = if direction.x > 0.0 { 1 } else { -1 };
        }
        if direction.y != 0.0 {
            let ahead = if direction.y > 0.0 { 1.0 } else { 0.0 };
            crossing.y = ((f64::from(tile.y) + ahead) * TILE_METRES - at.y) / direction.y;
            stride.y = TILE_METRES / direction.y.abs();
            step.1 = if direction.y > 0.0 { 1 } else { -1 };
        }

        loop {
            let travelled = crossing.x.min(crossing.y);
            if !travelled.is_finite() || travelled >= reach {
                return reach;
            }
            if crossing.x < crossing.y {
                tile.x += step.0;
                crossing.x += stride.x;
            } else {
                tile.y += step.1;
                crossing.y += stride.y;
            }
            if !self.terrain(tile).passable() {
                return travelled;
            }
        }
    }

    /// The shortest grid walk between two tiles, straightened.
    ///
    /// `None` when either end is jungle or the goal is walled off.
    pub fn route(&self, from: Tile, to: Tile) -> Option<Route> {
        if !self.passable_at(from.x, from.y) || !self.passable_at(to.x, to.y) {
            return None;
        }
        if from == to {
            return Some(Route::new(vec![from.centre()]));
        }

        let cells = self.width as usize * self.height as usize;
        let mut cost = vec![u64::MAX; cells];
        let mut came = vec![usize::MAX; cells];
        let mut closed = vec![false; cells];
        let (start, goal) = (self.index(from), self.index(to));
        cost[start] = 0;

        let mut open = BinaryHeap::new();
        open.push(Candidate {
            estimate: heuristic(from, to),
            cell: start,
        });
        while let Some(Candidate { cell, .. }) = open.pop() {
            if cell == goal {
                return Some(self.build_route(&came, goal));
            }
            if std::mem::replace(&mut closed[cell], true) {
                continue;
            }
            let (x, y) = (self.x_of(cell), self.y_of(cell));
            for (dx, dy) in NEIGHBOURS {
                let (nx, ny) = (x + dx, y + dy);
                if !self.passable_at(nx, ny) {
                    continue;
                }
                let diagonal = dx != 0 && dy != 0;
                // Refuse to squeeze between two blocked corners. A monkey that
                // did would clip the jungle it is supposed to be walled out of,
                // and the drawn path would leave the walkable map.
                if diagonal && (!self.passable_at(x + dx, y) || !self.passable_at(x, y + dy)) {
                    continue;
                }
                let neighbour = self.index(Tile::new(nx, ny));
                let step = cost[cell] + if diagonal { DIAGONAL_STEP } else { STEP };
                if step < cost[neighbour] {
                    cost[neighbour] = step;
                    came[neighbour] = cell;
                    open.push(Candidate {
                        estimate: step + heuristic(Tile::new(nx, ny), to),
                        cell: neighbour,
                    });
                }
            }
        }
        None
    }

    fn build_route(&self, came: &[usize], goal: usize) -> Route {
        let mut cells = vec![goal];
        let mut cell = goal;
        while came[cell] != usize::MAX {
            cell = came[cell];
            cells.push(cell);
        }
        cells.reverse();
        let tiles: Vec<Tile> = cells
            .into_iter()
            .map(|cell| Tile::new(self.x_of(cell), self.y_of(cell)))
            .collect();
        Route::new(self.string_pull(&tiles))
    }

    /// Drop every corner the walk does not need.
    ///
    /// A* on a grid can only turn in 45° steps, so it answers an open-ground
    /// diagonal with a staircase that is both ugly and measurably longer than
    /// the walk it stands for. Pulling the string taut against the jungle
    /// removes it.
    ///
    /// Greedy, and so *not* an optimal any-angle path: the result is the
    /// polyline a monkey walks, not necessarily the shortest line that exists
    /// between the two tiles. What holds regardless is the floor - no walk
    /// beats the straight line - and on the shipped map the worked route meets
    /// that floor exactly, so no tie-break can move the travel leg.
    fn string_pull(&self, tiles: &[Tile]) -> Vec<DVec2> {
        let mut points = vec![tiles[0].centre()];
        let mut anchor = 0;
        while anchor + 1 < tiles.len() {
            // Visibility is not monotone along a path that hugs an obstacle, so
            // this keeps scanning rather than stopping at the first corner it
            // cannot see past.
            let mut furthest = anchor + 1;
            for candidate in anchor + 2..tiles.len() {
                if self.clear_line(tiles[anchor], tiles[candidate]) {
                    furthest = candidate;
                }
            }
            points.push(tiles[furthest].centre());
            anchor = furthest;
        }
        points
    }

    /// Whether a straight walk between two tile centres stays on passable
    /// ground, under the same no-corner-cutting rule the search itself obeys.
    ///
    /// Every tile the segment *touches*, not the one tile per column a
    /// Bresenham line would pick. The difference matters: [`Map::string_pull`]
    /// only keeps a shortcut this call approves, so a line test that skipped a
    /// tile would approve a shortcut clipping the corner of a jungle block, and
    /// the straightened walk would leave the walkable map.
    fn clear_line(&self, from: Tile, to: Tile) -> bool {
        if !self.passable_at(from.x, from.y) {
            return false;
        }
        let (mut x, mut y) = (from.x, from.y);
        let (run, rise) = (to.x - from.x, to.y - from.y);
        let (step_x, step_y) = (run.signum(), rise.signum());

        // Distances along the segment, in units of its own length, to the next
        // tile boundary and between boundaries. The walk starts at a tile
        // centre, so the first boundary is half a tile away.
        let span_x = if run == 0 {
            f64::INFINITY
        } else {
            1.0 / f64::from(run.abs())
        };
        let span_y = if rise == 0 {
            f64::INFINITY
        } else {
            1.0 / f64::from(rise.abs())
        };
        let mut next_x = span_x * 0.5;
        let mut next_y = span_y * 0.5;

        // The segment crosses at most one boundary per tile in each axis, so
        // this bounds the walk without trusting the floating point to land.
        let limit = run.abs() + rise.abs() + 2;
        for _ in 0..limit {
            if x == to.x && y == to.y {
                return true;
            }
            if (next_x - next_y).abs() <= GRAZE {
                // Exactly through a corner. The segment touches both orthogonal
                // tiles, so both have to be walkable - the same rule the search
                // obeys, for the same reason.
                if !self.passable_at(x + step_x, y) || !self.passable_at(x, y + step_y) {
                    return false;
                }
                next_x += span_x;
                next_y += span_y;
                x += step_x;
                y += step_y;
            } else if next_x < next_y {
                next_x += span_x;
                x += step_x;
            } else {
                next_y += span_y;
                y += step_y;
            }
            if !self.passable_at(x, y) {
                return false;
            }
        }
        // Unreachable: the bound above is one more than the crossings a segment
        // can make. Saying so out loud because the failure is otherwise silent
        // and points the wrong way - a rejected shortcut leaves a corner in the
        // walk, and the travel leg quietly grows.
        debug_assert!(false, "clear_line exceeded its own crossing bound");
        false
    }

    fn passable_at(&self, x: i32, y: i32) -> bool {
        self.terrain(Tile::new(x, y)).passable()
    }

    fn index(&self, tile: Tile) -> usize {
        tile.y as usize * self.width as usize + tile.x as usize
    }

    fn x_of(&self, cell: usize) -> i32 {
        (cell % self.width as usize) as i32
    }

    fn y_of(&self, cell: usize) -> i32 {
        (cell / self.width as usize) as i32
    }
}

/// Octile distance: the exact cost of the cheapest unobstructed grid walk, so
/// A* never expands a cell it does not have to, and the path it returns is of
/// minimum *grid cost*.
///
/// That is not the same quantity as the straightened [`Route::length`] the
/// caller receives; see [`Map::string_pull`].
fn heuristic(from: Tile, to: Tile) -> u64 {
    let dx = u64::from(from.x.abs_diff(to.x));
    let dy = u64::from(from.y.abs_diff(to.y));
    dx.max(dy) * STEP + dx.min(dy) * (DIAGONAL_STEP - STEP)
}

/// A min-heap entry.
///
/// `BinaryHeap` is a max-heap, so the ordering is reversed. Ties break on the
/// cell index, which is what makes a route the same route on every run - the
/// headless economy contracts assert exact ticks, and they cannot do that if
/// the travel leg depends on heap order. Costs are integers ([`STEP`]) so that
/// equal costs really are equal and this rule is the one that decides.
#[derive(PartialEq, Eq)]
struct Candidate {
    estimate: u64,
    cell: usize,
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .cmp(&self.estimate)
            .then_with(|| other.cell.cmp(&self.cell))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::GROVE_DISTANCE;

    fn map(rows: &[&str]) -> Map {
        Map::parse(&rows.join("\n")).expect("the test map parses")
    }

    /// Every point a walk passes through, sampled finely enough that a leg
    /// clipping one tile of jungle cannot slip between samples.
    fn walk_stays_on_the_map(map: &Map, route: &Route) -> bool {
        route.points().windows(2).all(|leg| {
            let steps = (leg[0].distance(leg[1]) / (TILE_METRES * 0.1))
                .ceil()
                .max(1.0) as u32;
            (0..=steps).all(|step| {
                let at = leg[0].lerp(leg[1], f64::from(step) / f64::from(steps));
                let tile = Tile::new(
                    (at.x / TILE_METRES).floor() as i32,
                    (at.y / TILE_METRES).floor() as i32,
                );
                map.terrain(tile).passable()
            })
        })
    }

    #[test]
    fn the_reference_route_is_the_economys_travel_leg() {
        // `GROVE_DISTANCE` is a *measurement* of `assets/maps/start.txt` rather
        // than a number somebody chose. Move the town centre or a banana node
        // and this is the test that says the balance moved with them.
        let walked = start().reference_route().length();
        assert!(
            agrees_with_travel_leg(walked, GROVE_DISTANCE),
            "the map walks {walked:?} m; GROVE_DISTANCE is {GROVE_DISTANCE:?} m"
        );
    }

    #[test]
    fn a_walk_is_never_shorter_than_the_line_it_stands_for() {
        // `string_pull` is greedy, so a route's length is not the optimum of
        // anything and can drift with tie-breaking. This is the floor that
        // holds regardless: nothing walkable beats the straight line.
        let start = start();
        for grove in start.groves() {
            let route = start
                .route(start.town_centre(), grove.tile)
                .expect("reachable");
            let straight = start.town_centre().centre().distance(grove.tile.centre());
            assert!(
                route.length() >= straight - 1e-9,
                "{:?} walks {} m, under its own straight line of {straight} m",
                grove.tile,
                route.length()
            );
        }
        // And the worked route *meets* the floor, so the travel leg is minimal
        // outright and no tie-break inside the search can move it. This is the
        // precondition the exactness of `GROVE_DISTANCE` rests on.
        let worked = start.worked_grove();
        assert_eq!(
            start.reference_route().length(),
            start.town_centre().centre().distance(worked.tile.centre())
        );
    }

    #[test]
    fn the_starting_map_is_a_town_ringed_by_a_path_and_walled_by_jungle() {
        let start = start();
        assert_eq!((start.width(), start.height()), (69, 69));
        assert_eq!(start.terrain(start.town_centre()), Terrain::Town);
        assert_eq!(start.terrain(Tile::new(34, 34)), Terrain::Town);
        assert_eq!(start.terrain(Tile::new(34, 13)), Terrain::Path);
        assert_eq!(start.terrain(Tile::new(34, 0)), Terrain::Jungle);
        assert_eq!(start.terrain(start.worked_grove().tile), Terrain::Grove);
        // Off the map is jungle in every direction, including the ones a `u32`
        // tile could not have expressed.
        assert_eq!(start.terrain(Tile::new(69, 0)), Terrain::Jungle);
        assert_eq!(start.terrain(Tile::new(-1, 30)), Terrain::Jungle);
        assert_eq!(start.terrain(Tile::new(30, -1)), Terrain::Jungle);
    }

    #[test]
    fn the_jungle_stands_deep_behind_every_node() {
        // A node with a sliver of jungle behind it shows the player the edge of
        // the world from the one place they spend their time looking at.
        let start = start();
        let centre = start.town_centre();
        for grove in start.groves() {
            // Straight on out from the town, through the node, to the void.
            let (step_x, step_y) = (
                (grove.tile.x - centre.x).signum(),
                (grove.tile.y - centre.y).signum(),
            );
            let depth = (1..)
                .map(|step| Tile::new(grove.tile.x + step_x * step, grove.tile.y + step_y * step))
                .take_while(|tile| {
                    tile.x >= 0 && tile.y >= 0 && tile.x < start.width() && tile.y < start.height()
                })
                .filter(|tile| !start.terrain(*tile).passable())
                .count();
            assert!(
                depth >= 8,
                "{:?} has only {depth} tiles of jungle between it and the edge",
                grove.tile
            );
        }
    }

    #[test]
    fn a_home_tree_is_a_thumbs_reach_from_the_town_centre_and_never_worked() {
        let start = start();
        let centre = start.town_centre().centre();
        assert!(!start.home_trees().is_empty());
        for &tree in start.home_trees() {
            let reach = centre.distance(tree.centre());
            // Both ends of the harvest drag have to be on screen at once, and a
            // phone at a playable zoom holds well under twenty tiles.
            assert!(reach <= 8.0 * TILE_METRES, "{tree:?} is {reach} m out");
            assert!(
                start.groves().iter().all(|grove| grove.tile != tree),
                "a home tree among the worked nodes would take over as the \
                 travel leg: {tree:?}"
            );
        }
        assert!(start.worked_grove().walk > centre.distance(start.home_trees()[0].centre()));
    }

    #[test]
    fn the_starting_map_offers_two_worked_nodes_and_works_the_nearer() {
        let start = start();
        assert_eq!(start.groves().len(), 2);
        assert_eq!(start.worked_grove(), start.groves()[0]);
        assert!(
            start.groves()[1].walk > start.groves()[0].walk,
            "the worked grove must be the nearer of the two"
        );
        // The stored walk is the one the sort used, not a recomputation.
        assert_eq!(start.worked_grove().walk, start.reference_route().length());
    }

    #[test]
    fn the_shipped_walks_stay_out_of_the_jungle() {
        let start = start();
        for grove in start.groves() {
            let route = start
                .route(start.town_centre(), grove.tile)
                .expect("reachable");
            assert!(
                walk_stays_on_the_map(start, &route),
                "{route:?} leaves the map"
            );
        }
    }

    #[test]
    fn string_pulling_straightens_an_open_diagonal() {
        // A* can only turn in 45° steps, so without straightening this walk is
        // a staircase and the economy is charged for corners nobody walks.
        let open = map(&["@.....", "......", ".....*"]);
        let route = open.reference_route();
        assert_eq!(route.points().len(), 2, "open ground needs no corners");
        let expected = (5.0f64.powi(2) + 2.0f64.powi(2)).sqrt() * TILE_METRES;
        assert!(
            (route.length() - expected).abs() < 1e-9,
            "{}",
            route.length()
        );
    }

    #[test]
    fn jungle_is_a_hard_barrier() {
        let walled = map(&[
            "#########",
            "#.......#",
            "#.@.#.*.#",
            "#...#...#",
            "#.......#",
            "#########",
        ]);
        let route = walled.reference_route();
        let straight = 4.0 * TILE_METRES;
        assert!(
            route.length() > straight,
            "a walk through the wall would be {straight} m, got {}",
            route.length()
        );
        assert!(
            route.points().len() > 2,
            "the walk has to turn to get round"
        );
        assert!(walk_stays_on_the_map(&walled, &route));
    }

    #[test]
    fn a_diagonal_never_squeezes_between_two_blocked_corners() {
        // The node sits diagonally adjacent to the town centre with both
        // orthogonal neighbours walled. Cutting the corner would reach it; the
        // parse fails instead, which is the rule stated as an error.
        assert_eq!(
            Map::parse("####\n#@##\n##*#\n####").err(),
            Some(MapError::Unreachable(Tile::new(2, 2)))
        );
        // Opening one orthogonal is still not enough to cut the corner: the
        // walk turns, and is charged for turning.
        let elbow = map(&["####", "#@.#", "##*#", "####"]);
        assert_eq!(elbow.reference_route().length(), 2.0 * TILE_METRES);
    }

    #[test]
    fn routes_are_deterministic() {
        // Two equal-length ways round the wall. The headless contracts assert
        // exact ticks, so the tie must break the same way on every run - and
        // with integer step costs the tie is exact rather than an artefact of
        // the order two floats were added in.
        let symmetric = map(&["#######", "#.....#", "#.@#*.#", "#.....#", "#######"]);
        let first = symmetric.reference_route();
        for _ in 0..8 {
            assert_eq!(symmetric.reference_route(), first);
        }
    }

    #[test]
    fn a_route_off_the_walkable_map_is_no_route() {
        let start = start();
        assert!(start.route(start.town_centre(), Tile::new(0, 0)).is_none());
        assert!(start.route(Tile::new(0, 0), start.town_centre()).is_none());
        assert!(
            start
                .route(start.town_centre(), Tile::new(500, 500))
                .is_none()
        );
        assert!(
            start
                .route(start.town_centre(), Tile::new(-1, -1))
                .is_none()
        );
    }

    #[test]
    fn a_walk_to_where_you_stand_is_one_point_and_no_distance() {
        let start = start();
        let route = start
            .route(start.town_centre(), start.town_centre())
            .expect("standing still is a route");
        assert_eq!(route.points().len(), 1);
        assert_eq!(route.length(), 0.0);
    }

    #[test]
    fn an_unauthorable_map_says_why() {
        assert_eq!(Map::parse("").err(), Some(MapError::Empty));
        assert_eq!(Map::parse("@*\n#").err(), Some(MapError::Ragged { row: 1 }));
        assert_eq!(
            Map::parse("@?*").err(),
            Some(MapError::Unknown {
                row: 0,
                column: 1,
                found: '?'
            })
        );
        assert_eq!(Map::parse("..*").err(), Some(MapError::NoTownCentre));
        assert_eq!(Map::parse("@..").err(), Some(MapError::NoGrove));
        assert_eq!(Map::parse("@*@").err(), Some(MapError::TwoTownCentres));
        assert_eq!(
            Map::parse("@#*").err(),
            Some(MapError::Unreachable(Tile::new(2, 0)))
        );
        // A home tree is not a worked node, so it cannot stand in for one.
        assert_eq!(Map::parse("@.T").err(), Some(MapError::NoGrove));
    }
}
