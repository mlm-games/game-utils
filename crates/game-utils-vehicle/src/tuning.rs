//! Named stat modifiers: `(stat, flat, mult)` applied as
//! `(base + sum flat) x product mult`. Open string stats.

use serde::{Deserialize, Serialize};

/// One adjustment to a named stat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatMod {
    pub stat: String,
    pub flat: f32,
    pub mult: f32,
}

impl StatMod {
    pub fn new(stat: impl Into<String>, flat: f32, mult: f32) -> Self {
        Self {
            stat: stat.into(),
            flat,
            mult,
        }
    }

    /// Pure additive tweak.
    pub fn flat(stat: impl Into<String>, amount: f32) -> Self {
        Self::new(stat, amount, 1.0)
    }

    /// Multiplicative tweak (0.1 = +10%).
    pub fn mult(stat: impl Into<String>, multiplier: f32) -> Self {
        Self::new(stat, 0.0, multiplier)
    }
}

/// An ordered stack of [`StatMod`]s.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatMods {
    mods: Vec<StatMod>,
}

impl StatMods {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, m: StatMod) {
        self.mods.push(m);
    }

    pub fn extend(&mut self, mods: impl IntoIterator<Item = StatMod>) {
        self.mods.extend(mods);
    }

    pub fn len(&self) -> usize {
        self.mods.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mods.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, StatMod> {
        self.mods.iter()
    }

    pub fn remove_where(&mut self, mut pred: impl FnMut(&StatMod) -> bool) -> usize {
        let before = self.mods.len();
        self.mods.retain(|m| !pred(m));
        before - self.mods.len()
    }

    pub fn clear(&mut self) {
        self.mods.clear();
    }

    /// Apply all entries for `stat` to `base`.
    pub fn apply(&self, stat: &str, base: f32) -> f32 {
        let mut flat = 0.0;
        let mut mult = 1.0;
        for m in &self.mods {
            if m.stat == stat {
                flat += m.flat;
                mult *= m.mult;
            }
        }
        (base + flat) * mult
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuning_flat_then_mult() {
        let mut mods = StatMods::new();
        mods.push(StatMod::flat("top_speed", 4.0));
        mods.push(StatMod::mult("top_speed", 1.1));
        mods.push(StatMod::flat("grip", 99.0));
        assert!((mods.apply("top_speed", 20.0) - 26.4).abs() < 1e-4);
        assert_eq!(mods.apply("unknown", 5.0), 5.0);
        assert_eq!(mods.remove_where(|m| m.stat == "grip"), 1);
        assert_eq!(mods.len(), 2);
    }
}
