//! Full hit exchange: dodge, crit, kind factors, armor, block,
//! reduction, damage floor, lifesteal, on-hit ailments.

use std::collections::HashMap;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::dots::Dot;

/// Attacker side of one hit. `kind` is a free-form damage type.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Attack {
    pub amount: f32,
    pub kind: String,
    pub crit_chance: f32,
    pub crit_mult: f32,
    pub armor_pen: f32,
    pub undodgeable: bool,
    /// Per-kind damage bonus (0.2 = +20% fire).
    pub kind_bonus: HashMap<String, f32>,
    /// Heal-on-hit (only when already hurt).
    pub lifesteal_chance: f32,
    pub lifesteal_factor: f32,
    /// Ailments applied on hit (each rolls its own chance).
    pub secondary: Vec<Secondary>,
}

impl Attack {
    pub fn new(amount: f32, kind: impl Into<String>) -> Self {
        Self {
            amount,
            kind: kind.into(),
            crit_chance: 0.0,
            crit_mult: 2.0,
            armor_pen: 0.0,
            undodgeable: false,
            kind_bonus: HashMap::new(),
            lifesteal_chance: 0.0,
            lifesteal_factor: 0.0,
            secondary: Vec::new(),
        }
    }
}

/// One ailment application roll.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Secondary {
    pub chance: f32,
    pub dot: Dot,
}

/// Defender side of one hit.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Defense {
    pub armor: f32,
    pub dodge: f32,
    /// Multiplier per kind (0.5 = half damage).
    pub resist: HashMap<String, f32>,
    /// Flat damage erased after armor.
    pub block: f32,
    /// Fractional reduction per kind (0.2 = -20%).
    pub reduction: HashMap<String, f32>,
    /// Damage never drops below this once it is positive.
    pub min_damage: f32,
}

/// Resolved exchange outcome. `lifesteal` is healing for the attacker
/// (game applies it); `applied` are ailments the defender gains.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Exchange {
    pub dealt: f32,
    pub dodged: bool,
    pub crit: bool,
    pub lifesteal: f32,
    pub applied: Vec<Dot>,
}

/// Run the ordered pipeline: dodge, crit, kind bonus, armor, block,
/// reduction, floor, lifesteal roll, ailment rolls.
pub fn exchange(rng: &mut impl Rng, atk: &Attack, def: &Defense, attacker_hurt: bool) -> Exchange {
    if !atk.undodgeable && rng.random_bool(def.dodge.clamp(0.0, 1.0) as f64) {
        return Exchange {
            dealt: 0.0,
            dodged: true,
            crit: false,
            lifesteal: 0.0,
            applied: vec![],
        };
    }
    let crit = rng.random_bool(atk.crit_chance.clamp(0.0, 1.0) as f64);
    let mut dmg = atk.amount * if crit { atk.crit_mult.max(1.0) } else { 1.0 };
    dmg *= 1.0
        + atk
            .kind_bonus
            .get(&atk.kind)
            .copied()
            .unwrap_or(0.0)
            .max(-1.0);
    dmg = (dmg - (def.armor - atk.armor_pen).max(0.0)).max(0.0);
    dmg = (dmg - def.block.max(0.0)).max(0.0);
    dmg *= 1.0
        - def
            .reduction
            .get(&atk.kind)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
    dmg *= def.resist.get(&atk.kind).copied().unwrap_or(1.0).max(0.0);
    if dmg > 0.0 && dmg < def.min_damage {
        dmg = def.min_damage;
    }
    let lifesteal = if attacker_hurt && rng.random_bool(atk.lifesteal_chance.clamp(0.0, 1.0) as f64)
    {
        dmg * atk.lifesteal_factor.max(0.0)
    } else {
        0.0
    };
    let mut applied = Vec::new();
    for s in &atk.secondary {
        if rng.random_bool(s.chance.clamp(0.0, 1.0) as f64) {
            applied.push(s.dot.clone());
        }
    }
    Exchange {
        dealt: dmg,
        dodged: false,
        crit,
        lifesteal,
        applied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    fn rng() -> SmallRng {
        SmallRng::seed_from_u64(3)
    }

    fn def() -> Defense {
        Defense {
            armor: 10.0,
            dodge: 0.0,
            resist: [("fire".to_string(), 0.5)].into(),
            block: 0.0,
            reduction: HashMap::new(),
            min_damage: 0.0,
        }
    }

    #[test]
    fn armor_then_resist() {
        let r = exchange(&mut rng(), &Attack::new(30.0, "fire"), &def(), true);
        assert!(!r.dodged && !r.crit);
        assert_eq!(r.dealt, 10.0);
    }

    #[test]
    fn block_reduction_floor() {
        let mut d = def();
        d.block = 5.0;
        d.reduction.insert("phys".into(), 0.5);
        d.min_damage = 1.0;
        let r = exchange(&mut rng(), &Attack::new(12.0, "phys"), &d, true);
        // 12 - 10 armor - 5 block = 0 -> floor does not revive zero.
        assert_eq!(r.dealt, 0.0);
        let r = exchange(&mut rng(), &Attack::new(20.0, "phys"), &d, true);
        // 20 - 10 - 5 = 5, halved 2.5.
        assert_eq!(r.dealt, 2.5);
    }

    #[test]
    fn crit_lifesteal_secondary() {
        let mut a = Attack::new(10.0, "fire");
        a.crit_chance = 1.0;
        a.lifesteal_chance = 1.0;
        a.lifesteal_factor = 0.5;
        a.secondary.push(Secondary {
            chance: 1.0,
            dot: Dot::new("burn", 2.0, 1.0, 3),
        });
        let r = exchange(&mut rng(), &a, &Defense::default(), true);
        assert!(r.crit);
        assert_eq!(r.dealt, 20.0);
        assert_eq!(r.lifesteal, 10.0);
        assert_eq!(r.applied.len(), 1);
        // Full health: no lifesteal.
        let r = exchange(&mut rng(), &a, &Defense::default(), false);
        assert_eq!(r.lifesteal, 0.0);
    }

    #[test]
    fn dodge_eats_everything() {
        let d = Defense {
            dodge: 1.0,
            ..Default::default()
        };
        let r = exchange(&mut rng(), &Attack::new(99.0, "x"), &d, true);
        assert!(r.dodged && r.dealt == 0.0 && r.applied.is_empty());
    }
}
