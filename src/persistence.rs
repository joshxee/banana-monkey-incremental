//! Where a run goes between sessions.
//!
//! The format is built for a game that ships often. Two rules do most of that
//! work, and both are about *not losing a playtester's run*:
//!
//! 1. **Every field is optional on the way in.** Adding a field to the save is
//!    not a schema change: an older payload arrives with that field at its
//!    default, and a payload from a *newer* build arrives with the fields this
//!    build knows about and the rest ignored. So a build can ship without a
//!    migration, and a player who opens yesterday's build by accident still
//!    finds their run. `version` is the one required field, because a payload
//!    without it is not ours and guessing would be worse than refusing.
//! 2. **Nothing unreadable is ever overwritten.** Anything this build cannot
//!    use is copied to a quarantine slot before the game is allowed to save
//!    over it, and the save that *was* loaded is copied to a backup slot once
//!    per launch. A bad build can then cost a session, but never a run.
//!
//! The price of rule 1 is that a mistyped field name reads as a default rather
//! than as an error. That is the right trade here: a save that silently loses
//! one number is recoverable from the quarantine slot, and a save that refuses
//! to load is a playtester who stops playtesting.

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::domain::{Carts, Research, Staff, SupportRole, Treasury, Workforce};

pub use crate::domain::SavedRun;

/// Whether progress reaches the player's save at all.
///
/// A run launched from a named scenario is `Off`: it started from a state the
/// player never earned, and writing it back would overwrite the run they did.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    #[default]
    Live,
    Off,
}

/// Bumped only when a field's *meaning* changes, never when one is added or
/// removed - additive changes are handled by the defaults on [`Save`]. In
/// practice that means this number now moves rarely, which is the point: a
/// version bump is the one change that can strand a build behind or ahead of a
/// player's save.
const SAVE_VERSION: u32 = 4;

/// The build that wrote the save. Informational only - nothing branches on it -
/// but it turns "it broke after the update" into a reproducible report.
const BUILD_STAMP: &str = env!("CARGO_PKG_VERSION");

/// Where a payload lives. The names are a namespace, not a schema version: they
/// deliberately stay at `v1` across format bumps so existing players keep their
/// progress. Change them only to orphan every save on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The run.
    Save,
    /// The run as it was when this session opened. One session of rollback, for
    /// when a new build ruins a save while the player is inside it.
    Backup,
    /// A payload this build could not read at all. Written once and then left
    /// alone, so the *first* loss is the one kept - a second bad load is
    /// usually a consequence of the first, and the earlier payload is the one
    /// that still had the run in it.
    Quarantine,
    /// A payload written by a newer build, which this one can only read
    /// lossily. Separate from [`Slot::Quarantine`], and keep-*newest* rather
    /// than keep-first: these are two different accidents, and a months-old
    /// unreadable save must not be able to fill the slot that is protecting
    /// the run a player made yesterday.
    QuarantineNewer,
}

impl Slot {
    fn key(self) -> &'static str {
        match self {
            Slot::Save => "save-v1",
            Slot::Backup => "save-v1.backup",
            Slot::Quarantine => "save-v1.quarantine",
            Slot::QuarantineNewer => "save-v1.newer",
        }
    }
}

/// What a load found, beyond the run itself.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Loaded {
    pub run: SavedRun,
    /// Wall-clock seconds between the save being written and this launch.
    ///
    /// `None` when the save recorded no time, or when the clock has moved
    /// backwards since - a player whose machine corrects its clock gets no
    /// offline pay for that stretch, which is a great deal better than the
    /// alternative of paying them for it twice.
    pub away_seconds: Option<f64>,
    pub recovery: Option<Recovery>,
}

/// Why the save on disk could not simply be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Unreadable, and the run starts fresh.
    Quarantined {
        /// Whether the raw text actually reached its slot. The panel tells the
        /// player their save was set aside rather than deleted, so this has to
        /// be the truth: promising a playtester a save that was in fact
        /// dropped is worse than the loss itself.
        kept: bool,
    },
    /// Written by a build newer than this one. Loaded for everything this build
    /// understands, with the original kept so the newer build can still have it
    /// back intact.
    FromNewerBuild { kept: bool },
}

/// The payload, exactly as it is written.
///
/// Every field but `version` defaults, which is what makes a build's format
/// change a non-event. `#[serde(default)]` on a field and serde's default of
/// ignoring unknown fields are the two halves of the same promise: old saves
/// open in new builds, and new saves open in old ones.
#[derive(Debug, Deserialize, Serialize)]
struct Save {
    version: u32,
    /// Fractional, because meals are. Never round on the way in or out:
    /// flooring here would burn up to a banana on every reload.
    #[serde(default)]
    bananas: f64,
    #[serde(default)]
    workers: u32,
    #[serde(default)]
    chefs: u32,
    #[serde(default)]
    unpackers: u32,
    #[serde(default)]
    technologists: u32,
    /// Points only. The research *level* is derived from them on load, because
    /// two fields that must agree eventually will not: a tampered save or a
    /// rebalanced growth factor would leave a level its points do not justify,
    /// and nothing would notice.
    #[serde(default)]
    research: f64,
    #[serde(default)]
    carts: u32,
    /// Monkeys aboard, across every cart. One extra number, and it is what
    /// stops a reload either gifting a half-boarded cart its missing crew or
    /// stealing the wait the player has already served.
    #[serde(default)]
    crewed: u32,
    /// Milliseconds since the Unix epoch, as the writing machine saw it. In
    /// milliseconds because that is what the browser hands out, and as `f64`
    /// because that is what JSON numbers are - both ends stay exact well past
    /// any date this game will see.
    #[serde(default)]
    saved_at_ms: f64,
    /// The build that wrote it. Never read by the game.
    #[serde(default)]
    build: String,
}

/// Read the run, and everything the caller needs to know about how it got here.
pub fn load() -> Loaded {
    let raw = match platform::read(Slot::Save) {
        Ok(Some(raw)) => raw,
        Ok(None) => return Loaded::default(),
        Err(error) => {
            // The save may well be fine and the disk merely unreadable, so this
            // is deliberately *not* quarantined: quarantining requires reading
            // the thing first, and writing over a save we failed to read is the
            // exact accident this module exists to prevent. `store` will fail
            // for the same reason and back off.
            bevy::log::warn!("Could not read save data: {error}; starting a fresh run");
            return Loaded::default();
        }
    };

    let Some(save) = parse(&raw) else {
        bevy::log::warn!("Save data is invalid or unsupported; starting a fresh run");
        return Loaded {
            recovery: Some(Recovery::Quarantined {
                kept: keep_first(&raw),
            }),
            ..Loaded::default()
        };
    };

    let Some(run) = restore(&save) else {
        bevy::log::warn!("Save data did not describe a possible run; starting a fresh run");
        return Loaded {
            recovery: Some(Recovery::Quarantined {
                kept: keep_first(&raw),
            }),
            ..Loaded::default()
        };
    };

    let recovery = (save.version > SAVE_VERSION).then(|| {
        bevy::log::warn!(
            "Save was written by a newer build (format {} against this build's {SAVE_VERSION}); \
             loading what this build understands and keeping the original",
            save.version
        );
        Recovery::FromNewerBuild {
            kept: keep_newest(&raw),
        }
    });

    // One write per launch, and only of a payload that just proved it loads.
    if let Err(error) = platform::write(Slot::Backup, &raw) {
        bevy::log::warn!("Could not write the session backup: {error}");
    }

    Loaded {
        run,
        away_seconds: away_seconds(&save),
        recovery,
    }
}

pub fn store_run(run: SavedRun) -> Result<(), String> {
    platform::write(Slot::Save, &encode(run, platform::now_ms()))
}

/// The run as text the player can keep. Encoded from live state rather than
/// read back from the save slot, so what they copy is what is on their screen.
pub fn export(run: SavedRun) -> String {
    encode(run, platform::now_ms())
}

/// A run from text the player pasted.
///
/// Deliberately the same door as [`load`]: an import is a save from somewhere
/// else, and the validation that protects the game from a tampered save slot
/// protects it from a tampered clipboard too.
pub fn import(raw: &str) -> Option<SavedRun> {
    let raw = raw.trim();
    // Stricter than the save slot, and deliberately so. The defaults that let a
    // truncated save still open also mean `{"version":4}` restores as a valid
    // empty run - which is correct for a file we wrote and wrong for text a
    // player pasted, where it would report success and zero their progress. A
    // save slot is ours; a clipboard is anyone's.
    names_a_run(raw).then(|| restore(&parse(raw)?)).flatten()
}

/// Whether the payload mentions any part of a run at all, as opposed to merely
/// being shaped like one.
fn names_a_run(raw: &str) -> bool {
    let Ok(serde_json::Value::Object(fields)) = serde_json::from_str::<serde_json::Value>(raw)
    else {
        return false;
    };
    [
        "bananas",
        "workers",
        "chefs",
        "unpackers",
        "technologists",
        "research",
        "carts",
        "crewed",
    ]
    .iter()
    .any(|field| fields.contains_key(*field))
}

/// Hold an unreadable payload, unless one is already held.
///
/// Returns whether *this* payload is now held, which is what the player is
/// told. An earlier loss occupying the slot means the answer is no.
fn keep_first(raw: &str) -> bool {
    match platform::read(Slot::Quarantine) {
        Ok(Some(held)) => {
            bevy::log::warn!("A quarantined save is already held; keeping that one");
            held == raw
        }
        Ok(None) => store_aside(Slot::Quarantine, raw),
        Err(error) => {
            bevy::log::warn!("Could not check the quarantine slot: {error}");
            false
        }
    }
}

/// Hold a newer build's payload, replacing whatever was there.
///
/// Keep-newest, unlike [`keep_first`]: the run a player made in yesterday's
/// build is worth more than the one they made in last month's, and this slot
/// is read by going *forward* to the newer build rather than back.
fn keep_newest(raw: &str) -> bool {
    store_aside(Slot::QuarantineNewer, raw)
}

fn store_aside(slot: Slot, raw: &str) -> bool {
    match platform::write(slot, raw) {
        Ok(()) => {
            bevy::log::warn!("The save has been kept at {}", platform::describe(slot));
            true
        }
        Err(error) => {
            bevy::log::warn!("Could not set the save aside: {error}");
            false
        }
    }
}

fn away_seconds(save: &Save) -> Option<f64> {
    // Zero is "no time recorded", which is what every pre-v4 save arrives with.
    if !(save.saved_at_ms.is_finite() && save.saved_at_ms > 0.0) {
        return None;
    }
    let seconds = (platform::now_ms() - save.saved_at_ms) / 1000.0;
    (seconds.is_finite() && seconds >= 0.0).then_some(seconds)
}

fn encode(run: SavedRun, saved_at_ms: f64) -> String {
    serde_json::to_string(&Save {
        version: SAVE_VERSION,
        bananas: run.treasury.bananas(),
        workers: run.workforce.count(),
        chefs: run.staff.count(SupportRole::Chef),
        unpackers: run.staff.count(SupportRole::Unpacker),
        technologists: run.staff.count(SupportRole::Technologist),
        research: run.research.points(),
        carts: run.carts.owned(),
        crewed: run.carts.crewed(),
        saved_at_ms,
        build: BUILD_STAMP.to_string(),
    })
    .expect("valid run state always serializes")
}

/// Text to payload. Rejects only what is not one of our saves at all.
fn parse(raw: &str) -> Option<Save> {
    let save: Save = serde_json::from_str(raw).ok()?;
    // Version zero is what a payload with no `version` at all would get if the
    // field ever became optional; refusing it keeps that door shut.
    (save.version > 0).then_some(save)
}

/// Payload to run. Rejects what could not have been a real run.
///
/// Every count is validated rather than trusted: local storage is
/// player-writable, and an unchecked count spawns that many entities in a
/// single tick and is then re-persisted, so the tab never recovers.
fn restore(save: &Save) -> Option<SavedRun> {
    Some(SavedRun {
        treasury: Treasury::from_saved(save.bananas)?,
        workforce: Workforce::from_saved(save.workers)?,
        staff: Staff::from_saved(save.chefs, save.unpackers, save.technologists)?,
        research: Research::from_saved(save.research)?,
        // Cross-field, so it cannot live inside `Carts::from_saved`: a crew
        // larger than the workforce would put a running cart on an empty
        // payroll, harvesting for free.
        carts: (save.crewed <= save.workers)
            .then(|| Carts::from_saved(save.carts, save.crewed))
            .flatten()?,
    })
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use atomic_write_file::AtomicWriteFile;
    use directories::ProjectDirs;

    use super::Slot;

    pub fn read(slot: Slot) -> Result<Option<String>, String> {
        match fs::read_to_string(slot_path(slot)?) {
            Ok(raw) => Ok(Some(raw)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn write(slot: Slot, raw: &str) -> Result<(), String> {
        let path = slot_path(slot)?;
        let parent = path
            .parent()
            .ok_or_else(|| "save path has no parent directory".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;

        atomic_write(&path, raw.as_bytes())
    }

    pub fn describe(slot: Slot) -> String {
        slot_path(slot).map_or_else(|error| error, |path| path.display().to_string())
    }

    pub fn now_ms() -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |since| since.as_millis() as f64)
    }

    fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
        let mut file = AtomicWriteFile::open(path).map_err(|error| error.to_string())?;
        file.write_all(contents)
            .map_err(|error| error.to_string())?;
        file.commit().map_err(|error| error.to_string())
    }

    fn slot_path(slot: Slot) -> Result<PathBuf, String> {
        ProjectDirs::from("com", "Banana Monkey", "Banana Monkey Incremental")
            .map(|directories| {
                directories
                    .data_local_dir()
                    .join(format!("{}.json", slot.key()))
            })
            .ok_or_else(|| "platform application-data directory is unavailable".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use wasm_bindgen::JsValue;

    use super::Slot;

    const KEY_PREFIX: &str = "banana-monkey-incremental.";

    pub fn read(slot: Slot) -> Result<Option<String>, String> {
        storage()?.get_item(&key(slot)).map_err(format_js_error)
    }

    pub fn write(slot: Slot, raw: &str) -> Result<(), String> {
        storage()?
            .set_item(&key(slot), raw)
            .map_err(format_js_error)
    }

    pub fn describe(slot: Slot) -> String {
        format!("localStorage[\"{}\"]", key(slot))
    }

    pub fn now_ms() -> f64 {
        // Wall clock, not `performance.now()`: the question is how long the tab
        // was closed, and a monotonic clock that only ticks while the page is
        // open answers the opposite question.
        js_sys::Date::now()
    }

    fn key(slot: Slot) -> String {
        format!("{KEY_PREFIX}{}", slot.key())
    }

    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .ok_or_else(|| "browser window is unavailable".to_string())?
            .local_storage()
            .map_err(format_js_error)?
            .ok_or_else(|| "browser localStorage is unavailable".to_string())
    }

    fn format_js_error(error: JsValue) -> String {
        format!("{error:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MAX_SAFE_BANANAS;

    fn run(bananas: f64, workers: u32) -> SavedRun {
        staffed(bananas, workers, 0, 0, 0)
    }

    fn staffed(bananas: f64, workers: u32, c: u32, u: u32, x: u32) -> SavedRun {
        researched(bananas, workers, c, u, x, 0.0)
    }

    fn researched(bananas: f64, workers: u32, c: u32, u: u32, x: u32, r: f64) -> SavedRun {
        SavedRun {
            treasury: Treasury::from_saved(bananas).unwrap(),
            workforce: Workforce::from_saved(workers).unwrap(),
            staff: Staff::from_saved(c, u, x).unwrap(),
            research: Research::from_saved(r).unwrap(),
            carts: Carts::default(),
        }
    }

    /// The whole load path bar the storage slot, which is what the platform
    /// modules own and what the e2e suite exercises for real.
    fn decode(raw: &str) -> Option<SavedRun> {
        restore(&parse(raw)?)
    }

    fn written(run: SavedRun) -> String {
        encode(run, 0.0)
    }

    #[test]
    fn save_round_trip_preserves_the_run() {
        let mut saved = researched(123.0, 7, 2, 3, 1, 145.5);
        // Two carts running and a third halfway through boarding.
        saved.carts = Carts::from_saved(3, 7).unwrap();

        assert_eq!(decode(&written(saved)), Some(saved));
        assert_eq!(decode(&written(saved)).unwrap().carts.running(), 2);
        assert_eq!(decode(&written(saved)).unwrap().carts.berths_open(), 2);
        // The level has to come back with the points that bought it: level 0
        // costs 60 and level 1 costs 132, so 145.5 has bought exactly one.
        assert_eq!(saved.research.level(), 1);
        assert_eq!(decode(&written(saved)).unwrap().research.level(), 1);
    }

    #[test]
    fn fractional_bananas_survive_a_round_trip_exactly() {
        // Wages leave the treasury fractional, and flooring anywhere on this
        // path would quietly burn production on every reload.
        let saved = run(12.345_678_9, 3);

        assert_eq!(decode(&written(saved)), Some(saved));
    }

    #[test]
    fn an_exported_run_imports_as_itself() {
        let saved = researched(880.5, 12, 1, 2, 3, 61.0);

        assert_eq!(import(&export(saved)), Some(saved));
        // Players copy text, and text picks up whitespace.
        assert_eq!(import(&format!("  {}\n", export(saved))), Some(saved));
    }

    #[test]
    fn nonsense_does_not_import() {
        assert_eq!(import(""), None);
        assert_eq!(import("hello"), None);
        assert_eq!(import(r#"{"version":4,"bananas":-1}"#), None);
    }

    // ── the two promises the format makes to a build that ships often ──

    #[test]
    fn a_save_from_an_older_build_keeps_everything_that_build_had() {
        // v1: whole bananas, before workers existed.
        assert_eq!(decode(r#"{"version":1,"bananas":42}"#), Some(run(42.0, 0)));
        // v2: workers, before support staff existed.
        assert_eq!(
            decode(r#"{"version":2,"bananas":12.5,"workers":6}"#),
            Some(staffed(12.5, 6, 0, 0, 0))
        );
        // v3: everything but the clock. No migration code runs for any of
        // these - the fields they omit simply default - which is the point.
        assert_eq!(
            decode(
                r#"{"version":3,"bananas":9.5,"workers":4,"chefs":1,"unpackers":0,
                    "technologists":2,"research":61,"carts":0,"crewed":0}"#
            ),
            Some(researched(9.5, 4, 1, 0, 2, 61.0))
        );
    }

    #[test]
    fn a_save_from_a_newer_build_still_opens_here() {
        // A future build's format, with a field this one has never heard of.
        // The run has to survive: a playtester who opens yesterday's build must
        // not lose it. What this build cannot represent is dropped from the
        // *loaded* run only - `load` quarantines the original.
        let future = r#"{"version":9,"bananas":500,"workers":8,"chefs":1,"unpackers":1,
                         "technologists":1,"research":61,"carts":2,"crewed":6,
                         "saved_at_ms":1,"build":"9.9.9","zoos":3}"#;
        let restored = decode(future).expect("a newer save still restores");

        assert_eq!(restored.treasury.bananas(), 500.0);
        assert_eq!(restored.workforce.count(), 8);
        assert_eq!(restored.carts.running(), 2);
    }

    #[test]
    fn a_missing_field_is_a_default_and_not_a_refusal() {
        // The rule that lets a build add a field without writing a migration.
        assert_eq!(decode(r#"{"version":4,"bananas":3}"#), Some(run(3.0, 0)));
        assert_eq!(decode(r#"{"version":4,"workers":1}"#), Some(run(0.0, 1)));
        assert_eq!(decode(r#"{"version":4}"#), Some(SavedRun::default()));
    }

    #[test]
    fn a_payload_that_is_not_ours_is_refused() {
        // `version` is the one field with no default, so anything without it -
        // another game's storage key, a half-written file, a stray string - is
        // refused rather than read as an empty run.
        assert_eq!(decode("{}"), None);
        assert_eq!(decode(r#"{"bananas":3,"workers":1}"#), None);
        assert_eq!(decode(r#"{"version":0,"bananas":3}"#), None);
        assert_eq!(decode("not json"), None);
        assert_eq!(decode("[]"), None);
    }

    #[test]
    fn invalid_numeric_states_are_rejected() {
        assert_eq!(decode(r#"{"version":4,"bananas":-1,"workers":0}"#), None);
        assert_eq!(decode(r#"{"version":4,"bananas":null,"workers":0}"#), None);
        assert_eq!(decode(r#"{"version":4,"bananas":1,"workers":-1}"#), None);
        // Local storage is player-writable, so an absurd worker count has to be
        // rejected rather than spawned.
        assert_eq!(
            decode(r#"{"version":4,"bananas":1,"workers":4000000000}"#),
            None
        );
        assert_eq!(
            decode(&format!(
                r#"{{"version":4,"bananas":{},"workers":0}}"#,
                MAX_SAFE_BANANAS + 2.0
            )),
            None
        );
        assert_eq!(decode(r#"{"version":4,"bananas":1e999,"workers":0}"#), None);
        assert_eq!(decode(r#"{"version":4,"research":-1}"#), None);
        // Support counts are player-writable too, and each one drives a spawn
        // loop of its own.
        let staffed_payload = |c: &str| format!(r#"{{"version":4,"bananas":1,"chefs":{c}}}"#);
        assert_eq!(decode(&staffed_payload("-1")), None);
        assert_eq!(decode(&staffed_payload("4000000000")), None);
        assert!(decode(&staffed_payload("3")).is_some());
        // A crew larger than the berths it could sit in is a tampered save.
        let crewed = |carts: u32, crewed: u32| {
            format!(r#"{{"version":4,"bananas":1,"workers":9,"carts":{carts},"crewed":{crewed}}}"#)
        };
        assert!(decode(&crewed(2, 6)).is_some());
        assert_eq!(decode(&crewed(2, 7)), None);
        assert_eq!(decode(&crewed(0, 1)), None);
        // Nine workers is exactly enough for three carts, and not enough for
        // four - a crew the workforce cannot supply is a tampered save.
        assert!(decode(&crewed(3, 9)).is_some());
        assert_eq!(decode(&crewed(4, 10)), None);
    }

    // ── the clock ──

    #[test]
    fn a_save_records_when_it_was_written() {
        let raw = encode(run(5.0, 1), 1_700_000_000_000.0);
        let save = parse(&raw).unwrap();

        assert_eq!(save.saved_at_ms, 1_700_000_000_000.0);
        assert_eq!(save.build, BUILD_STAMP);
    }

    #[test]
    fn a_clock_that_is_missing_or_has_gone_backwards_pays_nothing() {
        let no_clock = parse(r#"{"version":3,"bananas":1}"#).unwrap();
        assert_eq!(away_seconds(&no_clock), None);

        // Every pre-v4 save arrives this way, and none of them may be read as
        // "written at the epoch" - that is fifty-six years of offline pay.
        let epoch = parse(r#"{"version":4,"bananas":1,"saved_at_ms":0}"#).unwrap();
        assert_eq!(away_seconds(&epoch), None);

        let future = parse(r#"{"version":4,"saved_at_ms":1e15}"#).unwrap();
        assert_eq!(away_seconds(&future), None);

        let nonsense = parse(r#"{"version":4,"saved_at_ms":-5}"#).unwrap();
        assert_eq!(away_seconds(&nonsense), None);
    }
}
