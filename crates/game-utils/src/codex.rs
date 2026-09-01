//! Genre-agnostic discovery ledger ("codex"), extracted from a roguelite's codex tracking.
//!
//! Tracks per-entity metadata by stable string id: whether it has been discovered (seen),
//! the best value ever recorded (e.g. the highest rarity tier observed), and a running
//! counter (e.g. kill counts). The source game used three separate dictionaries for these
//! (discoveries, mutation discoveries, enemy discoveries + kill counts) - here one
//! [`CodexEntry`] covers all three so any content type (enemies, upgrades, landables, ...)
//! maps to a single registry.
//!
//! [`Codex`] is pure serde data (no I/O) so it can be embedded in an existing save.
//! [`CodexStore`] wraps it with a crash-safe RON file on top of [`crate::save_store::SaveStore`]
//! when standalone persistence is preferred.

use std::collections::{BTreeMap, btree_map};

use serde::{Deserialize, Serialize};

use crate::save_store::{LoadStatus, SaveStore};
use crate::storage::FsStorage;
use crate::typed_id::CodexId;

/// Per-id metadata tracked by the ledger.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CodexEntry {
    /// Whether the id has been seen/discovered at least once.
    #[serde(default)]
    pub discovered: bool,
    /// Best value ever recorded (max-aggregated), e.g. highest rarity tier.
    #[serde(default)]
    pub best: Option<f32>,
    /// Running counter, e.g. kill/enemy count.
    #[serde(default)]
    pub count: u64,
}

impl CodexEntry {
    /// A completely blank entry (no value worth persisting).
    pub fn is_empty(&self) -> bool {
        !self.discovered && self.best.is_none() && self.count == 0
    }
}

/// A discovery ledger keyed by stable typed ids. Pure data: serialize it into a game save
/// or persist it via [`CodexStore`]. Keeps Ron as ` { "enemy_1": (... ) } ` via `CodexId` transparent ser.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Codex {
    pub entries: BTreeMap<CodexId, CodexEntry>,
}

impl Codex {
    /// Whether `id` has been discovered.
    pub fn is_discovered(&self, id: &CodexId) -> bool {
        self.entries.get(id).is_some_and(|e| e.discovered)
    }
    /// Keep string shim for migration (`Borrow<str>` still works at map level, but surface is now typed).
    pub fn is_discovered_str(&self, id: &str) -> bool {
        self.is_discovered(&CodexId::new(id))
    }

    /// Mark `id` discovered. Returns `true` if this changed its state (newly discovered).
    pub fn mark_discovered(&mut self, id: &CodexId) -> bool {
        if !self.is_discovered(id) {
            self.entries.entry(id.clone()).or_default().discovered = true;
            return true;
        }
        false
    }
    pub fn mark_discovered_str(&mut self, id: &str) -> bool {
        self.mark_discovered(&CodexId::new(id))
    }

    /// The best recorded value for `id`, if any.
    pub fn best(&self, id: &CodexId) -> Option<f32> {
        self.entries.get(id).and_then(|e| e.best)
    }
    pub fn best_str(&self, id: &str) -> Option<f32> {
        self.best(&CodexId::new(id))
    }

    /// Record `value` against `id`, keeping the highest. Returns `true` if the stored best
    /// changed (i.e. `value` beat it). Non-finite values (`NaN`/`inf`) are ignored.
    pub fn record_best(&mut self, id: &CodexId, value: f32) -> bool {
        if !value.is_finite() {
            return false;
        }
        let e = self.entries.entry(id.clone()).or_default();
        if e.best.is_none_or(|b| !b.is_finite() || value > b) {
            e.best = Some(value);
            return true;
        }
        false
    }
    pub fn record_best_str(&mut self, id: &str, value: f32) -> bool {
        self.record_best(&CodexId::new(id), value)
    }

    /// The counter for `id` (0 if never recorded).
    pub fn count(&self, id: &CodexId) -> u64 {
        self.entries.get(id).map_or(0, |e| e.count)
    }
    pub fn count_str(&self, id: &str) -> u64 {
        self.count(&CodexId::new(id))
    }

    /// Bump `id`'s counter by `by`. Returns the new count.
    pub fn increment_count(&mut self, id: &CodexId, by: u64) -> u64 {
        let e = self.entries.entry(id.clone()).or_default();
        e.count = e.count.saturating_add(by);
        e.count
    }
    pub fn increment_count_str(&mut self, id: &str, by: u64) -> u64 {
        self.increment_count(&CodexId::new(id), by)
    }

    /// Set `id`'s counter exactly.
    pub fn set_count(&mut self, id: &CodexId, count: u64) {
        self.entries.entry(id.clone()).or_default().count = count;
    }
    pub fn set_count_str(&mut self, id: &str, count: u64) {
        self.set_count(&CodexId::new(id), count)
    }

    /// The full entry for `id`.
    pub fn entry(&self, id: &CodexId) -> Option<&CodexEntry> {
        self.entries.get(id)
    }
    pub fn entry_str(&self, id: &str) -> Option<&CodexEntry> {
        self.entry(&CodexId::new(id))
    }

    /// Mutable entry for `id`, creating it if absent.
    pub fn entry_mut(&mut self, id: &CodexId) -> &mut CodexEntry {
        self.entries.entry(id.clone()).or_default()
    }
    pub fn entry_mut_str(&mut self, id: &str) -> &mut CodexEntry {
        self.entry_mut(&CodexId::new(id))
    }

    /// Iterate over `(id, entry)` pairs.
    pub fn iter(&self) -> btree_map::Iter<'_, CodexId, CodexEntry> {
        self.entries.iter()
    }

    /// Typed discovered ids, in key order (golden).
    pub fn discovered_ids(&self) -> Vec<CodexId> {
        self.entries
            .iter()
            .filter(|(_, e)| e.discovered)
            .map(|(id, _)| id.clone())
            .collect()
    }
    /// String shim for `discovered_ids` keep `&str` view.
    pub fn discovered_ids_str(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|(_, e)| e.discovered)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Merge `other` in: union `discovered`/`best`/`count` per id (best stays max, counts
    /// add). Used when combining per-session ledgers back into a persisted one.
    /// Non-finite `best` values are ignored (old corrupt saves may have stored `NaN`).
    pub fn merge(&mut self, other: &Self) {
        for (id, other_e) in &other.entries {
            let e = self.entries.entry(id.clone()).or_default();
            e.discovered |= other_e.discovered;
            if let Some(o) = other_e.best.filter(|v| v.is_finite())
                && e.best.is_none_or(|b| !b.is_finite() || o > b)
            {
                e.best = Some(o);
            }
            e.count = e.count.saturating_add(other_e.count);
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the ledger holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Validator for a persisted [`Codex`] RON file.
fn codex_intact(bytes: &[u8]) -> bool {
    ron::from_str::<Codex>(&String::from_utf8_lossy(bytes)).is_ok()
}

/// A [`Codex`] persisted as a crash-safe RON file, built on [`crate::save_store::SaveStore`].
/// Generic over `Storage` (keeps Ron codec).
#[derive(Debug, Clone)]
pub struct CodexStore<S: crate::storage::Storage = FsStorage> {
    store: SaveStore<S>,
    codex: Codex,
    loaded: bool,
    loaded_from: std::path::PathBuf,
}

impl CodexStore<FsStorage> {
    /// Create a store for `file_name` inside `dir` (FsStorage).
    pub fn new(dir: impl Into<std::path::PathBuf>, file_name: impl Into<String>) -> Self {
        Self {
            store: SaveStore::new(dir, file_name).with_validator(codex_intact),
            codex: Codex::default(),
            loaded: false,
            loaded_from: std::path::PathBuf::new(),
        }
    }
}

impl<S: crate::storage::Storage> CodexStore<S> {
    pub fn new_with_storage(
        dir: impl Into<std::path::PathBuf>,
        file_name: impl Into<String>,
        storage: S,
    ) -> Self {
        Self {
            store: SaveStore::new_with_storage(dir, file_name, storage)
                .with_validator(codex_intact),
            codex: Codex::default(),
            loaded: false,
            loaded_from: std::path::PathBuf::new(),
        }
    }

    /// The underlying crash-safe store (exposes path/delete).
    pub fn store(&self) -> &SaveStore<S> {
        &self.store
    }

    /// Load the ledger from disk with recover-on-corrupt semantics of [`SaveStore`]. If the
    /// file is missing or unreadable the ledger stays empty and the status reflects why.
    /// Idempotent per matching path.
    pub fn load(&mut self) -> LoadStatus {
        let res = self.store.load(&codex_intact, &[]);
        if let Some(bytes) = &res.data
            && let Ok(c) = ron::from_str::<Codex>(&String::from_utf8_lossy(bytes))
        {
            self.codex = c;
            self.loaded = true;
            self.loaded_from = self.store.path();
        } else {
            // If missing/corrupt/unreadable with no recovery.
            self.loaded = matches!(res.status, LoadStatus::Ok);
            if !self.loaded {
                self.codex = Codex::default();
            }
        }
        res.status
    }

    /// Whether a valid ledger was successfully loaded into memory.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Immutable access to the in-memory ledger.
    pub fn codex(&self) -> &Codex {
        &self.codex
    }

    /// Mutable access to the in-memory ledger. Call [`Self::save`] to persist.
    pub fn codex_mut(&mut self) -> &mut Codex {
        &mut self.codex
    }

    /// Serialize the in-memory ledger to the store (crash-safe write).
    pub fn save(&self) -> Result<(), String> {
        let s = ron::ser::to_string_pretty(&self.codex, Default::default())
            .map_err(|e| e.to_string())?;
        self.store.write(s.as_bytes())
    }

    // Typed delegation onto the inner ledger (golden). String shims keep compat.

    pub fn is_discovered(&self, id: &CodexId) -> bool {
        self.codex.is_discovered(id)
    }
    pub fn is_discovered_str(&self, id: &str) -> bool {
        self.codex.is_discovered_str(id)
    }

    pub fn mark_discovered(&mut self, id: &CodexId) -> bool {
        self.codex.mark_discovered(id)
    }
    pub fn mark_discovered_str(&mut self, id: &str) -> bool {
        self.codex.mark_discovered_str(id)
    }

    pub fn best(&self, id: &CodexId) -> Option<f32> {
        self.codex.best(id)
    }
    pub fn best_str(&self, id: &str) -> Option<f32> {
        self.codex.best_str(id)
    }

    pub fn record_best(&mut self, id: &CodexId, value: f32) -> bool {
        self.codex.record_best(id, value)
    }
    pub fn record_best_str(&mut self, id: &str, value: f32) -> bool {
        self.codex.record_best_str(id, value)
    }

    pub fn count(&self, id: &CodexId) -> u64 {
        self.codex.count(id)
    }
    pub fn count_str(&self, id: &str) -> u64 {
        self.codex.count_str(id)
    }

    pub fn increment_count(&mut self, id: &CodexId, by: u64) -> u64 {
        self.codex.increment_count(id, by)
    }
    pub fn increment_count_str(&mut self, id: &str, by: u64) -> u64 {
        self.codex.increment_count_str(id, by)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_id::CodexId;
    use std::path::PathBuf;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("game_utils_codex_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn discovery_lifecycle() {
        let mut c = Codex::default();
        let a = CodexId::new("enemy_1");
        let b = CodexId::new("enemy_2");
        assert!(c.mark_discovered(&a));
        assert!(c.is_discovered(&a));
        assert!(!c.mark_discovered(&a));
        assert!(!c.is_discovered(&b));
        assert_eq!(c.discovered_ids(), vec![a]);
    }

    #[test]
    fn best_keeps_max_and_counts_accumulate() {
        let mut c = Codex::default();
        let id = CodexId::new("bodypart_a");
        assert!(c.record_best(&id, 2.0));
        assert_eq!(c.best(&id), Some(2.0));
        assert!(!c.record_best(&id, 1.0));
        assert_eq!(c.best(&id), Some(2.0));
        assert!(c.record_best(&id, 3.0));
        assert_eq!(c.best(&id), Some(3.0));
        assert_eq!(c.count(&id), 0);
        assert_eq!(c.increment_count(&id, 1), 1);
        assert_eq!(c.increment_count(&id, 2), 3);
        assert_eq!(c.count(&id), 3);
    }

    #[test]
    fn merge_unions_fields() {
        let id = CodexId::new("x");
        let mut a = Codex::default();
        a.mark_discovered(&id);
        a.record_best(&id, 2.0);
        a.increment_count(&id, 1);
        let mut b = Codex::default();
        b.record_best(&id, 5.0);
        b.increment_count(&id, 10);
        let mut c = Codex::default();
        c.merge(&a);
        c.merge(&b);
        assert!(c.is_discovered(&id));
        assert_eq!(c.best(&id), Some(5.0));
        assert_eq!(c.count(&id), 11);
    }

    #[test]
    fn store_roundtrips_ron() {
        let dir = tmp_dir("roundtrip");
        let mut s = CodexStore::new(&dir, "codex.ron");
        s.mark_discovered(&CodexId::new("enemy_1"));
        s.record_best(&CodexId::new("bodypart_a"), 3.0);
        s.increment_count(&CodexId::new("enemy_1"), 7);
        s.save().unwrap();
        let mut s2 = CodexStore::new(&dir, "codex.ron");
        assert_eq!(s2.load(), LoadStatus::Ok);
        assert!(s2.is_loaded());
        assert!(s2.is_discovered(&CodexId::new("enemy_1")));
        assert_eq!(s2.best(&CodexId::new("bodypart_a")), Some(3.0));
        assert_eq!(s2.count(&CodexId::new("enemy_1")), 7);
        s2.store().delete();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_missing_stays_empty_best() {
        let dir = tmp_dir("missing");
        let mut s = CodexStore::new(&dir, "codex.ron");
        assert_eq!(s.load(), LoadStatus::Missing);
        assert!(s.codex().is_empty());
        assert!(!s.is_discovered(&CodexId::new("x")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_recovers_corrupt_from_template() {
        let dir = tmp_dir("recover");
        let mut s = CodexStore::new(&dir, "codex.ron");
        s.increment_count(&CodexId::new("enemy"), 4);
        s.save().unwrap();
        s.save().unwrap();
        let path = s.store().path();
        std::fs::write(&path, b"garbage").unwrap();
        let mut s2 = CodexStore::new(&dir, "codex.ron");
        assert_eq!(s2.load(), LoadStatus::Corrupt);
        assert_eq!(s2.count(&CodexId::new("enemy")), 4);
        s2.store().delete();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
