use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Implemented by save data types so the manager can stamp/roll the current version.
pub trait Versioned {
    fn version(&self) -> u32;
    fn set_version(&mut self, version: u32);
    fn migrate(&mut self, _from: u32, _to: u32) {}
}

/// Bevy-agnostic save manager: serializes generic data to RON under a platform data dir.
#[derive(Clone)]
pub struct SaveManager {
    pub qualifier: &'static str,
    pub org: &'static str,
    pub app: &'static str,
    pub file_name: &'static str,
    pub current_version: u32,
}

impl SaveManager {
    pub fn new(
        qualifier: &'static str,
        org: &'static str,
        app: &'static str,
        file_name: &'static str,
        current_version: u32,
    ) -> Self {
        Self {
            qualifier,
            org,
            app,
            file_name,
            current_version,
        }
    }

    fn path(&self) -> PathBuf {
        if let Some(proj) = directories::ProjectDirs::from(self.qualifier, self.org, self.app) {
            let dir = proj.data_dir();
            let _ = fs::create_dir_all(dir);
            dir.join(self.file_name)
        } else {
            PathBuf::from("saves").join(self.file_name)
        }
    }

    pub fn save<T: Serialize>(&self, data: &T) -> Result<(), String> {
        let path = self.path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let s = ron::ser::to_string_pretty(data, Default::default()).map_err(|e| e.to_string())?;

        let temp = path.with_extension("tmp");
        fs::write(&temp, s).map_err(|e| e.to_string())?;

        if path.exists() {
            let _ = fs::copy(&path, path.with_extension("bak"));
        }
        match fs::rename(&temp, &path) {
            Ok(()) => Ok(()),
            Err(first_err) => {
                // Windows can't rename over an existing target; retry after removing it,
                // otherwise fall back to a plain write so the save isn't lost.
                let _ = fs::remove_file(&path);
                if fs::rename(&temp, &path).is_ok() {
                    return Ok(());
                }
                let s = ron::ser::to_string_pretty(data, Default::default())
                    .map_err(|e| e.to_string())?;
                fs::write(&path, s).map_err(|e| e.to_string())?;
                let _ = fs::remove_file(&temp);
                Err(first_err.to_string())
            }
        }
    }

    pub fn load<T: DeserializeOwned + Default + Versioned>(&self) -> T {
        let path = self.path();
        let mut data: T = fs::read_to_string(path)
            .ok()
            .and_then(|s| ron::from_str(&s).ok())
            .unwrap_or_default();
        let from = data.version();
        if from < self.current_version {
            data.migrate(from, self.current_version);
            data.set_version(self.current_version);
        }
        data
    }
}
