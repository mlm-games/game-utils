//! Helpers for tableau columns: ordered stacks with a hidden prefix
//! and a visible suffix, for boards built from face-down / face-up
//! columns, meld rows, and build piles.

use serde::{Deserialize, Serialize};

/// One tableau column. The first `face_down` cards are hidden; the rest
/// are visible. Invariant: `face_down <= cards.len()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column<T> {
    face_down: usize,
    cards: Vec<T>,
}

impl<T> Default for Column<T> {
    fn default() -> Self {
        Self {
            face_down: 0,
            cards: Vec::new(),
        }
    }
}

impl<T> Column<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cards(face_down: usize, cards: Vec<T>) -> Self {
        let mut col = Self {
            face_down: 0,
            cards,
        };
        col.set_face_down(face_down);
        col
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn face_down(&self) -> usize {
        self.face_down
    }

    pub fn set_face_down(&mut self, n: usize) {
        self.face_down = n.min(self.cards.len());
    }

    pub fn cards(&self) -> &[T] {
        &self.cards
    }

    /// Visible suffix (everything past the hidden prefix).
    pub fn visible(&self) -> &[T] {
        &self.cards[self.face_down..]
    }

    pub fn top(&self) -> Option<&T> {
        self.cards.last()
    }

    pub fn push(&mut self, card: T) {
        self.cards.push(card);
        self.face_down = self.face_down.min(self.cards.len());
    }

    pub fn pop(&mut self) -> Option<T> {
        let card = self.cards.pop()?;
        self.face_down = self.face_down.min(self.cards.len());
        Some(card)
    }

    /// Reveal the topmost hidden card, if any. Returns true on change.
    pub fn flip_top(&mut self) -> bool {
        if self.face_down > 0 && self.face_down <= self.cards.len() {
            self.face_down -= 1;
            true
        } else {
            false
        }
    }

    /// Take up to `n` visible cards off the top, preserving order.
    /// Returns fewer when fewer are visible.
    pub fn take_visible(&mut self, n: usize) -> Vec<T> {
        let visible = self.cards.len().saturating_sub(self.face_down);
        let n = n.min(visible);
        self.cards.split_off(self.cards.len() - n)
    }

    pub fn clear(&mut self) -> Vec<T> {
        self.face_down = 0;
        std::mem::take(&mut self.cards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_flip_and_take() {
        let mut col = Column::with_cards(2, vec![1, 2, 3, 4]);
        assert_eq!(col.visible(), &[3, 4]);
        assert!(col.flip_top());
        assert_eq!(col.visible(), &[2, 3, 4]);
        let taken = col.take_visible(2);
        assert_eq!(taken, vec![3, 4]);
        assert_eq!(col.face_down(), 1);
        // Cannot take hidden cards.
        let taken = col.take_visible(10);
        assert_eq!(taken, vec![2]);
    }

    #[test]
    fn column_pop_clamps_hidden() {
        let mut col = Column::with_cards(3, vec![1, 2, 3]);
        assert_eq!(col.pop(), Some(3));
        assert_eq!(col.face_down(), 2);
        assert!(col.flip_top());
        assert_eq!(col.face_down(), 1);
    }
}
