use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Aggregation mode for a [`Stat`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Aggregation {
    #[default]
    Max,
    Min,
    Sum,
    Any,
}

/// Aggregate a sequence of values under a mode, ignoring `None` entries.
/// `Any` returns the value of any non-zero entry (loosely: "has ever been seen").
pub fn aggregate(agg: Aggregation, values: &[Option<f32>]) -> Option<f32> {
    let present: Vec<f32> = values.iter().filter_map(|v| *v).collect();
    if present.is_empty() {
        return None;
    }
    match agg {
        Aggregation::Max => present.into_iter().fold(f32::NEG_INFINITY, f32::max).into(),
        Aggregation::Min => present.into_iter().fold(f32::INFINITY, f32::min).into(),
        Aggregation::Sum => Some(present.into_iter().sum()),
        Aggregation::Any => present.into_iter().find(|v| *v != 0.0),
    }
}

/// A tracked game stat with an aggregation mode and a current in-session value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stat {
    pub id: String,
    pub aggregation: Aggregation,
    pub current: Option<f32>,
}

impl Stat {
    pub fn new(id: impl Into<String>, aggregation: Aggregation) -> Self {
        Self {
            id: id.into(),
            aggregation,
            current: None,
        }
    }

    /// Incorporate `value` into the current in-session aggregate. Returns whether the
    /// current value changed.
    pub fn update(&mut self, value: f32) -> bool {
        let prev = self.current;
        self.current = aggregate(self.aggregation, &[self.current, Some(value)]);
        self.current != prev
    }

    /// Reset the current value. SUM stats restart from their best so cumulative stats
    /// never regress across a run boundary.
    pub fn reset(&mut self, best: Option<f32>) {
        self.current = if self.aggregation == Aggregation::Sum {
            best
        } else {
            None
        };
    }

    /// Combine the current value with the persisted best (the cross-run best).
    pub fn best_with(&self, persisted: Option<f32>) -> Option<f32> {
        aggregate(self.aggregation, &[self.current, persisted])
    }
}

/// Holds the best value of every stat per category.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatsStore {
    /// category -> stat id -> best value
    by_category: HashMap<String, HashMap<String, f32>>,
}

impl StatsStore {
    /// Read the best persisted value of `stat` in `category`.
    pub fn best(&self, category: &str, stat_id: &str) -> Option<f32> {
        self.by_category.get(category)?.get(stat_id).copied()
    }

    /// Record the stat's current value into `category`, merging with the previous best
    /// under the stat's aggregation. Returns the new best.
    pub fn record(&mut self, category: &str, stat: &Stat) -> Option<f32> {
        let best = stat.best_with(self.best(category, &stat.id));
        if let Some(b) = best {
            self.by_category
                .entry(category.to_string())
                .or_default()
                .insert(stat.id.clone(), b);
        }
        best
    }

    /// Global best across all categories for a stat id.
    pub fn best_global(&self, stat_id: &str) -> Option<f32> {
        self.by_category
            .values()
            .filter_map(|m| m.get(stat_id).copied())
            .fold(None, |acc, v| Some(v.max(acc.unwrap_or(v))))
    }

    pub fn category_iter(&self) -> impl Iterator<Item = (&str, &HashMap<String, f32>)> {
        self.by_category.iter().map(|(c, m)| (c.as_str(), m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_modes() {
        let vals = [Some(3.0), None, Some(7.0), Some(2.0)];
        assert_eq!(aggregate(Aggregation::Max, &vals), Some(7.0));
        assert_eq!(aggregate(Aggregation::Min, &vals), Some(2.0));
        assert_eq!(aggregate(Aggregation::Sum, &vals), Some(12.0));
        assert_eq!(aggregate(Aggregation::Max, &[None, None]), None);
    }

    #[test]
    fn stat_update_and_best() {
        let mut s = Stat::new("hp", Aggregation::Max);
        assert!(s.update(5.0));
        assert_eq!(s.current, Some(5.0));
        assert!(!s.update(3.0)); // MAX keeps the max, so unchanged
        assert_eq!(s.current, Some(5.0));
        assert!(s.update(9.0));
        assert_eq!(s.best_with(Some(4.0)), Some(9.0));
    }

    #[test]
    fn sum_resets_to_best() {
        let mut s = Stat::new("kills", Aggregation::Sum);
        s.update(5.0);
        assert_eq!(s.current, Some(5.0));
        s.reset(Some(12.0));
        assert_eq!(s.current, Some(12.0));
    }

    #[test]
    fn store_tracks_per_category_best() {
        let mut store = StatsStore::default();
        let mut s = Stat::new("boss", Aggregation::Max);

        s.update(3.0);
        store.record("stats0", &s);
        s.update(1.0);
        store.record("stats0", &s); // MAX: stays 3

        s.current = None;
        store.record("stats1", &s); // inferred None -> no entry

        assert_eq!(store.best("stats0", "boss"), Some(3.0));
        assert_eq!(store.best("stats1", "boss"), None);
        assert_eq!(store.best_global("boss"), Some(3.0));
    }
}
