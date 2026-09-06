//! Derived-stat sheet: bases plus id-keyed add/mult modifiers.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One named modifier packet (item, buff, aura). Removed by id.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct StatMod {
    pub add: HashMap<String, f32>,
    pub mult: HashMap<String, f32>,
}

impl StatMod {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(mut self, key: impl Into<String>, v: f32) -> Self {
        self.add.insert(key.into(), v);
        self
    }

    pub fn mult(mut self, key: impl Into<String>, v: f32) -> Self {
        self.mult.insert(key.into(), v);
        self
    }
}

/// Base values with stacked modifiers.
/// `get = (base + add_sum) * (1 + mult_sum)`.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct StatSheet {
    base: HashMap<String, f32>,
    mods: HashMap<String, StatMod>,
}

impl StatSheet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_base(&mut self, key: impl Into<String>, v: f32) {
        self.base.insert(key.into(), v);
    }

    pub fn base(&self, key: &str) -> f32 {
        self.base.get(key).copied().unwrap_or(0.0)
    }

    pub fn add_mod(&mut self, id: impl Into<String>, m: StatMod) {
        self.mods.insert(id.into(), m);
    }

    pub fn remove_mod(&mut self, id: &str) -> bool {
        self.mods.remove(id).is_some()
    }

    pub fn get(&self, key: &str) -> f32 {
        let base = self.base(key);
        let mut add = 0.0;
        let mut mult = 0.0;
        for m in self.mods.values() {
            add += m.add.get(key).copied().unwrap_or(0.0);
            mult += m.mult.get(key).copied().unwrap_or(0.0);
        }
        (base + add) * (1.0 + mult)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_then_mult() {
        let mut s = StatSheet::new();
        s.set_base("damage", 10.0);
        s.add_mod("sword", StatMod::new().add("damage", 5.0));
        s.add_mod("rage", StatMod::new().mult("damage", 1.0));
        assert_eq!(s.get("damage"), 30.0);
        assert!(s.remove_mod("rage"));
        assert_eq!(s.get("damage"), 15.0);
        assert_eq!(s.get("missing"), 0.0);
    }
}
