//! Genre-agnostic multi-profile manager, .
//!
//! Owns the active profile and routes every per-profile file under `profile_<N>/` inside a
//! base directory. A small RON pointer config (`profiles.ron`) records the active profile,
//! whether a legacy (single-directory) save was migrated, and arbitrary boolean flags.
//!
//! The manager is self-initializing and idempotent ([`ProfileManager::init`] runs once), so
//! consumers can resolve profile paths at boot without depending on initialization order.
//! It only owns paths, the pointer config, and filesystem lifecycle (create / clear-with-
//! archive / migration); reloading the game's data stores after a switch is the caller's job
//! (read [`ProfileManager::active_path`] after `switch_to`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::save_store::{LoadStatus, SaveStore};
use crate::storage::{FsStorage, Storage};

/// Default name of the pointer config file inside the base directory.
pub const POINTER_FILE: &str = "profiles.ron";
/// Subdirectory under the base dir where cleared profiles are archived.
pub const BACKUP_DIR: &str = "backups";
/// Prefix of the pre-migration backup directory under [`BACKUP_DIR`].
pub const PRE_MIGRATION_BACKUP_DIR: &str = "pre_profiles";
/// Prefix of cleared-profile archive directories under [`BACKUP_DIR`].
pub const CLEARED_PREFIX: &str = "cleared_profile_";
/// Default cap on retained cleared-profile archives.
pub const DEFAULT_MAX_CLEARED_ARCHIVES: usize = 5;

/// Errors surfaced by [`ProfileManager`].
#[derive(Debug)]
pub enum ProfileError {
    Io(std::io::Error),
    Ron(ron::Error),
    /// Migration staging could not be verified or committed; the manager fell back to the
    /// legacy root files for profile 1 (see [`ProfileManager::legacy_fallback`]).
    MigrationFailed(&'static str),
    /// Nothing to migrate (no legacy files configured).
    NoLegacyFiles,
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Ron(e) => write!(f, "ron error: {e}"),
            Self::MigrationFailed(step) => write!(f, "profile migration failed at: {step}"),
            Self::NoLegacyFiles => write!(f, "no legacy profile files configured"),
        }
    }
}

impl std::error::Error for ProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Ron(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ProfileError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<ron::Error> for ProfileError {
    fn from(e: ron::Error) -> Self {
        Self::Ron(e)
    }
}

/// Pointer config persisted as RON in the base directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PointerConfig {
    /// 1-based index of the active profile.
    active: usize,
    /// Once true, Profile 1 permanently lives in `profile_1/` (never the legacy root
    /// files), so clearing it can't be undone by a re-migration on the next boot.
    migrated: bool,
    /// True only if a legacy save was actually carried over. Gates first-launch UX.
    migrated_legacy: bool,
    /// Arbitrary boolean flags (e.g. "has seen profile intro").
    #[serde(default)]
    flags: BTreeMap<String, bool>,
}

impl Default for PointerConfig {
    fn default() -> Self {
        Self {
            active: 1,
            migrated: false,
            migrated_legacy: false,
            flags: BTreeMap::new(),
        }
    }
}

fn ron_intact(bytes: &[u8]) -> bool {
    ron::from_str::<ron::Value>(&String::from_utf8_lossy(bytes)).is_ok()
}

/// Owns the active profile, per-profile paths, lifecycle, and legacy migration.
/// Generic over `Storage` (keeps Ron codec).
#[derive(Debug, Clone)]
pub struct ProfileManager<S: Storage = FsStorage> {
    base_dir: PathBuf,
    num_profiles: usize,
    /// Files that live per-profile. The first entry is also the probe used by
    /// [`ProfileManager::profile_exists`] and migration adoption checks.
    legacy_files: Vec<String>,
    max_cleared_archives: usize,
    active: usize,
    migrated: bool,
    migrated_legacy: bool,
    /// Set when this session's migration failed; routes Profile 1 back to the untouched
    /// legacy root files so the player still sees their save.
    legacy_fallback: bool,
    initialized: bool,
    flags: BTreeMap<String, bool>,
    storage: S,
}

impl ProfileManager<FsStorage> {
    /// Create an uninitialized manager. Call [`Self::init`] (idempotent) before resolving
    /// paths; it self-initializes exactly like the source game's lazy static logic.
    pub fn new(base_dir: impl Into<PathBuf>, num_profiles: usize, legacy_files: &[&str]) -> Self {
        Self::new_with_storage(base_dir, num_profiles, legacy_files, FsStorage)
    }
}

impl<S: Storage> ProfileManager<S> {
    pub fn new_with_storage(
        base_dir: impl Into<PathBuf>,
        num_profiles: usize,
        legacy_files: &[&str],
        storage: S,
    ) -> Self {
        Self {
            base_dir: base_dir.into(),
            num_profiles: num_profiles.max(1),
            legacy_files: legacy_files.iter().map(|s| s.to_string()).collect(),
            max_cleared_archives: DEFAULT_MAX_CLEARED_ARCHIVES,
            active: 1,
            migrated: false,
            migrated_legacy: false,
            legacy_fallback: false,
            initialized: false,
            flags: BTreeMap::new(),
            storage,
        }
    }

    /// Cap on retained cleared-profile archives. Defaults to [`DEFAULT_MAX_CLEARED_ARCHIVES`].
    pub fn with_max_cleared_archives(mut self, n: usize) -> Self {
        self.max_cleared_archives = n;
        self
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Load the pointer config, run migration, and ensure the active profile dir exists.
    /// Idempotent. On failure, leaves the manager uninitialized so a later call can retry.
    pub fn init(&mut self) -> Result<(), ProfileError> {
        if self.initialized {
            return Ok(());
        }
        self.load_pointer();
        let active = self.active.clamp(1, self.num_profiles);
        if self.active != active {
            self.active = active;
        }
        self.migrate_if_needed()?;
        self.storage
            .create_dir_all(&self.profile_dir(self.active))?;
        if self.fallback_path_none_left() || self.migrated {
            self.legacy_fallback = false;
        }
        self.initialized = true;
        Ok(())
    }

    fn fallback_path_none_left(&self) -> bool {
        self.storage
            .exists(&self.profile_dir(1).join(self.probe_file()))
    }

    fn pointer_store(&self) -> SaveStore<S> {
        SaveStore::new_with_storage(&self.base_dir, POINTER_FILE, self.storage.clone())
            .with_validator(ron_intact)
    }

    fn load_pointer(&mut self) {
        let store = self.pointer_store();
        let res = store.load(&validate_pointer, &[]);
        if res.status == LoadStatus::Ok
            && let Some(data) = res.data
            && let Ok(cfg) = ron::from_str::<PointerConfig>(&String::from_utf8_lossy(&data))
        {
            self.active = cfg.active.clamp(1, self.num_profiles);
            self.migrated = cfg.migrated;
            self.migrated_legacy = cfg.migrated_legacy;
            self.flags = cfg.flags;
        }
    }

    fn persist_pointer(&mut self) -> Result<(), ProfileError> {
        let cfg = PointerConfig {
            active: self.active,
            migrated: self.migrated,
            migrated_legacy: self.migrated_legacy,
            flags: self.flags.clone(),
        };
        let s = ron::ser::to_string(&cfg).map_err(ProfileError::Ron)?;
        self.pointer_store()
            .write(s.as_bytes())
            .map_err(|e| ProfileError::Io(std::io::Error::other(e)))
    }

    /// True only for players carrying over a legacy save - gates first-launch UX.
    pub fn had_legacy_migration(&self) -> bool {
        self.migrated_legacy
    }

    /// Read an arbitrary boolean flag from the pointer config.
    pub fn get_flag(&self, key: &str) -> bool {
        self.flags.get(key).copied().unwrap_or(false)
    }

    /// Write an arbitrary boolean flag to the pointer config.
    pub fn set_flag(&mut self, key: &str, value: bool) -> Result<(), ProfileError> {
        self.flags.insert(key.to_string(), value);
        self.persist_pointer()
    }

    /// The base directory containing `profile_<N>/` (and the pointer config).
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// 1-based index of the active profile.
    pub fn active(&self) -> usize {
        self.active
    }

    /// First configured legacy file, used as the presence probe.
    pub fn probe_file(&self) -> &str {
        self.legacy_files.first().map(String::as_str).unwrap_or("")
    }

    /// Directory for profile `idx` (`profile_<idx>/` under the base dir).
    pub fn profile_dir(&self, idx: usize) -> PathBuf {
        self.base_dir.join(format!("profile_{}", idx))
    }

    /// Path for `file` inside profile `idx`. While a legacy migration is pending/failed,
    /// profile 1 resolves to the legacy root files instead.
    pub fn profile_path(&self, file: &str, idx: usize) -> PathBuf {
        if idx == 1 && self.legacy_fallback {
            self.base_dir.join(file)
        } else {
            self.profile_dir(idx).join(file)
        }
    }

    /// Directory of the active profile.
    pub fn active_dir(&self) -> PathBuf {
        self.profile_dir(self.active)
    }

    /// Path for `file` inside the active profile.
    pub fn active_path(&self, file: &str) -> PathBuf {
        self.profile_path(file, self.active)
    }

    /// Whether a profile exists: its probe file is present on disk.
    pub fn profile_exists(&self, idx: usize) -> bool {
        let probe = self.probe_file();
        if probe.is_empty() {
            return self.storage.is_dir(&self.profile_dir(idx));
        }
        self.storage.is_file(&self.profile_path(probe, idx))
    }

    /// Copy one file to another, creating the destination parent dir.
    pub fn copy_file(&self, src: &Path, dst: &Path) -> Result<(), ProfileError> {
        if let Some(parent) = dst.parent() {
            self.storage.create_dir_all(parent)?;
        }
        self.storage.copy(src, dst)?;
        Ok(())
    }

    /// Removes a directory tree recursively.
    pub fn remove_dir_recursive(&self, path: &Path) -> Result<(), ProfileError> {
        if self.storage.exists(path) {
            self.storage.remove_dir_all(path)?;
        }
        Ok(())
    }

    /// Switch the active profile, persisting the pointer and ensuring the new profile dir.
    ///
    /// The caller must flush the outgoing profile's data stores *before* calling this and
    /// reload them from [`Self::active_path`] afterwards; the manager only owns the pointer.
    pub fn switch_to(&mut self, idx: usize) -> Result<(), ProfileError> {
        let idx = idx.clamp(1, self.num_profiles);
        if !self.migrated {
            self.migrate_if_needed()?;
        }
        if self.migrated {
            self.legacy_fallback = false;
        }
        self.active = idx;
        self.persist_pointer()?;
        self.storage
            .create_dir_all(&self.profile_dir(self.active))?;
        Ok(())
    }

    /// Ensure a profile directory exists (fresh profile).
    pub fn create_profile(&self, idx: usize) -> Result<(), ProfileError> {
        let idx = idx.clamp(1, self.num_profiles);
        self.storage.create_dir_all(&self.profile_dir(idx))?;
        Ok(())
    }

    /// Archive the profile directory under the backup dir (timestamped) instead of
    /// hard-deleting, then prune old archives. The archive path is returned. If the
    /// directory doesn't exist, returns `Ok(None)`.
    pub fn clear_profile(&self, idx: usize) -> Result<Option<PathBuf>, ProfileError> {
        let idx = idx.clamp(1, self.num_profiles);
        let dir = self.profile_dir(idx);
        if !self.storage.is_dir(&dir) {
            return Ok(None);
        }
        self.storage.create_dir_all(&self.backup_dir())?;
        let stamp = unix_now_nanos();
        let mut dest = self
            .backup_dir()
            .join(format!("{CLEARED_PREFIX}{idx}_{stamp}"));
        let mut ctr = 0u32;
        while self.storage.exists(&dest) {
            ctr += 1;
            dest = self
                .backup_dir()
                .join(format!("{CLEARED_PREFIX}{idx}_{stamp}_{ctr}"));
            if ctr > 100 {
                return Err(ProfileError::Io(std::io::Error::other(
                    "too many archive collisions",
                )));
            }
        }
        if let Err(e) = self.storage.rename(&dir, &dest) {
            if e.kind() == std::io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(18) {
                copy_dir_recursive(&self.storage, &dir, &dest)?;
                self.storage.remove_dir_all(&dir)?;
            } else {
                return Err(e.into());
            }
        }
        self.prune_cleared_archives()?;
        Ok(Some(dest))
    }

    fn backup_dir(&self) -> PathBuf {
        self.base_dir.join(BACKUP_DIR)
    }

    /// Prune cleared-profile archives down to `max_cleared_archives`, oldest first.
    pub fn prune_cleared_archives(&self) -> Result<(), ProfileError> {
        let entries = match self.storage.read_dir(&self.backup_dir()) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let mut archives: Vec<(PathBuf, u64)> = entries
            .into_iter()
            .filter(|p| {
                self.storage.is_dir(p)
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(CLEARED_PREFIX))
            })
            .map(|p| {
                let mtime = self.storage.mtime_secs(&p).unwrap_or(0);
                (p, mtime)
            })
            .collect();
        archives.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        while archives.len() > self.max_cleared_archives {
            let (oldest, _) = archives.remove(0);
            let _ = self.storage.remove_dir_all(&oldest);
        }
        Ok(())
    }

    fn migrate_if_needed(&mut self) -> Result<(), ProfileError> {
        if self.migrated {
            return Ok(());
        }
        let probe = self.probe_file();
        if probe.is_empty() {
            self.migrated = true;
            self.migrated_legacy = false;
            return self.persist_pointer();
        }
        if self.storage.exists(&self.profile_dir(1).join(probe)) {
            self.migrated = true;
            self.migrated_legacy = true;
            return self.persist_pointer();
        }
        let legacy: Vec<String> = self
            .legacy_files
            .iter()
            .filter(|f| self.storage.is_file(&self.base_dir.join(f)))
            .cloned()
            .collect();
        if legacy.is_empty() {
            self.migrated = true;
            self.migrated_legacy = false;
            return self.persist_pointer();
        }
        self.run_migration(&legacy)
    }

    fn run_migration(&mut self, legacy: &[String]) -> Result<(), ProfileError> {
        let backup = self.backup_dir().join(PRE_MIGRATION_BACKUP_DIR);
        self.storage.create_dir_all(&backup)?;
        for f in legacy {
            let _ = self.storage.copy(&self.base_dir.join(f), &backup.join(f));
        }
        let staging = self.base_dir.join("profile_1_migrating");
        let _ = self.storage.remove_dir_all(&staging);
        self.storage.create_dir_all(&staging)?;
        for f in legacy {
            let src = self.base_dir.join(f);
            let dst = staging.join(f);
            if let Err(e) = self.storage.copy(&src, &dst) {
                let _ = self.storage.remove_dir_all(&staging);
                self.legacy_fallback = true;
                return Err(ProfileError::Io(e));
            }
            if !self.verify_copy(&src, &dst) {
                let _ = self.storage.remove_dir_all(&staging);
                self.legacy_fallback = true;
                return Err(ProfileError::MigrationFailed("copy/verify"));
            }
        }
        let profile_1 = self.profile_dir(1);
        let probe = self.probe_file();
        if self.storage.is_dir(&profile_1) && !self.storage.exists(&profile_1.join(probe)) {
            let _ = self.storage.remove_dir_all(&profile_1);
        }
        if let Err(e) = self.storage.rename(&staging, &profile_1) {
            if e.kind() == std::io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(18) {
                if let Err(copy_err) = copy_dir_recursive(&self.storage, &staging, &profile_1) {
                    let _ = self.storage.remove_dir_all(&staging);
                    self.legacy_fallback = true;
                    return Err(ProfileError::Io(copy_err));
                }
                let _ = self.storage.remove_dir_all(&staging);
            } else {
                let _ = self.storage.remove_dir_all(&staging);
                self.legacy_fallback = true;
                return Err(ProfileError::Io(e));
            }
        }
        self.migrated = true;
        self.migrated_legacy = true;
        self.persist_pointer()
    }

    fn verify_copy(&self, src: &Path, dst: &Path) -> bool {
        let ok_src = self.storage.metadata_len(src).unwrap_or(0);
        let ok_dst = self.storage.metadata_len(dst).unwrap_or(0);
        if ok_src != ok_dst {
            return false;
        }
        if let (Ok(Some(a)), Ok(Some(b))) = (self.storage.read(src), self.storage.read(dst)) {
            if a != b {
                return false;
            }
        } else {
            return false;
        }
        if dst.extension().and_then(|e| e.to_str()) == Some("ron") {
            if let Ok(Some(bytes)) = self.storage.read(dst) {
                if !ron_intact(&bytes) {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
}

fn validate_pointer(bytes: &[u8]) -> bool {
    ron::from_str::<PointerConfig>(&String::from_utf8_lossy(bytes)).is_ok()
}

#[allow(dead_code)]
fn unix_now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unix_now_nanos() -> u128 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn copy_dir_recursive<S: Storage>(storage: &S, src: &Path, dst: &Path) -> std::io::Result<()> {
    storage.create_dir_all(dst)?;
    let entries = storage.read_dir(src)?;
    for entry_path in entries {
        let dst_path = dst.join(entry_path.file_name().unwrap());
        if storage.is_dir(&entry_path) {
            copy_dir_recursive(storage, &entry_path, &dst_path)?;
        } else {
            storage.copy(&entry_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "game_utils_profiles_{}_{}",
            tag,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn make_legacy(root: &Path, files: &[&str]) {
        fs::create_dir_all(root).unwrap();
        for f in files {
            fs::write(root.join(f), r#"{"v": 1}"#).unwrap();
        }
    }

    #[test]
    fn fresh_install_marks_migrated_without_carryover() {
        let root = tmp_root("fresh");
        let mut pm = ProfileManager::new(&root, 3, &["save.ron"]);
        pm.init().unwrap();
        assert!(!pm.had_legacy_migration());
        assert_eq!(pm.active(), 1);
        assert!(pm.profile_dir(1).is_dir());
        let pointer = root.join(POINTER_FILE);
        assert!(pointer.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn legacy_save_migrates_into_profile_1() {
        let root = tmp_root("migrate");
        make_legacy(&root, &["save.ron", "run_save.ron"]);
        let mut pm = ProfileManager::new(&root, 3, &["save.ron", "run_save.ron"]);
        pm.init().unwrap();
        assert!(pm.had_legacy_migration());
        assert!(pm.profile_path("save.ron", 1).is_file(), "save migrated");
        assert!(pm.profile_path("run_save.ron", 1).is_file());
        assert!(root.join("save.ron").is_file());
        assert!(
            root.join(BACKUP_DIR)
                .join(PRE_MIGRATION_BACKUP_DIR)
                .join("save.ron")
                .is_file()
        );
        let mut pm2 = ProfileManager::new(&root, 3, &["save.ron"]);
        pm2.init().unwrap();
        assert!(pm2.had_legacy_migration());
        assert!(!pm2.legacy_fallback);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn pointers_and_flags_roundtrip() {
        let root = tmp_root("flags");
        let mut pm = ProfileManager::new(&root, 3, &["save.ron"]);
        pm.init().unwrap();
        pm.set_flag("seen_profile_intro", true).unwrap();
        let mut pm2 = ProfileManager::new(&root, 3, &["save.ron"]);
        pm2.init().unwrap();
        assert!(pm2.get_flag("seen_profile_intro"));
        assert!(!pm2.get_flag("other"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn switch_to_persists_active_and_creates_dir() {
        let root = tmp_root("switch");
        let mut pm = ProfileManager::new(&root, 3, &["save.ron"]);
        pm.init().unwrap();
        pm.switch_to(2).unwrap();
        assert_eq!(pm.active(), 2);
        assert_eq!(pm.active_path("save.ron"), root.join("profile_2/save.ron"));
        assert!(pm.profile_dir(2).is_dir());
        let mut pm2 = ProfileManager::new(&root, 3, &["save.ron"]);
        pm2.init().unwrap();
        assert_eq!(pm2.active(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn clear_archives_and_prunes() {
        let root = tmp_root("clear");
        let mut pm = ProfileManager::new(&root, 3, &["save.ron"]).with_max_cleared_archives(2);
        pm.init().unwrap();
        for i in 1..=3 {
            pm.create_profile(i).unwrap();
            pm.clear_profile(i).unwrap();
        }
        assert!(!pm.profile_dir(1).is_dir());
        let remaining: Vec<_> = fs::read_dir(root.join(BACKUP_DIR))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(CLEARED_PREFIX))
            .collect();
        assert_eq!(remaining.len(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_pointer_falls_back_to_defaults() {
        let root = tmp_root("corrupt");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(POINTER_FILE), b"garbage").unwrap();
        let mut pm = ProfileManager::new(&root, 3, &["save.ron"]);
        pm.init().unwrap();
        assert_eq!(pm.active(), 1);
        assert!(!pm.had_legacy_migration());
        let corrupted: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("corrupted_"))
            .collect();
        assert!(!corrupted.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn memory_storage_profile_roundtrip() {
        use crate::storage::MemoryStorage;
        let mem = MemoryStorage::new();
        let mut pm = ProfileManager::new_with_storage(
            PathBuf::from("/tmp/mem_profiles"),
            3,
            &["save.ron"],
            mem.clone(),
        );
        pm.init().unwrap();
        assert_eq!(pm.active(), 1);
        pm.set_flag("seen", true).unwrap();
        let mut pm2 = ProfileManager::new_with_storage(
            PathBuf::from("/tmp/mem_profiles"),
            3,
            &["save.ron"],
            mem,
        );
        pm2.init().unwrap();
        assert!(pm2.get_flag("seen"));
    }
}
