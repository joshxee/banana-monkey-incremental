//! The drawn art, and the one number that fixes its scale.
//!
//! Every sprite in `assets/` is authored at a single shared pixel scale: the
//! jungle plants, the town centre and the spider worker were all drawn against
//! the same 64x64 worker reference, and their manifests give a ground anchor in
//! art pixels rather than a centre. That is what makes this module small — the
//! art already agrees with itself, so the game needs one conversion from art
//! pixels to world texels and one rule for where a sprite's feet are.
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
    /// The one conversion every prop, badge and seat placed against the art
    /// goes through. Placing them in texels read off the old placeholder is how
    /// a chef's cap came to sit on a face and three cart riders came to stand
    /// on the lid of their cart.
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
/// The bins themselves, lifted out of the treehouse into a sprite of their own
/// (D31).
///
/// The same canvas as the house and the same ground anchor, which is what makes
/// the lift free: drawn on the delivery point the bins land exactly where the
/// artist put them, to the pixel, and
/// `the_treehouse_split_draws_exactly_the_artists_picture` holds that. What it
/// buys is that the bins are now a *place* rather than part of a picture - they
/// sort at the ground they stand on rather than at the house's depth, and a
/// second set can be stood anywhere on the board by the same anchor when the
/// carts arrive.
pub(crate) const BANANA_BINS: Cell = Cell::new((672.0, 704.0), (375.0, 555.0)).shrunk(0.5);
/// How far the bins' art reaches to either side of their anchor, in art pixels.
///
/// Measured off the sprite by `the_bins_are_a_sprite_of_their_own`, not guessed:
/// what parks around the bins is placed against this, so a cart stands clear of
/// the boxes it is unloading into rather than inside one.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BINS_HALF_WIDTH: f32 = 75.0;
/// The middle of the treehouse's opaque art, which the opening view centres
/// on: its bounds are (63, 38) to (574, 606).
pub(crate) const TOWN_CENTRE_MIDDLE: Vec2 = Vec2::new(318.5, 322.0);
/// One frame of the spider worker (see `assets/Monkey/Spider Worker`).
pub(crate) const WORKER: Cell = Cell::new((64.0, 64.0), (32.0, 56.0));
/// And of the squirrel monkey courier (see `assets/Monkey/Squirrel Unpacker`).
///
/// The same 64 x 64 cell and the same ground anchor as the spider worker, which
/// is the artist's doing rather than a coincidence: the two were drawn against
/// each other, and the squirrel is smaller *within* the cell - a 30-pixel body
/// against the worker's 44 - so it needs no scale of its own to read as the
/// smaller animal.
pub(crate) const SQUIRREL: Cell = Cell::new((64.0, 64.0), (32.0, 56.0));

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
/// The hips of a sitting monkey: how deep a rider's legs reach into a seat.
pub(crate) const WORKER_HIP_ROW: f32 = 42.0;
/// The middle of the hunched back, which is where a carried banana rides. The
/// head is forward of it and the tail behind, and both are wrong places for
/// cargo: on the head it reads as a hat, on the tail it floats.
pub(crate) const WORKER_BACK: Vec2 = Vec2::new(31.0, 24.0);

/// How much smaller a cart's rider is drawn than a walker.
pub(crate) const RIDER_SCALE: f32 = 0.72;

/// The banana, in world texels: one art pixel to one texel.
///
/// `assets/Banana` is an icon, not part of the shared-scale set, so it cannot
/// inherit its size from the monkey the way the plants do. A whole number of
/// texels per art pixel is what keeps a 16-pixel sprite from crawling when it
/// moves, and one is the only whole number that leaves it smaller than the
/// monkey carrying it home.
pub(crate) const BANANA_TEXELS: f32 = 16.0;
/// And carried on a monkey's back: exactly half, so it stays on the grid.
pub(crate) const CARRIED_BANANA_TEXELS: f32 = BANANA_TEXELS * 0.5;
/// Frames in the banana's spin.
pub(crate) const BANANA_FRAMES: u32 = 12;
/// The frame the banana rests on: lying on its side, the shape it is recognised
/// by, with no glint - the first half of the spin catches the light and a
/// carried banana frozen on a white flash reads as a gem.
pub(crate) const BANANA_REST_FRAME: u32 = 8;

/// A contact shadow, in world texels: a little wider than a monkey's feet and
/// a third as deep as it is wide, which is the 2:1 ground seen from above.
pub(crate) const SHADOW_TEXELS: Vec2 = Vec2::new(35.0 * ART_SCALE, 12.5 * ART_SCALE);

/// Frames in the walk loop, and in the idle loop.
const WALK_FRAMES: u32 = 12;
const IDLE_FRAMES: u32 = 4;

/// How long each walk frame holds, in the manifest's milliseconds.
///
/// Three lengths repeating, straight from the animation manifest: the quick
/// recoveries and the small pause as the weight transfers are the whole
/// character of a spider monkey's gait, and averaging them out loses it. The
/// walk is no longer *played* against the clock (see [`WALK_STRIDE_TEXELS`]),
/// but the three are kept as the proportion of the stride each frame covers.
const WALK_TIMING: [f32; 3] = [0.070, 0.060, 0.050];
/// And each idle frame, which breathes rather than steps, in seconds.
const IDLE_TIMING: [f32; 4] = [0.300, 0.250, 0.300, 0.250];

/// How far the walk carries a monkey over one whole loop, in world texels
/// along the ground.
///
/// Measured off the sheet: the planted foot slides about twenty art pixels
/// down the 2:1 diagonal over the twelve frames, twenty art pixels of stride, at whatever scale the art is drawn. The
/// first version played the loop against the clock in 0.72 s, which is 11
/// texels a second of stepping against 27 of walking: every monkey on the board
/// skated at two and a half times its own stride, and faster again with every
/// Chef. The playhead is now driven by how far the monkey is *drawn* moving, so
/// the feet grip the ground at any speed, any Chef bonus and any swarm remap.
pub(crate) const WALK_STRIDE_TEXELS: f32 = 20.0 * ART_SCALE;

/// Which loop a monkey is playing.
///
/// Only two, and that is a deliberate limit rather than an oversight. The
/// artist also supplied `rise` and `settle` transitions, but they are one-shot
/// clips that need playback state per monkey and a rule for interrupting them;
/// the two loops carry the reading — moving or not moving — on their own.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Clip {
    Idle,
    Walk,
}

impl Clip {
    pub(crate) fn frames(self) -> u32 {
        match self {
            Self::Idle => IDLE_FRAMES,
            Self::Walk => WALK_FRAMES,
        }
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
}

/// What a squirrel monkey courier is doing.
///
/// Three of the artist's five clips. `take` and `drop` are one-shot
/// interactions with their own feet planted, and playing them needs per-courier
/// playback state and a rule for interrupting them; the three loops carry the
/// reading the game needs - waiting, fetching, delivering - on their own, and
/// the banana in its hands is what says which leg it is on.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Courier {
    Idle,
    Dart,
    Carry,
}

/// Directions each courier sheet is drawn in: N, NE, E, SE, S, SW, W, NW, down
/// the rows, in screen compass bearings.
pub(crate) const COURIER_HEADINGS: u32 = 8;
/// Frames across an idle row, and across a travelling one.
const COURIER_IDLE_FRAMES: u32 = 4;
const COURIER_TRAVEL_FRAMES: u32 = 8;
/// How long each idle frame is held, in seconds (the manifest's 300 ms).
const COURIER_IDLE_HOLD: f32 = 0.300;

/// How far one loop of the courier's dart carries it, in world texels along the
/// ground.
///
/// The manifest's own travel: eight frames of 50 ms at about 150 native pixels
/// a second is 60 art pixels of ground per loop. Driven by distance drawn, like
/// the worker's walk, so the squirrel's feet grip the ground whatever speed the
/// shuttle is running at - and the shuttle's speed is set by how fast the
/// harvester it is helping unloads, which every Chef and Unpacker changes.
pub(crate) const COURIER_STRIDE_TEXELS: f32 = 60.0 * ART_SCALE;

impl Courier {
    pub(crate) fn frames(self) -> u32 {
        match self {
            Self::Idle => COURIER_IDLE_FRAMES,
            Self::Dart | Self::Carry => COURIER_TRAVEL_FRAMES,
        }
    }

    /// How long an idle frame is held, in seconds.
    pub(crate) fn idle_hold() -> f32 {
        COURIER_IDLE_HOLD
    }

    /// Which row of the sheet a screen-space heading is drawn on.
    ///
    /// The rows run N, NE, E, SE, S, SW, W, NW, so this is a bearing clockwise
    /// from up-screen in eighths of a turn. A heading of nothing keeps the row
    /// it was given, which is what stops a courier spinning through all eight
    /// rows in the frame it turns around.
    pub(crate) fn heading(travel: Vec2, previous: u32) -> u32 {
        if travel.length_squared() <= f32::EPSILON {
            return previous % COURIER_HEADINGS;
        }
        let clockwise = std::f32::consts::FRAC_PI_2 - travel.y.atan2(travel.x);
        let eighth = clockwise / (std::f32::consts::TAU / COURIER_HEADINGS as f32);
        (eighth.round().rem_euclid(COURIER_HEADINGS as f32)) as u32
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
    /// And the three collection bins, which stand on the ground in front of it
    /// and sort there rather than at the house's depth (D31).
    pub(crate) banana_bins: Handle<Image>,
    /// The three jungle plants, in the order a scatter picks between them.
    pub(crate) jungle: [Handle<Image>; 3],
    /// The banana plant with its bunch still on: the node workers walk to.
    pub(crate) banana_fruiting: Handle<Image>,
    /// And with the bunch cut: the home tree, whose one banana is the loose
    /// one lying at its foot for the player to pick up.
    pub(crate) banana_harvested: Handle<Image>,
    worker_walk: Handle<Image>,
    worker_idle: Handle<Image>,
    walk_layout: Handle<TextureAtlasLayout>,
    idle_layout: Handle<TextureAtlasLayout>,
    banana: Handle<Image>,
    banana_layout: Handle<TextureAtlasLayout>,
    squirrel_idle: Handle<Image>,
    squirrel_dart: Handle<Image>,
    squirrel_carry: Handle<Image>,
    squirrel_idle_layout: Handle<TextureAtlasLayout>,
    squirrel_travel_layout: Handle<TextureAtlasLayout>,
    /// A flat ellipse, generated rather than drawn: see [`Art::shadow`].
    shadow: Handle<Image>,
}

impl Art {
    pub(crate) fn load(
        assets: &AssetServer,
        layouts: &mut Assets<TextureAtlasLayout>,
        images: &mut Assets<Image>,
    ) -> Self {
        let mut grid = |cell: UVec2, columns: u32, rows: u32| {
            layouts.add(TextureAtlasLayout::from_grid(
                cell, columns, rows, None, None,
            ))
        };
        let worker_cell = WORKER.canvas.as_uvec2();
        let squirrel_cell = SQUIRREL.canvas.as_uvec2();
        Self {
            town_centre: assets.load("TownCenter/town-center-structure.png"),
            town_centre_ground: assets.load("TownCenter/town-center-ground.png"),
            banana_bins: assets.load("TownCenter/town-center-bins.png"),
            jungle: [
                assets.load("Jungle/jungle-broad.png"),
                assets.load("Jungle/jungle-leaning.png"),
                assets.load("Jungle/jungle-fern.png"),
            ],
            banana_fruiting: assets.load("Jungle/banana-fruiting.png"),
            banana_harvested: assets.load("Jungle/banana-harvested.png"),
            worker_walk: assets.load("Monkey/Spider Worker/spider_monkey_walk_sheet.png"),
            worker_idle: assets.load("Monkey/Spider Worker/spider_monkey_idle_sheet.png"),
            walk_layout: grid(worker_cell, WALK_FRAMES, 1),
            idle_layout: grid(worker_cell, IDLE_FRAMES, 1),
            banana: assets.load("Banana/Banana.png"),
            banana_layout: grid(UVec2::splat(BANANA_TEXELS as u32), BANANA_FRAMES, 1),
            squirrel_idle: assets.load("Monkey/Squirrel Unpacker/squirrel-monkey-idle.png"),
            squirrel_dart: assets.load("Monkey/Squirrel Unpacker/squirrel-monkey-dart.png"),
            squirrel_carry: assets.load("Monkey/Squirrel Unpacker/squirrel-monkey-carry.png"),
            squirrel_idle_layout: grid(squirrel_cell, COURIER_IDLE_FRAMES, COURIER_HEADINGS),
            squirrel_travel_layout: grid(squirrel_cell, COURIER_TRAVEL_FRAMES, COURIER_HEADINGS),
            shadow: images.add(shadow_image()),
        }
    }

    /// The image and atlas a clip plays out of.
    pub(crate) fn clip(&self, clip: Clip) -> (Handle<Image>, Handle<TextureAtlasLayout>) {
        match clip {
            Clip::Idle => (self.worker_idle.clone(), self.idle_layout.clone()),
            Clip::Walk => (self.worker_walk.clone(), self.walk_layout.clone()),
        }
    }

    /// A worker sprite on a given frame of a clip.
    pub(crate) fn worker(&self, clip: Clip, frame: u32) -> Sprite {
        let (image, layout) = self.clip(clip);
        Sprite {
            custom_size: Some(WORKER.size()),
            ..Sprite::from_atlas_image(
                image,
                TextureAtlas {
                    layout,
                    index: (frame % clip.frames()) as usize,
                },
            )
        }
    }

    /// A cart's rider: the same monkey, sitting still and drawn smaller so the
    /// crew reads as cargo rather than as three more walkers.
    pub(crate) fn rider(&self, seat: u32) -> Sprite {
        let (image, layout) = self.clip(Clip::Idle);
        Sprite {
            custom_size: Some(WORKER.size() * RIDER_SCALE),
            ..Sprite::from_atlas_image(
                image,
                TextureAtlas {
                    layout,
                    // Frozen, and on a different frame per seat, so three
                    // riders are not one monkey drawn three times.
                    index: (seat % Clip::Idle.frames()) as usize,
                },
            )
        }
    }

    /// The sheet and atlas a courier clip plays out of.
    pub(crate) fn courier_clip(
        &self,
        clip: Courier,
    ) -> (Handle<Image>, Handle<TextureAtlasLayout>) {
        match clip {
            Courier::Idle => (
                self.squirrel_idle.clone(),
                self.squirrel_idle_layout.clone(),
            ),
            Courier::Dart => (
                self.squirrel_dart.clone(),
                self.squirrel_travel_layout.clone(),
            ),
            Courier::Carry => (
                self.squirrel_carry.clone(),
                self.squirrel_travel_layout.clone(),
            ),
        }
    }

    /// Which cell of a courier sheet a heading and a frame name.
    ///
    /// The sheets are a grid rather than a strip - one row per screen bearing -
    /// so an index is a row times a row length, never a bare frame. Reading one
    /// as a strip draws a courier heading north-east while it walks south.
    pub(crate) fn courier_cell(clip: Courier, heading: u32, frame: u32) -> usize {
        let row = heading % COURIER_HEADINGS;
        (row * clip.frames() + frame % clip.frames()) as usize
    }

    /// A squirrel monkey courier, on one frame of one clip, facing one of the
    /// eight bearings its sheets are drawn in.
    ///
    /// No `flip_x`: the artist drew all eight, and the west three already *are*
    /// the mirrored east three with the anchor kept. Mirroring on top of that
    /// would face a courier the wrong way on half the board.
    pub(crate) fn courier(&self, clip: Courier, heading: u32, frame: u32) -> Sprite {
        let (image, layout) = self.courier_clip(clip);
        Sprite {
            custom_size: Some(SQUIRREL.size()),
            ..Sprite::from_atlas_image(
                image,
                TextureAtlas {
                    layout,
                    index: Self::courier_cell(clip, heading, frame),
                },
            )
        }
    }

    /// The banana, drawn `texels` across, on a given frame of its spin.
    pub(crate) fn banana(&self, texels: f32, frame: u32) -> Sprite {
        Sprite {
            custom_size: Some(Vec2::splat(texels)),
            ..Sprite::from_atlas_image(
                self.banana.clone(),
                TextureAtlas {
                    layout: self.banana_layout.clone(),
                    index: (frame % BANANA_FRAMES) as usize,
                },
            )
        }
    }

    /// A flat ellipse on the ground, `size` texels across, in `color`.
    ///
    /// Everything drawn by the artist carries its own shadow and the cast did
    /// not, so the near-black monkeys sat on bright grass like stickers. One
    /// white ellipse, tinted per use: a contact shadow under a walker, and a
    /// coloured disc under a support monkey that says which role it is.
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

    /// The opaque rows of one frame of a horizontal strip, top and bottom.
    fn opaque_rows(sheet: &(u32, u32, Vec<u8>), frame: u32, cell: u32) -> (u32, u32) {
        let (width, height, data) = sheet;
        let alpha = |x: u32, y: u32| data[((y * width + x) * 4 + 3) as usize];
        let rows: Vec<u32> = (0..*height)
            .filter(|&y| (frame * cell..(frame + 1) * cell).any(|x| alpha(x, y) > 0))
            .collect();
        (*rows.first().unwrap(), *rows.last().unwrap())
    }

    #[test]
    fn every_cell_matches_the_file_it_is_drawn_from() {
        // `custom_size` stretches whatever canvas it is given, so a cell that
        // disagrees with its PNG does not fail - it draws a squashed monkey or
        // a plant whose feet are off its anchor. Nothing else would notice.
        for (path, cell, frames) in [
            ("Jungle/banana-fruiting.png", PLANT, 1),
            ("Jungle/banana-harvested.png", PLANT, 1),
            ("Jungle/jungle-broad.png", PLANT, 1),
            ("Jungle/jungle-leaning.png", PLANT, 1),
            ("Jungle/jungle-fern.png", PLANT, 1),
            ("TownCenter/town-center.png", TOWN_CENTRE, 1),
            ("TownCenter/town-center-structure.png", TOWN_CENTRE, 1),
            ("TownCenter/town-center-ground.png", TOWN_CENTRE, 1),
            ("TownCenter/town-center-bins.png", BANANA_BINS, 1),
            (
                "Monkey/Spider Worker/spider_monkey_walk_sheet.png",
                WORKER,
                WALK_FRAMES,
            ),
            (
                "Monkey/Spider Worker/spider_monkey_idle_sheet.png",
                WORKER,
                IDLE_FRAMES,
            ),
        ] {
            let (width, height, _) = png(path);
            assert_eq!(
                Vec2::new(width as f32, height as f32),
                cell.canvas * Vec2::new(frames as f32, 1.0),
                "{path}"
            );
        }
        let (width, height, _) = png("Banana/Banana.png");
        assert_eq!((width, height), (BANANA_TEXELS as u32 * BANANA_FRAMES, 16));
    }

    #[test]
    fn the_worker_is_as_tall_as_the_board_was_tuned_against() {
        // Everything downstream of the art scale is anchored to this: the
        // camera's zoom floor is "a monkey is at least 44 logical pixels", the
        // support fan is measured against a monkey's width, and the swarm's
        // corridor was picked so a crowd of them fits a gap. Measured off the
        // frames that are actually played, every one of them, rather than off
        // a constant: the art may be redrawn, and the monkey must still come
        // out the height the board expects.
        let idle = png("Monkey/Spider Worker/spider_monkey_idle_sheet.png");
        let walk = png("Monkey/Spider Worker/spider_monkey_walk_sheet.png");
        let cell = WORKER.canvas.x as u32;
        for (sheet, frames) in [(&idle, IDLE_FRAMES), (&walk, WALK_FRAMES)] {
            for frame in 0..frames {
                let (top, bottom) = opaque_rows(sheet, frame, cell);
                let drawn = (bottom + 1 - top) as f32 * ART_SCALE;
                assert!(
                    (26.0..=30.0).contains(&drawn),
                    "frame {frame} draws {drawn} texels tall"
                );
                assert!(
                    drawn * 2.0 >= 42.0,
                    "at the zoom floor frame {frame} is {} px, under a thumbnail",
                    drawn * 2.0
                );
            }
        }
        // And the rows things are placed against are where the art has them.
        let (top, _) = opaque_rows(&idle, 0, cell);
        assert_eq!(top as f32, WORKER_TOP_ROW, "the tail tip moved");
        let (width, _, data) = &idle;
        let crown = (0..cell)
            .find(|&y| (40..cell).any(|x| data[((y * width + x) * 4 + 3) as usize] > 0))
            .unwrap();
        assert_eq!(crown as f32, WORKER_CROWN_ROW, "the head moved");
    }

    #[test]
    fn a_ground_anchor_puts_the_art_ground_on_the_transform() {
        // Bevy draws a sprite so that the point `(0.5 + anchor) * size` from
        // its bottom-left sits on the transform. The art says where its ground
        // is from the top-left. Getting that conversion wrong does not look
        // like a bug, it looks like everything hovering - so this checks the
        // geometry rather than restating the formula.
        for cell in [WORKER, PLANT, TOWN_CENTRE] {
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
        let bins = png("TownCenter/town-center-bins.png");
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
            // Nearest first: the bins stand in front of the house, the house in
            // front of its own shade. That is the order the three are drawn in,
            // and it is the artist's own layer order.
            let drawn = [&bins, &structure, &ground]
                .into_iter()
                .map(pixel)
                .find(|layer| layer[3] > 0)
                .unwrap_or([0, 0, 0, 0]);
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

    /// The opaque bounds of a whole PNG: top-left and bottom-right inclusive.
    fn opaque_bounds((width, height, data): &(u32, u32, Vec<u8>)) -> (UVec2, UVec2) {
        let (mut min, mut max) = (UVec2::MAX, UVec2::ZERO);
        for y in 0..*height {
            for x in 0..*width {
                if data[((y * width + x) * 4 + 3) as usize] > 0 {
                    min = min.min(UVec2::new(x, y));
                    max = max.max(UVec2::new(x, y));
                }
            }
        }
        (min, max)
    }

    #[test]
    fn the_bins_are_a_sprite_of_their_own() {
        // What the lift has to be: the three bins, their fruit and the bunch
        // fallen against them, and nothing of the house. The house is timber
        // and foliage - warm, or green - and the bins are cool blue boxes with
        // gold in them, so a stray beam or root that came across with them
        // shows up as a colour that has no business in a bin.
        let bins = png("TownCenter/town-center-bins.png");
        let (min, max) = opaque_bounds(&bins);
        assert_eq!((min, max), (UVec2::new(307, 479), UVec2::new(450, 606)));
        // The anchor is inside those bounds, and the half-width other things
        // are placed against reaches the far edge of them.
        assert!(min.as_vec2().cmple(TOWN_CENTRE_BINS).all());
        assert!(max.as_vec2().cmpge(TOWN_CENTRE_BINS).all());
        assert_eq!(
            BINS_HALF_WIDTH,
            (max.x as f32 - TOWN_CENTRE_BINS.x).max(TOWN_CENTRE_BINS.x - min.x as f32)
        );

        // Every opaque pixel is a bin colour: the box's four cool blues, the
        // three yellows of the fruit, or the brown of a bunch's stem.
        const BIN_COLOURS: [[u8; 3]; 8] = [
            [0x40, 0x52, 0x73],
            [0x6c, 0x81, 0xa1],
            [0x96, 0xa9, 0xc1],
            [0x30, 0x38, 0x43],
            [0xde, 0x9f, 0x47],
            [0xfd, 0xd1, 0x79],
            [0xfe, 0xe1, 0xb8],
            [0x73, 0x4c, 0x44],
        ];
        let (width, height, data) = &bins;
        for y in 0..*height {
            for x in 0..*width {
                let at = ((y * width + x) * 4) as usize;
                if data[at + 3] == 0 {
                    continue;
                }
                let rgb = [data[at], data[at + 1], data[at + 2]];
                assert!(
                    BIN_COLOURS.contains(&rgb),
                    "({x}, {y}) is {rgb:?}, which is no part of a bin"
                );
            }
        }
    }

    #[test]
    fn every_courier_sheet_is_eight_bearings_of_the_squirrel_monkey() {
        // The sheets are grids, not strips: one row per screen bearing. A cell
        // size or a row count that disagrees with the file does not fail, it
        // draws a courier facing north-east while it walks south.
        for (path, frames) in [
            ("Monkey/Squirrel Unpacker/squirrel-monkey-idle.png", 4),
            ("Monkey/Squirrel Unpacker/squirrel-monkey-dart.png", 8),
            ("Monkey/Squirrel Unpacker/squirrel-monkey-carry.png", 8),
        ] {
            let (width, height, _) = png(path);
            assert_eq!(width as f32, SQUIRREL.canvas.x * frames as f32, "{path}");
            assert_eq!(
                height as f32,
                SQUIRREL.canvas.y * COURIER_HEADINGS as f32,
                "{path}"
            );
        }
        // And the courier is the smaller animal at the shared scale: both cells
        // are 64 x 64 with their ground anchor on the same row, so what says
        // "smaller" is how much less of the cell the squirrel fills. Its first
        // cell only - the sheets are grids, and a whole-sheet scan would
        // measure all eight bearings stacked.
        let squirrel = png("Monkey/Squirrel Unpacker/squirrel-monkey-idle.png");
        let top = (0..SQUIRREL.canvas.y as u32)
            .find(|&y| {
                (0..SQUIRREL.canvas.x as u32)
                    .any(|x| squirrel.2[((y * squirrel.0 + x) * 4 + 3) as usize] > 0)
            })
            .expect("the courier's first cell is drawn");
        assert!(
            top as f32 > WORKER_TOP_ROW,
            "the courier tops out at row {top}, no lower than the worker's {WORKER_TOP_ROW}"
        );
        assert_eq!(
            SQUIRREL.ground, WORKER.ground,
            "the two share a ground line"
        );
    }

    #[test]
    fn a_courier_bearing_is_read_clockwise_from_up_the_screen() {
        // The sheets run N, NE, E, SE, S, SW, W, NW down the rows. Getting this
        // backwards is invisible in a still and unmistakable in motion.
        let named = [
            (Vec2::new(0.0, 1.0), 0),
            (Vec2::new(1.0, 1.0), 1),
            (Vec2::new(1.0, 0.0), 2),
            (Vec2::new(1.0, -1.0), 3),
            (Vec2::new(0.0, -1.0), 4),
            (Vec2::new(-1.0, -1.0), 5),
            (Vec2::new(-1.0, 0.0), 6),
            (Vec2::new(-1.0, 1.0), 7),
        ];
        for (travel, row) in named {
            assert_eq!(Courier::heading(travel, 3), row, "{travel:?}");
        }
        // A courier that has stopped keeps the row it was facing rather than
        // snapping north.
        assert_eq!(Courier::heading(Vec2::ZERO, 5), 5);
        // And a cell is a row of the grid, never a bare frame index.
        assert_eq!(Art::courier_cell(Courier::Carry, 3, 2), 3 * 8 + 2);
        assert_eq!(Art::courier_cell(Courier::Idle, 2, 1), 2 * 4 + 1);
    }

    #[test]
    fn the_treehouse_fits_the_tightest_safe_area() {
        // The deviation from the shared scale exists for one reason, so hold
        // it to that reason: its opaque art, at the zoom floor, fits the
        // 286-pixel square an 844x390 phone leaves. Raise the scale and this
        // is what says by how much it now covers the village.
        let whole = png("TownCenter/town-center.png");
        let (width, _, data) = &whole;
        let (width, data) = (*width, data.clone());
        let (min, max) = opaque_bounds(&whole);
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
