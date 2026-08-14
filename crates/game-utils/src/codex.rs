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

/// A discovery ledger keyed by stable string ids. Pure data: serialize it into a game save
/// or persist it via [`CodexStore`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Codex {
    pub entries: BTreeMap<String, CodexEntry>,
}

impl Codex {
    /// Whether `id` has been discovered.
    pub fn is_discovered(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|e| e.discovered)
    }

    /// Mark `id` discovered. Returns `true` if this changed its state (newly discovered).
    pub fn mark_discovered(&mut self, id: &str) -> bool {
        if !self.is_discovered(id) {
            self.entries.entry(id.to_string()).or_default().discovered = true;
            return true;
        }
        false
    }

    /// The best recorded value for `id`, if any.
    pub fn best(&self, id: &str) -> Option<f32> {
        self.entries.get(id).and_then(|e| e.best)
    }

    /// Record `value` against `id`, keeping the highest. Returns `true` if the stored best
    /// changed (i.e. `value` beat it).
    pub fn record_best(&mut self, id: &str, value: f32) -> bool {
        let e = self.entries.entry(id.to_string()).or_default();
        if e.best.is_none_or(|b| value > b) {
            e.best = Some(value);
            return true;
        }
        false
    }

    /// The counter for `id` (0 if never recorded).
    pub fn count(&self, id: &str) -> u64 {
        self.entries.get(id).map_or(0, |e| e.count)
    }

    /// Bump `id`'s counter by `by`. Returns the new count.
    pub fn increment_count(&mut self, id: &str, by: u64) -> u64 {
        let e = self.entries.entry(id.to_string()).or_default();
        e.count = e.count.saturating_add(by);
        e.count
    }

    /// Set `id`'s counter exactly.
    pub fn set_count(&mut self, id: &str, count: u64) {
        self.entries.entry(id.to_string()).or_default().count = count;
    }

    /// The full entry for `id`.
    pub fn entry(&self, id: &str) -> Option<&CodexEntry> {
        self.entries.get(id)
    }

    /// Mutable entry for `id`, creating it if absent.
    pub fn entry_mut(&mut self, id: &str) -> &mut CodexEntry {
        self.entries.entry(id.to_string()).or_default()
    }

    /// Iterate over `(id, entry)` pairs.
    pub fn iter(&self) -> btree_map::Iter<'_, String, CodexEntry> {
        self.entries.iter()
    }

    /// Ids that have been discovered, in key order.
    pub fn discovered_ids(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|(_, e)| e.discovered)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Merge `other` in: union `discovered`/`best`/`count` per id (best stays max, counts
    /// add). Used when combining per-session ledgers back into a persisted one.
    pub fn merge(&mut self, other: &Self) {
        for (id, other_e) in &other.entries {
            let e = self.entries.entry(id.clone()).or_default();
            e.discovered |= other_e.discovered;
            if e.best.is_none_or(|b| other_e.best.is_some_and(|o| o > b)) {
                e.best = other_e.best;
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
#[derive(Debug, Clone)]
pub struct CodexStore {
    store: SaveStore,
    codex: Codex,
    loaded: bool,
    loaded_from: std::path::PathBuf,
}

impl CodexStore {
    /// Create a store for `file_name` inside `dir`.
    pub fn new(dir: impl Into<std::path::PathBuf>, file_name: impl Into<String>) -> Self {
        Self {
            store: SaveStore::new(dir, file_name).with_validator(codex_intact),
            codex: Codex::default(),
            loaded: false,
            loaded_from: std::path::PathBuf::new(),
        }
    }

    /// The underlying crash-safe store (exposes path/delete).
    pub fn store(&self) -> &SaveStore {
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
        }
        self.loaded = true;
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

    // Convenience delegation onto the inner ledger.

    /// Whether `id` has been discovered.
    pub fn is_discovered(&self, id: &str) -> bool {
        self.codex.is_discovered(id)
    }

    /// Mark `id` discovered. Returns `true` if newly discovered.
    pub fn mark_discovered(&mut self, id: &str) -> bool {
        self.codex.mark_discovered(id)
    }

    /// Best recorded value for `id`.
    pub fn best(&self, id: &str) -> Option<f32> {
        self.codex.best(id)
    }

    /// Record `value` against `id`, keeping the highest. Returns `true` if it changed.
    pub fn record_best(&mut self, id: &str, value: f32) -> bool {
        self.codex.record_best(id, value)
    }

    /// Counter for `id`.
    pub fn count(&self, id: &str) -> u64 {
        self.codex.count(id)
    }

    /// Bump `id`'s counter, returning the new count.
    pub fn increment_count(&mut self, id: &str, by: u64) -> u64 {
        self.codex.increment_count(id, by)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(c.mark_discovered("enemy_1"));
        assert!(c.is_discovered("enemy_1"));
        // Second mark is a no-op.
        assert!(!c.mark_discovered("enemy_1"));
        assert!(!c.is_discovered("enemy_2"));
        assert_eq!(c.discovered_ids(), vec!["enemy_1"]);
    }

    #[test]
    fn best_keeps_max_and_counts_accumulate() {
        let mut c = Codex::default();
        assert!(c.record_best("bodypart_a", 2.0));
        assert_eq!(c.best("bodypart_a"), Some(2.0));
        // Lower value doesn't regress.
        assert!(!c.record_best("bodypart_a", 1.0));
        assert_eq!(c.best("bodypart_a"), Some(2.0));
        // Higher value wins.
        assert!(c.record_best("bodypart_a", 3.0));
        assert_eq!(c.best("bodypart_a"), Some(3.0));
        assert_eq!(c.count("bodypart_a"), 0);
        assert_eq!(c.increment_count("bodypart_a", 1), 1);
        assert_eq!(c.increment_count("bodypart_a", 2), 3);
        assert_eq!(c.count("bodypart_a"), 3);
    }

    #[test]
    fn merge_unions_fields() {
        let mut a = Codex::default();
        a.mark_discovered("x");
        a.record_best("x", 2.0);
        a.increment_count("x", 1);

        let mut b = Codex::default();
        b.record_best("x", 5.0); // better best, not discovered
        b.increment_count("x", 10);

        let mut c = Codex::default();
        c.merge(&a);
        c.merge(&b);
        assert!(c.is_discovered("x"));
        assert_eq!(c.best("x"), Some(5.0));
        assert_eq!(c.count("x"), 11);
    }

    #[test]
    fn store_roundtrips_ron() {
        let dir = tmp_dir("roundtrip");
        let mut s = CodexStore::new(&dir, "codex.ron");
        s.mark_discovered("enemy_1");
        s.record_best("bodypart_a", 3.0);
        s.increment_count("enemy_1", 7);
        s.save().unwrap();

        let mut s2 = CodexStore::new(&dir, "codex.ron");
        assert_eq!(s2.load(), LoadStatus::Ok);
        assert!(s2.is_loaded());
        assert!(s2.is_discovered("enemy_1"));
        assert_eq!(s2.best("bodypart_a"), Some(3.0));
        assert_eq!(s2.count("enemy_1"), 7);
        s2.store().delete();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_missing_stays_empty_best() {
        let dir = tmp_dir("missing");
        let mut s = CodexStore::new(&dir, "codex.ron");
        assert_eq!(s.load(), LoadStatus::Missing);
        assert!(s.codex().is_empty());
        assert!(!s.is_discovered("x"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_recovers_corrupt_from_template() {
        let dir = tmp_dir("recover");
        let mut s = CodexStore::new(&dir, "codex.ron");
        s.increment_count("enemy", 4);
        s.save().unwrap();
        // Second save rotates the first copy into a .bak so recovery has a fallback.
        s.save().unwrap();
        let path = s.store().path();
        std::fs::write(&path, b"garbage").unwrap();
        let mut s2 = CodexStore::new(&dir, "codex.ron");
        assert_eq!(s2.load(), LoadStatus::Corrupt);
        assert_eq!(s2.count("enemy"), 4); // recovered from the .bak
        s2.store().delete();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
