use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::save_store::SaveStore;

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

    /// Resolve the on-disk directory (creates it).
    pub fn data_dir(&self) -> PathBuf {
        if let Some(proj) = directories::ProjectDirs::from(self.qualifier, self.org, self.app) {
            let dir = proj.data_dir().to_path_buf();
            let _ = fs::create_dir_all(&dir);
            dir
        } else {
            let dir = PathBuf::from("saves");
            let _ = fs::create_dir_all(&dir);
            dir
        }
    }

    pub fn path(&self) -> PathBuf {
        self.data_dir().join(self.file_name)
    }

    fn store(&self) -> SaveStore {
        SaveStore::new(self.data_dir(), self.file_name).with_validator(SaveStore::is_intact_ron)
    }

    pub fn save<T: Serialize>(&self, data: &T) -> Result<(), String> {
        let s = ron::ser::to_string_pretty(data, Default::default()).map_err(|e| e.to_string())?;
        self.store().write(s.as_bytes())
    }

    pub fn load<T: DeserializeOwned + Default + Versioned>(&self) -> T {
        let store = self.store();
        let res = store.load(&SaveStore::is_intact_ron, &[]);
        let mut data: T = res
            .data
            .as_deref()
            .and_then(|b| ron::from_str(&String::from_utf8_lossy(b)).ok())
            .unwrap_or_default();
        let from = data.version();
        if from < self.current_version {
            data.migrate(from, self.current_version);
            data.set_version(self.current_version);
        }
        data
    }
}
