//! Engine-agnostic card/deck primitives. Taxonomy-free core (open
//! kinds, rarities, zones); ready-made conventions under [`presets`].
//!
//! Map: [`Pile`]/[`Zone`] ordered piles, [`Bag`] unordered pools,
//! [`Hand`] bounded holders, [`CardDef`] headers with open payload,
//! [`Cost`]/[`ResourcePool`]/[`ResourceMap`] costs, [`EventLog`],
//! [`Registry`], [`WeightedPool`].

pub mod bag;
pub mod card;
pub mod energy;
pub mod hand;
pub mod log;
pub mod pile;
pub mod presets;
pub mod registry;

pub use bag::{Bag, WeightedPool};
pub use card::{CardDef, CardId, CardKind, DeckTemplate, Rarity};
pub use energy::{Cost, EnergyPool, ResourceMap, ResourcePool};
pub use hand::{FanLayout, Hand, HandLayout, LinearLayout, OverlapLayout, Transform2d};
pub use log::EventLog;
pub use pile::{Pile, Zone};
pub use registry::Registry;
