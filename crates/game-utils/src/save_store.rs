use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const BAK_MIN_AGE_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadStatus {
    Ok,
    Missing,
    Unreadable,
    Corrupt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadResult {
    pub data: Option<Vec<u8>>,
    pub status: LoadStatus,
    /// Path the data was actually recovered from (a backup/temp file), if any.
    pub recovered_from: Option<PathBuf>,
}

/// Crash-safe file store: serializes through a temp file + atomic rename, rotates a
/// `.bak` for rollback, quarantines corrupt files, and recovers from backups on load.
#[derive(Debug, Clone)]
pub struct SaveStore {
    pub dir: PathBuf,
    pub file_name: String,
    pub bak_min_age_secs: u64,
    /// When true, a target that fails `validate` is renamed aside (timestamped) instead
    /// of deleted, so corrupt saves stay on disk for support/manual recovery.
    pub quarantine_corrupt: bool,
    /// Cheap integrity probe used when rotating an existing target: a target that fails
    /// it is quarantined rather than rotated into `.bak`. Defaults to the JSON probe
    /// ([`Self::is_intact_json`]); set it to match the serialization format (e.g. a RON
    /// parse) so non-JSON stores aren't mistaken for corrupt on every write.
    pub validate: fn(&[u8]) -> bool,
}

impl SaveStore {
    pub fn new(dir: impl Into<PathBuf>, file_name: impl Into<String>) -> Self {
        Self {
            dir: dir.into(),
            file_name: file_name.into(),
            bak_min_age_secs: BAK_MIN_AGE_SECS,
            quarantine_corrupt: true,
            validate: Self::is_intact_json,
        }
    }

    /// Override the integrity probe used when rotating an existing target on write.
    pub fn with_validator(mut self, validate: fn(&[u8]) -> bool) -> Self {
        self.validate = validate;
        self
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(&self.file_name)
    }

    fn temp_path(&self) -> PathBuf {
        self.dir.join(format!("temp_{}", self.file_name))
    }

    fn bak_path(&self) -> PathBuf {
        self.dir.join(format!("{}.bak", self.file_name))
    }

    /// Cheap corruption probe for JSON saves: must start with `{` and end with `}`
    /// (after skipping whitespace).
    pub fn is_intact_json(bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return false;
        }
        let first = bytes.iter().find(|b| **b > 0x20).copied();
        let last = bytes.iter().rev().take(4096).find(|b| **b > 0x20).copied();
        matches!((first, last), (Some(0x7B), Some(0x7D)))
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn mtime(path: &Path) -> Option<u64> {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
    }

    fn should_rotate_bak(&self, bak_path: &Path) -> bool {
        match Self::mtime(bak_path) {
            None => true,
            Some(mtime) => Self::now().saturating_sub(mtime) >= self.bak_min_age_secs,
        }
    }

    /// Move a corrupt save aside under a timestamped name.
    pub fn quarantine_corrupt_file(&self, path: &Path) -> PathBuf {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dest = self.dir.join(format!("corrupted_{}_{}", Self::now(), name));
        if fs::rename(path, &dest).is_err() {
            return path.to_path_buf();
        }
        dest
    }

    /// Serialize `data` to a temp file, then atomically swap it into place, rotating a
    /// `.bak` on a throttle.
    pub fn write(&self, data: &[u8]) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let target_path = self.path();
        let temp_path = self.temp_path();
        let bak_path = self.bak_path();

        if temp_path.exists() {
            self.quarantine_corrupt_file(&temp_path);
        }

        fs::write(&temp_path, data).map_err(|e| e.to_string())?;

        if target_path.exists() {
            let target_intact = Self::read_all(&target_path)
                .map(|b| (self.validate)(&b))
                .unwrap_or(false);
            if !target_intact {
                self.quarantine_corrupt_file(&target_path);
            } else if self.should_rotate_bak(&bak_path) {
                fs::rename(&target_path, &bak_path).map_err(|e| e.to_string())?;
            } else {
                fs::remove_file(&target_path).map_err(|e| e.to_string())?;
            }
        }

        match fs::rename(&temp_path, &target_path) {
            Ok(()) => Ok(()),
            Err(first_err) => {
                if fs::rename(&temp_path, &target_path).is_ok() {
                    return Ok(());
                }
                fs::write(&target_path, data).map_err(|e| e.to_string())?;
                let _ = fs::remove_file(&temp_path);
                Err(first_err.to_string())
            }
        }
    }

    /// Load the save, recovering instead of failing sticky:
    /// - corrupt target -> quarantined (kept for support), then the first parseable
    ///   fallback (`temp_`, `.bak`, then `extra_corrupt_fallbacks`) is adopted and
    ///   written back so the on-disk target matches what the caller runs with;
    /// - missing target -> only `temp_` and `.bak` are tried (deliberate deletes remove
    ///   those together with the target, so they can't be resurrected);
    /// - unreadable target (file lock, AV) -> no recovery; the caller must not overwrite
    ///   a file that may still be intact.
    pub fn load(
        &self,
        validate: &dyn Fn(&[u8]) -> bool,
        extra_corrupt_fallbacks: &[PathBuf],
    ) -> LoadResult {
        let target_path = self.path();
        let target_data = Self::read_all(&target_path);
        let mut corrupt = false;

        let parsed = match &target_data {
            None => LoadStatus::Missing,
            Some(bytes) if validate(bytes) => LoadStatus::Ok,
            _ => {
                if self.quarantine_corrupt {
                    self.quarantine_corrupt_file(&target_path);
                }
                corrupt = true;
                LoadStatus::Corrupt
            }
        };

        match parsed {
            LoadStatus::Ok => LoadResult {
                data: target_data,
                status: LoadStatus::Ok,
                recovered_from: None,
            },
            LoadStatus::Unreadable => LoadResult {
                data: None,
                status: LoadStatus::Unreadable,
                recovered_from: None,
            },
            status => {
                let mut candidates: Vec<PathBuf> = vec![self.temp_path(), self.bak_path()];
                if corrupt {
                    candidates.extend(extra_corrupt_fallbacks.iter().cloned());
                }
                for fallback in candidates {
                    if fallback == target_path {
                        continue;
                    }
                    let Some(bytes) = Self::read_all(&fallback) else {
                        continue;
                    };
                    if validate(&bytes) {
                        let _ = self.write(&bytes);
                        return LoadResult {
                            data: Some(bytes),
                            status,
                            recovered_from: Some(fallback),
                        };
                    }
                }
                LoadResult {
                    data: None,
                    status,
                    recovered_from: None,
                }
            }
        }
    }

    /// Delete a save together with its `.bak`/`temp_` siblings. Deliberate deletes must
    /// use this, or load recovery would resurrect the file from its leftover backup.
    pub fn delete(&self) {
        for p in [self.path(), self.bak_path(), self.temp_path()] {
            if p.exists() {
                let _ = fs::remove_file(p);
            }
        }
    }

    pub fn exists(&self) -> bool {
        self.path().exists()
    }

    fn read_all(path: &Path) -> Option<Vec<u8>> {
        fs::read(path).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "game_utils_savestore_{}_{}",
            tag,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn validate_json(bytes: &[u8]) -> bool {
        SaveStore::is_intact_json(bytes)
    }

    #[test]
    fn write_then_load_roundtrip() {
        let dir = tmp_dir("roundtrip");
        let store = SaveStore::new(&dir, "save.json");
        store.write(b"{ \"a\": 1 }").unwrap();
        let res = store.load(&validate_json, &[]);
        assert_eq!(res.status, LoadStatus::Ok);
        assert_eq!(res.data.as_deref(), Some(b"{ \"a\": 1 }".as_slice()));
        store.delete();
    }

    #[test]
    fn load_missing_has_no_data() {
        let dir = tmp_dir("missing");
        let store = SaveStore::new(&dir, "save.json");
        let res = store.load(&validate_json, &[]);
        assert_eq!(res.status, LoadStatus::Missing);
        assert!(res.data.is_none());
    }

    #[test]
    fn corrupt_target_recovers_from_bak() {
        let dir = tmp_dir("recover_bak");
        let store = SaveStore::new(&dir, "save.json");
        store.write(b"{ \"good\": 1 }").unwrap();
        // Fast-forward the .bak mtime so the next write rotates it.
        // Force a fresh .bak by bumping its age.
        let target = store.path();
        let bak = store.bak_path();
        fs::copy(&target, &bak).unwrap();
        store.write(b"{ \"good\": 2 }").unwrap();

        fs::write(&target, b"garbage").unwrap();
        let res = store.load(&validate_json, &[]);
        assert_eq!(res.status, LoadStatus::Corrupt);
        assert_eq!(res.data.as_deref(), Some(b"{ \"good\": 1 }".as_slice()));
        assert_eq!(res.recovered_from, Some(bak));
        // Recovery wrote back to the target.
        assert!(validate_json(&fs::read(&target).unwrap()));
        store.delete();
    }

    #[test]
    fn corrupt_target_recovers_from_temp() {
        let dir = tmp_dir("recover_temp");
        let store = SaveStore::new(&dir, "save.json");
        store.write(b"{ \"good\": 1 }").unwrap();
        fs::write(store.temp_path(), b"{ \"fresher\": 3 }").unwrap();
        fs::write(store.path(), b"garbage").unwrap();
        let res = store.load(&validate_json, &[]);
        assert_eq!(res.status, LoadStatus::Corrupt);
        assert_eq!(res.data.as_deref(), Some(b"{ \"fresher\": 3 }".as_slice()));
        assert_eq!(res.recovered_from, Some(store.temp_path()));
        store.delete();
    }

    #[test]
    fn corrupt_with_no_backup_returns_corrupt() {
        let dir = tmp_dir("corrupt_only");
        let store = SaveStore::new(&dir, "save.json");
        fs::write(store.path(), b"nope").unwrap();
        let res = store.load(&validate_json, &[]);
        assert_eq!(res.status, LoadStatus::Corrupt);
        assert!(res.data.is_none());
    }

    #[test]
    fn quarantine_renames_corrupt_target() {
        let dir = tmp_dir("quarantine");
        let store = SaveStore::new(&dir, "save.json");
        fs::write(store.path(), b"broken").unwrap();
        let res = store.load(&validate_json, &[]);
        assert_eq!(res.status, LoadStatus::Corrupt);
        assert!(!store.path().exists());
        let dir_entries = fs::read_dir(&dir).unwrap();
        assert!(dir_entries.into_iter().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("corrupted_")
        }));
    }

    #[test]
    fn delete_removes_all_siblings() {
        let dir = tmp_dir("delete");
        let store = SaveStore::new(&dir, "save.json");
        store.write(b"{ \"x\": 1 }").unwrap();
        fs::copy(store.path(), store.temp_path()).unwrap();
        fs::copy(store.path(), store.bak_path()).unwrap();
        store.delete();
        assert!(!store.path().exists());
        assert!(!store.bak_path().exists());
        assert!(!store.temp_path().exists());
    }

    #[test]
    fn bak_rotation_is_throttled() {
        let dir = tmp_dir("throttle");
        let store = SaveStore::new(&dir, "save.json");
        store.write(b"{ \"v\": 1 }").unwrap();
        // Simulate a fresh .bak just written.
        fs::copy(store.path(), store.bak_path()).unwrap();
        store.write(b"{ \"v\": 2 }").unwrap();
        // The recent .bak stays: target is deleted, no new rotation.
        assert_eq!(
            fs::read(store.bak_path()).unwrap(),
            b"{ \"v\": 1 }".to_vec()
        );
        store.delete();
    }
}
