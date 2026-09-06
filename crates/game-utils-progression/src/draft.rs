//! Upgrade offers: weighted picks, reroll, lock, banish.

use std::collections::{HashMap, HashSet};

use rand::Rng;
use rand::distr::weighted::WeightedIndex;
use rand::prelude::Distribution;
use serde::{Deserialize, Serialize};

/// One draftable upgrade. Zero-weight entries never appear.
/// `cost` is in game currency (0 = free pick on level-up).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Offer {
    pub id: String,
    pub weight: f32,
    pub tags: Vec<String>,
    pub max_copies: u32,
    pub cost: i64,
}

/// Pickable pool plus banished ids.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct DraftPool {
    pub offers: Vec<Offer>,
    pub banished: HashSet<String>,
}

impl DraftPool {
    pub fn new(offers: Vec<Offer>) -> Self {
        Self {
            offers,
            banished: HashSet::new(),
        }
    }

    pub fn banish(&mut self, id: &str) {
        self.banished.insert(id.to_owned());
    }

    pub fn pardon(&mut self, id: &str) -> bool {
        self.banished.remove(id)
    }
}

/// One offer set. Locked drafts survive rerolls.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub options: Vec<String>,
    pub locked: bool,
    pub rerolls: u32,
}

/// Reroll refusal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RerollError {
    Locked,
}

/// Escalating reroll price: `base * (rerolls + 1)`.
pub fn reroll_cost(base: i64, rerolls: u32) -> i64 {
    base.max(0) * (rerolls as i64 + 1)
}

fn pick(rng: &mut impl Rng, pool: &DraftPool, taken: &[String]) -> Option<String> {
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for t in taken {
        *counts.entry(t.as_str()).or_insert(0) += 1;
    }
    let mut ids = Vec::new();
    let mut weights = Vec::new();
    for o in &pool.offers {
        if o.weight <= 0.0 || pool.banished.contains(&o.id) {
            continue;
        }
        if counts.get(o.id.as_str()).copied().unwrap_or(0) >= o.max_copies.max(1) {
            continue;
        }
        ids.push(o.id.clone());
        weights.push(o.weight.max(f32::MIN_POSITIVE));
    }
    if ids.is_empty() {
        return None;
    }
    let dist = WeightedIndex::new(weights).ok()?;
    Some(ids[dist.sample(rng)].clone())
}

/// Draw `count` distinct-eligible options.
pub fn offer(rng: &mut impl Rng, pool: &DraftPool, count: usize) -> Draft {
    let mut options = Vec::new();
    for _ in 0..count {
        match pick(rng, pool, &options) {
            Some(id) => options.push(id),
            None => break,
        }
    }
    Draft {
        options,
        locked: false,
        rerolls: 0,
    }
}

/// Redraw an unlocked draft, bumping its reroll count.
pub fn reroll(
    rng: &mut impl Rng,
    pool: &DraftPool,
    draft: &mut Draft,
    count: usize,
) -> Result<(), RerollError> {
    if draft.locked {
        return Err(RerollError::Locked);
    }
    let next = offer(rng, pool, count);
    draft.options = next.options;
    draft.rerolls += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    fn rng() -> SmallRng {
        SmallRng::seed_from_u64(11)
    }

    fn pool() -> DraftPool {
        DraftPool::new(vec![
            Offer {
                id: "dmg".into(),
                weight: 3.0,
                tags: vec![],
                max_copies: 1,
                cost: 10,
            },
            Offer {
                id: "spd".into(),
                weight: 1.0,
                tags: vec![],
                max_copies: 2,
                cost: 0,
            },
            Offer {
                id: "dead".into(),
                weight: 0.0,
                tags: vec![],
                max_copies: 1,
                cost: 10,
            },
        ])
    }

    #[test]
    fn offers_respect_copies_and_weight() {
        let d = offer(&mut rng(), &pool(), 3);
        assert_eq!(d.options.len(), 3);
        assert!(!d.options.contains(&"dead".to_string()));
        assert_eq!(d.options.iter().filter(|o| *o == "dmg").count(), 1);
    }

    #[test]
    fn banish_removes() {
        let mut p = pool();
        p.banish("dmg");
        let d = offer(&mut rng(), &p, 3);
        assert!(!d.options.contains(&"dmg".to_string()));
        assert!(p.pardon("dmg"));
    }

    #[test]
    fn reroll_locked_refused() {
        let mut d = offer(&mut rng(), &pool(), 2);
        d.locked = true;
        assert_eq!(
            reroll(&mut rng(), &pool(), &mut d, 2),
            Err(RerollError::Locked)
        );
        d.locked = false;
        assert!(reroll(&mut rng(), &pool(), &mut d, 2).is_ok());
        assert_eq!(d.rerolls, 1);
    }

    #[test]
    fn cost_escalates() {
        assert_eq!(reroll_cost(5, 0), 5);
        assert_eq!(reroll_cost(5, 2), 15);
    }
}
