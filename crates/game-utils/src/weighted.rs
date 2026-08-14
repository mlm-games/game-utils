use rand::rngs::StdRng;
use rand::{Rng, RngExt, SeedableRng};

pub const SEED_ALPHABET: &str = "ABCDEFGHJKMNPQRTUVWXY346789";

/// Efraimidis-Spirakis key for weighted sampling without replacement.
fn es_key(item_seed: u64, weight: f32) -> f64 {
    if weight <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let mut rng = StdRng::seed_from_u64(item_seed);
    let u: f64 = rng.random();
    let u = if u <= 0.0 { 1e-12 } else { u };
    u.ln() / weight as f64
}

/// Deterministic per-(parent_seed, index) seed for [`es_key`].
fn item_seed(parent_seed: u64, index: usize) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    parent_seed.hash(&mut hasher);
    index.hash(&mut hasher);
    hasher.finish()
}

pub struct Weighted;

impl Weighted {
    /// Pick an index from `weights`, where the probability of `i` is `weights[i] / sum`.
    /// Returns `None` for an empty or non-positive-sum slice.
    pub fn pick_index<R: Rng + ?Sized>(rng: &mut R, weights: &[f32]) -> Option<usize> {
        if weights.is_empty() {
            return None;
        }
        let total: f32 = weights.iter().sum();
        if total <= 0.0 {
            return None;
        }
        let mut roll = rng.random_range(0.0..total);
        for (i, w) in weights.iter().enumerate() {
            if roll < *w {
                return Some(i);
            }
            roll -= w;
        }
        Some(weights.len() - 1)
    }

    pub fn pick_by<'a, T, R: Rng + ?Sized, F: Fn(&T) -> f32>(
        rng: &mut R,
        items: &'a [T],
        weight: F,
    ) -> Option<&'a T> {
        let weights: Vec<f32> = items.iter().map(&weight).collect();
        Self::pick_index(rng, &weights).map(|i| &items[i])
    }

    /// Weighted pick that retries until it draws an unlocked item.
    pub fn pick_weighted_unlocked<'a, T, R: Rng + ?Sized, F: Fn(&T) -> f32, U: Fn(&T) -> bool>(
        rng: &mut R,
        items: &'a [T],
        weight: F,
        unlocked: U,
    ) -> Option<&'a T> {
        let mut sub = StdRng::from_rng(rng);
        let weights: Vec<f32> = items.iter().map(&weight).collect();
        if let Some(i) = Self::pick_index(&mut sub, &weights) {
            let item = &items[i];
            if unlocked(item) {
                return Some(item);
            }
        }
        let mut unlocked_weights = Vec::new();
        let mut unlocked_items = Vec::new();
        for (item, w) in items.iter().zip(weights.iter()) {
            if unlocked(item) {
                unlocked_items.push(item);
                unlocked_weights.push(*w);
            }
        }
        if unlocked_items.is_empty() {
            return None;
        }
        let mut sub = StdRng::from_rng(rng);
        Self::pick_index(&mut sub, &unlocked_weights).map(|i| unlocked_items[i])
    }

    /// Pick a random element.
    pub fn pick<'a, T, R: Rng + ?Sized>(rng: &mut R, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        let i = rng.random_range(0..items.len());
        Some(&items[i])
    }

    /// Uniform pick that retries until it draws an unlocked item.
    pub fn pick_unlocked<'a, T, R: Rng + ?Sized, U: Fn(&T) -> bool>(
        rng: &mut R,
        items: &'a [T],
        unlocked: U,
    ) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        if let Some(item) = Self::pick(rng, items).filter(|i| unlocked(i)) {
            return Some(item);
        }
        let unlocked_items: Vec<&T> = items.iter().filter(|i| unlocked(i)).collect();
        if unlocked_items.is_empty() {
            return None;
        }
        let mut sub = StdRng::from_rng(rng);
        let i = sub.random_range(0..unlocked_items.len());
        Some(unlocked_items[i])
    }

    /// Pick top-`n` distinct indices by Efraimidis-Spirakis key.
    pub fn pick_stable_top_n(
        weights: &[f32],
        parent_seed: u64,
        n: usize,
        unlocked: impl Fn(usize) -> bool,
    ) -> Vec<usize> {
        let mut scored: Vec<(f64, usize)> = Vec::new();
        for (i, w) in weights.iter().enumerate() {
            if !unlocked(i) || *w <= 0.0 {
                continue;
            }
            scored.push((es_key(item_seed(parent_seed, i), *w), i));
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(n).map(|(_, i)| i).collect()
    }

    pub fn pick_stable(
        weights: &[f32],
        parent_seed: u64,
        unlocked: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        Self::pick_stable_top_n(weights, parent_seed, 1, unlocked)
            .into_iter()
            .next()
    }

    pub fn shuffle<T, R: Rng + ?Sized>(rng: &mut R, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = rng.random_range(0..=i);
            items.swap(i, j);
        }
    }

    pub fn random_seed_string<R: Rng + ?Sized>(rng: &mut R, length: usize) -> String {
        let chars: Vec<char> = SEED_ALPHABET.chars().collect();
        (0..length)
            .map(|_| chars[rng.random_range(0..chars.len())])
            .collect()
    }

    pub fn seed_string_from_seed(parent_seed: u64, length: usize) -> String {
        let mut rng = StdRng::seed_from_u64(parent_seed);
        Self::random_seed_string(&mut rng, length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rng;

    #[test]
    fn pick_index_respects_weights() {
        let weights = [0.0, 0.0, 1.0];
        for _ in 0..100 {
            assert_eq!(Weighted::pick_index(&mut rng(), &weights), Some(2));
        }
        assert_eq!(Weighted::pick_index(&mut rng(), &[0.0, 0.0]), None);
        assert_eq!(Weighted::pick_index(&mut rng(), &[]), None);
    }

    #[test]
    fn pick_index_returns_within_bounds() {
        let weights = [1.0, 2.0, 3.0];
        for _ in 0..1000 {
            let i = Weighted::pick_index(&mut rng(), &weights).unwrap();
            assert!(i < weights.len());
        }
    }

    #[test]
    fn shuffle_preserves_elements() {
        let mut items = vec![1, 2, 3, 4, 5];
        let mut sorted = items.clone();
        Weighted::shuffle(&mut rng(), &mut items);
        sorted.sort();
        items.sort();
        assert_eq!(items, sorted);
    }

    #[test]
    fn stable_top_n_distinct() {
        let weights = [1.0, 1.0, 1.0, 1.0];
        for _ in 0..50 {
            let picks = Weighted::pick_stable_top_n(&weights, 42, 2, |_| true);
            assert_eq!(picks.len(), 2);
            assert_ne!(picks[0], picks[1]);
        }
    }

    #[test]
    fn stable_top_n_skips_locked() {
        let weights = [1000.0, 0.0, 0.0001];
        let picks = Weighted::pick_stable_top_n(&weights, 7, 3, |i| i != 1);
        assert!(!picks.contains(&1));
        assert!(picks.contains(&0));
    }

    #[test]
    fn stable_is_reproducible() {
        let weights = [5.0, 3.0, 2.0, 1.0];
        for _ in 0..20 {
            let a = Weighted::pick_stable(&weights, 123, |_| true);
            let b = Weighted::pick_stable(&weights, 123, |_| true);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn seed_string_length_and_charset() {
        let mut rng = rng();
        let s = Weighted::random_seed_string(&mut rng, 8);
        assert_eq!(s.len(), 8);
        assert!(s.chars().all(|c| SEED_ALPHABET.contains(c)));
    }
}
