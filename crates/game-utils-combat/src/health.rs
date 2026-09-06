//! Health pool with shield absorption and overkill tracking.

use serde::{Deserialize, Serialize};

/// Hit-point pool. Shields absorb first and never restore health.
/// `ward` ignores whole hits (invuln-frame charges); `undying` turns
/// lethal hits into 1-hp survivals (charges).
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Health {
    pub max: f32,
    pub current: f32,
    pub shield: f32,
    pub ward: u32,
    pub undying: u32,
}

/// What one damage application did.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct DamageResult {
    pub to_shield: f32,
    pub to_health: f32,
    pub overkill: f32,
    pub killed: bool,
    pub warded: bool,
    pub saved: bool,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self {
            max: max.max(1.0),
            current: max.max(1.0),
            shield: 0.0,
            ward: 0,
            undying: 0,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.current > 0.0
    }

    pub fn ratio(&self) -> f32 {
        (self.current / self.max).clamp(0.0, 1.0)
    }

    /// Apply post-mitigation damage. Ward charges eat whole hits.
    /// Returns None when already dead or amount <= 0.
    pub fn damage(&mut self, amount: f32) -> Option<DamageResult> {
        if !self.is_alive() || amount <= 0.0 {
            return None;
        }
        if self.ward > 0 {
            self.ward -= 1;
            return Some(DamageResult {
                to_shield: 0.0,
                to_health: 0.0,
                overkill: amount,
                killed: false,
                warded: true,
                saved: false,
            });
        }
        let to_shield = amount.min(self.shield);
        self.shield -= to_shield;
        let mut to_health = (amount - to_shield).min(self.current);
        let mut saved = false;
        if to_health >= self.current && self.undying > 0 {
            self.undying -= 1;
            to_health = self.current - 1.0;
            saved = true;
        }
        self.current -= to_health;
        let overkill = amount - to_shield - to_health;
        Some(DamageResult {
            to_shield,
            to_health,
            overkill,
            killed: !saved && self.current <= 0.0,
            warded: false,
            saved,
        })
    }

    /// Regenerate up to max. Dead pools stay dead.
    pub fn regen(&mut self, dt: f32, per_second: f32) -> f32 {
        self.heal(dt.max(0.0) * per_second.max(0.0))
    }

    /// Restore health up to max. Returns actual healed (no overheal).
    pub fn heal(&mut self, amount: f32) -> f32 {
        if !self.is_alive() || amount <= 0.0 {
            return 0.0;
        }
        let healed = amount.min(self.max - self.current);
        self.current += healed;
        healed
    }

    /// Resize max, keeping the health ratio.
    pub fn set_max(&mut self, max: f32) {
        let r = self.ratio();
        self.max = max.max(1.0);
        self.current = (self.max * r)
            .max(if self.is_alive() { 1.0 } else { 0.0 })
            .min(self.max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shield_then_health_then_overkill() {
        let mut h = Health {
            max: 100.0,
            current: 100.0,
            shield: 30.0,
            ward: 0,
            undying: 0,
        };
        let r = h.damage(50.0).unwrap();
        assert_eq!((r.to_shield, r.to_health, r.overkill), (30.0, 20.0, 0.0));
        assert!(!r.killed);
        let r = h.damage(200.0).unwrap();
        assert_eq!(r.to_health, 80.0);
        assert_eq!(r.overkill, 120.0);
        assert!(r.killed);
        assert!(h.damage(10.0).is_none());
    }

    #[test]
    fn heal_clamps_and_dead_stays() {
        let mut h = Health::new(100.0);
        h.current = 90.0;
        assert_eq!(h.heal(20.0), 10.0);
        h.current = 0.0;
        assert_eq!(h.heal(50.0), 0.0);
    }

    #[test]
    fn set_max_keeps_ratio() {
        let mut h = Health::new(100.0);
        h.current = 50.0;
        h.set_max(200.0);
        assert_eq!(h.current, 100.0);
    }

    #[test]
    fn ward_eats_whole_hits() {
        let mut h = Health::new(50.0);
        h.ward = 1;
        let r = h.damage(999.0).unwrap();
        assert!(r.warded && !r.killed && r.to_health == 0.0);
        assert_eq!(h.ward, 0);
        assert!(h.damage(999.0).unwrap().killed);
    }

    #[test]
    fn undying_leaves_one_hp() {
        let mut h = Health::new(50.0);
        h.undying = 1;
        let r = h.damage(999.0).unwrap();
        assert!(r.saved && !r.killed);
        assert_eq!(h.current, 1.0);
        assert!(h.damage(999.0).unwrap().killed);
    }

    #[test]
    fn regen_heals_over_time() {
        let mut h = Health::new(100.0);
        h.current = 50.0;
        assert_eq!(h.regen(1.0, 10.0), 10.0);
        assert_eq!(h.regen(99.0, 10.0), 40.0);
    }
}
