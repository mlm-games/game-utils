//! Target selection: nearest-with-priority scoring plus sticky locks.
//!
//! Score is distance minus `priority * scale` (lowest wins), so high
//! priority targets pull selection without extra passes. An incumbent
//! keeps its lock until a challenger beats it by `margin` (anti-flicker).

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// One selectable target.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub id: u32,
    pub pos: Vec2,
    /// 0 = fodder, higher = focus (bosses, medics, objectives).
    pub priority: f32,
}

impl Candidate {
    pub fn new(id: u32, pos: Vec2, priority: f32) -> Self {
        Self { id, pos, priority }
    }
}

fn score(from: Vec2, c: &Candidate, priority_scale: f32) -> f32 {
    from.distance(c.pos) - c.priority * priority_scale.max(0.0)
}

/// Pick the best candidate. `incumbent` (current target id) wins ties
/// and keeps its lock until beaten by `margin` distance-equivalent.
pub fn pick(
    from: Vec2,
    candidates: &[Candidate],
    priority_scale: f32,
    incumbent: Option<u32>,
    margin: f32,
) -> Option<u32> {
    let mut best: Option<(u32, f32)> = None;
    for c in candidates {
        let s = score(from, c, priority_scale);
        let m = margin.max(0.0);
        let take = match best {
            None => true,
            Some((id, bs)) => {
                if Some(c.id) == incumbent {
                    s <= bs + m
                } else if Some(id) == incumbent {
                    s + m < bs
                } else {
                    s < bs
                }
            }
        };
        if take {
            best = Some((c.id, s));
        }
    }
    best.map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cands() -> Vec<Candidate> {
        vec![
            Candidate::new(1, Vec2::X * 10.0, 0.0),
            Candidate::new(2, Vec2::X * 12.0, 5.0),
            Candidate::new(3, Vec2::X * 30.0, 0.0),
        ]
    }

    #[test]
    fn nearest_wins_without_priority() {
        assert_eq!(pick(Vec2::ZERO, &cands(), 0.0, None, 0.0), Some(1));
    }

    #[test]
    fn priority_pulls_distant() {
        assert_eq!(pick(Vec2::ZERO, &cands(), 1.0, None, 0.0), Some(2));
    }

    #[test]
    fn incumbent_holds_within_margin() {
        // Scores: 1 -> 10, 2 -> 7. Incumbent 1 with margin 5: 10 < 7+5.
        assert_eq!(pick(Vec2::ZERO, &cands(), 1.0, Some(1), 5.0), Some(1));
        // Margin 1: 10 > 7+1, challenger takes it.
        assert_eq!(pick(Vec2::ZERO, &cands(), 1.0, Some(1), 1.0), Some(2));
    }

    #[test]
    fn empty_none() {
        assert_eq!(pick(Vec2::ZERO, &[], 1.0, None, 0.0), None);
    }

    #[test]
    fn incumbent_wins_ties_regardless_of_order() {
        let a = Candidate::new(1, Vec2::X * 10.0, 0.0);
        let b = Candidate::new(2, Vec2::X * -10.0, 0.0);
        assert_eq!(
            pick(Vec2::ZERO, &[a.clone(), b.clone()], 1.0, Some(2), 0.0),
            Some(2)
        );
        assert_eq!(pick(Vec2::ZERO, &[b, a], 1.0, Some(2), 0.0), Some(2));
    }
}
