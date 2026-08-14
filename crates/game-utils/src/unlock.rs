use crate::stats::{Aggregation, StatsStore, aggregate};

/// Unlock condition gating on a stat's value against a threshold.
#[derive(Debug, Clone)]
pub struct UnlockCondition {
    /// Which stat id to read out of the [`StatsStore`].
    pub stat_id: String,
    pub aggregation: Aggregation,
    pub threshold: f32,
    /// When true, evaluate against the global best across all categories; otherwise
    /// evaluate against the selected category.
    pub global: bool,
    pub was_unlocked_before_run: bool,
}

impl UnlockCondition {
    pub fn new(
        stat_id: impl Into<String>,
        aggregation: Aggregation,
        threshold: f32,
        global: bool,
    ) -> Self {
        Self {
            stat_id: stat_id.into(),
            aggregation,
            threshold,
            global,
            was_unlocked_before_run: false,
        }
    }

    pub fn snapshot(&mut self, store: &StatsStore, category: &str) {
        self.was_unlocked_before_run = self.is_unlocked(store, category);
    }

    fn value(&self, store: &StatsStore, category: &str) -> Option<f32> {
        if self.global {
            return store.best_global(&self.stat_id);
        }
        store.best(category, &self.stat_id)
    }

    pub fn is_unlocked(&self, store: &StatsStore, category: &str) -> bool {
        let Some(val) = self.value(store, category) else {
            return false;
        };
        match self.aggregation {
            Aggregation::Max | Aggregation::Sum => val >= self.threshold,
            Aggregation::Min => val <= self.threshold,
            Aggregation::Any => val != 0.0,
        }
    }

    /// Percent progress toward the threshold for UI bars on the unlocked range,
    /// clamped to `[0,1]` (MIN reaches 1 at/under the threshold).
    pub fn progress(&self, store: &StatsStore, category: &str) -> f32 {
        let Some(val) = self.value(store, category) else {
            return 0.0;
        };
        match self.aggregation {
            Aggregation::Min if self.threshold != 0.0 => {
                (self.threshold / val.max(f32::MIN_POSITIVE)).min(1.0)
            }
            Aggregation::Any => f32::from(val != 0.0),
            _ => (val / self.threshold).clamp(0.0, 1.0),
        }
    }
}

/// Convenience: evaluate a whole list of conditions under one category, returning the
/// first (by `global` then `unlocked`) that unlocks.
pub fn any_unlocked(conditions: &[UnlockCondition], store: &StatsStore, category: &str) -> bool {
    conditions.iter().any(|c| c.is_unlocked(store, category))
}

/// Aggregate multiple per-parasite condition results into a single best value,
/// reused by achievement gating.
pub fn best_across_categories(
    store: &StatsStore,
    stat_id: &str,
    aggregation: Aggregation,
) -> Option<f32> {
    let values: Vec<Option<f32>> = store
        .category_iter()
        .map(|(_cat, map)| map.get(stat_id).copied())
        .collect();
    aggregate(aggregation, &values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Stat;

    fn seeded_store() -> StatsStore {
        let mut store = StatsStore::default();
        let mut s = Stat::new("boss", Aggregation::Max);
        s.update(10.0);
        store.record("stats0", &s);
        s.current = None;
        store.record("stats1", &s);
        store
    }

    #[test]
    fn unlocked_vs_threshold() {
        let store = seeded_store();
        let c = UnlockCondition::new("boss", Aggregation::Max, 5.0, true);
        assert!(c.is_unlocked(&store, "stats0"));
        let c2 = UnlockCondition::new("boss", Aggregation::Max, 15.0, true);
        assert!(!c2.is_unlocked(&store, "stats0"));
    }

    #[test]
    fn global_vs_per_category() {
        let store = seeded_store();
        // stats1 has no entry (None), so per-category fails while global passes.
        let c = UnlockCondition::new("boss", Aggregation::Max, 10.0, false);
        assert!(!c.is_unlocked(&store, "stats1"));
        let c_global = UnlockCondition::new("boss", Aggregation::Max, 10.0, true);
        assert!(c_global.is_unlocked(&store, "stats1"));
    }

    #[test]
    fn snapshot_flags_new_unlock() {
        let store = seeded_store();
        let mut c = UnlockCondition::new("boss", Aggregation::Max, 10.0, true);
        assert!(!c.was_unlocked_before_run);
        c.snapshot(&store, "stats0");
        assert!(c.was_unlocked_before_run);
    }

    #[test]
    fn progress_clamped() {
        let store = seeded_store();
        let c = UnlockCondition::new("boss", Aggregation::Max, 5.0, true);
        assert_eq!(c.progress(&store, "stats0"), 1.0);
        let c2 = UnlockCondition::new("boss", Aggregation::Max, 20.0, true);
        assert!((c2.progress(&store, "stats0") - 0.5).abs() < 1e-3);
    }
}
