use rand::Rng;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::card::{CardDef, CardId};
use crate::pile::{Pile, Zone};

/// Card definitions by id. Load once, resolve everywhere.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(bound(
    serialize = "P: Serialize",
    deserialize = "P: Deserialize<'de> + Default"
))]
pub struct Registry<P = ()> {
    defs: HashMap<String, CardDef<P>>,
}

impl<P> Registry<P> {
    pub fn new() -> Self {
        Self {
            defs: HashMap::new(),
        }
    }

    pub fn insert(&mut self, def: CardDef<P>) {
        self.defs.insert(def.id.as_str().to_string(), def);
    }

    pub fn extend(&mut self, defs: impl IntoIterator<Item = CardDef<P>>) {
        for def in defs {
            self.insert(def);
        }
    }

    pub fn get(&self, id: &CardId) -> Option<&CardDef<P>> {
        self.defs.get(id.as_str())
    }

    pub fn get_str(&self, id: &str) -> Option<&CardDef<P>> {
        self.defs.get(id)
    }

    pub fn contains(&self, id: &CardId) -> bool {
        self.defs.contains_key(id.as_str())
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, String, CardDef<P>> {
        self.defs.iter()
    }

    pub fn remove(&mut self, id: &CardId) -> Option<CardDef<P>> {
        self.defs.remove(id.as_str())
    }

    /// Ids with no definition. Empty means the deck is fully known.
    pub fn missing<'a>(&self, ids: &'a [CardId]) -> Vec<&'a CardId> {
        ids.iter().filter(|id| !self.contains(id)).collect()
    }

    /// Resolve ids to definitions, skipping unknown ones.
    pub fn resolve(&self, ids: &[CardId]) -> Vec<&CardDef<P>> {
        ids.iter().filter_map(|id| self.get(id)).collect()
    }

    /// Build an unshuffled pile from ids. Errors on unknown ids.
    pub fn build_pile(&self, zone: Zone, ids: &[CardId]) -> Result<Pile<CardId>, CardId> {
        let mut pile = Pile::new(zone);
        for id in ids {
            if !self.contains(id) {
                return Err(id.clone());
            }
            pile.push(id.clone());
        }
        Ok(pile)
    }

    /// Roll up to `count` distinct filtered offers. Caller gives RNG.
    pub fn offers<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
        count: usize,
        mut filter: impl FnMut(&CardDef<P>) -> bool,
    ) -> Vec<CardId> {
        let mut pool: Vec<&CardDef<P>> = self.defs.values().filter(|d| filter(d)).collect();
        let mut out = Vec::new();
        // Partial Fisher-Yates: only shuffle as far as needed.
        let n = count.min(pool.len());
        for i in 0..n {
            let j = rng.random_range(i..pool.len());
            pool.swap(i, j);
            out.push(pool[i].id.clone());
        }
        // Deduplicate defensively (ids are unique by construction).
        let mut seen = HashSet::new();
        out.into_iter()
            .filter(|id| seen.insert(id.as_str().to_string()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::Cost;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn sample_registry() -> Registry {
        let mut r = Registry::new();
        for (id, kind) in [
            ("c1", "unit"),
            ("c2", "spell"),
            ("c3", "unit"),
            ("c4", "relic"),
        ] {
            r.insert(CardDef::new(id, id, Cost::Fixed(1)).with_kind(kind));
        }
        r
    }

    #[test]
    fn registry_lookup_and_validate() {
        let r = sample_registry();
        assert_eq!(r.len(), 4);
        assert!(r.contains(&CardId::new("c1")));
        let ids = vec![CardId::new("c1"), CardId::new("nope")];
        let missing = r.missing(&ids);
        assert_eq!(missing, vec![&CardId::new("nope")]);
        assert_eq!(r.resolve(&ids).len(), 1);
    }

    #[test]
    fn registry_build_pile() {
        let r = sample_registry();
        let ok = r.build_pile(Zone::draw(), &[CardId::new("c1")]);
        assert!(ok.is_ok());
        let err = r.build_pile(Zone::draw(), &[CardId::new("c1"), CardId::new("x")]);
        match err {
            Err(missing) => assert_eq!(missing.as_str(), "x"),
            Ok(_) => panic!("expected unknown id error"),
        }
    }

    #[test]
    fn registry_offers_distinct_and_filtered() {
        let r = sample_registry();
        let mut rng = StdRng::seed_from_u64(11);
        let offers = r.offers(&mut rng, 2, |d| d.kind.as_str() == "unit");
        assert_eq!(offers.len(), 2);
        let set: HashSet<String> = offers.iter().map(|id| id.as_str().to_string()).collect();
        assert_eq!(set.len(), 2);
        // Asking for more than exist returns all matches.
        let mut rng = StdRng::seed_from_u64(11);
        let offers = r.offers(&mut rng, 10, |d| d.kind.as_str() == "relic");
        assert_eq!(offers.len(), 1);
    }

    #[test]
    fn registry_roundtrip() {
        let r = sample_registry();
        let s = ron::ser::to_string(&r).unwrap();
        let de: Registry<()> = ron::from_str(&s).unwrap();
        assert_eq!(de.len(), 4);
    }
}
