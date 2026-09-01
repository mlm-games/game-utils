use crate::stats::{Aggregation, StatsStore, aggregate};
use crate::typed_id::{AchievementId, StatId};
use std::collections::HashSet;

/// Storage backends an achievement persists to.
pub trait AchievementBackend {
    fn is_unlocked(&self, id: &AchievementId) -> bool;
    fn unlock(&mut self, id: &AchievementId);
}

/// A backend with no external state.
impl AchievementBackend for HashSet<AchievementId> {
    fn is_unlocked(&self, id: &AchievementId) -> bool {
        self.contains(id)
    }
    fn unlock(&mut self, id: &AchievementId) {
        self.insert(id.clone());
    }
}

#[derive(Debug, Clone)]
pub enum AchievementCondition {
    Stat {
        stat_id: StatId,
        aggregation: Aggregation,
        threshold: f32,
        /// When true, evaluate against the global best across all categories.
        global: bool,
    },
}

impl AchievementCondition {
    pub fn stat(
        stat_id: impl Into<StatId>,
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
                stat_id,
                aggregation,
                global,
                ..
            } => {
                if *global {
                    store.best_global_with(stat_id, *aggregation)
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
    pub id: AchievementId,
    pub title: String,
    pub description: String,
    pub condition: AchievementCondition,
}

impl Achievement {
    pub fn new(
        id: impl Into<AchievementId>,
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
    unlocked: HashSet<AchievementId>,
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
    /// Returns the ids newly unlocked by this call (typed `AchievementId` — keep Ron as string).
    pub fn update_from_stats(
        &mut self,
        store: &StatsStore,
        category: &str,
        saved: &mut impl AchievementBackend,
        external: &mut impl AchievementBackend,
    ) -> Vec<AchievementId> {
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
    /// every category if the condition is global.  Global aggregation is computed
    /// per-achievement (so mixed `Max`/`Any` achievements for the same `stat_id`
    /// don't share a wrong global value).
    pub fn reached_with(
        &self,
        store: &StatsStore,
        stat_id: &str,
        aggregation: Aggregation,
        category: &str,
    ) -> bool {
        self.achievements.iter().any(|a| match &a.condition {
            AchievementCondition::Stat {
                stat_id: cid,
                aggregation: cagg,
                threshold,
                global: cglobal,
            } => {
                if cid.as_str() != stat_id || cagg != &aggregation {
                    return false;
                }
                let val = if *cglobal {
                    let values: Vec<Option<f32>> = store
                        .category_iter()
                        .map(|(_cat, map)| map.get(stat_id).copied())
                        .collect();
                    aggregate(*cagg, &values)
                } else {
                    store.best(category, stat_id)
                };
                match val {
                    None => false,
                    Some(v) if !v.is_finite() => false,
                    Some(v) => match cagg {
                        Aggregation::Max | Aggregation::Sum => v >= *threshold,
                        Aggregation::Min => v <= *threshold,
                        Aggregation::Any => v != 0.0,
                    },
                }
            }
        })
    }

    /// Typed primary.
    pub fn is_unlocked(&self, id: &AchievementId) -> bool {
        self.unlocked.contains(id)
    }
    pub fn is_unlocked_str(&self, id: &str) -> bool {
        self.unlocked.contains(&AchievementId::new(id))
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
        let mut saved: HashSet<AchievementId> = HashSet::new();
        let mut external: HashSet<AchievementId> = HashSet::new();
        external.insert(AchievementId::new("a"));
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
        assert!(reg.is_unlocked(&AchievementId::new("a")));
        assert!(saved.contains("a")); // external-only -> saved (Borrow<str>)
        assert!(!reg.is_unlocked(&AchievementId::new("b")));
        assert!(!saved.contains("b"));
        assert!(!external.contains("b"));
    }

    #[test]
    fn stat_achievement_auto_unlocks() {
        let store = store_with("boss", 10.0);
        let mut saved: HashSet<AchievementId> = HashSet::new();
        let mut external: HashSet<AchievementId> = HashSet::new();
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
        assert_eq!(new, vec![AchievementId::new("boss_1")]);
        assert!(saved.contains("boss_1"));
        assert!(external.contains("boss_1"));
        assert!(!reg.is_unlocked(&AchievementId::new("boss_2")));
    }

    #[test]
    fn existing_unlocks_are_kept() {
        let store = store_with("kills", 3.0);
        let mut saved: HashSet<AchievementId> = HashSet::new();
        saved.insert(AchievementId::new("kills_5"));
        let mut external: HashSet<AchievementId> = HashSet::new();
        let mut reg = AchievementRegistry::new(vec![Achievement::new(
            "kills_5",
            "Killer",
            "5 kills",
            AchievementCondition::stat("kills", Aggregation::Sum, 5.0, true),
        )]);
        reg.reconcile(&mut saved, &mut external);
        assert!(reg.is_unlocked(&AchievementId::new("kills_5")));
        let new = reg.update_from_stats(&store, "stats0", &mut saved, &mut external);
        assert!(new.is_empty());
    }
}
