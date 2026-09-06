//! Change journal drained by UI/save code.

use serde::{Deserialize, Serialize};

use crate::item::ItemId;

/// One container mutation. Order is chronological.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum InventoryEvent {
    Inserted { id: ItemId, qty: u32 },
    Removed { id: ItemId, qty: u32 },
    Moved { id: ItemId, from: usize, to: usize },
    Swapped { a: usize, b: usize },
    Split { id: ItemId, qty: u32 },
    Merged { id: ItemId, qty: u32 },
    Equipped { slot: String, id: ItemId },
    Unequipped { slot: String, id: ItemId },
    Crafted { id: ItemId, qty: u32 },
    Bought { id: ItemId, qty: u32, price: i64 },
    Sold { id: ItemId, qty: u32, price: i64 },
    Dropped { id: ItemId, qty: u32 },
    Broken { id: ItemId },
}
