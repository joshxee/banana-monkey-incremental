mod domain;
mod game;
#[cfg(test)]
mod headless;
mod hud;
mod isometric;
mod launch;
mod persistence;
mod scenario;
#[cfg(test)]
mod sim_tests;
mod support;
mod worker;

use bevy::{
    asset::{AssetMetaCheck, AssetPlugin},
    prelude::*,
    window::{PresentMode, WindowResolution},
};
use bevy_flair::prelude::*;

use game::{PresentationPlugin, SimulationPlugin};

fn main() {
    // Before any plugin: `--help` and `--scenarios` print and exit, and a bad
    // argument must not be buried under the audio and GPU start-up log.
    let launch = launch::Launch::from_environment();

    let mut app = App::new();
    app.insert_resource(ClearColor(isometric::BOARD_SKY))
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
        )
        .add_plugins(FlairPlugin);

    // A scenario replaces the save; otherwise the save is the run.
    match &launch.scenario {
        Some(scenario) => scenario.install(&mut app),
        None => scenario::install(
            &mut app,
            persistence::load_run(),
            scenario::Placement::Restored { seed: None },
        ),
    }

    app.insert_resource(launch)
        .add_plugins((SimulationPlugin, PresentationPlugin))
        .run();
}
