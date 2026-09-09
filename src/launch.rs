//! What a run was launched with: a scenario, a clock speed and a view.
//!
//! All three are developer switches. A scenario skips the grind to a state
//! worth looking at, a speed compresses a 50-second harvest cycle into a couple
//! of real seconds, and the stage view drops the HUD so a playtest of the board
//! is only the board. None of them may reach a player: `?scenario=rich` would be
//! a cheat code in the released game, so [`Launch::from_environment`] only
//! reads its environment under the `test-hooks` feature and is a fixed default
//! everywhere else. The parsers themselves are always compiled, so their tests
//! run in every configuration.
//!
//! Two syntaxes carry the same three keys, because the two playtest surfaces
//! have different front doors (`./play` wraps `cargo play`, which is itself an
//! alias for `cargo run --features test-hooks --`, in the Nix shell that holds
//! Bevy's system libraries):
//!
//! ```text
//! ./play --scenario one-worker --speed 5 --view stage
//! http://127.0.0.1:5173/?scenario=one-worker&speed=5&view=stage
//! ```

// The release build reads neither syntax, by design. Both parsers are
// exercised by this module's tests in every configuration.
#![cfg_attr(not(feature = "test-hooks"), allow(dead_code))]

use bevy::prelude::*;

use crate::scenario::{self, Scenario};

/// Slower than this and a playtest is a screensaver.
pub const MIN_SPEED: f64 = 0.1;
/// Beyond this the fixed-step catch-up loop does more work per frame than a
/// frame has time for, and the run stops being faster in wall-clock terms.
pub const MAX_SPEED: f64 = 60.0;

#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct Launch {
    /// A named starting state, already resolved against the registry. `None`
    /// means the player's save.
    pub scenario: Option<Scenario>,
    /// `Time<Virtual>` relative speed. `None` is real time.
    pub speed: Option<f64>,
    pub view: View,
}

/// How much of the presentation to spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// The game: board, banner, store and menu.
    #[default]
    Full,
    /// The board and its actors alone. For looking at one thing.
    Stage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// The caller asked for the usage text, which is not an error but has to
    /// stop the launch the same way one does.
    Help,
    Invalid(String),
}

impl Launch {
    /// `--scenario one-worker --speed 5 --view stage`, with `--key=value` also
    /// accepted. `--help` and `--scenarios` both return [`LaunchError::Help`].
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn from_args<I>(args: I) -> Result<Self, LaunchError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut pairs = Vec::new();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" | "--scenarios" => return Err(LaunchError::Help),
                _ => {}
            }
            let Some(key) = arg.strip_prefix("--") else {
                return Err(LaunchError::Invalid(format!("unexpected argument `{arg}`")));
            };
            if let Some((key, value)) = key.split_once('=') {
                pairs.push((key.to_owned(), value.to_owned()));
            } else {
                let Some(value) = args.next() else {
                    return Err(LaunchError::Invalid(format!("`--{key}` needs a value")));
                };
                pairs.push((key.to_owned(), value));
            }
        }
        Self::from_pairs(pairs)
    }

    /// `?scenario=one-worker&speed=5&view=stage`. Keys this module does not
    /// own are ignored, since a URL can carry anything.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn from_query(search: &str) -> Result<Self, LaunchError> {
        let pairs = search
            .trim_start_matches('?')
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| match pair.split_once('=') {
                Some((key, value)) => (key.to_owned(), value.to_owned()),
                None => (pair.to_owned(), String::new()),
            })
            .filter(|(key, _)| matches!(key.as_str(), "scenario" | "speed" | "view"));
        Self::from_pairs(pairs)
    }

    fn from_pairs<I>(pairs: I) -> Result<Self, LaunchError>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut launch = Self::default();
        for (key, value) in pairs {
            match key.as_str() {
                "scenario" => {
                    launch.scenario = Some(scenario::named(&value).ok_or_else(|| {
                        LaunchError::Invalid(format!("unknown scenario `{value}`"))
                    })?);
                }
                "speed" => {
                    let speed = value.parse::<f64>().ok().filter(|speed| {
                        speed.is_finite() && (MIN_SPEED..=MAX_SPEED).contains(speed)
                    });
                    let Some(speed) = speed else {
                        return Err(LaunchError::Invalid(format!(
                            "speed must be a number from {MIN_SPEED} to {MAX_SPEED}, not `{value}`"
                        )));
                    };
                    launch.speed = Some(speed);
                }
                "view" => {
                    launch.view = match value.as_str() {
                        "full" => View::Full,
                        "stage" => View::Stage,
                        other => {
                            return Err(LaunchError::Invalid(format!(
                                "view must be `full` or `stage`, not `{other}`"
                            )));
                        }
                    };
                }
                other => {
                    return Err(LaunchError::Invalid(format!("unknown option `{other}`")));
                }
            }
        }
        Ok(launch)
    }

    /// The usage text, listing every scenario the registry knows.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn usage() -> String {
        let mut text = String::from(
            "usage: ./play [--scenario NAME] [--speed N] [--view full|stage]\n\
             \n\
             web:   ?scenario=NAME&speed=N&view=full|stage\n\
             \n\
             --scenario  start from a named state instead of the save (saving is off)\n\
             --speed     run the clock N times faster than real time (0.1 to 60)\n\
             --view      `stage` draws the board without the HUD\n\
             \n\
             scenarios:\n",
        );
        let width = scenario::all()
            .iter()
            .map(|scenario| scenario.name.len())
            .max()
            .unwrap_or(0);
        for scenario in scenario::all() {
            text.push_str(&format!(
                "  {:width$}  {}\n",
                scenario.name,
                scenario.summary,
                width = width
            ));
        }
        text
    }

    /// The launch this process was started with. Reads the command line on
    /// native and the page URL on the web, and only under `test-hooks`.
    #[cfg(all(feature = "test-hooks", not(target_arch = "wasm32")))]
    pub fn from_environment() -> Self {
        match Self::from_args(std::env::args().skip(1)) {
            Ok(launch) => launch,
            Err(LaunchError::Help) => {
                print!("{}", Self::usage());
                std::process::exit(0);
            }
            Err(LaunchError::Invalid(message)) => {
                eprintln!("error: {message}\n\n{}", Self::usage());
                std::process::exit(2);
            }
        }
    }

    #[cfg(all(feature = "test-hooks", target_arch = "wasm32"))]
    pub fn from_environment() -> Self {
        let search = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .unwrap_or_default();
        match Self::from_query(&search) {
            Ok(launch) => launch,
            Err(LaunchError::Help) => Self::default(),
            Err(LaunchError::Invalid(message)) => {
                // A page cannot refuse to load, so the run starts from the save
                // and says why. Straight to the console: this runs before the
                // log plugin exists.
                web_sys::console::warn_1(
                    &format!("launch: {message}; starting from the save").into(),
                );
                Self::default()
            }
        }
    }

    #[cfg(not(feature = "test-hooks"))]
    pub fn from_environment() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Launch, LaunchError> {
        Launch::from_args(list.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn the_command_line_and_the_query_string_mean_the_same_thing() {
        let from_args =
            args(&["--scenario", "one-worker", "--speed=5", "--view", "stage"]).unwrap();
        let from_query = Launch::from_query("?scenario=one-worker&speed=5&view=stage").unwrap();

        assert_eq!(from_args, from_query);
        assert_eq!(
            from_args.scenario.as_ref().map(|s| s.name),
            Some("one-worker")
        );
        assert_eq!(from_args.speed, Some(5.0));
        assert_eq!(from_args.view, View::Stage);
    }

    #[test]
    fn nothing_asked_for_is_the_players_own_game() {
        assert_eq!(args(&[]).unwrap(), Launch::default());
        assert_eq!(Launch::from_query("").unwrap(), Launch::default());
        assert_eq!(Launch::from_query("?").unwrap(), Launch::default());
        // A URL carries whatever it likes; only our keys are ours.
        assert_eq!(
            Launch::from_query("?utm_source=x&speed=2").unwrap().speed,
            Some(2.0)
        );
    }

    #[test]
    fn a_scenario_that_does_not_exist_is_refused_by_name() {
        assert_eq!(
            args(&["--scenario", "bogus"]),
            Err(LaunchError::Invalid("unknown scenario `bogus`".into()))
        );
    }

    #[test]
    fn the_speed_is_clamped_to_what_the_fixed_loop_can_keep_up_with() {
        assert!(args(&["--speed", "0.1"]).is_ok());
        assert!(args(&["--speed", "60"]).is_ok());
        assert!(args(&["--speed", "0"]).is_err());
        assert!(args(&["--speed", "61"]).is_err());
        assert!(args(&["--speed", "inf"]).is_err());
        assert!(args(&["--speed", "fast"]).is_err());
    }

    #[test]
    fn help_and_stray_arguments_stop_the_launch() {
        assert_eq!(args(&["--help"]), Err(LaunchError::Help));
        assert_eq!(args(&["--scenarios"]), Err(LaunchError::Help));
        assert!(args(&["--speed"]).is_err());
        assert!(args(&["--view", "sideways"]).is_err());
        assert!(args(&["--colour", "blue"]).is_err());
        assert!(args(&["one-worker"]).is_err());
    }

    #[test]
    fn usage_lists_every_registered_scenario() {
        let usage = Launch::usage();
        for scenario in scenario::all() {
            assert!(usage.contains(scenario.name), "{} missing", scenario.name);
        }
    }
}
