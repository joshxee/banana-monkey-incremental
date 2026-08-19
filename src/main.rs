mod domain;
mod game;
mod hud;
mod persistence;
mod support;
mod worker;

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

    let run = persistence::load_run();
    // The *pool*, not the workforce: crewed monkeys get no avatar, so counting
    // them here leaves the restore budget unspent and the next workers the
    // player actually buys spawn as restored ghosts - placed at a random point
    // on the route, no hire flash, and producing nothing for a full cycle.
    let restored_workers = run.workforce.count().saturating_sub(run.carts.crewed());
    app.insert_resource(run.treasury)
        .insert_resource(run.workforce)
        .insert_resource(run.staff)
        .insert_resource(run.research)
        .insert_resource(run.carts)
        .insert_resource(worker::RestoreWorkers::new(restored_workers))
        .insert_resource(worker::RestoreCarts::new(run.carts.running()))
        .add_plugins(HarvestGamePlugin)
        .run();
}
