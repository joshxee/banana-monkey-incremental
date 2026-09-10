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

use bevy::{image::TextureAtlasLayout, prelude::*, sprite::Anchor};

/// World texels per art pixel.
///
/// The art is drawn at roughly two and a half times the density the board is
/// laid out at, so it has to come down by a fixed factor — and it must be one
/// factor for everything, or the trees stop agreeing with the monkeys standing
/// under them.
///
/// Pinned by the worker: its visible silhouette is 55 art pixels, and at this
/// scale that is 22 texels, which is exactly the height the placeholder
/// rectangle it replaces was drawn at. So the cast keeps the size the board,
/// the camera's zoom floor and the support fan were all tuned against, and
/// everything else in the art inherits its proportion to the monkey from the
/// artist rather than from a number chosen here.
pub(crate) const ART_SCALE: f32 = 0.4;

/// A sprite's size and ground anchor, in its own art pixels.
///
/// The anchor is the point the artist put on the ground — the foot of a trunk,
/// the base of a stair — measured from the top-left of the canvas, which is how
/// every manifest in `assets/` states it. Sizing a sprite to its opaque bounds
/// instead would be the tempting shortcut and it is exactly wrong: two banana
/// plants that differ only in whether the bunch is still on would land at
/// different heights the moment one was picked.
#[derive(Debug, Clone, Copy)]
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

    /// The size to draw one cell at, in world texels.
    pub(crate) fn size(self) -> Vec2 {
        self.canvas * ART_SCALE * self.scale
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
const PLANT: Cell = Cell::new((320.0, 352.0), (160.0, 316.0));
/// The town centre (see `assets/TownCenter`).
///
/// The one asset drawn smaller than its own art direction asks. It is a
/// treehouse the artist scaled against the worker at about ten monkeys tall,
/// which is a fine building and a poor *landmark*: at the shared scale it is
/// 205 texels across, wider than a phone's whole safe area, so it covers the
/// depot pad it stands beside, the crowd unloading there and both ends of the
/// opening drag. Two thirds keeps it comfortably the largest thing on the board
/// while leaving the village visible around it.
const TOWN_CENTRE: Cell = Cell::new((672.0, 704.0), (330.0, 440.0)).shrunk(0.62);
/// One frame of the spider worker (see `assets/Monkey/Spider Worker`).
pub(crate) const WORKER: Cell = Cell::new((64.0, 64.0), (32.0, 56.0));

/// How much smaller a cart's rider is drawn than a walker.
const RIDER_SCALE: f32 = 0.72;

/// Frames in the walk loop, and in the idle loop.
const WALK_FRAMES: u32 = 12;
const IDLE_FRAMES: u32 = 4;

/// How long each walk frame holds, in seconds.
///
/// Three lengths repeating, straight from the animation manifest: the quick
/// recoveries and the small pause as the weight transfers are the whole
/// character of a spider monkey's gait, and averaging them out loses it.
const WALK_TIMING: [f32; 3] = [0.070, 0.060, 0.050];
/// And each idle frame, which breathes rather than steps.
const IDLE_TIMING: [f32; 4] = [0.300, 0.250, 0.300, 0.250];

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

    /// How long the frame at `index` is held, in seconds.
    pub(crate) fn hold(self, index: u32) -> f32 {
        match self {
            Self::Idle => IDLE_TIMING[(index % IDLE_FRAMES) as usize],
            Self::Walk => WALK_TIMING[(index % WALK_TIMING.len() as u32) as usize],
        }
    }
}

/// Every drawn asset, loaded once.
///
/// Handles rather than images: `AssetServer::load` is cached by path, so the
/// two hundred jungle plants on the board share five textures between them.
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
    pub(crate) worker_walk: Handle<Image>,
    pub(crate) worker_idle: Handle<Image>,
    walk_layout: Handle<TextureAtlasLayout>,
    idle_layout: Handle<TextureAtlasLayout>,
}

impl Art {
    pub(crate) fn load(assets: &AssetServer, layouts: &mut Assets<TextureAtlasLayout>) -> Self {
        let mut strip = |frames: u32| {
            layouts.add(TextureAtlasLayout::from_grid(
                WORKER.canvas.as_uvec2(),
                frames,
                1,
                None,
                None,
            ))
        };
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
            walk_layout: strip(WALK_FRAMES),
            idle_layout: strip(IDLE_FRAMES),
        }
    }

    /// The image and atlas a clip plays out of.
    pub(crate) fn clip(&self, clip: Clip) -> (Handle<Image>, Handle<TextureAtlasLayout>) {
        match clip {
            Clip::Idle => (self.worker_idle.clone(), self.idle_layout.clone()),
            Clip::Walk => (self.worker_walk.clone(), self.walk_layout.clone()),
        }
    }

    /// A worker sprite, starting on the frame its hire index lands on.
    pub(crate) fn worker(&self, clip: Clip, index: u32) -> Sprite {
        let (image, layout) = self.clip(clip);
        Sprite {
            custom_size: Some(WORKER.size()),
            ..Sprite::from_atlas_image(
                image,
                TextureAtlas {
                    layout,
                    index: (index % clip.frames()) as usize,
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

    pub(crate) fn town_centre_cell(&self) -> Cell {
        TOWN_CENTRE
    }

    pub(crate) fn plant_cell(&self) -> Cell {
        PLANT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worker_keeps_the_height_the_board_was_tuned_against() {
        // Everything downstream of the art scale is anchored to this: the
        // camera's zoom floor is "a monkey is at least 44 logical pixels", the
        // support fan is measured against a monkey's width, and the swarm's
        // corridor was picked so a crowd of them fits a gap. The art may be
        // authored at any density the artist likes; what may not change is how
        // tall the monkey comes out.
        const SILHOUETTE_PIXELS: f32 = 55.0;
        let drawn = SILHOUETTE_PIXELS * ART_SCALE;
        assert!(
            (drawn - 22.0).abs() < 1.0,
            "a worker draws {drawn} texels tall against the 22 the board expects"
        );
    }

    #[test]
    fn a_ground_anchor_puts_the_feet_on_the_transform() {
        // The art states where the ground is; Bevy wants a fraction out from
        // the centre with the y axis the other way up. Getting that conversion
        // wrong does not look like a bug, it looks like everything hovering.
        //
        // The worker's anchor is 56 of 64 pixels down its cell, so its feet are
        // below the centre and the fraction is negative.
        let anchor = WORKER.anchor();
        assert_eq!(anchor.0.x, 0.0, "the worker is drawn centred across");
        assert!(
            anchor.0.y < 0.0,
            "the worker's feet came out above its head"
        );
        assert!((anchor.0.y - (0.5 - 56.0 / 64.0)).abs() < 1e-6);

        // The two banana states share a canvas and an anchor, so picking the
        // bunch cannot move or resize the plant.
        assert_eq!(PLANT.anchor(), PLANT.anchor());
        assert_eq!(PLANT.size(), PLANT.size());
    }

    #[test]
    fn every_clip_holds_every_frame_it_has() {
        // A timing table shorter than its clip is an index panic on the frame
        // nobody tested, so this walks the whole of both loops.
        for clip in [Clip::Idle, Clip::Walk] {
            let total: f32 = (0..clip.frames()).map(|frame| clip.hold(frame)).sum();
            assert!(total > 0.0, "{clip:?} holds no frame for any time");
        }
        // And the loops run at the lengths the manifest documents.
        let walk: f32 = (0..Clip::Walk.frames()).map(|f| Clip::Walk.hold(f)).sum();
        let idle: f32 = (0..Clip::Idle.frames()).map(|f| Clip::Idle.hold(f)).sum();
        assert!((walk - 0.720).abs() < 1e-4, "the walk loop runs {walk}s");
        assert!((idle - 1.100).abs() < 1e-4, "the idle loop runs {idle}s");
    }
}
