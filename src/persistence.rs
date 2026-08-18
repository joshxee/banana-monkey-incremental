use serde::{Deserialize, Serialize};

use crate::domain::{Treasury, Workforce};

const SAVE_VERSION: u32 = 2;
// The storage key is a namespace, not a schema version: it deliberately stays
// at `v1` across schema bumps so that existing players keep their progress.
// Bump it only to orphan every save on purpose.
#[cfg(not(target_arch = "wasm32"))]
const SAVE_FILE_NAME: &str = "save-v1.json";
#[cfg(target_arch = "wasm32")]
const WEB_STORAGE_KEY: &str = "banana-monkey-incremental.save-v1";

/// What the game restores on launch. Cycle phase is deliberately absent:
/// workers re-jitter on load, which is tolerable only because a worker cycle is
/// 47.5 seconds. A longer-cycle unit (the Net Cart) will have to persist phase,
/// or reloading becomes both a save-scum surface and an invisible punishment.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct SavedRun {
    pub treasury: Treasury,
    pub workforce: Workforce,
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

#[derive(Debug, Deserialize, Serialize)]
struct SaveV2 {
    version: u32,
    /// Fractional, because wages drain continuously. Never round on the way in
    /// or out: flooring here would burn up to a banana on every reload.
    bananas: f64,
    workers: u32,
}

impl From<SaveV1> for SaveV2 {
    fn from(old: SaveV1) -> Self {
        Self {
            version: SAVE_VERSION,
            bananas: old.bananas as f64,
            workers: 0,
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
    serde_json::to_string(&SaveV2 {
        version: SAVE_VERSION,
        bananas: run.treasury.bananas(),
        workers: run.workforce.count(),
    })
    .expect("valid run state always serializes")
}

fn decode(raw: &str) -> Option<SavedRun> {
    let probe: VersionProbe = serde_json::from_str(raw).ok()?;
    let data: SaveV2 = match probe.version {
        1 => serde_json::from_str::<SaveV1>(raw).ok()?.into(),
        2 => serde_json::from_str(raw).ok()?,
        _ => return None,
    };

    Some(SavedRun {
        treasury: Treasury::from_saved(data.bananas)?,
        workforce: Workforce::from_saved(data.workers)?,
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
        SavedRun {
            treasury: Treasury::from_saved(bananas).unwrap(),
            workforce: Workforce::from_saved(workers).unwrap(),
        }
    }

    #[test]
    fn save_round_trip_preserves_the_run() {
        let saved = run(123.0, 7);

        assert_eq!(decode(&encode(saved)), Some(saved));
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
    fn missing_fields_and_unknown_versions_are_rejected() {
        assert_eq!(decode("{}"), None);
        assert_eq!(decode(r#"{"version":2,"bananas":3}"#), None);
        assert_eq!(decode(r#"{"version":2,"workers":1}"#), None);
        assert_eq!(decode(r#"{"version":3,"bananas":3,"workers":1}"#), None);
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
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert_eq!(decode("not json"), None);
    }
}
