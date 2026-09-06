//! Health pools, dodge/crit resolution, mitigation, buffs, cooldowns,
//! DoTs, drop tables.
//!
//! Stateless resolve fns take `rng` (rand) so rolls stay game-seeded.

pub mod buffs;
pub mod cooldowns;
pub mod damage;
pub mod dots;
pub mod health;
pub mod loot;
pub mod stats;

pub use buffs::{Buff, BuffList, resist_duration};
pub use cooldowns::{Cooldown, Cooldowns};
pub use damage::{Attack, Defense, Exchange, Secondary, exchange};
pub use dots::{Dot, Dots};
pub use health::{DamageResult, Health};
pub use loot::{LootEntry, LootTable};
pub use stats::{StatMod, StatSheet};
