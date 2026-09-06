//! Draw-pile battler helpers: redraws with auto-reshuffle. Kind
//! strings are conventions, not an enum.

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::card::CardId;
use crate::pile::{Pile, Zone};

/// Conventional kind labels (extensible, or use `CardKind`).
pub mod kind {
    pub const ATTACK: &str = "attack";
    pub const SKILL: &str = "skill";
    pub const POWER: &str = "power";
    pub const STATUS: &str = "status";
    pub const CURSE: &str = "curse";
}

/// Run events. Pair with [`EventLog`](crate::log::EventLog).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RunEvent {
    Draw { card: CardId, from: Zone },
    Discard { card: CardId },
    Remove { card: CardId },
    Shuffle { zone: Zone },
    Play { card: CardId, resource_spent: i32 },
    ResourceChanged { from: i32, to: i32 },
}

/// Draw up to `n`, reshuffling discard as needed, in draw order.
pub fn draw_step<T, R: Rng + ?Sized>(
    draw: &mut Pile<T>,
    discard: &mut Pile<T>,
    rng: &mut R,
    n: usize,
) -> Vec<T> {
    let mut out = Vec::new();
    for _ in 0..n {
        if draw.is_empty() {
            draw.replenish_from(discard, rng);
        }
        match draw.draw_one() {
            Some(card) => out.push(card),
            None => break,
        }
    }
    out
}

/// Move `hand` cards into `discard`.
pub fn discard_hand<T>(hand: &mut Pile<T>, discard: &mut Pile<T>) {
    let cards = hand.clear();
    discard.extend(cards);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn draw_step_reshuffles() {
        let mut rng = StdRng::seed_from_u64(3);
        let mut draw = Pile::with_cards(Zone::draw(), vec![1]);
        let mut discard = Pile::with_cards(Zone::discard(), vec![2, 3, 4]);
        let got = draw_step(&mut draw, &mut discard, &mut rng, 3);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], 1);
        assert!(discard.is_empty());
    }

    #[test]
    fn draw_step_stops_when_empty() {
        let mut rng = StdRng::seed_from_u64(3);
        let mut draw: Pile<i32> = Pile::new(Zone::draw());
        let mut discard: Pile<i32> = Pile::new(Zone::discard());
        assert!(draw_step(&mut draw, &mut discard, &mut rng, 5).is_empty());
    }

    #[test]
    fn discard_hand_moves_all() {
        let mut hand = Pile::with_cards(Zone::hand(), vec![1, 2]);
        let mut discard = Pile::new(Zone::discard());
        discard_hand(&mut hand, &mut discard);
        assert!(hand.is_empty());
        assert_eq!(discard.len(), 2);
    }
}
