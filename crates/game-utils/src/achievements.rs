use crate::stats::{Aggregation, StatsStore, aggregate};
use std::collections::HashSet;

/// Storage backends an achievement persists to. The registry reconciles a "primary" saved
/// source against a "secondary" (server) source so both stay in sync.
pub trait AchievementBackend {
    /// Whether `id` is unlocked in this backend.
    fn is_unlocked(&self, id: &str) -> bool;
    /// Mark `id` unlocked in this backend.
    fn unlock(&mut self, id: &str);
}

/// A backend with no external state.
impl AchievementBackend for HashSet<String> {
    fn is_unlocked(&self, id: &str) -> bool {
        self.contains(id)
    }
    fn unlock(&mut self, id: &str) {
        self.insert(id.to_string());
    }
}

#[derive(Debug, Clone)]
pub enum AchievementCondition {
    Stat {
        stat_id: String,
        aggregation: Aggregation,
        threshold: f32,
        /// When true, evaluate against the global best across all categories.
        global: bool,
    },
}

impl AchievementCondition {
    pub fn stat(
        stat_id: impl Into<String>,
        aggregation: Aggregation,
        threshold: f32,
        global: bool,
    ) -> Self {
        Self::Stat {
            stat_id: stat_id.into(),
            aggregation,
            threshold,
            global,
        }
    }

    /// Compute the current value of the condition from the stats store.
    pub fn current_value(&self, store: &StatsStore, category: &str) -> Option<f32> {
        match self {
            Self::Stat {
                stat_id, global, ..
            } => {
                if *global {
                    store.best_global(stat_id)
                } else {
                    store.best(category, stat_id)
                }
            }
        }
    }

    fn is_reached(&self, store: &StatsStore, category: &str) -> bool {
        let Some(val) = self.current_value(store, category) else {
            return false;
        };
        match self {
            Self::Stat {
                aggregation,
                threshold,
                ..
            } => match aggregation {
                Aggregation::Max | Aggregation::Sum => val >= *threshold,
                Aggregation::Min => val <= *threshold,
                Aggregation::Any => val != 0.0,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct Achievement {
    pub id: String,
    pub title: String,
    pub description: String,
    pub condition: AchievementCondition,
}

impl Achievement {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        condition: AchievementCondition,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            condition,
        }
    }
}

/// Achievement registry that owns unlocked-state reconciliation between two backends
/// (e.g. local save + online) and can scan stats to auto-unlock stat achievements.
#[derive(Debug, Clone)]
pub struct AchievementRegistry {
    pub achievements: Vec<Achievement>,
    unlocked: HashSet<String>,
}

impl AchievementRegistry {
    pub fn new(achievements: Vec<Achievement>) -> Self {
        Self {
            achievements,
            unlocked: HashSet::new(),
        }
    }

    pub fn reconcile<A: AchievementBackend, B: AchievementBackend>(
        &mut self,
        saved: &mut A,
        external: &mut B,
    ) {
        for a in &self.achievements {
            let sid = saved.is_unlocked(&a.id);
            let ext = external.is_unlocked(&a.id);
            if sid && !ext {
                external.unlock(&a.id);
            }
            if ext && !sid {
                saved.unlock(&a.id);
            }
            if sid || ext {
                self.unlocked.insert(a.id.clone());
            }
        }
    }

    /// Scan stat achievements against the current stats; unlock any reached ones.
    /// Returns the ids newly unlocked by this call.
    pub fn update_from_stats(
        &mut self,
        store: &StatsStore,
        category: &str,
        saved: &mut impl AchievementBackend,
        external: &mut impl AchievementBackend,
    ) -> Vec<String> {
        let mut unlocked_now = Vec::new();
        for a in &self.achievements {
            if self.unlocked.contains(&a.id) {
                continue;
            }
            if a.condition.is_reached(store, category) {
                self.unlocked.insert(a.id.clone());
                saved.unlock(&a.id);
                external.unlock(&a.id);
                unlocked_now.push(a.id.clone());
            }
        }
        unlocked_now
    }

    /// Whether a stat value crosses an achievement's threshold, aggregating across
    /// every category if the condition is global.
    pub fn reached_with(
        &self,
        store: &StatsStore,
        stat_id: &str,
        aggregation: Aggregation,
        category: &str,
    ) -> bool {
        let values: Vec<Option<f32>> = store
            .category_iter()
            .map(|(_cat, map)| map.get(stat_id).copied())
            .collect();
        let global = aggregate(aggregation, &values);
        self.achievements.iter().any(|a| match &a.condition {
            AchievementCondition::Stat {
                stat_id: cid,
                aggregation: cagg,
                threshold,
                global: cglobal,
            } => {
                if cid != stat_id || cagg != &aggregation {
                    return false;
                }
                let val = if *cglobal {
                    global
                } else {
                    store.best(category, stat_id)
                };
                match val {
                    None => false,
                    Some(v) => match cagg {
                        Aggregation::Max | Aggregation::Sum => v >= *threshold,
                        Aggregation::Min => v <= *threshold,
                        Aggregation::Any => v != 0.0,
                    },
                }
            }
        })
    }

    pub fn is_unlocked(&self, id: &str) -> bool {
        self.unlocked.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Stat;

    fn store_with(stat_id: &str, value: f32) -> StatsStore {
        let mut store = StatsStore::default();
        let mut s = Stat::new(stat_id, Aggregation::Max);
        s.update(value);
        store.record("stats0", &s);
        store
    }

    #[test]
    fn reconcile_dual_backends() {
        let mut saved = HashSet::new();
        let mut external = HashSet::new();
        external.insert("a".to_string());
        let mut reg = AchievementRegistry::new(vec![
            Achievement::new(
                "a",
                "A",
                "d",
                AchievementCondition::stat("x", Aggregation::Max, 5.0, true),
            ),
            Achievement::new(
                "b",
                "B",
                "d",
                AchievementCondition::stat("x", Aggregation::Max, 5.0, true),
            ),
        ]);
        reg.reconcile(&mut saved, &mut external);
        assert!(reg.is_unlocked("a"));
        assert!(saved.contains("a")); // external-only -> saved
        assert!(!reg.is_unlocked("b"));
        assert!(!saved.contains("b")); // locked in both stays locked
        assert!(!external.contains("b"));
    }

    #[test]
    fn stat_achievement_auto_unlocks() {
        let store = store_with("boss", 10.0);
        let mut saved = HashSet::new();
        let mut external = HashSet::new();
        let mut reg = AchievementRegistry::new(vec![
            Achievement::new(
                "boss_1",
                "First Blood",
                "Beat a boss",
                AchievementCondition::stat("boss", Aggregation::Max, 5.0, true),
            ),
            Achievement::new(
                "boss_2",
                "Overkill",
                "Reach 20",
                AchievementCondition::stat("boss", Aggregation::Max, 20.0, true),
            ),
        ]);
        let new = reg.update_from_stats(&store, "stats0", &mut saved, &mut external);
        assert_eq!(new, vec!["boss_1"]);
        assert!(saved.contains("boss_1"));
        assert!(external.contains("boss_1"));
        assert!(!reg.is_unlocked("boss_2"));
    }

    #[test]
    fn existing_unlocks_are_kept() {
        let store = store_with("kills", 3.0);
        let mut saved = HashSet::new();
        saved.insert("kills_5".to_string());
        let mut external = HashSet::new();
        let mut reg = AchievementRegistry::new(vec![Achievement::new(
            "kills_5",
            "Killer",
            "5 kills",
            AchievementCondition::stat("kills", Aggregation::Sum, 5.0, true),
        )]);
        reg.reconcile(&mut saved, &mut external);
        assert!(reg.is_unlocked("kills_5"));
        // Already unlocked -> no new unlock on a later stats scan.
        let new = reg.update_from_stats(&store, "stats0", &mut saved, &mut external);
        assert!(new.is_empty());
    }
}
