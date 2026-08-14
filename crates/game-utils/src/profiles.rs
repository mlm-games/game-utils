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
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::save_store::{LoadStatus, SaveStore};

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
#[derive(Debug, Clone)]
pub struct ProfileManager {
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
}

impl ProfileManager {
    /// Create an uninitialized manager. Call [`Self::init`] (idempotent) before resolving
    /// paths; it self-initializes exactly like the source game's lazy static logic.
    pub fn new(base_dir: impl Into<PathBuf>, num_profiles: usize, legacy_files: &[&str]) -> Self {
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
        }
    }

    /// Cap on retained cleared-profile archives. Defaults to [`DEFAULT_MAX_CLEARED_ARCHIVES`].
    pub fn with_max_cleared_archives(mut self, n: usize) -> Self {
        self.max_cleared_archives = n;
        self
    }

    /// Load the pointer config, run migration, and ensure the active profile dir exists.
    /// Idempotent.
    pub fn init(&mut self) -> Result<(), ProfileError> {
        if self.initialized {
            return Ok(());
        }
        self.initialized = true;
        self.load_pointer();

        let active = self.active.clamp(1, self.num_profiles);
        if self.active != active {
            self.active = active;
        }
        self.migrate_if_needed()?;
        fs::create_dir_all(self.profile_dir(self.active))?;
        if self.fallback_path_none_left() || self.migrated {
            self.legacy_fallback = false;
        }
        Ok(())
    }

    fn fallback_path_none_left(&self) -> bool {
        // A failed migration leaves the real save in the root; once profile_1 exists the
        // fallback is obsolete.
        self.profile_dir(1).join(self.probe_file()).exists()
    }

    fn pointer_store(&self) -> SaveStore {
        SaveStore::new(&self.base_dir, POINTER_FILE).with_validator(ron_intact)
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
        // Missing / corrupt / unreadable: fall back to defaults. A corrupt pointer is
        // quarantined by the store; a missing one on first boot is expected.
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
            return self.profile_dir(idx).is_dir();
        }
        self.profile_path(probe, idx).is_file()
    }

    /// Copy one file to another, creating the destination parent dir.
    pub fn copy_file(&self, src: &Path, dst: &Path) -> Result<(), ProfileError> {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
        Ok(())
    }

    /// Removes a directory tree recursively.
    pub fn remove_dir_recursive(&self, path: &Path) -> Result<(), ProfileError> {
        if path.exists() {
            fs::remove_dir_all(path)?;
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
            // A failed boot migration left the real save at the root files. Retry it
            // instead of force-flipping the flag.
            self.migrate_if_needed()?;
        }
        if self.migrated {
            self.legacy_fallback = false;
        }
        self.active = idx;
        self.persist_pointer()?;
        fs::create_dir_all(self.profile_dir(self.active))?;
        Ok(())
    }

    /// Ensure a profile directory exists (fresh profile).
    pub fn create_profile(&self, idx: usize) -> Result<(), ProfileError> {
        let idx = idx.clamp(1, self.num_profiles);
        fs::create_dir_all(self.profile_dir(idx))?;
        Ok(())
    }

    /// Archive the profile directory under the backup dir (timestamped) instead of
    /// hard-deleting, then prune old archives. The archive path is returned. If the
    /// directory doesn't exist, returns `Ok(None)`.
    pub fn clear_profile(&self, idx: usize) -> Result<Option<PathBuf>, ProfileError> {
        let idx = idx.clamp(1, self.num_profiles);
        let dir = self.profile_dir(idx);
        if !dir.is_dir() {
            return Ok(None);
        }
        fs::create_dir_all(self.backup_dir())?;
        let stamp = unix_now();
        let dest = self
            .backup_dir()
            .join(format!("{CLEARED_PREFIX}{idx}_{stamp}"));
        fs::rename(&dir, &dest)?;
        self.prune_cleared_archives()?;
        Ok(Some(dest))
    }

    fn backup_dir(&self) -> PathBuf {
        self.base_dir.join(BACKUP_DIR)
    }

    /// Prune cleared-profile archives down to `max_cleared_archives`, oldest first.
    pub fn prune_cleared_archives(&self) -> Result<(), ProfileError> {
        let Some(entries) = fs::read_dir(self.backup_dir()).ok() else {
            return Ok(());
        };
        let mut archives: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| {
                e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    && e.file_name().to_string_lossy().starts_with(CLEARED_PREFIX)
            })
            .map(|e| e.path())
            .collect();
        archives.sort_by_key(|p| p.to_string_lossy().into_owned());
        while archives.len() > self.max_cleared_archives {
            let oldest = archives.remove(0);
            let _ = fs::remove_dir_all(oldest);
        }
        Ok(())
    }

    fn migrate_if_needed(&mut self) -> Result<(), ProfileError> {
        if self.migrated {
            return Ok(());
        }
        let probe = self.probe_file();
        if probe.is_empty() {
            // Nothing is configured to migrate; treat as migrated.
            self.migrated = true;
            self.migrated_legacy = false;
            return self.persist_pointer();
        }
        // If profile_1 already has a save, migration ran on a previous boot and the flag
        // was simply lost - adopt it. NEVER re-run: the atomic rename onto the existing
        // profile_1 would fail and drop us into legacy-fallback (cross-profile corruption).
        if self.profile_dir(1).join(probe).exists() {
            self.migrated = true;
            self.migrated_legacy = true; // a pre-existing profile_1 means data carried over before
            return self.persist_pointer();
        }
        let legacy: Vec<String> = self
            .legacy_files
            .iter()
            .filter(|f| self.base_dir.join(f).is_file())
            .cloned()
            .collect();
        if legacy.is_empty() {
            // Fresh install: nothing to migrate, no demo carry-over.
            self.migrated = true;
            self.migrated_legacy = false;
            return self.persist_pointer();
        }
        self.run_migration(&legacy)
    }

    fn run_migration(&mut self, legacy: &[String]) -> Result<(), ProfileError> {
        // 1) Untouched backup first.
        let backup = self.backup_dir().join(PRE_MIGRATION_BACKUP_DIR);
        fs::create_dir_all(&backup)?;
        for f in legacy {
            let _ = fs::copy(self.base_dir.join(f), backup.join(f));
        }

        // 2) Stage copies under a temporary name; verify each.
        let staging = self.base_dir.join("profile_1_migrating");
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging)?;
        for f in legacy {
            let src = self.base_dir.join(f);
            let dst = staging.join(f);
            if let Err(e) = fs::copy(&src, &dst) {
                let _ = fs::remove_dir_all(&staging);
                self.legacy_fallback = true;
                return Err(ProfileError::Io(e));
            }
            if !self.verify_copy(&src, &dst) {
                let _ = fs::remove_dir_all(&staging);
                self.legacy_fallback = true;
                return Err(ProfileError::MigrationFailed("copy/verify"));
            }
        }

        // 3) Atomic commit: a single directory rename. A prior failed boot may have left an
        //    empty profile_1 behind (Windows can't rename over an existing dir). It can only
        //    be stale here - any profile_1 containing the probe file was adopted above - so
        //    clear it before renaming.
        let profile_1 = self.profile_dir(1);
        let probe = self.probe_file();
        if profile_1.is_dir() && !profile_1.join(probe).exists() {
            let _ = fs::remove_dir_all(&profile_1);
        }
        if let Err(e) = fs::rename(&staging, &profile_1) {
            let _ = fs::remove_dir_all(&staging);
            self.legacy_fallback = true;
            return Err(ProfileError::Io(e));
        }

        // 4) Originals are left in place as a second backup; never deleted.
        self.migrated = true;
        self.migrated_legacy = true;
        self.persist_pointer()
    }

    fn verify_copy(&self, src: &Path, dst: &Path) -> bool {
        let ok_src = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
        let ok_dst = fs::metadata(dst).map(|m| m.len()).unwrap_or(0);
        if ok_src == 0 || ok_src != ok_dst {
            return false;
        }
        if dst.extension().and_then(|e| e.to_str()) == Some("ron") {
            let bytes = fs::read(dst).unwrap_or_default();
            if !ron_intact(&bytes) {
                return false;
            }
        }
        true
    }
}

fn validate_pointer(bytes: &[u8]) -> bool {
    ron::from_str::<PointerConfig>(&String::from_utf8_lossy(bytes)).is_ok()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Originals kept as a second backup.
        assert!(root.join("save.ron").is_file());
        // Backup copy under backups/pre_profiles.
        assert!(
            root.join(BACKUP_DIR)
                .join(PRE_MIGRATION_BACKUP_DIR)
                .join("save.ron")
                .is_file()
        );
        // Re-init is a no-op.
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
        // The corrupt pointer was quarantined aside, not deleted.
        let corrupted: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("corrupted_"))
            .collect();
        assert!(!corrupted.is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
