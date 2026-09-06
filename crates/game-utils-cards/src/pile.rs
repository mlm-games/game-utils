use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

/// Open zone identifier. Vocabularies differ per game, so only the
/// most common zones get constructors; the rest are `Zone::custom`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Zone(pub String);

impl Zone {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn custom(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn draw() -> Self {
        Self("draw".to_string())
    }

    pub fn hand() -> Self {
        Self("hand".to_string())
    }

    pub fn discard() -> Self {
        Self("discard".to_string())
    }

    pub fn deck() -> Self {
        Self("deck".to_string())
    }

    pub fn play() -> Self {
        Self("play".to_string())
    }

    /// Removed-from-game zone (burned, consumed, ...).
    pub fn removed() -> Self {
        Self("removed".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Zone {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for Zone {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for Zone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::borrow::Borrow<str> for Zone {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Ordered pile in a [`Zone`]. Back is top. Shuffles take an
/// explicit RNG for determinism.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pile<T> {
    pub zone: Zone,
    cards: Vec<T>,
}

impl<T> Pile<T> {
    pub fn new(zone: Zone) -> Self {
        Self {
            zone,
            cards: Vec::new(),
        }
    }

    pub fn with_cards(zone: Zone, cards: Vec<T>) -> Self {
        Self { zone, cards }
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// All cards, bottom-to-top.
    pub fn cards(&self) -> &[T] {
        &self.cards
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.cards.iter()
    }

    /// Top card without removing it.
    pub fn peek(&self) -> Option<&T> {
        self.cards.last()
    }

    pub fn contains(&self, mut pred: impl FnMut(&T) -> bool) -> bool {
        self.cards.iter().any(&mut pred)
    }

    pub fn position(&self, mut pred: impl FnMut(&T) -> bool) -> Option<usize> {
        self.cards.iter().position(&mut pred)
    }

    pub fn get(&self, idx: usize) -> Option<&T> {
        self.cards.get(idx)
    }

    pub fn push(&mut self, card: T) {
        self.cards.push(card);
    }

    pub fn extend(&mut self, cards: impl IntoIterator<Item = T>) {
        self.cards.extend(cards);
    }

    pub fn insert_at(&mut self, idx: usize, card: T) {
        if idx >= self.cards.len() {
            self.cards.push(card);
        } else {
            self.cards.insert(idx, card);
        }
    }

    pub fn remove_at(&mut self, idx: usize) -> Option<T> {
        if idx < self.cards.len() {
            Some(self.cards.remove(idx))
        } else {
            None
        }
    }

    pub fn retain(&mut self, pred: impl FnMut(&T) -> bool) {
        self.cards.retain(pred);
    }

    /// Reorder in place via a scoped closure.
    pub fn reorder(&mut self, f: impl FnOnce(&mut Vec<T>)) {
        f(&mut self.cards);
    }

    pub fn move_to_top(&mut self, idx: usize) {
        if idx < self.cards.len() {
            let c = self.cards.remove(idx);
            self.cards.push(c);
        }
    }

    pub fn move_to_bottom(&mut self, idx: usize) {
        if idx < self.cards.len() {
            let c = self.cards.remove(idx);
            self.cards.insert(0, c);
        }
    }

    /// Draw the top card.
    pub fn draw_one(&mut self) -> Option<T> {
        self.cards.pop()
    }

    /// Draw up to `n` cards from the top, preserving their order.
    pub fn draw(&mut self, n: usize) -> Vec<T> {
        let n = n.min(self.cards.len());
        self.cards.split_off(self.cards.len() - n)
    }

    /// Move up to `n` top cards onto `other`'s top.
    pub fn transfer_to(&mut self, other: &mut Pile<T>, n: usize) {
        let mut drawn = self.draw(n);
        other.cards.append(&mut drawn);
    }

    pub fn clear(&mut self) -> Vec<T> {
        std::mem::take(&mut self.cards)
    }

    /// Shuffle in place. Caller provides the RNG for determinism.
    pub fn shuffle<R: Rng + ?Sized>(&mut self, rng: &mut R) {
        self.cards.shuffle(rng);
    }

    /// Adopt and shuffle `other` when empty. No-op otherwise.
    pub fn replenish_from<R: Rng + ?Sized>(&mut self, other: &mut Pile<T>, rng: &mut R) -> bool {
        if !self.is_empty() || other.is_empty() {
            return false;
        }
        let mut cards = other.clear();
        cards.shuffle(rng);
        self.cards = cards;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn pile_draw_transfer() {
        let mut draw = Pile::with_cards(Zone::draw(), vec![1, 2, 3, 4]);
        let mut hand = Pile::new(Zone::hand());
        let mut discard = Pile::new(Zone::discard());
        assert_eq!(draw.len(), 4);
        hand.extend(draw.draw(2));
        assert_eq!(hand.cards(), &[3, 4]);
        assert_eq!(draw.cards(), &[1, 2]);
        assert_eq!(hand.peek(), Some(&4));
        hand.transfer_to(&mut discard, 1);
        assert_eq!(discard.cards(), &[4]);
    }

    #[test]
    fn pile_custom_zones() {
        let tableau = Pile::with_cards(Zone::custom("tableau-3"), vec![1]);
        assert_eq!(tableau.zone.as_str(), "tableau-3");
        let sort: Pile<i32> = Pile::new(Zone::from("sort"));
        assert!(sort.is_empty());
    }

    #[test]
    fn pile_remove_and_reorder() {
        let mut p = Pile::with_cards(Zone::draw(), vec![1, 2, 3]);
        assert_eq!(p.remove_at(5), None);
        assert_eq!(p.remove_at(0), Some(1));
        p.reorder(|v| v.sort());
        assert_eq!(p.cards(), &[2, 3]);
        assert_eq!(p.position(|c| *c == 3), Some(1));
    }

    #[test]
    fn pile_shuffle_deterministic() {
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let mut a = Pile::with_cards(Zone::draw(), (0..10).collect::<Vec<_>>());
        let mut b = Pile::with_cards(Zone::draw(), (0..10).collect::<Vec<_>>());
        a.shuffle(&mut rng1);
        b.shuffle(&mut rng2);
        assert_eq!(a.cards(), b.cards());
    }

    #[test]
    fn pile_replenish() {
        let mut rng = StdRng::seed_from_u64(0);
        let mut draw: Pile<i32> = Pile::new(Zone::draw());
        let mut discard = Pile::with_cards(Zone::discard(), vec![10, 20, 30]);
        assert!(draw.replenish_from(&mut discard, &mut rng));
        assert_eq!(draw.len(), 3);
        assert!(discard.is_empty());
        assert!(!draw.replenish_from(&mut discard, &mut rng));
    }
}
