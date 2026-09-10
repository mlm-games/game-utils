//! Level thresholds and multi-level gain.

use serde::{Deserialize, Serialize};

/// XP needed to advance. Levels are 1-based; `need_for(l)` is the cost
/// to go from `l` to `l + 1`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Curve {
    /// Explicit per-level costs (level 1 first; last repeats).
    Table(Vec<u32>),
    /// `scale * (base + level)^exp` (e.g. base 3, exp 2 mirrors a
    /// well-known survivors-like).
    Power { base: f32, exp: f32, scale: f32 },
    /// Flat cost per level.
    Linear(u32),
}

/// XP to advance from `level` (1-based).
pub fn need_for(curve: &Curve, level: u32) -> u32 {
    let level = level.max(1);
    match curve {
        Curve::Table(t) => t
            .get(level as usize - 1)
            .or(t.last())
            .copied()
            .unwrap_or(100),
        Curve::Power { base, exp, scale } => (scale * (base + level as f32).powf(*exp)) as u32,
        Curve::Linear(n) => *n,
    }
}

/// A levelled entity's XP purse.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Level {
    pub level: u32,
    pub xp: u32,
}

impl Level {
    pub fn new() -> Self {
        Self { level: 1, xp: 0 }
    }

    /// Add XP with a gain multiplier. Carries over across thresholds.
    /// Returns levels gained.
    pub fn gain(&mut self, curve: &Curve, amount: u32, mult: f32) -> u32 {
        if amount == 0 || mult <= 0.0 {
            return 0;
        }
        self.xp += (amount as f32 * mult) as u32;
        let mut gained = 0;
        while self.xp >= need_for(curve, self.level).max(1) {
            self.xp -= need_for(curve, self.level).max(1);
            self.level += 1;
            gained += 1;
        }
        gained
    }
}

impl Default for Level {
    fn default() -> Self {
        Self::new()
    }
}

/// Reset loop: maxed runs convert into permanent count. Games keep
/// meta-progression (retained upgrades, currencies) themselves; this
/// tracks the count and the meta bonus.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Prestige {
    pub count: u32,
    /// Level required to prestige.
    pub at_level: u32,
    /// Bonus per prestige, as a fraction (0.1 = +10%).
    pub per_prestige: f32,
}

impl Prestige {
    pub fn new(at_level: u32, per_prestige: f32) -> Self {
        Self {
            count: 0,
            at_level: at_level.max(2),
            per_prestige: per_prestige.max(0.0),
        }
    }

    pub fn can_prestige(&self, level: &Level) -> bool {
        level.level >= self.at_level
    }

    /// Permanent multiplier from past prestiges.
    pub fn bonus(&self) -> f32 {
        1.0 + self.count as f32 * self.per_prestige
    }

    /// Reset `level` to 1 and bank one prestige. False when too early.
    pub fn apply(&mut self, level: &mut Level) -> bool {
        if !self.can_prestige(level) {
            return false;
        }
        self.count += 1;
        *level = Level::new();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_curve_shape() {
        let c = Curve::Power {
            base: 3.0,
            exp: 2.0,
            scale: 1.0,
        };
        assert_eq!(need_for(&c, 1), 16);
        assert_eq!(need_for(&c, 2), 25);
    }

    #[test]
    fn table_repeats_last() {
        let c = Curve::Table(vec![10, 20]);
        assert_eq!(need_for(&c, 1), 10);
        assert_eq!(need_for(&c, 5), 20);
    }

    #[test]
    fn gain_carries_over_levels() {
        let c = Curve::Linear(10);
        let mut l = Level::new();
        assert_eq!(l.gain(&c, 25, 1.0), 2);
        assert_eq!((l.level, l.xp), (3, 5));
        assert_eq!(l.gain(&c, 10, 0.5), 1);
    }

    #[test]
    fn zero_cost_curve_terminates() {
        let mut l = Level::new();
        assert_eq!(l.gain(&Curve::Linear(0), 10, 1.0), 10);
        assert_eq!((l.level, l.xp), (11, 0));
    }

    #[test]
    fn prestige_resets_and_bonuses() {
        let mut l = Level { level: 10, xp: 3 };
        let mut p = Prestige::new(10, 0.1);
        assert!(p.can_prestige(&l));
        assert!(p.apply(&mut l));
        assert_eq!((l.level, l.xp), (1, 0));
        assert!((p.bonus() - 1.1).abs() < 1e-6);
        assert!(!p.apply(&mut l));
    }
}
