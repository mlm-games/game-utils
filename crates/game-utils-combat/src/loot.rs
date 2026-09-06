//! Weighted drop tables: entries with qty ranges, N rolls per kill.

use rand::distr::weighted::WeightedIndex;
use rand::prelude::Distribution;
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

/// One drop: weight, qty range per hit.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct LootEntry {
    pub id: String,
    pub weight: f32,
    pub min_qty: u32,
    pub max_qty: u32,
}

impl LootEntry {
    pub fn new(id: impl Into<String>, weight: f32, min_qty: u32, max_qty: u32) -> Self {
        Self {
            id: id.into(),
            weight,
            min_qty,
            max_qty: max_qty.max(min_qty),
        }
    }
}

/// Rollable table. Zero-weight entries never drop.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct LootTable {
    pub entries: Vec<LootEntry>,
    /// Rolls per kill.
    pub rolls: u32,
}

impl LootTable {
    pub fn new(entries: Vec<LootEntry>, rolls: u32) -> Self {
        Self {
            entries,
            rolls: rolls.max(1),
        }
    }

    /// Roll the table: one (id, qty) per successful pick.
    pub fn roll(&self, rng: &mut impl Rng) -> Vec<(String, u32)> {
        let mut out = Vec::new();
        let pool: Vec<&LootEntry> = self.entries.iter().filter(|e| e.weight > 0.0).collect();
        if pool.is_empty() {
            return out;
        }
        let weights: Vec<f32> = pool
            .iter()
            .map(|e| e.weight.max(f32::MIN_POSITIVE))
            .collect();
        let Ok(dist) = WeightedIndex::new(weights) else {
            return out;
        };
        for _ in 0..self.rolls {
            let e = pool[dist.sample(rng)];
            let qty = if e.max_qty <= e.min_qty {
                e.min_qty
            } else {
                rng.random_range(e.min_qty..=e.max_qty)
            };
            if qty > 0 {
                out.push((e.id.clone(), qty));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    fn rng() -> SmallRng {
        SmallRng::seed_from_u64(5)
    }

    fn table() -> LootTable {
        LootTable::new(
            vec![
                LootEntry::new("gold", 3.0, 1, 5),
                LootEntry::new("gem", 1.0, 1, 1),
                LootEntry::new("nothing", 0.0, 1, 9),
            ],
            2,
        )
    }

    #[test]
    fn rolls_hit_count_and_ranges() {
        let drops = table().roll(&mut rng());
        assert_eq!(drops.len(), 2);
        for (id, qty) in &drops {
            assert_ne!(id, "nothing");
            if id == "gold" {
                assert!((1..=5).contains(qty));
            } else {
                assert_eq!(*qty, 1);
            }
        }
    }

    #[test]
    fn empty_or_zero_weight_drops_nothing() {
        assert!(LootTable::default().roll(&mut rng()).is_empty());
        let t = LootTable::new(vec![LootEntry::new("x", 0.0, 1, 1)], 3);
        assert!(t.roll(&mut rng()).is_empty());
    }
}
