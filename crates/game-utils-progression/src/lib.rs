//! XP curves with carry-over level-ups, plus weighted upgrade drafts.
//!
//! Weighted picks use rand's WeightedIndex (external, already a dep).

pub mod draft;
pub mod xp;

pub use draft::{Draft, DraftPool, Offer, offer, reroll, reroll_cost};
pub use xp::{Curve, Level, Prestige, need_for};
