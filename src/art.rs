//! The drawn art, and the one number that fixes its scale.
//!
//! Every sprite in `assets/` is authored at a single shared pixel scale: the
//! jungle plants, the town centre, the spider worker, the banana bunch, the
//! banana cart and the ground tiles were all drawn against the same 64x64
//! worker reference, and their manifests give a ground anchor in art pixels
//! rather than a centre. That is what makes this module small — the art already
//! agrees with itself, so the game needs one conversion from art pixels to
//! world texels and one rule for where a sprite's feet are.
//!
//! The manifests are also where every clip's length, frame order and timing
//! come from. The constants here restate them, and the tests read the JSON
//! beside each sheet and hold the two to each other, so a re-export that moves
//! a row or retimes a frame fails here rather than playing the wrong direction.
//!
//! Nothing here decides *where* anything stands. `isometric` and `worker` do
//! that, in metres, exactly as they did when the same things were meshes.

use bevy::{
    asset::RenderAssetUsages,
    image::TextureAtlasLayout,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    sprite::Anchor,
};

use crate::isometric;

/// World texels per art pixel.
///
/// The art is drawn at roughly two and a half times the density the board is
/// laid out at, so it has to come down by a fixed factor — and it must be one
/// factor for everything, or the trees stop agreeing with the monkeys standing
/// under them.
///
/// One half, so that at the camera's zoom floor of 2 **one art pixel is one
/// logical pixel**. That is the whole argument for it: every other ratio drops
/// or doubles rows of the art under nearest-neighbour sampling, and at 0.4 a
/// fifth of the artist's rows and columns simply never reached the screen on a
/// standard-density display. The frames the game plays stand 58 art pixels
/// from tail tip to toe, so a monkey is 29 texels and 58 logical pixels at the
/// floor. Texel constants placed against the monkey are written as art pixels
/// times this, so they move with it.
///
/// The first statement of this constant pinned it to a 55-pixel *standing*
/// study the game never loads (`docs/references/spider-worker.png`), and its
/// test multiplied two literals together. The test now measures the sheet.
pub(crate) const ART_SCALE: f32 = 0.5;

/// A sprite's size and ground anchor, in its own art pixels.
///
/// The anchor is the point the artist put on the ground — the foot of a trunk,
/// the base of a stair — measured from the top-left of the canvas, which is how
/// every manifest in `assets/` states it. Sizing a sprite to its opaque bounds
/// instead would be the tempting shortcut and it is exactly wrong: two banana
/// plants that differ only in whether the bunch is still on would land at
/// different heights the moment one was picked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cell {
    canvas: Vec2,
    ground: Vec2,
    /// A deviation from [`ART_SCALE`], for the one asset that needs one.
    scale: f32,
}

impl Cell {
    const fn new(canvas: (f32, f32), ground: (f32, f32)) -> Self {
        Self {
            canvas: Vec2::new(canvas.0, canvas.1),
            ground: Vec2::new(ground.0, ground.1),
            scale: 1.0,
        }
    }

    /// Draw this one smaller than the art says.
    ///
    /// Used once, and stated as the deviation it is rather than hidden in a
    /// second scale constant. Every other asset inherits its proportion to the
    /// monkey from the artist.
    const fn shrunk(self, scale: f32) -> Self {
        Self { scale, ..self }
    }

    /// World texels per art pixel, for this cell.
    fn texels(self) -> f32 {
        ART_SCALE * self.scale
    }

    /// The size to draw one cell at, in world texels.
    pub(crate) fn size(self) -> Vec2 {
        self.canvas * self.texels()
    }

    /// Where an art pixel of this cell lands, in world texels measured from the
    /// ground anchor, with y running up the screen.
    ///
    /// The one conversion every prop and badge placed against the art goes
    /// through. Placing them in texels read off the old placeholder is how a
    /// chef's cap came to sit on a face.
    pub(crate) fn offset_of(self, art: Vec2) -> Vec2 {
        Vec2::new(art.x - self.ground.x, self.ground.y - art.y) * self.texels()
    }

    /// How far above the ground an art row is drawn, in world texels.
    pub(crate) fn height_above(self, row: f32) -> f32 {
        (self.ground.y - row) * self.texels()
    }

    /// Where the transform sits within the sprite, as Bevy wants it: a fraction
    /// of the size out from the centre, with y running *up* the screen while
    /// the art's own coordinates run down.
    pub(crate) fn anchor(self) -> Anchor {
        Anchor(Vec2::new(
            self.ground.x / self.canvas.x - 0.5,
            0.5 - self.ground.y / self.canvas.y,
        ))
    }
}

/// The plants, all sharing one canvas and one anchor (see `assets/Jungle`).
pub(crate) const PLANT: Cell = Cell::new((320.0, 352.0), (160.0, 316.0));
/// The town centre (see `assets/TownCenter`).
///
/// The one asset drawn smaller than its own art direction asks. It is a
/// treehouse the artist scaled against the worker at about ten monkeys tall,
/// which is a fine building and too big a one for a phone: at the shared scale
/// its opaque art is 256 texels across and 512 logical pixels at the zoom
/// floor, against a landscape phone's 286-pixel safe area. Half is a quarter of
/// a texel per art pixel - a whole-number ratio, so it samples cleanly - and
/// 256 x 284 pixels at the floor, which is exactly what still fits the
/// tightest safe area. `the_treehouse_fits_the_tightest_safe_area` holds that,
/// and is what to read before moving this number.
pub(crate) const TOWN_CENTRE: Cell = Cell::new((672.0, 704.0), (330.0, 440.0)).shrunk(0.5);
/// Where the treehouse's banana bins stand, in its art pixels: the middle of
/// the three at its bottom right, on the ground. Monkeys unload *here*, so
/// this is the point the building is placed by (D30).
pub(crate) const TOWN_CENTRE_BINS: Vec2 = Vec2::new(375.0, 555.0);
/// The middle of the treehouse's opaque art, which the opening view centres
/// on: its bounds are (63, 38) to (574, 606).
pub(crate) const TOWN_CENTRE_MIDDLE: Vec2 = Vec2::new(318.5, 322.0);
/// One frame of the spider worker (see `assets/Monkey/Spider Worker`).
pub(crate) const WORKER: Cell = Cell::new((64.0, 64.0), (32.0, 56.0));
/// One frame of the banana bunch (see `assets/Banana`).
pub(crate) const BUNCH: Cell = Cell::new((48.0, 48.0), (24.0, 36.0));
/// One frame of the banana cart, crew and all (see `assets/BananaCart`).
pub(crate) const CART: Cell = Cell::new((208.0, 176.0), (104.0, 126.0));

/// One ground tile, in art pixels (see `assets/Ground`): a 2:1 diamond on a
/// transparent canvas. At the shared scale it spans two board tiles each way.
pub(crate) const GROUND_TILE: Vec2 = Vec2::new(128.0, 64.0);
/// The packed ground atlas: eight tiles across, sixteen corner masks of four
/// detail variants each, in mask-major order.
const GROUND_ATLAS: Vec2 = Vec2::new(1024.0, 512.0);
const GROUND_COLUMNS: u32 = 8;
const GROUND_VARIANTS: u32 = 4;

/// The top row of the banana plant's crown, in both of its states: how far up
/// the plant a press still means the plant. Measured off the art by
/// `a_press_on_the_plant_reaches_its_crown`.
pub(crate) const PLANT_CROWN_ROW: f32 = 96.0;

/// Rows of the worker's art that things are placed against, measured off the
/// sheets the game plays rather than read off a reference drawing. The tests
/// decode the PNGs and hold each one.
///
/// The tip of the curled tail: the top of the monkey's silhouette.
pub(crate) const WORKER_TOP_ROW: f32 = 1.0;
/// The top of the head, in every idle frame. A hat sits here, not on the tail.
pub(crate) const WORKER_CROWN_ROW: f32 = 14.0;
/// The middle of the hunched back, which is where a banana rides while the
/// monkey stands. The head is forward of it and the tail behind, and both are
/// wrong places for cargo: on the head it reads as a hat, on the tail it
/// floats. A walking monkey carries it in its hand instead, drawn into the
/// carry sheets.
pub(crate) const WORKER_BACK: Vec2 = Vec2::new(31.0, 24.0);

/// The single banana a standing worker carries on its back, in world texels.
///
/// `assets/Banana/Banana.png` is the one piece of the old set still drawn: the
/// carry walk puts a single banana in the monkey's hand, and a whole bunch on
/// the back of a monkey that has just put one down would read as a second,
/// bigger load. One art pixel to half a texel, the shared scale.
pub(crate) const CARRIED_BANANA_TEXELS: f32 = 16.0 * ART_SCALE;
/// Frames in that banana's spin, and the one it rests on: lying on its side,
/// the shape it is recognised by, with no glint.
const BANANA_FRAMES: u32 = 12;
const BANANA_REST_FRAME: u32 = 8;

/// A contact shadow, in world texels: a little wider than a monkey's feet and
/// a third as deep as it is wide, which is the 2:1 ground seen from above.
pub(crate) const SHADOW_TEXELS: Vec2 = Vec2::new(35.0 * ART_SCALE, 12.5 * ART_SCALE);

/// Frames in each walk loop, and in the idle loop.
const WALK_FRAMES: u32 = 12;
const IDLE_FRAMES: u32 = 4;

/// How long each walk frame holds, in the manifest's milliseconds.
///
/// Three lengths repeating, straight from the animation manifest: the quick
/// recoveries and the small pause as the weight transfers are the whole
/// character of a spider monkey's gait, and averaging them out loses it. The
/// walk is not *played* against the clock (see [`WALK_STRIDE_TEXELS`]), but
/// the three are kept as the proportion of the stride each frame covers. All
/// sixteen directional clips share them.
const WALK_TIMING: [f32; 3] = [0.070, 0.060, 0.050];
/// And each idle frame, which breathes rather than steps, in seconds.
const IDLE_TIMING: [f32; 4] = [0.300, 0.250, 0.300, 0.250];

/// How far the walk carries a monkey over one whole loop, in world texels
/// along the south-east diagonal the stride was measured on.
///
/// Measured off the down-right sheet: the planted foot slides about twenty art
/// pixels down the 2:1 diagonal over the twelve frames. The playhead is driven
/// by how far the monkey is *drawn* moving, so the feet grip the ground at any
/// speed, any Chef bonus and any swarm remap - see [`walked_texels`] for how
/// the other seven directions are held to the same stride.
pub(crate) const WALK_STRIDE_TEXELS: f32 = 20.0 * ART_SCALE;

/// How far a step of `ground` metres walks, in texels of the stride.
///
/// A stride is a length on the *ground*: the same monkey takes the same step
/// whichever way it faces, and the eight views are projections of it. On
/// screen the step is foreshortened - a monkey walking straight up the board
/// covers seven tenths of the pixels one walking down the diagonal does - so
/// measuring screen pixels would have a monkey on the north-south walk, which
/// is the shipped route, step a third too slowly and skate. Measured as if the
/// step were taken down the diagonal instead, which is where the stride was
/// read off the art, so the down-right walk is unchanged to the texel.
pub(crate) fn walked_texels(ground: Vec2) -> f32 {
    isometric::project(Vec2::new(ground.length(), 0.0)).length()
}

/// Which of the eight directions the art is drawn in a monkey or a cart faces.
///
/// Screen compass, in the order the sheets' rows run: north is straight up
/// the screen, and the diagonals follow the 2:1 ground axes rather than 45°
/// on screen, so a monkey walking along a tile edge plays a diagonal row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Facing {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Facing {
    /// Every facing, in sheet-row order.
    pub(crate) const ALL: [Self; 8] = [
        Self::N,
        Self::NE,
        Self::E,
        Self::SE,
        Self::S,
        Self::SW,
        Self::W,
        Self::NW,
    ];

    /// The row of a directional sheet this facing is drawn on.
    fn row(self) -> u32 {
        self as u32
    }

    /// The facing for a movement on the screen, with y running up.
    ///
    /// The projection squashes the ground to half its height, so it is
    /// unfolded before the angle is taken: that puts both ground axes on true
    /// diagonals and splits the circle into eight equal sectors around them.
    /// Taking the angle on screen instead would put a monkey walking a few
    /// degrees off a tile edge on a cardinal row.
    pub(crate) fn of(screen: Vec2) -> Self {
        let unfolded = Vec2::new(screen.x, screen.y * 2.0);
        if unfolded.length_squared() <= f32::EPSILON {
            return Self::SE;
        }
        let octant = (unfolded.y.atan2(unfolded.x) / std::f32::consts::FRAC_PI_4).round() as i32;
        match octant.rem_euclid(8) {
            0 => Self::E,
            1 => Self::NE,
            2 => Self::N,
            3 => Self::NW,
            4 => Self::W,
            5 => Self::SW,
            6 => Self::S,
            _ => Self::SE,
        }
    }

    /// The facing for a movement on the ground, in metres.
    pub(crate) fn of_ground(travel: Vec2) -> Self {
        Self::of(isometric::project(travel))
    }

    /// Whether the idle, drawn once facing down-right, is mirrored for this
    /// facing. Only the three that face left: a monkey that walked up or down
    /// the board stops facing right, as the idle is drawn, rather than guessing.
    pub(crate) fn mirrors_idle(self) -> bool {
        matches!(self, Self::SW | Self::W | Self::NW)
    }
}

/// Which loop a monkey is playing.
///
/// The artist also supplied `rise` and `settle` transitions for the idle, but
/// they are drawn facing down-right only and are one-shot clips that need a
/// rule for interrupting them; the loops carry the reading - moving or not,
/// and carrying or not - on their own.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Clip {
    /// Resting on all fours, drawn facing down-right and mirrored for left.
    Idle,
    /// Walking empty-handed, in all eight directions.
    Walk,
    /// Walking with a banana held in one hand, in all eight directions, in
    /// step with [`Clip::Walk`] frame for frame.
    CarryWalk,
}

impl Clip {
    pub(crate) fn frames(self) -> u32 {
        match self {
            Self::Idle => IDLE_FRAMES,
            Self::Walk | Self::CarryWalk => WALK_FRAMES,
        }
    }

    pub(crate) fn is_walk(self) -> bool {
        matches!(self, Self::Walk | Self::CarryWalk)
    }

    /// How long the idle frame at `index` is held, in seconds.
    pub(crate) fn hold(index: u32) -> f32 {
        IDLE_TIMING[(index % IDLE_FRAMES) as usize]
    }

    /// Which walk frame is showing a fraction `stride` of the way through one
    /// loop, keeping the manifest's uneven spacing.
    pub(crate) fn walk_frame(stride: f32) -> u32 {
        let total: f32 = (0..WALK_FRAMES)
            .map(|frame| WALK_TIMING[(frame as usize) % WALK_TIMING.len()])
            .sum();
        let mut left = stride.rem_euclid(1.0) * total;
        for frame in 0..WALK_FRAMES {
            left -= WALK_TIMING[(frame as usize) % WALK_TIMING.len()];
            if left < 0.0 {
                return frame;
            }
        }
        WALK_FRAMES - 1
    }

    /// The atlas index for a frame of this clip in a facing, and whether the
    /// sprite is mirrored to show it.
    ///
    /// The walks are never mirrored at runtime: their three left-facing rows
    /// are the artist's own reflections, already on the sheet. Mirroring a
    /// walk row as well would turn a monkey walking west to face east.
    fn cell(self, facing: Facing, frame: u32) -> (usize, bool) {
        let frame = frame % self.frames();
        match self {
            Self::Idle => (frame as usize, facing.mirrors_idle()),
            Self::Walk | Self::CarryWalk => ((facing.row() * WALK_FRAMES + frame) as usize, false),
        }
    }
}

/// The banana bunch's four clips (see `assets/Banana/banana-bunch.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bunch {
    /// Lying still: held in the hand, or anywhere it should not glint.
    Still,
    /// Growing into place with a brief squash, once, then idle.
    Spawn,
    /// Planted, with a slow glint along the front fruit, looping.
    Idle,
    /// Lifting, shrinking and leaving two flecks, once, then gone.
    Despawn,
}

impl Bunch {
    const ALL: [Self; 4] = [Self::Still, Self::Spawn, Self::Idle, Self::Despawn];

    /// Each frame's hold, in seconds, from the manifest.
    pub(crate) fn durations(self) -> &'static [f32] {
        match self {
            Self::Still => &[1.0],
            Self::Spawn => &[0.05, 0.05, 0.05, 0.05, 0.06, 0.06, 0.07, 0.09, 0.12],
            Self::Idle => &[
                0.9, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.9,
            ],
            Self::Despawn => &[0.06, 0.05, 0.05, 0.05, 0.05, 0.06, 0.07, 0.1],
        }
    }

    /// Whether the clip loops. Spawn and despawn are actions and hold their
    /// last frame; the still is one frame and has nothing to loop.
    pub(crate) fn loops(self) -> bool {
        matches!(self, Self::Idle)
    }

    fn frames(self) -> u32 {
        self.durations().len() as u32
    }

    fn path(self) -> &'static str {
        match self {
            Self::Still => "Banana/banana-bunch-still.png",
            Self::Spawn => "Banana/banana-bunch-spawn-sheet.png",
            Self::Idle => "Banana/banana-bunch-idle-sheet.png",
            Self::Despawn => "Banana/banana-bunch-despawn-sheet.png",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Where a clip with per-frame holds has got to.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Playhead {
    pub(crate) frame: u32,
    elapsed: f32,
}

impl Playhead {
    /// Spend `dt` seconds on a clip whose frames hold `durations`, and say
    /// whether a one-shot clip has finished. A one-shot holds its last frame
    /// rather than wrapping to its first, which for the spawn would flash an
    /// empty cell and for the despawn would bring the bunch back.
    pub(crate) fn advance(&mut self, durations: &[f32], looping: bool, dt: f32) -> bool {
        let last = durations.len() as u32 - 1;
        // Capped, so a frame that arrives after a backgrounded tab is not paid
        // out one animation frame at a time.
        self.elapsed += dt.min(1.0);
        loop {
            let hold = durations[self.frame as usize];
            if self.elapsed < hold {
                return false;
            }
            if self.frame == last && !looping {
                self.elapsed = hold;
                return true;
            }
            self.elapsed -= hold;
            self.frame = if self.frame == last { 0 } else { self.frame + 1 };
        }
    }
}

/// The banana cart's four clips (see `assets/BananaCart/banana-cart.json`).
///
/// The sheets' seams are exact: fill starts on the empty cart's first travel
/// frame and ends on the full cart's, and offload does the reverse, so a cart
/// that parks on travel frame zero can go into and out of either without a
/// pixel moving. `the_cart_clips_meet_pixel_for_pixel` holds that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CartClip {
    /// Rolling with an empty bed, looping.
    TravelEmpty,
    /// Parked, bunches dropping into the bed, once.
    Fill,
    /// Rolling with the bed full, looping, in phase with the empty roll.
    TravelFull,
    /// Parked, the gate open and the bed tipping out, once.
    Offload,
}

impl CartClip {
    const ALL: [Self; 4] = [Self::TravelEmpty, Self::Fill, Self::TravelFull, Self::Offload];

    pub(crate) fn frames(self) -> u32 {
        match self {
            Self::TravelEmpty | Self::TravelFull => 12,
            Self::Fill | Self::Offload => 24,
        }
    }

    /// Every frame's hold, in seconds: the manifest times each clip evenly.
    pub(crate) fn frame_seconds(self) -> f32 {
        match self {
            Self::TravelEmpty | Self::TravelFull => 0.080,
            Self::Fill | Self::Offload => 0.100,
        }
    }

    /// The whole clip, in seconds.
    pub(crate) fn seconds(self) -> f32 {
        self.frames() as f32 * self.frame_seconds()
    }

    fn path(self) -> &'static str {
        match self {
            Self::TravelEmpty => "BananaCart/banana-cart-travel_empty.png",
            Self::Fill => "BananaCart/banana-cart-fill.png",
            Self::TravelFull => "BananaCart/banana-cart-travel_full.png",
            Self::Offload => "BananaCart/banana-cart-offload.png",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The cart's four sheets, loaded the first time a cart is bought.
///
/// Not part of [`Art`], which loads at startup: the four are ninety megabytes
/// of texture once decoded, for a vehicle most sessions never unlock, and a
/// phone pays for every one of them. Held for the rest of the run once loaded,
/// so a cart changing state never waits on a sheet.
#[derive(Resource, Debug, Clone)]
pub(crate) struct CartSheets([Handle<Image>; 4]);

impl CartSheets {
    pub(crate) fn load(assets: &AssetServer) -> Self {
        Self(CartClip::ALL.map(|clip| assets.load(clip.path())))
    }
}

/// Every drawn asset, loaded once.
///
/// Handles rather than images: `AssetServer::load` is cached by path, so the
/// seventy-odd jungle plants on the board share three textures between them.
#[derive(Resource, Debug, Clone)]
pub(crate) struct Art {
    /// The treehouse as it stands: house, deck, stair, tree and bins.
    pub(crate) town_centre: Handle<Image>,
    /// And the shade it casts on the ground, which is drawn flat, under the
    /// depot glow and the crowd's shadows, rather than with the house (D30).
    pub(crate) town_centre_ground: Handle<Image>,
    /// The three jungle plants, in the order a scatter picks between them.
    pub(crate) jungle: [Handle<Image>; 3],
    /// The banana plant with its bunch still on: the node workers walk to.
    pub(crate) banana_fruiting: Handle<Image>,
    /// And with the bunch cut: the home tree, whose bunch is the loose one
    /// lying at its foot for the player to pick up.
    pub(crate) banana_harvested: Handle<Image>,
    /// The packed ground tiles: see [`ground_uv`].
    pub(crate) ground: Handle<Image>,
    worker_idle: Handle<Image>,
    worker_walk: Handle<Image>,
    worker_carry: Handle<Image>,
    idle_layout: Handle<TextureAtlasLayout>,
    walk_layout: Handle<TextureAtlasLayout>,
    bunch: [Handle<Image>; 4],
    bunch_layouts: [Handle<TextureAtlasLayout>; 4],
    cart_layouts: [Handle<TextureAtlasLayout>; 4],
    banana: Handle<Image>,
    banana_layout: Handle<TextureAtlasLayout>,
    /// A flat ellipse, generated rather than drawn: see [`Art::shadow`].
    shadow: Handle<Image>,
}

impl Art {
    pub(crate) fn load(
        assets: &AssetServer,
        layouts: &mut Assets<TextureAtlasLayout>,
        images: &mut Assets<Image>,
    ) -> Self {
        let mut grid = |cell: Vec2, columns: u32, rows: u32| {
            layouts.add(TextureAtlasLayout::from_grid(
                cell.as_uvec2(),
                columns,
                rows,
                None,
                None,
            ))
        };
        let worker = WORKER.canvas;
        let walk_layout = grid(worker, WALK_FRAMES, Facing::ALL.len() as u32);
        let idle_layout = grid(worker, IDLE_FRAMES, 1);
        let bunch_layouts = Bunch::ALL.map(|clip| grid(BUNCH.canvas, clip.frames(), 1));
        let cart_layouts =
            CartClip::ALL.map(|clip| grid(CART.canvas, clip.frames(), Facing::ALL.len() as u32));
        let banana_layout = grid(Vec2::splat(16.0), BANANA_FRAMES, 1);
        Self {
            town_centre: assets.load("TownCenter/town-center-structure.png"),
            town_centre_ground: assets.load("TownCenter/town-center-ground.png"),
            jungle: [
                assets.load("Jungle/jungle-broad.png"),
                assets.load("Jungle/jungle-leaning.png"),
                assets.load("Jungle/jungle-fern.png"),
            ],
            banana_fruiting: assets.load("Jungle/banana-fruiting.png"),
            banana_harvested: assets.load("Jungle/banana-harvested.png"),
            ground: assets.load("Ground/ground-atlas.png"),
            worker_idle: assets.load("Monkey/Spider Worker/spider_monkey_idle_sheet.png"),
            worker_walk: assets.load("Monkey/Spider Worker/spider_monkey_walk_8dir.png"),
            worker_carry: assets.load("Monkey/Spider Worker/spider_monkey_carry_walk_8dir.png"),
            idle_layout,
            walk_layout,
            bunch: Bunch::ALL.map(|clip| assets.load(clip.path())),
            bunch_layouts,
            cart_layouts,
            banana: assets.load("Banana/Banana.png"),
            banana_layout,
            shadow: images.add(shadow_image()),
        }
    }

    /// The image and atlas a clip plays out of.
    pub(crate) fn clip(&self, clip: Clip) -> (Handle<Image>, Handle<TextureAtlasLayout>) {
        match clip {
            Clip::Idle => (self.worker_idle.clone(), self.idle_layout.clone()),
            Clip::Walk => (self.worker_walk.clone(), self.walk_layout.clone()),
            Clip::CarryWalk => (self.worker_carry.clone(), self.walk_layout.clone()),
        }
    }

    /// A worker sprite on a given frame of a clip, facing `facing`.
    pub(crate) fn worker(&self, clip: Clip, facing: Facing, frame: u32) -> Sprite {
        let (image, layout) = self.clip(clip);
        let (index, flip_x) = clip.cell(facing, frame);
        Sprite {
            custom_size: Some(WORKER.size()),
            flip_x,
            ..Sprite::from_atlas_image(image, TextureAtlas { layout, index })
        }
    }

    /// Put a worker sprite on a frame of a clip, facing `facing`, writing only
    /// what changed, so a sprite already showing it is not marked changed.
    pub(crate) fn pose(&self, sprite: &mut Mut<Sprite>, clip: Clip, facing: Facing, frame: u32) {
        let (image, layout) = self.clip(clip);
        let (index, flip_x) = clip.cell(facing, frame);
        set_cell(sprite, image, layout, index);
        if sprite.flip_x != flip_x {
            sprite.flip_x = flip_x;
        }
    }

    /// The banana bunch on a frame of a clip.
    pub(crate) fn bunch(&self, clip: Bunch, frame: u32) -> Sprite {
        Sprite {
            custom_size: Some(BUNCH.size()),
            ..Sprite::from_atlas_image(
                self.bunch[clip.index()].clone(),
                TextureAtlas {
                    layout: self.bunch_layouts[clip.index()].clone(),
                    index: (frame % clip.frames()) as usize,
                },
            )
        }
    }

    /// Put a bunch sprite on a frame of a clip.
    pub(crate) fn pose_bunch(&self, sprite: &mut Mut<Sprite>, clip: Bunch, frame: u32) {
        set_cell(
            sprite,
            self.bunch[clip.index()].clone(),
            self.bunch_layouts[clip.index()].clone(),
            (frame % clip.frames()) as usize,
        );
    }

    /// The cart on a frame of a clip, facing `facing`. Never mirrored: all
    /// eight directions are drawn, each with its crew and cargo layered for
    /// that view.
    pub(crate) fn cart(
        &self,
        sheets: &CartSheets,
        clip: CartClip,
        facing: Facing,
        frame: u32,
    ) -> Sprite {
        Sprite {
            custom_size: Some(CART.size()),
            ..Sprite::from_atlas_image(
                sheets.0[clip.index()].clone(),
                TextureAtlas {
                    layout: self.cart_layouts[clip.index()].clone(),
                    index: cart_index(clip, facing, frame),
                },
            )
        }
    }

    /// Put a cart sprite on a frame of a clip, facing `facing`.
    pub(crate) fn pose_cart(
        &self,
        sprite: &mut Mut<Sprite>,
        sheets: &CartSheets,
        clip: CartClip,
        facing: Facing,
        frame: u32,
    ) {
        set_cell(
            sprite,
            sheets.0[clip.index()].clone(),
            self.cart_layouts[clip.index()].clone(),
            cart_index(clip, facing, frame),
        );
    }

    /// The single banana a standing worker carries on its back.
    pub(crate) fn carried_banana(&self) -> Sprite {
        Sprite {
            custom_size: Some(Vec2::splat(CARRIED_BANANA_TEXELS)),
            ..Sprite::from_atlas_image(
                self.banana.clone(),
                TextureAtlas {
                    layout: self.banana_layout.clone(),
                    index: BANANA_REST_FRAME as usize,
                },
            )
        }
    }

    /// A flat ellipse on the ground, `size` texels across, in `color`.
    ///
    /// The cast's own art carries no shadow, so the near-black monkeys sat on
    /// the ground like stickers. One white ellipse, tinted per use: a contact
    /// shadow under a walker, and a coloured disc under a support monkey that
    /// says which role it is.
    pub(crate) fn shadow(&self, size: Vec2, color: Color) -> Sprite {
        Sprite {
            image: self.shadow.clone(),
            color,
            custom_size: Some(size),
            ..default()
        }
    }

    /// A scenery sprite, sized and anchored at its feet.
    pub(crate) fn standing(&self, image: &Handle<Image>, cell: Cell) -> (Sprite, Anchor) {
        (
            Sprite {
                image: image.clone(),
                custom_size: Some(cell.size()),
                ..default()
            },
            cell.anchor(),
        )
    }
}

/// Point a sprite at one cell of one atlas, writing only what changed. Read
/// through the shared borrow first: reaching for a field through the mutable
/// one marks the sprite changed whether or not anything is written.
fn set_cell(
    sprite: &mut Mut<Sprite>,
    image: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
    index: usize,
) {
    if sprite.image != image {
        sprite.image = image;
    }
    let showing = sprite
        .texture_atlas
        .as_ref()
        .is_some_and(|atlas| atlas.layout == layout && atlas.index == index);
    if !showing {
        sprite.texture_atlas = Some(TextureAtlas { layout, index });
    }
}

/// A cart frame's index in its clip's sheet: one row per facing.
fn cart_index(clip: CartClip, facing: Facing, frame: u32) -> usize {
    (facing.row() * clip.frames() + frame % clip.frames()) as usize
}

/// Where one ground tile sits in the packed atlas, as texture coordinates:
/// its top-left and bottom-right corners.
///
/// `mask` is the tile's four corners, one bit each - top 1, right 2, bottom 4,
/// left 8 - set for dirt and clear for jungle floor; `variant` is one of four
/// patterns of detail over the same edges.
pub(crate) fn ground_uv(mask: u8, variant: u8) -> (Vec2, Vec2) {
    let index = u32::from(mask) * GROUND_VARIANTS + u32::from(variant) % GROUND_VARIANTS;
    let at = Vec2::new(
        (index % GROUND_COLUMNS) as f32 * GROUND_TILE.x,
        (index / GROUND_COLUMNS) as f32 * GROUND_TILE.y,
    );
    (at / GROUND_ATLAS, (at + GROUND_TILE) / GROUND_ATLAS)
}

/// The shadow's texture: a hard-edged ellipse, one art pixel per texel, so it
/// sits on the same grid as the pixel art around it rather than as a soft blur
/// from a different renderer.
fn shadow_image() -> Image {
    let (width, height) = (SHADOW_TEXELS.x as u32, SHADOW_TEXELS.y as u32);
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f32 + 0.5) / width as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / height as f32 * 2.0 - 1.0;
            let alpha = if dx * dx + dy * dy <= 1.0 { 255 } else { 0 };
            data.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};

    /// A shipped PNG, decoded: width, height and RGBA bytes.
    fn png(path: &str) -> (u32, u32, Vec<u8>) {
        let bytes = std::fs::read(format!("{}/assets/{path}", env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|error| panic!("{path}: {error}"));
        let image = Image::from_buffer(
            &bytes,
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::Default,
            RenderAssetUsages::default(),
        )
        .unwrap_or_else(|error| panic!("{path}: {error}"));
        let size = image.size();
        (size.x, size.y, image.data.expect("decoded PNG has pixels"))
    }

    /// A shipped manifest, parsed.
    fn manifest(path: &str) -> serde_json::Value {
        let text = std::fs::read_to_string(format!("{}/assets/{path}", env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|error| panic!("{path}: {error}"));
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"))
    }

    /// One cell of a sheet, as RGBA rows.
    fn cell(sheet: &(u32, u32, Vec<u8>), size: UVec2, column: u32, row: u32) -> Vec<[u8; 4]> {
        let (width, _, data) = sheet;
        let mut pixels = Vec::with_capacity((size.x * size.y) as usize);
        for y in row * size.y..(row + 1) * size.y {
            for x in column * size.x..(column + 1) * size.x {
                let at = ((y * width + x) * 4) as usize;
                pixels.push([data[at], data[at + 1], data[at + 2], data[at + 3]]);
            }
        }
        pixels
    }

    /// The opaque rows of one cell of a sheet, top and bottom.
    fn opaque_rows(sheet: &(u32, u32, Vec<u8>), column: u32, row: u32, size: u32) -> (u32, u32) {
        let (width, _, data) = sheet;
        let alpha = |x: u32, y: u32| data[((y * width + x) * 4 + 3) as usize];
        let rows: Vec<u32> = (row * size..(row + 1) * size)
            .filter(|&y| (column * size..(column + 1) * size).any(|x| alpha(x, y) > 0))
            .map(|y| y - row * size)
            .collect();
        (*rows.first().unwrap(), *rows.last().unwrap())
    }

    const WALK_SHEET: &str = "Monkey/Spider Worker/spider_monkey_walk_8dir.png";
    const CARRY_SHEET: &str = "Monkey/Spider Worker/spider_monkey_carry_walk_8dir.png";
    const IDLE_SHEET: &str = "Monkey/Spider Worker/spider_monkey_idle_sheet.png";

    #[test]
    fn every_cell_matches_the_file_it_is_drawn_from() {
        // `custom_size` stretches whatever canvas it is given, so a cell that
        // disagrees with its PNG does not fail - it draws a squashed monkey or
        // a plant whose feet are off its anchor. Nothing else would notice.
        let mut sheets = vec![
            ("Jungle/banana-fruiting.png", PLANT, 1, 1),
            ("Jungle/banana-harvested.png", PLANT, 1, 1),
            ("Jungle/jungle-broad.png", PLANT, 1, 1),
            ("Jungle/jungle-leaning.png", PLANT, 1, 1),
            ("Jungle/jungle-fern.png", PLANT, 1, 1),
            ("TownCenter/town-center.png", TOWN_CENTRE, 1, 1),
            ("TownCenter/town-center-structure.png", TOWN_CENTRE, 1, 1),
            ("TownCenter/town-center-ground.png", TOWN_CENTRE, 1, 1),
            (WALK_SHEET, WORKER, WALK_FRAMES, 8),
            (CARRY_SHEET, WORKER, WALK_FRAMES, 8),
            (IDLE_SHEET, WORKER, IDLE_FRAMES, 1),
        ];
        for clip in Bunch::ALL {
            sheets.push((clip.path(), BUNCH, clip.frames(), 1));
        }
        for clip in CartClip::ALL {
            sheets.push((clip.path(), CART, clip.frames(), 8));
        }
        for (path, cell, columns, rows) in sheets {
            let (width, height, _) = png(path);
            assert_eq!(
                Vec2::new(width as f32, height as f32),
                cell.canvas * Vec2::new(columns as f32, rows as f32),
                "{path}"
            );
        }
        let (width, height, _) = png("Banana/Banana.png");
        assert_eq!((width, height), (16 * BANANA_FRAMES, 16));
        let (width, height, _) = png("Ground/ground-atlas.png");
        assert_eq!(Vec2::new(width as f32, height as f32), GROUND_ATLAS);
    }

    #[test]
    fn the_walks_play_the_manifests_directions_and_timing() {
        // The row a facing reads is the whole of whether a monkey walks the
        // way it is going. The manifest names every row; hold the enum's
        // order, the sheets and the stride timing to it.
        let walks = manifest("Monkey/Spider Worker/spider_monkey_directional_walks.json");
        assert_eq!(walks["anchor"]["x"], 32);
        assert_eq!(walks["anchor"]["y"], 56);
        let names: Vec<&str> = walks["directions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect();
        let ours: Vec<String> = Facing::ALL.iter().map(|f| format!("{f:?}")).collect();
        assert_eq!(names, ours, "the sheet's rows are in a different order");
        let timing: Vec<f32> = walks["frameDurationMs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|ms| ms.as_f64().unwrap() as f32 / 1000.0)
            .collect();
        assert_eq!(timing.len(), WALK_FRAMES as usize);
        for (frame, hold) in timing.iter().enumerate() {
            assert!((hold - WALK_TIMING[frame % 3]).abs() < 1e-6, "frame {frame}");
        }
        for clip in walks["clips"].as_array().unwrap() {
            let sheet = clip["sheet"].as_str().unwrap();
            let (state, direction) = (clip["state"].as_str().unwrap(), clip["direction"].as_str().unwrap());
            let facing = Facing::ALL
                .into_iter()
                .find(|f| format!("{f:?}") == direction)
                .unwrap();
            let ours = match state {
                "walk" => Clip::Walk,
                "carry_walk" => Clip::CarryWalk,
                other => panic!("an unplayed state {other}"),
            };
            assert!(WALK_SHEET.ends_with(sheet) == (ours == Clip::Walk), "{sheet}");
            for (frame, rect) in clip["frames"].as_array().unwrap().iter().enumerate() {
                let (index, flip) = ours.cell(facing, frame as u32);
                assert!(!flip, "a walk row is mirrored twice");
                let column = index as u64 % u64::from(WALK_FRAMES);
                let row = index as u64 / u64::from(WALK_FRAMES);
                assert_eq!(rect["x"].as_u64().unwrap(), column * 64, "{state} {direction}");
                assert_eq!(rect["y"].as_u64().unwrap(), row * 64, "{state} {direction}");
            }
        }
    }

    #[test]
    fn a_left_facing_walk_is_its_right_facing_twin_reflected() {
        // Which rows are reflections is the one thing the order test cannot
        // see - a swapped pair of names in the manifest and the sheet would
        // still agree with each other. The pixels cannot lie: the artist's
        // west rows are the east rows mirrored about x = 32, so check them.
        for path in [WALK_SHEET, CARRY_SHEET] {
            let sheet = png(path);
            for (right, left) in [(Facing::NE, Facing::NW), (Facing::E, Facing::W), (Facing::SE, Facing::SW)] {
                for frame in 0..WALK_FRAMES {
                    let a = cell(&sheet, UVec2::splat(64), frame, right.row());
                    let b = cell(&sheet, UVec2::splat(64), frame, left.row());
                    for y in 0..64 {
                        for x in 0..64 {
                            assert_eq!(
                                a[y * 64 + x],
                                b[y * 64 + (63 - x)],
                                "{path} {left:?} frame {frame} is not {right:?} reflected"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_monkey_faces_the_way_it_walks_across_the_ground() {
        // The ground axes project onto the screen diagonals, and the sums and
        // differences of them onto the screen's own axes. A route that runs
        // along a tile edge plays a diagonal row, not a cardinal one.
        let cases = [
            (Vec2::new(1.0, 0.0), Facing::SE),
            (Vec2::new(-1.0, 0.0), Facing::NW),
            (Vec2::new(0.0, 1.0), Facing::SW),
            (Vec2::new(0.0, -1.0), Facing::NE),
            (Vec2::new(1.0, 1.0), Facing::S),
            (Vec2::new(-1.0, -1.0), Facing::N),
            (Vec2::new(1.0, -1.0), Facing::E),
            (Vec2::new(-1.0, 1.0), Facing::W),
        ];
        for (ground, facing) in cases {
            assert_eq!(Facing::of_ground(ground), facing, "{ground}");
            // And a walk a little off the line still reads the same way.
            let off = Vec2::from_angle(0.3).rotate(ground);
            assert_eq!(Facing::of_ground(off), facing, "{off}");
        }
        // The shipped route runs from the depot to a grove nineteen tiles
        // across and eighteen up: straight up the board.
        assert_eq!(Facing::of_ground(Vec2::new(-19.0, -18.0)), Facing::N);
        assert_eq!(Facing::of_ground(Vec2::new(19.0, 18.0)), Facing::S);
        // And standing still faces the way the idle is drawn.
        assert_eq!(Facing::of(Vec2::ZERO), Facing::SE);
    }

    #[test]
    fn a_step_is_the_same_length_whichever_way_it_is_taken() {
        // The stride was read off the down-right walk, so that direction is
        // unchanged, and every other direction takes the same step on the
        // ground rather than the same number of pixels on the screen.
        let down_right = Vec2::new(3.0, 0.0);
        assert!((walked_texels(down_right) - isometric::project(down_right).length()).abs() < 1e-4);
        for facing in 0..16 {
            let step = Vec2::from_angle(facing as f32 * std::f32::consts::TAU / 16.0) * 3.0;
            assert!((walked_texels(step) - walked_texels(down_right)).abs() < 1e-4);
        }
    }

    #[test]
    fn the_worker_is_as_tall_as_the_board_was_tuned_against() {
        // Everything downstream of the art scale is anchored to this: the
        // camera's zoom floor is "a monkey is at least 44 logical pixels", the
        // support fan is measured against a monkey's width, and the swarm's
        // corridor was picked so a crowd of them fits a gap. Measured off the
        // frames that are actually played, every one of them in every
        // direction, rather than off a constant.
        let idle = png(IDLE_SHEET);
        let cell = WORKER.canvas.x as u32;
        let mut frames = vec![];
        for frame in 0..IDLE_FRAMES {
            frames.push((&idle, frame, 0));
        }
        let walks = [png(WALK_SHEET), png(CARRY_SHEET)];
        for sheet in &walks {
            for facing in Facing::ALL {
                for frame in 0..WALK_FRAMES {
                    frames.push((sheet, frame, facing.row()));
                }
            }
        }
        for (sheet, column, row) in frames {
            let (top, bottom) = opaque_rows(sheet, column, row, cell);
            let drawn = (bottom + 1 - top) as f32 * ART_SCALE;
            assert!(
                (24.0..=30.0).contains(&drawn),
                "row {row} frame {column} draws {drawn} texels tall"
            );
            assert!(
                drawn * 2.0 >= 42.0,
                "at the zoom floor row {row} frame {column} is {} px, under a thumbnail",
                drawn * 2.0
            );
            // Every direction stands on the manifest's shared anchor row.
            assert!(
                bottom as f32 <= WORKER.ground.y + 2.0,
                "row {row} frame {column} stands below the ground line at {bottom}"
            );
        }
        // And the rows things are placed against are where the art has them.
        let (top, _) = opaque_rows(&idle, 0, 0, cell);
        assert_eq!(top as f32, WORKER_TOP_ROW, "the tail tip moved");
        let (width, _, data) = &idle;
        let crown = (0..cell)
            .find(|&y| (40..cell).any(|x| data[((y * width + x) * 4 + 3) as usize] > 0))
            .unwrap();
        assert_eq!(crown as f32, WORKER_CROWN_ROW, "the head moved");
    }

    #[test]
    fn the_bunch_plays_its_manifest() {
        let bunch = manifest("Banana/banana-bunch.json");
        assert_eq!(bunch["anchor"]["x"], 24);
        assert_eq!(bunch["anchor"]["y"], 36);
        for clip in Bunch::ALL {
            let name = format!("{clip:?}").to_lowercase();
            let entry = &bunch["animations"][&name];
            assert!(BUNCH.canvas.x > 0.0 && Bunch::path(clip).ends_with(entry["image"].as_str().unwrap()));
            assert_eq!(entry["loop"].as_bool().unwrap(), clip.loops(), "{name}");
            let holds: Vec<f32> = entry["frames"]
                .as_array()
                .unwrap()
                .iter()
                .map(|frame| frame["durationMs"].as_f64().unwrap() as f32 / 1000.0)
                .collect();
            assert_eq!(holds.len(), clip.durations().len(), "{name}");
            for (ours, theirs) in clip.durations().iter().zip(&holds) {
                assert!((ours - theirs).abs() < 1e-6, "{name}");
            }
        }
        // And the seams the manifest promises: spawn settles on the still, the
        // idle starts and ends on it, and the despawn leaves nothing behind.
        let still = png(Bunch::Still.path());
        let size = UVec2::splat(48);
        let resting = cell(&still, size, 0, 0);
        let spawn = png(Bunch::Spawn.path());
        assert_eq!(cell(&spawn, size, Bunch::Spawn.frames() - 1, 0), resting);
        let idle = png(Bunch::Idle.path());
        assert_eq!(cell(&idle, size, 0, 0), resting);
        assert_eq!(cell(&idle, size, Bunch::Idle.frames() - 1, 0), resting);
        let despawn = png(Bunch::Despawn.path());
        assert_eq!(cell(&despawn, size, 0, 0), resting);
        assert!(
            cell(&despawn, size, Bunch::Despawn.frames() - 1, 0)
                .iter()
                .all(|pixel| pixel[3] == 0),
            "the despawn ends on a visible frame"
        );
    }

    #[test]
    fn a_one_shot_holds_its_last_frame_and_a_loop_wraps() {
        let mut spawn = Playhead::default();
        let total: f32 = Bunch::Spawn.durations().iter().sum();
        assert!(!spawn.advance(Bunch::Spawn.durations(), false, total * 0.5));
        let mut finished = false;
        for _ in 0..20 {
            finished |= spawn.advance(Bunch::Spawn.durations(), false, 0.1);
        }
        assert!(finished);
        assert_eq!(spawn.frame, Bunch::Spawn.frames() - 1, "a one-shot wrapped");

        let mut idle = Playhead::default();
        let lap: f32 = Bunch::Idle.durations().iter().sum();
        assert!((lap - 2.8).abs() < 1e-4, "the idle runs {lap} s");
        for _ in 0..28 {
            assert!(!idle.advance(Bunch::Idle.durations(), true, 0.1));
        }
        assert_eq!(idle.frame, 0, "a lap of the idle does not come back round");
    }

    #[test]
    fn the_cart_plays_its_manifest() {
        let cart = manifest("BananaCart/banana-cart.json");
        assert_eq!(cart["anchor"]["x"], 104);
        assert_eq!(cart["anchor"]["y"], 126);
        let names: Vec<&str> = cart["directions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect();
        let ours: Vec<String> = Facing::ALL.iter().map(|f| format!("{f:?}")).collect();
        assert_eq!(names, ours);
        for entry in cart["clips"].as_array().unwrap() {
            let clip = match entry["state"].as_str().unwrap() {
                "travel_empty" => CartClip::TravelEmpty,
                "fill" => CartClip::Fill,
                "travel_full" => CartClip::TravelFull,
                "offload" => CartClip::Offload,
                other => panic!("an unplayed state {other}"),
            };
            assert!(clip.path().ends_with(entry["sheet"].as_str().unwrap()));
            assert_eq!(
                entry["loop"].as_bool().unwrap(),
                matches!(clip, CartClip::TravelEmpty | CartClip::TravelFull)
            );
            let direction = entry["direction"].as_str().unwrap();
            let facing = Facing::ALL
                .into_iter()
                .find(|f| format!("{f:?}") == direction)
                .unwrap();
            let frames = entry["frames"].as_array().unwrap();
            assert_eq!(frames.len() as u32, clip.frames());
            for (frame, rect) in frames.iter().enumerate() {
                let index = cart_index(clip, facing, frame as u32) as u64;
                let columns = u64::from(clip.frames());
                assert_eq!(rect["x"].as_u64().unwrap(), index % columns * 208);
                assert_eq!(rect["y"].as_u64().unwrap(), index / columns * 176);
                let hold = rect["durationMs"].as_f64().unwrap() as f32 / 1000.0;
                assert!((hold - clip.frame_seconds()).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn the_cart_clips_meet_pixel_for_pixel() {
        // What lets a parked cart go into and out of its fill and offload
        // without a pop: the one-shots begin and end on the travel loops'
        // first frames, in every direction.
        let sheets = CartClip::ALL.map(|clip| png(clip.path()));
        let size = CART.canvas.as_uvec2();
        let frame = |clip: CartClip, facing: Facing, at: u32| {
            cell(&sheets[clip.index()], size, at, facing.row())
        };
        for facing in Facing::ALL {
            let empty = frame(CartClip::TravelEmpty, facing, 0);
            let full = frame(CartClip::TravelFull, facing, 0);
            assert!(frame(CartClip::Fill, facing, 0) == empty, "{facing:?}: fill starts off");
            assert!(frame(CartClip::Fill, facing, 23) == full, "{facing:?}: fill ends off");
            assert!(frame(CartClip::Offload, facing, 0) == full, "{facing:?}: offload starts off");
            assert!(frame(CartClip::Offload, facing, 23) == empty, "{facing:?}: offload ends off");
        }
    }

    #[test]
    fn the_ground_atlas_is_read_where_its_manifest_packs_it() {
        let ground = manifest("Ground/ground-atlas.json");
        assert_eq!(ground["tileWidth"].as_f64().unwrap() as f32, GROUND_TILE.x);
        assert_eq!(ground["tileHeight"].as_f64().unwrap() as f32, GROUND_TILE.y);
        assert_eq!(ground["cornerBits"]["top"], 1);
        assert_eq!(ground["cornerBits"]["right"], 2);
        assert_eq!(ground["cornerBits"]["bottom"], 4);
        assert_eq!(ground["cornerBits"]["left"], 8);
        let tiles = ground["tiles"].as_array().unwrap();
        assert_eq!(tiles.len(), 64);
        for tile in tiles {
            let mask = tile["mask"].as_u64().unwrap() as u8;
            let variant = tile["variant"].as_u64().unwrap() as u8;
            let (min, max) = ground_uv(mask, variant);
            let x = tile["x"].as_f64().unwrap() as f32;
            let y = tile["y"].as_f64().unwrap() as f32;
            assert_eq!(min * GROUND_ATLAS, Vec2::new(x, y), "mask {mask} variant {variant}");
            assert_eq!(max * GROUND_ATLAS - min * GROUND_ATLAS, GROUND_TILE);
        }
    }

    #[test]
    fn a_ground_anchor_puts_the_art_ground_on_the_transform() {
        // Bevy draws a sprite so that the point `(0.5 + anchor) * size` from
        // its bottom-left sits on the transform. The art says where its ground
        // is from the top-left. Getting that conversion wrong does not look
        // like a bug, it looks like everything hovering - so this checks the
        // geometry rather than restating the formula.
        for cell in [WORKER, PLANT, TOWN_CENTRE, BUNCH, CART] {
            let size = cell.size();
            let from_bottom_left = (Vec2::splat(0.5) + cell.anchor().0) * size;
            let ground = Vec2::new(cell.ground.x, cell.canvas.y - cell.ground.y) * cell.texels();
            assert!(
                from_bottom_left.distance(ground) < 1e-3,
                "{cell:?} puts its transform at {from_bottom_left}, not {ground}"
            );
            // And `offset_of` is measured from the same point.
            assert_eq!(cell.offset_of(cell.ground), Vec2::ZERO);
        }
    }

    #[test]
    fn picking_the_bunch_cannot_move_the_plant() {
        // The two banana states are drawn from one cell, so they can only
        // agree if the artist drew them on the same canvas with the trunk in
        // the same place. Checked on the pixels: the plant's opaque bounds
        // below the bunch must be identical in both.
        let fruiting = png("Jungle/banana-fruiting.png");
        let harvested = png("Jungle/banana-harvested.png");
        let bounds = |(width, height, data): &(u32, u32, Vec<u8>)| {
            let (mut min, mut max) = (UVec2::MAX, UVec2::ZERO);
            // The bottom third: trunk, roots and shadow, where no bunch hangs.
            for y in height * 2 / 3..*height {
                for x in 0..*width {
                    if data[((y * width + x) * 4 + 3) as usize] > 0 {
                        min = min.min(UVec2::new(x, y));
                        max = max.max(UVec2::new(x, y));
                    }
                }
            }
            (min, max)
        };
        assert_eq!(bounds(&fruiting), bounds(&harvested));
    }

    #[test]
    fn a_press_on_the_plant_reaches_its_crown() {
        // The hand-harvest grab reaches up the home plant to this row, so it
        // must be where the art's crown actually tops out - in both states,
        // since picking the bunch must not move what grabs.
        for path in ["Jungle/banana-fruiting.png", "Jungle/banana-harvested.png"] {
            let (width, height, data) = png(path);
            let top = (0..height)
                .find(|&y| (0..width).any(|x| data[((y * width + x) * 4 + 3) as usize] > 0))
                .unwrap();
            assert_eq!(top as f32, PLANT_CROWN_ROW, "{path}");
        }
    }

    #[test]
    fn the_treehouse_split_draws_exactly_the_artists_picture() {
        // The house is drawn as two sprites - its ground paint flat under the
        // glow and the shadows, the structure at the house's depth - exported
        // from the master's own layers. Drawn one over the other they must be
        // the artist's picture, pixel for pixel: nothing lost at the seam, and
        // no ground-layer pixel that the artist painted *over* the house now
        // drawn under it.
        let whole = png("TownCenter/town-center.png");
        let ground = png("TownCenter/town-center-ground.png");
        let structure = png("TownCenter/town-center-structure.png");
        let mut wrong = 0;
        for at in (0..whole.2.len()).step_by(4) {
            let pixel = |image: &(u32, u32, Vec<u8>)| -> [u8; 4] {
                [
                    image.2[at],
                    image.2[at + 1],
                    image.2[at + 2],
                    image.2[at + 3],
                ]
            };
            let drawn = if pixel(&structure)[3] > 0 {
                pixel(&structure)
            } else {
                pixel(&ground)
            };
            let expected = pixel(&whole);
            if (expected[3] > 0 || drawn[3] > 0) && drawn != expected {
                wrong += 1;
            }
        }
        assert_eq!(
            wrong, 0,
            "{wrong} pixels of the treehouse change when it is split"
        );
        // And the ground layer really is on the ground: none of it reaches up
        // past the top of the bins, which are the highest thing standing on
        // the ground in front of the house.
        let (width, height, data) = &ground;
        let top = (0..*height)
            .find(|&y| (0..*width).any(|x| data[((y * width + x) * 4 + 3) as usize] > 0))
            .unwrap();
        assert!(
            top as f32 > TOWN_CENTRE_MIDDLE.y,
            "ground paint reaches art row {top}, up the house"
        );
    }

    #[test]
    fn the_treehouse_fits_the_tightest_safe_area() {
        // The deviation from the shared scale exists for one reason, so hold
        // it to that reason: its opaque art, at the zoom floor, fits the
        // 286-pixel square an 844x390 phone leaves. Raise the scale and this
        // is what says by how much it now covers the village.
        let (width, height, data) = png("TownCenter/town-center.png");
        let (mut min, mut max) = (UVec2::MAX, UVec2::ZERO);
        for y in 0..height {
            for x in 0..width {
                if data[((y * width + x) * 4 + 3) as usize] > 0 {
                    min = min.min(UVec2::new(x, y));
                    max = max.max(UVec2::new(x, y));
                }
            }
        }
        // The middle the opening view centres on is the middle of these.
        assert_eq!((min, max), (UVec2::new(63, 38), UVec2::new(574, 606)));
        assert_eq!(
            TOWN_CENTRE_MIDDLE,
            (min.as_vec2() + max.as_vec2()) * 0.5,
            "the opening view is centred on a middle the art does not have"
        );
        // And the bins are where the constant says: an opaque, cool, dark bin
        // pixel, not grass or the house's shadow - so a redraw that moves them
        // fails here rather than leaving the crowd unloading into the lawn.
        let at = (TOWN_CENTRE_BINS.y as u32 * width + TOWN_CENTRE_BINS.x as u32) as usize * 4;
        let [r, g, b, alpha] = [data[at], data[at + 1], data[at + 2], data[at + 3]];
        assert_eq!(alpha, 255, "the bins point is transparent");
        assert!(
            b > r && b > g && r < 140,
            "the bins point is ({r}, {g}, {b}), not the inside of a bin"
        );
        const MIN_ZOOM: f32 = 2.0;
        const TIGHTEST_SAFE_AREA: f32 = 286.0;
        let drawn = (max - min + UVec2::ONE).as_vec2() * TOWN_CENTRE.texels() * MIN_ZOOM;
        assert!(
            drawn.max_element() <= TIGHTEST_SAFE_AREA,
            "the treehouse draws {drawn} px against a {TIGHTEST_SAFE_AREA} px safe area"
        );
    }

    #[test]
    fn the_walk_covers_every_frame_in_the_manifests_proportions() {
        // The playhead is a fraction of a stride now, not a clock, so what can
        // go wrong is the mapping: a frame skipped, or a frame held for a
        // different share of the stride than the manifest gives it.
        let mut counts = [0u32; WALK_FRAMES as usize];
        const SAMPLES: u32 = 7200;
        for step in 0..SAMPLES {
            counts[Clip::walk_frame(step as f32 / SAMPLES as f32) as usize] += 1;
        }
        for (frame, count) in counts.iter().enumerate() {
            let share = *count as f32 / SAMPLES as f32;
            let wanted = WALK_TIMING[frame % WALK_TIMING.len()] / 0.720;
            assert!(
                (share - wanted).abs() < 2.0 / SAMPLES as f32,
                "walk frame {frame} holds {share} of the stride, not {wanted}"
            );
        }
        // And the stride wraps rather than running off the end of the sheet.
        assert_eq!(Clip::walk_frame(1.0), Clip::walk_frame(0.0));
        assert_eq!(Clip::walk_frame(-0.01), WALK_FRAMES - 1);
    }

    #[test]
    fn every_idle_frame_is_held() {
        // A timing table shorter than its clip is an index panic on the frame
        // nobody tested, so this walks the loop - and holds it to the length
        // the manifest documents, 1.1 s.
        let idle: f32 = (0..Clip::Idle.frames()).map(Clip::hold).sum();
        assert!((idle - 1.100).abs() < 1e-4, "the idle loop runs {idle}s");
    }
}
