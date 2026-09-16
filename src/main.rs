mod art;
mod domain;
mod game;
#[cfg(test)]
mod headless;
mod hud;
mod isometric;
mod launch;
mod map;
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
        None => {
            let resumed = resume();
            scenario::install(
                &mut app,
                resumed.run,
                scenario::Placement::Restored { seed: None },
            );
            app.insert_resource(resumed.welcome);
        }
    }

    app.insert_resource(launch)
        .add_plugins((SimulationPlugin, PresentationPlugin))
        .run();
}

struct Resumed {
    run: persistence::SavedRun,
    welcome: game::WelcomeBack,
}

/// Open the player's save and pay it for the time it was closed.
///
/// The banking order matters and is the whole of this function. The absence is
/// measured from the timestamp *in the save*, so the credited run has to reach
/// the save slot before the game can be closed again - otherwise the next
/// launch measures the same absence from the same timestamp and pays for it
/// twice. Crediting only once the write has succeeded is what makes that
/// impossible rather than merely unlikely, and it is why a failed write costs
/// the player nothing: the absence is still on the clock for next time.
fn resume() -> Resumed {
    resume_from(persistence::load(), persistence::store_run)
}

/// The decision, with the two pieces of I/O handed in, so the banking order can
/// be tested without a save slot to write to.
fn resume_from(
    loaded: persistence::Loaded,
    store: impl FnOnce(persistence::SavedRun) -> Result<(), String>,
) -> Resumed {
    let mut run = loaded.run;
    let mut earned = None;

    if let Some(away) = loaded.away_seconds
        && let Some(offline) = domain::offline_yield(run, away)
    {
        let mut credited = run;
        credited.credit_offline(offline);
        match store(credited) {
            Ok(()) => {
                run = credited;
                earned = Some(offline);
            }
            Err(error) => {
                bevy::log::warn!("Could not bank time away: {error}; leaving it on the clock")
            }
        }
    }

    Resumed {
        run,
        welcome: game::WelcomeBack {
            earned,
            recovery: loaded.recovery,
            // Read off the run as it was *saved*, before the absence was
            // credited into it, which is the camp the panel is describing.
            staff_and_harvesters: (
                loaded.run.staff.total() > 0,
                loaded.run.workforce.count() > loaded.run.carts.crewed()
                    || loaded.run.carts.running() > 0,
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{SavedRun, Treasury, Workforce};
    use std::cell::RefCell;

    fn away(seconds: f64) -> persistence::Loaded {
        persistence::Loaded {
            run: SavedRun {
                treasury: Treasury::from_saved(100.0).unwrap(),
                workforce: Workforce::from_saved(6).unwrap(),
                ..SavedRun::default()
            },
            away_seconds: Some(seconds),
            recovery: None,
        }
    }

    #[test]
    fn an_absence_reaches_the_save_slot_before_it_reaches_the_player() {
        // The absence is measured from the timestamp in the save. If the
        // credited run is not written back, the next launch measures the same
        // absence from the same timestamp - so a night away would pay out
        // again on every reload, for ever.
        let written = RefCell::new(None);
        let resumed = resume_from(away(3_600.0), |run| {
            *written.borrow_mut() = Some(run);
            Ok(())
        });

        assert!(resumed.welcome.earned.is_some());
        assert!(resumed.run.treasury.bananas() > 100.0);
        assert_eq!(
            written.into_inner(),
            Some(resumed.run),
            "the run handed to the player is the run that was banked"
        );
    }

    #[test]
    fn an_absence_that_could_not_be_banked_is_left_on_the_clock() {
        // A failed write must not credit in memory either: the save still
        // carries the old timestamp, so the absence is still owed and will be
        // paid next launch. Crediting anyway would be the same double payment,
        // reachable by anyone who can make a write fail.
        let resumed = resume_from(away(3_600.0), |_| Err("storage is full".into()));

        assert_eq!(resumed.welcome.earned, None);
        assert_eq!(resumed.run.treasury.bananas(), 100.0);
    }

    #[test]
    fn a_launch_with_nothing_to_bank_writes_nothing() {
        // A reload must not rewrite the save on the way in. Doing so would
        // reset the clock the *real* absence is going to be measured from, so
        // a player who opened the tab, saw the wrong build and closed it would
        // lose the night they had already been away.
        let resumed = resume_from(away(5.0), |_| panic!("nothing to bank"));

        assert_eq!(resumed.welcome.earned, None);
        assert_eq!(resumed.run.treasury.bananas(), 100.0);
        assert!(!resumed.welcome.has_news());
    }
}
