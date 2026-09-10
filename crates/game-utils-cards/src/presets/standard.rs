//! Classic suit/rank cards: 52-card decks, dual faces, finishes.

use serde::{Deserialize, Serialize};

/// Card suit. Non-French decks are [`Suit::Custom`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
    Custom(String),
}

impl Suit {
    pub fn of(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "spades" | "s" => Self::Spades,
            "hearts" | "h" => Self::Hearts,
            "diamonds" | "d" => Self::Diamonds,
            "clubs" | "c" => Self::Clubs,
            other => Self::Custom(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Spades => "spades",
            Self::Hearts => "hearts",
            Self::Diamonds => "diamonds",
            Self::Clubs => "clubs",
            Self::Custom(s) => s,
        }
    }

    pub fn is_red(&self) -> bool {
        matches!(self, Self::Hearts | Self::Diamonds)
    }

    pub fn is_black(&self) -> bool {
        matches!(self, Self::Spades | Self::Clubs)
    }
}

impl std::fmt::Display for Suit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Card rank: 1 (ace) through 13 (king).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rank(pub u8);

impl Rank {
    pub const ACE: Self = Self(1);
    pub const JACK: Self = Self(11);
    pub const QUEEN: Self = Self(12);
    pub const KING: Self = Self(13);

    /// Returns `None` outside 1..=13.
    pub fn new(rank: u8) -> Option<Self> {
        (1..=13).contains(&rank).then_some(Self(rank))
    }

    pub fn value(self) -> u8 {
        self.0
    }

    pub fn is_ace(self) -> bool {
        self.0 == 1
    }

    pub fn is_face(self) -> bool {
        self.0 >= 11
    }

    /// Counting value, faces worth 10 (aces need [`FlexibleValue`]).
    pub fn pip_value(self) -> u8 {
        self.0.min(10)
    }

    pub fn label(self) -> &'static str {
        match self.0 {
            1 => "A",
            11 => "J",
            12 => "Q",
            13 => "K",
            _ => "?",
        }
    }
}

/// One standard playing card, plus cosmetic finish flag.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StandardCard {
    pub suit: Suit,
    pub rank: Rank,
    #[serde(default)]
    pub foil: bool,
}

impl StandardCard {
    pub fn new(suit: Suit, rank: Rank) -> Self {
        Self {
            suit,
            rank,
            foil: false,
        }
    }

    pub fn with_foil(mut self) -> Self {
        self.foil = true;
        self
    }
}

impl std::fmt::Display for StandardCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.rank.is_face() || self.rank.is_ace() {
            write!(f, "{} of {}", self.rank.label(), self.suit)
        } else {
            write!(f, "{} of {}", self.rank.value(), self.suit)
        }
    }
}

/// Full 52-card deck: every suit x every rank, unshuffled.
pub fn full_deck() -> Vec<StandardCard> {
    let suits = [Suit::Spades, Suit::Hearts, Suit::Diamonds, Suit::Clubs];
    let mut deck = Vec::with_capacity(52);
    for suit in suits {
        for rank in 1..=13 {
            deck.push(StandardCard::new(suit.clone(), Rank(rank)));
        }
    }
    deck
}

/// Dual-faced value: `high` while the total fits `limit`, else `low`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlexibleValue {
    pub low: i32,
    pub high: i32,
}

impl FlexibleValue {
    pub fn new(low: i32, high: i32) -> Self {
        Self { low, high }
    }

    /// Contribution given the total of everything else and the limit.
    pub fn contribute(&self, rest: i32, limit: i32) -> i32 {
        if rest + self.high <= limit {
            self.high
        } else {
            self.low
        }
    }
}

/// Total of fixed plus flexible values without passing `limit`.
/// Exact (best sum ≤ `limit`, least bust when everything busts) for up
/// to 20 flexibles via exhaustive high/low search; greedy high-first
/// above that (exact for single-pivot values like aces either way).
pub fn total_with_flexibles(fixed: i32, flexibles: &[FlexibleValue], limit: i32) -> i32 {
    if flexibles.len() <= 20 {
        let mut best_under: Option<i32> = None;
        let mut best_over = i32::MAX;
        for mask in 0..(1u32 << flexibles.len()) {
            let mut total = fixed;
            for (i, f) in flexibles.iter().enumerate() {
                total += if mask & (1 << i) != 0 { f.high } else { f.low };
            }
            if total <= limit {
                best_under = Some(best_under.map_or(total, |b: i32| b.max(total)));
            } else {
                best_over = best_over.min(total);
            }
        }
        return best_under.unwrap_or(best_over);
    }
    let mut total = fixed;
    let mut ordered: Vec<FlexibleValue> = flexibles.to_vec();
    ordered.sort_by_key(|f| f.high);
    for f in ordered.iter().rev() {
        total += f.contribute(total, limit);
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_has_52_unique() {
        let deck = full_deck();
        assert_eq!(deck.len(), 52);
        let set: std::collections::HashSet<String> = deck.iter().map(|c| c.to_string()).collect();
        assert_eq!(set.len(), 52);
    }

    #[test]
    fn suits_and_ranks() {
        assert!(Suit::Hearts.is_red());
        assert!(Suit::Spades.is_black());
        assert_eq!(Suit::of("stars"), Suit::Custom("stars".to_string()));
        assert_eq!(Rank::new(0), None);
        assert_eq!(Rank::new(14), None);
        assert!(Rank::QUEEN.is_face());
        assert_eq!(Rank::KING.pip_value(), 10);
        assert_eq!(Rank(7).pip_value(), 7);
        assert_eq!(
            StandardCard::new(Suit::Spades, Rank::ACE).to_string(),
            "A of spades"
        );
    }

    #[test]
    fn flexible_ace_totals() {
        let ace = FlexibleValue::new(1, 11);
        assert_eq!(ace.contribute(10, 21), 11);
        assert_eq!(ace.contribute(11, 21), 1);
        assert_eq!(total_with_flexibles(10, &[ace], 21), 21);
        assert_eq!(total_with_flexibles(12, &[ace, ace], 21), 14);
        assert_eq!(total_with_flexibles(20, &[ace], 21), 21);
    }

    #[test]
    fn flexible_general_case_is_optimal() {
        let f = FlexibleValue::new(5, 6);
        assert_eq!(total_with_flexibles(0, &[f, f], 10), 10);
    }
}
