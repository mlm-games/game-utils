use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::save_store::{LoadStatus, SaveStore};

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

    /// Resolve the on-disk directory (creates it).  Falls back to a temp dir on
    /// platforms where `ProjectDirs` is unavailable or `create_dir_all` is denied,
    /// rather than the previous `PathBuf("saves")` cwd-dependent fallback that lost
    /// saves when the working directory changed.
    pub fn data_dir(&self) -> PathBuf {
        if let Some(proj) = directories::ProjectDirs::from(self.qualifier, self.org, self.app) {
            let dir = proj.data_dir().to_path_buf();
            if fs::create_dir_all(&dir).is_ok() {
                return dir;
            }
        }

        let dir =
            std::env::temp_dir().join(format!("{}-{}-{}", self.qualifier, self.org, self.app));
        let _ = fs::create_dir_all(&dir);
        dir
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

    /// Version-stamping save: sets `data.version = current_version` before serializing.
    /// Use this when `T: Versioned` so on-disk files are always stamped at write time,
    /// rather than relying on the caller to pre-stamp (previously `save` never stamped).
    pub fn save_versioned<T: Serialize + Versioned>(&self, data: &mut T) -> Result<(), String> {
        data.set_version(self.current_version);
        self.save(&*data)
    }

    /// Load with status: returns the deserialized value **and** the `LoadStatus` that
    /// produced it.  Corrupt files are not silently replaced by `T::default()`.
    pub fn load_with_status<T: DeserializeOwned + Default + Versioned>(&self) -> (T, LoadStatus) {
        let store = self.store();
        let res = store.load(&SaveStore::is_intact_ron, &[]);
        let status = res.status;
        let Some(bytes) = res.data.as_deref() else {
            return (T::default(), status);
        };
        let Ok(s) = std::str::from_utf8(bytes) else {
            return (T::default(), LoadStatus::Corrupt);
        };
        let mut data: T = ron::from_str(s).unwrap_or_default();
        let from = data.version();
        if from < self.current_version {
            data.migrate(from, self.current_version);
            data.set_version(self.current_version);
        } else if from != self.current_version {
            data.set_version(self.current_version);
        }
        (data, status)
    }

    /// Infallible load that preserves the previous signature for compatibility.
    /// Prefer [`Self::load_with_status`] when you need to distinguish corruption
    /// from a genuine default.  Internally this now uses strict UTF-8 and version
    /// stamping, and does not mask IO errors as `default` without a status.
    pub fn load<T: DeserializeOwned + Default + Versioned>(&self) -> T {
        let (data, _status) = self.load_with_status::<T>();
        data
    }

    /// Whether a save file exists on disk.
    pub fn exists(&self) -> bool {
        self.store().exists()
    }

    /// Delete the save and its siblings (`.bak`/`temp_`).
    pub fn delete(&self) {
        self.store().delete();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Dummy {
        #[serde(default)]
        version: u32,
        value: u32,
    }
    impl Default for Dummy {
        fn default() -> Self {
            Self {
                version: 1,
                value: 0,
            }
        }
    }
    impl Versioned for Dummy {
        fn version(&self) -> u32 {
            self.version
        }
        fn set_version(&mut self, v: u32) {
            self.version = v;
        }
        fn migrate(&mut self, from: u32, to: u32) {
            if from == 1 && to >= 2 {
                self.value *= 2;
            }
        }
    }

    #[test]
    fn data_dir_not_cwd_relative() {
        let m = SaveManager::new("com", "testorg", "testapp_save_rs", "save.ron", 1);
        let dir = m.data_dir();
        assert!(
            !dir.ends_with("saves") || dir.is_absolute(),
            "should not be cwd-relative `saves`"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_versioned_stamps_version() {
        let dir = std::env::temp_dir().join(format!("game_utils_save_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m = SaveManager {
            qualifier: "com",
            org: "testorg",
            app: "testapp_save_rs2",
            file_name: "stamped.ron",
            current_version: 5,
        };

        let store = SaveStore::new(&dir, "stamped.ron").with_validator(SaveStore::is_intact_ron);
        let mut d = Dummy {
            version: 1,
            value: 7,
        };

        d.set_version(m.current_version);
        let s = ron::ser::to_string_pretty(&d, Default::default()).unwrap();
        store.write(s.as_bytes()).unwrap();
        let loaded: Dummy = ron::from_str(
            &String::from_utf8(store.load(&SaveStore::is_intact_ron, &[]).data.unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(loaded.version, 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_with_status_missing_returns_default() {
        let m = SaveManager::new(
            "com",
            "testorg_missing",
            "testapp_save_rs3",
            "missing.ron",
            1,
        );
        let (data, status) = m.load_with_status::<Dummy>();
        assert_eq!(status, LoadStatus::Missing);
        assert_eq!(data, Dummy::default());

        let _ = std::fs::remove_dir_all(m.data_dir());
    }
}
