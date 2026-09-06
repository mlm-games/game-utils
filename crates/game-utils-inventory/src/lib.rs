//! Item stacks, slot/shaped/equipment containers, crafting, and trade.
//!
//! Taxonomy-free: categories and sockets are plain tag strings.

pub mod crafting;
pub mod equipment;
pub mod events;
pub mod item;
pub mod limits;
pub mod shaped;
pub mod slots;
pub mod trade;

pub use crafting::{CraftError, Recipe, can_craft, craft};
pub use equipment::{EquipError, EquipSlot, Equipment};
pub use events::InventoryEvent;
pub use item::{Condition, ItemDef, ItemId, ItemRegistry, ItemStack};
pub use limits::{Encumbrance, over_limit, total_weight};
pub use shaped::{PlaceError, PlacedItem, ShapedInventory};
pub use slots::SlotInventory;
pub use trade::{PriceList, TradeError, buy, sell};
