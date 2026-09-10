use rand::Rng;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Unordered multiset with random draws. Use [`Pile`](crate::pile::Pile)
/// when order matters. Staged items serve first, FIFO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bag<T> {
    items: Vec<T>,
    staged: VecDeque<T>,
}

impl<T> Default for Bag<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            staged: VecDeque::new(),
        }
    }
}

impl<T> Bag<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_items(items: Vec<T>) -> Self {
        Self {
            items,
            staged: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len() + self.staged.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.staged.is_empty()
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    pub fn staged_len(&self) -> usize {
        self.staged.len()
    }

    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }

    pub fn extend(&mut self, items: impl IntoIterator<Item = T>) {
        self.items.extend(items);
    }

    /// Force an upcoming draw; served before random draws, FIFO.
    pub fn stage_next(&mut self, item: T) {
        self.staged.push_back(item);
    }

    /// Draw a random item (staged first). Caller provides the RNG.
    pub fn draw_random<R: Rng + ?Sized>(&mut self, rng: &mut R) -> Option<T> {
        if let Some(item) = self.staged.pop_front() {
            return Some(item);
        }
        if self.items.is_empty() {
            return None;
        }
        let idx = rng.random_range(0..self.items.len());
        Some(self.items.remove(idx))
    }

    /// Draw up to `n` random items.
    pub fn draw_many<R: Rng + ?Sized>(&mut self, rng: &mut R, n: usize) -> Vec<T> {
        (0..n).filter_map(|_| self.draw_random(rng)).collect()
    }

    /// Remove the first item matching `pred`. Searches staged cards first
    /// (they are what `draw_random` serves first), then the main items.
    pub fn draw_where(&mut self, mut pred: impl FnMut(&T) -> bool) -> Option<T> {
        if let Some(pos) = self.staged.iter().position(&mut pred) {
            return self.staged.remove(pos);
        }
        let pos = self.items.iter().position(pred)?;
        Some(self.items.remove(pos))
    }

    /// True when any card (staged or main) matches `pred`.
    pub fn contains(&self, mut pred: impl FnMut(&T) -> bool) -> bool {
        self.staged.iter().any(&mut pred) || self.items.iter().any(pred)
    }

    /// Keep only matching cards, in both staged and main items.
    pub fn retain(&mut self, mut pred: impl FnMut(&T) -> bool) {
        self.items.retain(&mut pred);
        self.staged.retain(&mut pred);
    }

    pub fn clear(&mut self) -> Vec<T> {
        let mut out = std::mem::take(&mut self.items);
        out.extend(std::mem::take(&mut self.staged));
        out
    }
}

/// Weighted pool (packs, spawn tables, tier rolls). Dead weights
/// (zero/negative/non-finite) never win; empty pools draw `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedPool<T> {
    entries: Vec<WeightedEntry<T>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WeightedEntry<T> {
    item: T,
    weight: f32,
}

impl<T> Default for WeightedPool<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T> WeightedPool<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_entries(entries: Vec<(T, f32)>) -> Self {
        let mut pool = Self::new();
        for (item, weight) in entries {
            pool.add(item, weight);
        }
        pool
    }

    pub fn add(&mut self, item: T, weight: f32) {
        self.entries.push(WeightedEntry {
            item,
            weight: sanitize(weight),
        });
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Sum of live weights.
    pub fn total(&self) -> f32 {
        self.entries.iter().map(|e| e.weight).sum()
    }

    pub fn set_weight(&mut self, mut pred: impl FnMut(&T) -> bool, weight: f32) -> bool {
        let w = sanitize(weight);
        let mut hit = false;
        for e in &mut self.entries {
            if pred(&e.item) {
                e.weight = w;
                hit = true;
            }
        }
        hit
    }

    pub fn remove_where(&mut self, mut pred: impl FnMut(&T) -> bool) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| !pred(&e.item));
        before - self.entries.len()
    }

    fn roll<R: Rng + ?Sized>(&self, rng: &mut R) -> Option<usize> {
        let total = self.total();
        if !total.is_finite() || total <= 0.0 {
            return None;
        }
        let mut roll = rng.random_range(0.0..total);
        for (i, e) in self.entries.iter().enumerate() {
            if roll < e.weight {
                return Some(i);
            }
            roll -= e.weight;
        }
        // Float dust: fall back to the last live entry.
        self.entries.iter().rposition(|e| e.weight > 0.0)
    }

    /// Draw one entry, keeping it in the pool (pack slots, spawns).
    pub fn draw_replace<R: Rng + ?Sized>(&mut self, rng: &mut R) -> Option<&T> {
        let i = self.roll(rng)?;
        Some(&self.entries[i].item)
    }

    /// Draw one entry and remove it (drafts, one-shot lotteries).
    pub fn draw_remove<R: Rng + ?Sized>(&mut self, rng: &mut R) -> Option<T> {
        let i = self.roll(rng)?;
        Some(self.entries.remove(i).item)
    }

    /// Draw up to `n` entries with replacement.
    pub fn draw_many_replace<R: Rng + ?Sized>(&mut self, rng: &mut R, n: usize) -> Vec<&T> {
        let mut out = Vec::new();
        for _ in 0..n {
            match self.roll(rng) {
                Some(i) => out.push(&self.entries[i].item),
                None => break,
            }
        }
        out
    }
}

fn sanitize(weight: f32) -> f32 {
    if !weight.is_finite() || weight <= 0.0 {
        0.0
    } else {
        weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn bag_random_and_staged() {
        let mut rng = StdRng::seed_from_u64(99);
        let mut bag = Bag::with_items(vec![1, 2, 3, 4]);
        bag.stage_next(99);
        bag.stage_next(100);
        assert_eq!(bag.staged_len(), 2);
        assert_eq!(bag.draw_random(&mut rng), Some(99));
        assert_eq!(bag.draw_random(&mut rng), Some(100));
        let v = bag.draw_random(&mut rng).unwrap();
        assert!((1..=4).contains(&v));
        assert_eq!(bag.len(), 3);
    }

    #[test]
    fn bag_draw_where_and_many() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut bag = Bag::with_items(vec!["apple", "banana", "cherry"]);
        assert_eq!(bag.draw_where(|s| *s == "banana"), Some("banana"));
        assert!(bag.contains(|s| *s == "apple"));
        let rest = bag.draw_many(&mut rng, 5);
        assert_eq!(rest.len(), 2);
        assert!(bag.is_empty());
    }

    #[test]
    fn bag_staged_visible_to_search() {
        let mut bag = Bag::with_items(vec![1, 2, 3]);
        bag.stage_next(99);
        assert!(bag.contains(|&x| x == 99));
        assert_eq!(bag.draw_where(|&x| x == 99), Some(99));
        bag.stage_next(100);
        bag.retain(|&x| x != 100);
        assert!(!bag.contains(|&x| x == 100));
        assert_eq!(bag.staged_len(), 0);
    }

    #[test]
    fn weighted_pool_replace_and_remove() {
        let mut rng = StdRng::seed_from_u64(21);
        let mut pool = WeightedPool::with_entries(vec![("a", 1.0), ("b", 3.0)]);
        assert_eq!(pool.total(), 4.0);
        // Replacement draws keep the pool intact.
        for _ in 0..10 {
            assert!(pool.draw_replace(&mut rng).is_some());
        }
        assert_eq!(pool.len(), 2);
        // Removal drains it.
        assert!(pool.draw_remove(&mut rng).is_some());
        assert!(pool.draw_remove(&mut rng).is_some());
        assert!(pool.draw_remove(&mut rng).is_none());
    }

    #[test]
    fn weighted_pool_ignores_dead_weights() {
        let mut rng = StdRng::seed_from_u64(1);
        let mut pool = WeightedPool::with_entries(vec![
            ("dead0", 0.0),
            ("deadneg", -5.0),
            ("deadnan", f32::NAN),
            ("live", 2.0),
        ]);
        for _ in 0..20 {
            assert_eq!(pool.draw_replace(&mut rng), Some(&"live"));
        }
        assert!(pool.set_weight(|s| *s == "live", 0.0));
        assert!(pool.draw_replace(&mut rng).is_none());
        assert_eq!(pool.remove_where(|s| s.starts_with("dead")), 3);
        assert_eq!(pool.len(), 1);
    }
}
