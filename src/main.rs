mod domain;
mod game;
mod persistence;

use bevy::{
    asset::{AssetMetaCheck, AssetPlugin},
    prelude::*,
    window::{PresentMode, WindowResolution},
};

use game::HarvestGamePlugin;

fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.42, 0.76, 0.95)))
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    meta_check: AssetMetaCheck::Never,
                    ..default()
                })
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Banana Monkey Incremental".into(),
                        name: Some("banana-monkey-incremental".into()),
                        resolution: WindowResolution::new(1280, 720),
                        present_mode: PresentMode::AutoVsync,
                        canvas: Some("#banana-monkey-canvas".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: false,
                        ..default()
                    }),
                    ..default()
                }),
        );

    let treasury = persistence::load_treasury();
    app.insert_resource(treasury)
        .add_plugins(HarvestGamePlugin)
        .run();
}
