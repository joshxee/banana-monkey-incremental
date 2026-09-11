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
/// Pinned by the worker. The frames the game plays stand 58 art pixels from
/// tail tip to toe — [`WORKER_TOP_ROW`] to the bottom of the cell's opaque
/// rows — which is 23.2 texels here, so a monkey at the camera's zoom floor is
/// 46 logical pixels tall. That is the size the board, the zoom floor and the
/// support fan were tuned against (to within the texel the tail adds), and
/// everything else in the art inherits its proportion to the monkey from the
/// artist rather than from a number chosen here.
///
/// The first statement of this constant pinned it to a 55-pixel *standing*
/// study the game never loads (`docs/references/spider-worker.png`), and its
/// test multiplied two literals together. The test now measures the sheet.
pub(crate) const ART_SCALE: f32 = 0.4;

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
/// which is a fine building and a poor *landmark*: at the shared scale its
/// opaque art is 205 texels across and 410 logical pixels at the zoom floor,
/// against a landscape phone's 286-pixel safe area, so it covers the depot pad
/// it stands beside, the crowd unloading there and both ends of the opening
/// drag. At 0.62 it is 254 x 282 pixels at the zoom floor: the largest thing
/// on the board by far, and exactly what still fits the tightest safe area —
/// `the_treehouse_fits_the_tightest_safe_area` holds that, and is what to read
/// before moving this number.
pub(crate) const TOWN_CENTRE: Cell = Cell::new((672.0, 704.0), (330.0, 440.0)).shrunk(0.62);
/// One frame of the spider worker (see `assets/Monkey/Spider Worker`).
pub(crate) const WORKER: Cell = Cell::new((64.0, 64.0), (32.0, 56.0));

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
pub(crate) const SHADOW_TEXELS: Vec2 = Vec2::new(14.0, 5.0);

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
/// down the 2:1 diagonal over the twelve frames, which is eight texels. The
/// first version played the loop against the clock in 0.72 s, which is 11
/// texels a second of stepping against 27 of walking: every monkey on the board
/// skated at two and a half times its own stride, and faster again with every
/// Chef. The playhead is now driven by how far the monkey is *drawn* moving, so
/// the feet grip the ground at any speed, any Chef bonus and any swarm remap.
pub(crate) const WALK_STRIDE_TEXELS: f32 = 8.0;

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

/// Every drawn asset, loaded once.
///
/// Handles rather than images: `AssetServer::load` is cached by path, so the
/// seventy-odd jungle plants on the board share three textures between them.
#[derive(Resource, Debug, Clone)]
pub(crate) struct Art {
    pub(crate) town_centre: Handle<Image>,
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
    /// A flat ellipse, generated rather than drawn: see [`Art::shadow`].
    shadow: Handle<Image>,
}

impl Art {
    pub(crate) fn load(
        assets: &AssetServer,
        layouts: &mut Assets<TextureAtlasLayout>,
        images: &mut Assets<Image>,
    ) -> Self {
        let mut strip = |cell: UVec2, frames: u32| {
            layouts.add(TextureAtlasLayout::from_grid(cell, frames, 1, None, None))
        };
        let worker_cell = WORKER.canvas.as_uvec2();
        Self {
            town_centre: assets.load("TownCenter/town-center.png"),
            jungle: [
                assets.load("Jungle/jungle-broad.png"),
                assets.load("Jungle/jungle-leaning.png"),
                assets.load("Jungle/jungle-fern.png"),
            ],
            banana_fruiting: assets.load("Jungle/banana-fruiting.png"),
            banana_harvested: assets.load("Jungle/banana-harvested.png"),
            worker_walk: assets.load("Monkey/Spider Worker/spider_monkey_walk_sheet.png"),
            worker_idle: assets.load("Monkey/Spider Worker/spider_monkey_idle_sheet.png"),
            walk_layout: strip(worker_cell, WALK_FRAMES),
            idle_layout: strip(worker_cell, IDLE_FRAMES),
            banana: assets.load("Banana/Banana.png"),
            banana_layout: strip(UVec2::splat(BANANA_TEXELS as u32), BANANA_FRAMES),
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
                    (21.0..=24.0).contains(&drawn),
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
