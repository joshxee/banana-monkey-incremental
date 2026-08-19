use serde::{Deserialize, Serialize};

use crate::domain::{Carts, Research, Staff, Treasury, Workforce};

const SAVE_VERSION: u32 = 3;
// The storage key is a namespace, not a schema version: it deliberately stays
// at `v1` across schema bumps so that existing players keep their progress.
// Bump it only to orphan every save on purpose.
#[cfg(not(target_arch = "wasm32"))]
const SAVE_FILE_NAME: &str = "save-v1.json";
#[cfg(target_arch = "wasm32")]
const WEB_STORAGE_KEY: &str = "banana-monkey-incremental.save-v1";

/// What the game restores on launch. Worker cycle phase is deliberately absent:
/// restored workers receive a random phase because this save format does not
/// claim to preserve in-flight simulation progress. Their first partial cycle
/// is presentation-only, so placement cannot create income or wages. A
/// longer-cycle unit (the Net Cart) will need to persist phase if its progress
/// is player-visible.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct SavedRun {
    pub treasury: Treasury,
    pub workforce: Workforce,
    pub staff: Staff,
    pub research: Research,
    pub carts: Carts,
}

/// Read just enough to route to the right schema. Untagged deserialisation
/// would happily accept a v1 payload as v2 and give useless errors when it
/// did not, so version dispatch is explicit.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

/// Bananas were whole and workers did not exist.
#[derive(Debug, Deserialize)]
struct SaveV1 {
    bananas: u64,
}

/// Bananas were fractional and workers existed, but nobody else did.
#[derive(Debug, Deserialize)]
struct SaveV2 {
    bananas: f64,
    workers: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct SaveV3 {
    version: u32,
    /// Fractional, because meals are. Never round on the way in or out:
    /// flooring here would burn up to a banana on every reload.
    bananas: f64,
    workers: u32,
    chefs: u32,
    unpackers: u32,
    technologists: u32,
    /// Points only. The research *level* is derived from them on load, because
    /// two fields that must agree eventually will not: a tampered save or a
    /// rebalanced growth factor would leave a level its points do not justify,
    /// and nothing would notice.
    research: f64,
    carts: u32,
    /// Monkeys aboard, across every cart. One extra number, and it is what
    /// stops a reload either gifting a half-boarded cart its missing crew or
    /// stealing the wait the player has already served.
    crewed: u32,
}

impl From<SaveV1> for SaveV3 {
    fn from(old: SaveV1) -> Self {
        Self {
            version: SAVE_VERSION,
            bananas: old.bananas as f64,
            workers: 0,
            chefs: 0,
            unpackers: 0,
            technologists: 0,
            research: 0.0,
            carts: 0,
            crewed: 0,
        }
    }
}

impl From<SaveV2> for SaveV3 {
    fn from(old: SaveV2) -> Self {
        Self {
            version: SAVE_VERSION,
            bananas: old.bananas,
            workers: old.workers,
            chefs: 0,
            unpackers: 0,
            technologists: 0,
            research: 0.0,
            carts: 0,
            crewed: 0,
        }
    }
}

pub fn load_run() -> SavedRun {
    match platform::read() {
        Ok(Some(raw)) => decode(&raw).unwrap_or_else(|| {
            bevy::log::warn!("Save data is invalid or unsupported; starting a fresh run");
            SavedRun::default()
        }),
        Ok(None) => SavedRun::default(),
        Err(error) => {
            bevy::log::warn!("Could not load save data: {error}; starting a fresh run");
            SavedRun::default()
        }
    }
}

pub fn store_run(run: SavedRun) -> Result<(), String> {
    platform::write(&encode(run))
}

fn encode(run: SavedRun) -> String {
    use crate::domain::SupportRole;
    serde_json::to_string(&SaveV3 {
        version: SAVE_VERSION,
        bananas: run.treasury.bananas(),
        workers: run.workforce.count(),
        chefs: run.staff.count(SupportRole::Chef),
        unpackers: run.staff.count(SupportRole::Unpacker),
        technologists: run.staff.count(SupportRole::Technologist),
        research: run.research.points(),
        carts: run.carts.owned(),
        crewed: run.carts.crewed(),
    })
    .expect("valid run state always serializes")
}

fn decode(raw: &str) -> Option<SavedRun> {
    let probe: VersionProbe = serde_json::from_str(raw).ok()?;
    // Chained rather than parallel, so each schema only has to know about the
    // one after it.
    let data: SaveV3 = match probe.version {
        1 => SaveV3::from(serde_json::from_str::<SaveV1>(raw).ok()?),
        2 => SaveV3::from(serde_json::from_str::<SaveV2>(raw).ok()?),
        3 => serde_json::from_str(raw).ok()?,
        _ => return None,
    };

    Some(SavedRun {
        treasury: Treasury::from_saved(data.bananas)?,
        workforce: Workforce::from_saved(data.workers)?,
        // Validated like the rest: local storage is player-writable, and an
        // unchecked count spawns that many entities in a single tick and is
        // then re-persisted, so the tab never recovers.
        staff: Staff::from_saved(data.chefs, data.unpackers, data.technologists)?,
        research: Research::from_saved(data.research)?,
        // Cross-field, so it cannot live inside `Carts::from_saved`: a crew
        // larger than the workforce would put a running cart on an empty
        // payroll, harvesting for free.
        carts: (data.crewed <= data.workers)
            .then(|| Carts::from_saved(data.carts, data.crewed))
            .flatten()?,
    })
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
    };

    use atomic_write_file::AtomicWriteFile;
    use directories::ProjectDirs;

    use super::SAVE_FILE_NAME;

    pub fn read() -> Result<Option<String>, String> {
        let path = save_path()?;
        match fs::read_to_string(path) {
            Ok(raw) => Ok(Some(raw)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn write(raw: &str) -> Result<(), String> {
        let path = save_path()?;
        let parent = path
            .parent()
            .ok_or_else(|| "save path has no parent directory".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;

        atomic_write(&path, raw.as_bytes())
    }

    fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
        let mut file = AtomicWriteFile::open(path).map_err(|error| error.to_string())?;
        file.write_all(contents)
            .map_err(|error| error.to_string())?;
        file.commit().map_err(|error| error.to_string())
    }

    fn save_path() -> Result<PathBuf, String> {
        ProjectDirs::from("com", "Banana Monkey", "Banana Monkey Incremental")
            .map(|directories| directories.data_local_dir().join(SAVE_FILE_NAME))
            .ok_or_else(|| "platform application-data directory is unavailable".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use wasm_bindgen::JsValue;

    use super::WEB_STORAGE_KEY;

    pub fn read() -> Result<Option<String>, String> {
        storage()?
            .get_item(WEB_STORAGE_KEY)
            .map_err(format_js_error)
    }

    pub fn write(raw: &str) -> Result<(), String> {
        storage()?
            .set_item(WEB_STORAGE_KEY, raw)
            .map_err(format_js_error)
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

    #[test]
    fn save_round_trip_preserves_the_run() {
        let mut saved = researched(123.0, 7, 2, 3, 1, 145.5);
        // Two carts running and a third halfway through boarding.
        saved.carts = Carts::from_saved(3, 7).unwrap();

        assert_eq!(decode(&encode(saved)), Some(saved));
        assert_eq!(decode(&encode(saved)).unwrap().carts.running(), 2);
        assert_eq!(decode(&encode(saved)).unwrap().carts.berths_open(), 2);
        // The level has to come back with the points that bought it: level 0
        // costs 60 and level 1 costs 132, so 145.5 has bought exactly one.
        assert_eq!(saved.research.level(), 1);
        assert_eq!(decode(&encode(saved)).unwrap().research.level(), 1);
    }

    #[test]
    fn fractional_bananas_survive_a_round_trip_exactly() {
        // Wages leave the treasury fractional, and flooring anywhere on this
        // path would quietly burn production on every reload.
        let saved = run(12.345_678_9, 3);

        assert_eq!(decode(&encode(saved)), Some(saved));
    }

    #[test]
    fn version_1_saves_migrate_to_a_workerless_run() {
        assert_eq!(decode(r#"{"version":1,"bananas":42}"#), Some(run(42.0, 0)));
    }

    #[test]
    fn version_2_saves_keep_their_workers_and_arrive_unstaffed() {
        // The schema that shipped before support staff existed. Its workforce
        // has to survive intact - this is a live player's run - and it can only
        // arrive with an empty payroll.
        assert_eq!(
            decode(r#"{"version":2,"bananas":12.5,"workers":6}"#),
            Some(staffed(12.5, 6, 0, 0, 0))
        );
    }

    #[test]
    fn missing_fields_and_unknown_versions_are_rejected() {
        assert_eq!(decode("{}"), None);
        assert_eq!(decode(r#"{"version":2,"bananas":3}"#), None);
        assert_eq!(decode(r#"{"version":2,"workers":1}"#), None);
        // A v3 payload missing any of its required fields is still rejected,
        // which is what this assertion was protecting before v3 existed.
        assert_eq!(decode(r#"{"version":3,"bananas":3,"workers":1}"#), None);
        assert_eq!(
            decode(r#"{"version":3,"bananas":3,"workers":1,"chefs":1,"unpackers":1}"#),
            None
        );
        assert_eq!(
            decode(
                r#"{"version":3,"bananas":3,"workers":1,"chefs":1,"unpackers":1,"technologists":1,"research":-1,"carts":0,"crewed":0}"#
            ),
            None
        );
        assert_eq!(decode(r#"{"version":4,"bananas":3,"workers":1}"#), None);
        // A v1 payload must not be read as a v2 one.
        assert_eq!(decode(r#"{"version":1,"bananas":1.5}"#), None);
    }

    #[test]
    fn invalid_numeric_states_are_rejected() {
        assert_eq!(decode(r#"{"version":2,"bananas":-1,"workers":0}"#), None);
        assert_eq!(decode(r#"{"version":2,"bananas":null,"workers":0}"#), None);
        assert_eq!(decode(r#"{"version":2,"bananas":1,"workers":-1}"#), None);
        // Local storage is player-writable, so an absurd worker count has to be
        // rejected rather than spawned.
        assert_eq!(
            decode(r#"{"version":2,"bananas":1,"workers":4000000000}"#),
            None
        );
        assert_eq!(
            decode(&format!(
                r#"{{"version":2,"bananas":{},"workers":0}}"#,
                MAX_SAFE_BANANAS + 2.0
            )),
            None
        );
        assert_eq!(decode(r#"{"version":2,"bananas":1e999,"workers":0}"#), None);
        // Support counts are player-writable too, and each one drives a spawn
        // loop of its own.
        let staffed_payload = |c: &str| {
            format!(
                r#"{{"version":3,"bananas":1,"workers":0,"chefs":{c},"unpackers":0,"technologists":0,"research":0,"carts":0,"crewed":0}}"#
            )
        };
        assert_eq!(decode(&staffed_payload("-1")), None);
        assert_eq!(decode(&staffed_payload("4000000000")), None);
        assert!(decode(&staffed_payload("3")).is_some());
        // A crew larger than the berths it could sit in is a tampered save.
        let crewed = |carts: u32, crewed: u32| {
            format!(
                r#"{{"version":3,"bananas":1,"workers":9,"chefs":0,"unpackers":0,"technologists":0,"research":0,"carts":{carts},"crewed":{crewed}}}"#
            )
        };
        assert!(decode(&crewed(2, 6)).is_some());
        assert_eq!(decode(&crewed(2, 7)), None);
        assert_eq!(decode(&crewed(0, 1)), None);
        // Nine workers is exactly enough for three carts, and not enough for
        // four - a crew the workforce cannot supply is a tampered save.
        assert!(decode(&crewed(3, 9)).is_some());
        assert_eq!(decode(&crewed(4, 10)), None);
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert_eq!(decode("not json"), None);
    }
}
