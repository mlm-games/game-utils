//! Named cooldowns with optional charges.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One timer. Single-use abilities are `max_charges = 1`.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Cooldown {
    pub duration: f32,
    pub remaining: f32,
    pub charges: u32,
    pub max_charges: u32,
}

impl Cooldown {
    pub fn ready(&self) -> bool {
        self.charges > 0
    }
}

/// Keyed timer set (abilities, consumables, rerolls).
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Cooldowns {
    map: HashMap<String, Cooldown>,
}

impl Cooldowns {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, key: impl Into<String>, duration: f32) {
        self.start_charged(key, duration, 1);
    }

    pub fn start_charged(&mut self, key: impl Into<String>, duration: f32, max_charges: u32) {
        let max_charges = max_charges.max(1);
        self.map.insert(
            key.into(),
            Cooldown {
                duration: duration.max(0.0),
                remaining: duration.max(0.0),
                charges: 0,
                max_charges,
            },
        );
    }

    /// Spend one charge. False when not ready (and starts tracking).
    pub fn use_charge(&mut self, key: &str) -> bool {
        match self.map.get_mut(key) {
            Some(cd) if cd.charges > 0 => {
                cd.charges -= 1;
                if cd.charges < cd.max_charges && cd.remaining <= 0.0 {
                    cd.remaining = cd.duration;
                }
                true
            }
            _ => false,
        }
    }

    pub fn ready(&self, key: &str) -> bool {
        self.map.get(key).is_none_or(|cd| cd.ready())
    }

    pub fn remaining(&self, key: &str) -> f32 {
        self.map
            .get(key)
            .map(|cd| if cd.ready() { 0.0 } else { cd.remaining })
            .unwrap_or(0.0)
    }

    /// Advance all timers, recharging spent charges.
    pub fn tick(&mut self, dt: f32) {
        for cd in self.map.values_mut() {
            if cd.charges >= cd.max_charges {
                cd.remaining = 0.0;
                continue;
            }
            cd.remaining -= dt;
            while cd.remaining <= 0.0 && cd.charges < cd.max_charges {
                cd.charges += 1;
                if cd.charges < cd.max_charges {
                    cd.remaining += cd.duration;
                } else {
                    cd.remaining = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_and_recharge() {
        let mut c = Cooldowns::new();
        assert!(c.ready("dash"));
        assert!(!c.use_charge("dash"));
        c.start("dash", 2.0);
        assert!(!c.ready("dash"));
        c.tick(2.0);
        assert!(c.ready("dash"));
        assert!(c.use_charge("dash"));
    }

    #[test]
    fn charges_accumulate() {
        let mut c = Cooldowns::new();
        c.start_charged("nade", 1.0, 2);
        c.tick(2.5);
        assert!(c.use_charge("nade"));
        assert!(c.use_charge("nade"));
        assert!(!c.use_charge("nade"));
    }
}
