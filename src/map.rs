//! The village map, and the routes across it.
//!
//! Pure and Bevy-light, like `domain`: a grid of terrain and an A* that answers
//! "how do I get from here to there, and how far is it".
//!
//! Two callers want that answer for different reasons. The economy takes the
//! *length* as its travel leg, and the presentation walks the *polyline*.
//! Serving both from one place is what stops a monkey's drawn path and its
//! cycle time being two different journeys - the failure this module exists to
//! make impossible.
//!
//! The map is compiled in with `include_str!` rather than loaded as an asset,
//! because the headless economy needs it: `cargo test` has no asset server and
//! no window, and [`crate::domain::GROVE_DISTANCE`] is now a *measurement of
//! this file* rather than a number somebody chose.

use std::{cmp::Ordering, collections::BinaryHeap, fmt, sync::OnceLock};

use bevy::math::DVec2;

/// A tile's edge, in metres.
///
/// The grid is a pathfinding granularity, not the resolution anything is drawn
/// at: monkeys move continuously along the smoothed polyline [`Map::route`]
/// returns. Two metres keeps a 69-tile map hand-authorable in a text editor
/// while leaving a monkey comfortably smaller than the square it stands on.
pub const TILE_METRES: f64 = 2.0;

/// A diagonal step, in tiles.
const DIAGONAL: f64 = std::f64::consts::SQRT_2;

const NEIGHBOURS: [(i64, i64); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terrain {
    /// The hard barrier. Nothing routes through it, which is the whole reason
    /// the ring path is worth walking.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
}

impl Tile {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    /// The tile's centre, in metres. Routes are built from centres, so a route
    /// between two tiles in the same row is exactly their separation.
    pub fn centre(self) -> DVec2 {
        DVec2::new(
            (self.x as f64 + 0.5) * TILE_METRES,
            (self.y as f64 + 0.5) * TILE_METRES,
        )
    }
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

impl Route {
    fn new(points: Vec<DVec2>) -> Self {
        let length = points.windows(2).map(|leg| leg[0].distance(leg[1])).sum();
        Self { points, length }
    }

    /// The corners, in order, starting at the origin tile's centre.
    pub fn points(&self) -> &[DVec2] {
        &self.points
    }

    /// Metres, walked. This is the economy's travel leg.
    pub fn length(&self) -> f64 {
        self.length
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    Empty,
    Ragged {
        row: u32,
    },
    Unknown {
        row: u32,
        column: u32,
        found: char,
    },
    NoTownCentre,
    TwoTownCentres,
    NoGrove,
    /// A grove the town centre cannot reach. Caught at parse time because the
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
    width: u32,
    height: u32,
    terrain: Vec<Terrain>,
    town_centre: Tile,
    /// Nearest first. See [`Map::parse`].
    groves: Vec<Tile>,
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
    /// #  jungle, impassable      o  a clearing in the jungle
    /// +  the ring path           *  a clearing holding a banana node
    /// .  town ground             @  town ground holding the town centre
    /// ```
    pub fn parse(source: &str) -> Result<Self, MapError> {
        let rows: Vec<&str> = source.lines().collect();
        let width = rows.first().map_or(0, |row| row.chars().count()) as u32;
        if rows.is_empty() || width == 0 {
            return Err(MapError::Empty);
        }

        let mut terrain = Vec::with_capacity(rows.len() * width as usize);
        let mut town_centre = None;
        let mut groves = Vec::new();
        for (y, row) in rows.iter().enumerate() {
            if row.chars().count() as u32 != width {
                return Err(MapError::Ragged { row: y as u32 });
            }
            for (x, glyph) in row.chars().enumerate() {
                let tile = Tile::new(x as u32, y as u32);
                terrain.push(match glyph {
                    '#' => Terrain::Jungle,
                    '+' => Terrain::Path,
                    '.' => Terrain::Town,
                    'o' => Terrain::Grove,
                    '*' => {
                        groves.push(tile);
                        Terrain::Grove
                    }
                    '@' => {
                        if town_centre.replace(tile).is_some() {
                            return Err(MapError::TwoTownCentres);
                        }
                        Terrain::Town
                    }
                    found => {
                        return Err(MapError::Unknown {
                            row: y as u32,
                            column: x as u32,
                            found,
                        });
                    }
                });
            }
        }

        if groves.is_empty() {
            return Err(MapError::NoGrove);
        }
        let mut map = Self {
            width,
            height: rows.len() as u32,
            terrain,
            town_centre: town_centre.ok_or(MapError::NoTownCentre)?,
            groves,
        };

        // Nearest first, so "the grove the workforce harvests" is a lookup
        // rather than a search, and so it does not depend on where in the file
        // a node happens to be written. Ties break on position, for a stable
        // order whatever the author's layout.
        let mut measured = Vec::with_capacity(map.groves.len());
        for &grove in &map.groves {
            let route = map
                .route(map.town_centre, grove)
                .ok_or(MapError::Unreachable(grove))?;
            measured.push((route.length(), grove));
        }
        measured.sort_by(|(a_len, a), (b_len, b)| {
            a_len
                .partial_cmp(b_len)
                .expect("no NaN in a route length")
                .then_with(|| (a.y, a.x).cmp(&(b.y, b.x)))
        });
        map.groves = measured.into_iter().map(|(_, tile)| tile).collect();
        Ok(map)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Off-map reads as jungle, which is what it is: the barrier does not stop
    /// at the edge of the file.
    pub fn terrain(&self, tile: Tile) -> Terrain {
        if tile.x >= self.width || tile.y >= self.height {
            return Terrain::Jungle;
        }
        self.terrain[(tile.y * self.width + tile.x) as usize]
    }

    pub fn town_centre(&self) -> Tile {
        self.town_centre
    }

    /// Every banana node, nearest first.
    pub fn groves(&self) -> &[Tile] {
        &self.groves
    }

    /// The node the workforce harvests: the nearest, and for the MVP the only
    /// one worked. A second node becomes live when workers can be assigned,
    /// which is the point at which travel stops being one number.
    pub fn worked_grove(&self) -> Tile {
        self.groves[0]
    }

    /// The walk the economy is balanced against.
    pub fn reference_route(&self) -> Route {
        self.route(self.town_centre, self.worked_grove())
            .expect("every grove was proved reachable at parse time")
    }

    /// The shortest walk between two tiles, straightened.
    ///
    /// `None` when either end is jungle or the goal is walled off.
    pub fn route(&self, from: Tile, to: Tile) -> Option<Route> {
        if !self.passable_at(from.x as i64, from.y as i64)
            || !self.passable_at(to.x as i64, to.y as i64)
        {
            return None;
        }
        if from == to {
            return Some(Route::new(vec![from.centre()]));
        }

        let cells = (self.width * self.height) as usize;
        let mut cost = vec![f64::INFINITY; cells];
        let mut came = vec![usize::MAX; cells];
        let mut closed = vec![false; cells];
        let (start, goal) = (self.index(from), self.index(to));
        cost[start] = 0.0;

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
                let neighbour = (ny as usize) * self.width as usize + nx as usize;
                let step = cost[cell] + if diagonal { DIAGONAL } else { 1.0 };
                if step < cost[neighbour] {
                    cost[neighbour] = step;
                    came[neighbour] = cell;
                    open.push(Candidate {
                        estimate: step + heuristic(Tile::new(nx as u32, ny as u32), to),
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
            .map(|cell| Tile::new(self.x_of(cell) as u32, self.y_of(cell) as u32))
            .collect();
        Route::new(self.string_pull(&tiles))
    }

    /// Drop every corner the walk does not need.
    ///
    /// A* on a grid can only turn in 45° steps, so it answers an open-ground
    /// diagonal with a staircase that is both ugly and measurably longer than
    /// the walk it stands for. Pulling the string taut against the jungle
    /// removes the corners and, with them, the error: what the economy is
    /// charged for becomes the distance a monkey actually covers.
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
    /// Bresenham line would pick. The difference matters: string-pulling only
    /// keeps a shortcut this call approves, so a line test that skipped a tile
    /// would approve a shortcut clipping the corner of a jungle block, and the
    /// straightened walk would leave the walkable map.
    fn clear_line(&self, from: Tile, to: Tile) -> bool {
        if !self.passable_at(from.x as i64, from.y as i64) {
            return false;
        }
        let (mut x, mut y) = (from.x as i64, from.y as i64);
        let (goal_x, goal_y) = (to.x as i64, to.y as i64);
        let (run, rise) = (
            f64::from(to.x) - f64::from(from.x),
            f64::from(to.y) - f64::from(from.y),
        );
        let step_x = (run.signum() as i64) * i64::from(run != 0.0);
        let step_y = (rise.signum() as i64) * i64::from(rise != 0.0);

        // Distances along the segment, in units of its own length, to the next
        // tile boundary and between boundaries. The walk starts at a tile
        // centre, so the first boundary is half a tile away.
        let span_x = if run == 0.0 {
            f64::INFINITY
        } else {
            1.0 / run.abs()
        };
        let span_y = if rise == 0.0 {
            f64::INFINITY
        } else {
            1.0 / rise.abs()
        };
        let mut next_x = span_x * 0.5;
        let mut next_y = span_y * 0.5;

        // The segment crosses at most one boundary per tile in each axis, so
        // this bounds the walk without trusting the floating point to land.
        let limit = (goal_x - x).abs() + (goal_y - y).abs() + 2;
        for _ in 0..limit {
            if x == goal_x && y == goal_y {
                return true;
            }
            const GRAZE: f64 = 1e-9;
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
        x == goal_x && y == goal_y
    }

    fn passable_at(&self, x: i64, y: i64) -> bool {
        x >= 0
            && y >= 0
            && x < self.width as i64
            && y < self.height as i64
            && self.terrain[(y as usize) * self.width as usize + x as usize].passable()
    }

    fn index(&self, tile: Tile) -> usize {
        (tile.y * self.width + tile.x) as usize
    }

    fn x_of(&self, cell: usize) -> i64 {
        (cell % self.width as usize) as i64
    }

    fn y_of(&self, cell: usize) -> i64 {
        (cell / self.width as usize) as i64
    }
}

/// Octile distance: the exact cost of the cheapest unobstructed grid walk, so
/// A* never expands a cell it does not have to and never returns a route that
/// is not shortest.
fn heuristic(from: Tile, to: Tile) -> f64 {
    let dx = from.x.abs_diff(to.x) as f64;
    let dy = from.y.abs_diff(to.y) as f64;
    dx.max(dy) + (DIAGONAL - 1.0) * dx.min(dy)
}

/// A min-heap entry.
///
/// `BinaryHeap` is a max-heap, so the ordering is reversed. Ties break on the
/// cell index, which is what makes a route the same route on every run - the
/// headless economy contracts assert exact ticks, and they cannot do that if
/// the travel leg depends on hash or heap order.
struct Candidate {
    estimate: f64,
    cell: usize,
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .partial_cmp(&self.estimate)
            .expect("no NaN in a route estimate")
            .then_with(|| other.cell.cmp(&self.cell))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Candidate {}

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
                let tile = Tile::new((at.x / TILE_METRES) as u32, (at.y / TILE_METRES) as u32);
                map.terrain(tile).passable()
            })
        })
    }

    #[test]
    fn the_reference_route_is_the_economys_travel_leg() {
        // `GROVE_DISTANCE` is a *measurement* of `assets/maps/start.txt` rather
        // than a number somebody chose. Move the town centre or the banana node
        // and this is the test that says the balance moved with them.
        assert_eq!(start().reference_route().length(), GROVE_DISTANCE);
    }

    #[test]
    fn the_starting_map_is_a_town_ringed_by_a_path_and_walled_by_jungle() {
        let start = start();
        assert_eq!((start.width(), start.height()), (69, 69));
        assert_eq!(start.terrain(start.town_centre()), Terrain::Town);
        assert_eq!(start.terrain(Tile::new(34, 34)), Terrain::Town);
        assert_eq!(start.terrain(Tile::new(34, 6)), Terrain::Path);
        assert_eq!(start.terrain(Tile::new(34, 0)), Terrain::Jungle);
        assert_eq!(start.terrain(start.worked_grove()), Terrain::Grove);
        // Off the map is jungle too: the barrier does not stop at the file.
        assert_eq!(start.terrain(Tile::new(69, 0)), Terrain::Jungle);
    }

    #[test]
    fn the_starting_map_offers_two_nodes_and_works_the_nearer() {
        let start = start();
        assert_eq!(start.groves().len(), 2);
        assert_eq!(start.worked_grove(), start.groves()[0]);
        let far = start
            .route(start.town_centre(), start.groves()[1])
            .expect("the second node is reachable");
        assert!(
            far.length() > start.reference_route().length(),
            "the worked grove must be the nearer of the two"
        );
        // The far node's walk turns, so the shipped map exercises the search
        // and the straightening rather than only ever answering a straight
        // line. Losing that would make the whole module untested in situ.
        assert!(far.points().len() > 2, "{far:?} does not turn");
        assert_eq!(start.reference_route().points().len(), 2);
    }

    #[test]
    fn the_shipped_walk_stays_out_of_the_jungle() {
        let start = start();
        for &grove in start.groves() {
            let route = start.route(start.town_centre(), grove).expect("reachable");
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
        // exact ticks, so the tie must break the same way on every run.
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
    }
}
