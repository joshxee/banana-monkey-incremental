use serde::{Deserialize, Serialize};

use crate::domain::{MAX_SAFE_BANANAS_COUNT, Treasury};

const SAVE_VERSION: u32 = 1;
#[cfg(not(target_arch = "wasm32"))]
const SAVE_FILE_NAME: &str = "save-v1.json";
#[cfg(target_arch = "wasm32")]
const WEB_STORAGE_KEY: &str = "banana-monkey-incremental.save-v1";

#[derive(Debug, Deserialize, Serialize)]
struct SaveData {
    version: u32,
    bananas: u64,
}

pub fn load_treasury() -> Treasury {
    match platform::read() {
        Ok(Some(raw)) => decode(&raw).unwrap_or_else(|| {
            bevy::log::warn!("Save data is invalid or unsupported; starting with 0 bananas");
            Treasury::default()
        }),
        Ok(None) => Treasury::default(),
        Err(error) => {
            bevy::log::warn!("Could not load save data: {error}; starting with 0 bananas");
            Treasury::default()
        }
    }
}

pub fn store_treasury(treasury: Treasury) -> Result<(), String> {
    platform::write(&encode(treasury))
}

fn encode(treasury: Treasury) -> String {
    serde_json::to_string(&SaveData {
        version: SAVE_VERSION,
        bananas: treasury.display_count(),
    })
    .expect("valid treasury always serializes")
}

fn decode(raw: &str) -> Option<Treasury> {
    let data: SaveData = serde_json::from_str(raw).ok()?;
    (data.version == SAVE_VERSION && data.bananas <= MAX_SAFE_BANANAS_COUNT)
        .then(|| Treasury::from_saved(data.bananas as f64))
        .flatten()
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

    #[test]
    fn save_round_trip_preserves_treasury() {
        let treasury = Treasury::from_saved(123.0).unwrap();

        assert_eq!(decode(&encode(treasury)), Some(treasury));
    }

    #[test]
    fn missing_fields_and_unknown_versions_are_rejected() {
        assert_eq!(decode("{}"), None);
        assert_eq!(decode(r#"{"version":2,"bananas":3}"#), None);
    }

    #[test]
    fn invalid_numeric_states_are_rejected() {
        assert_eq!(decode(r#"{"version":1,"bananas":-1}"#), None);
        assert_eq!(decode(r#"{"version":1,"bananas":1.5}"#), None);
        assert_eq!(
            decode(r#"{"version":1,"bananas":9007199254740991.4}"#),
            None
        );
        assert_eq!(
            decode(&format!(
                r#"{{"version":1,"bananas":{}}}"#,
                MAX_SAFE_BANANAS + 1.0
            )),
            None
        );
        assert_eq!(decode(r#"{"version":1,"bananas":1e999}"#), None);
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert_eq!(decode("not json"), None);
    }
}
