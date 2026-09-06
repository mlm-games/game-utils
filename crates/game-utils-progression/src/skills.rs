//! Skill unlock graphs: parent-gated nodes with ranks and a point purse.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One unlockable node. Parents need rank >= 1 each.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SkillDef {
    pub id: String,
    pub max_rank: u32,
    pub requires: Vec<String>,
}

impl SkillDef {
    pub fn new(id: impl Into<String>, max_rank: u32, requires: Vec<String>) -> Self {
        Self {
            id: id.into(),
            max_rank: max_rank.max(1),
            requires,
        }
    }
}

/// Spend refusal.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SpendError {
    NoPoints,
    Maxed,
    Locked(Vec<String>),
}

/// Rank map plus spendable points.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct SkillSet {
    pub ranks: HashMap<String, u32>,
    pub points: u32,
}

impl SkillSet {
    pub fn new(points: u32) -> Self {
        Self {
            ranks: HashMap::new(),
            points,
        }
    }

    pub fn rank(&self, id: &str) -> u32 {
        self.ranks.get(id).copied().unwrap_or(0)
    }

    pub fn unmet(&self, def: &SkillDef) -> Vec<String> {
        def.requires
            .iter()
            .filter(|r| self.rank(r) == 0)
            .cloned()
            .collect()
    }

    /// Spend one point into `def`. Ranks clamp at max.
    pub fn spend(&mut self, def: &SkillDef) -> Result<u32, SpendError> {
        let unmet = self.unmet(def);
        if !unmet.is_empty() {
            return Err(SpendError::Locked(unmet));
        }
        if self.rank(&def.id) >= def.max_rank {
            return Err(SpendError::Maxed);
        }
        if self.points == 0 {
            return Err(SpendError::NoPoints);
        }
        self.points -= 1;
        let r = self.rank(&def.id) + 1;
        self.ranks.insert(def.id.clone(), r);
        Ok(r)
    }

    /// Refund one rank, returning the point. False when untrained.
    pub fn refund(&mut self, id: &str) -> bool {
        match self.ranks.get_mut(id) {
            Some(r) if *r > 0 => {
                *r -= 1;
                self.points += 1;
                if *r == 0 {
                    self.ranks.remove(id);
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> Vec<SkillDef> {
        vec![
            SkillDef::new("root", 1, vec![]),
            SkillDef::new("fire", 3, vec!["root".into()]),
            SkillDef::new("inferno", 1, vec!["fire".into()]),
        ]
    }

    #[test]
    fn gates_ranks_points() {
        let ds = defs();
        let mut s = SkillSet::new(4);
        assert!(matches!(s.spend(&ds[1]), Err(SpendError::Locked(_))));
        assert_eq!(s.spend(&ds[0]), Ok(1));
        assert_eq!(s.spend(&ds[0]), Err(SpendError::Maxed));
        assert_eq!(s.spend(&ds[1]), Ok(1));
        assert_eq!(s.spend(&ds[1]), Ok(2));
        assert_eq!(s.spend(&ds[2]), Ok(1));
        assert_eq!(s.spend(&ds[1]), Err(SpendError::NoPoints));
    }

    #[test]
    fn refund_returns_point() {
        let ds = defs();
        let mut s = SkillSet::new(1);
        s.spend(&ds[0]).unwrap();
        assert!(s.refund("root"));
        assert_eq!((s.points, s.rank("root")), (1, 0));
        assert!(!s.refund("root"));
    }
}
