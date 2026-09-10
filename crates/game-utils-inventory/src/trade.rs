//! Shop buy/sell against a funds balance.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::events::InventoryEvent;
use crate::item::{ItemId, ItemRegistry, ItemStack};
use crate::slots::SlotInventory;

/// Trade failure.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TradeError {
    UnknownPrice(ItemId),
    NoFunds { need: i64, have: i64 },
    NoStock { id: ItemId, need: u32, have: u32 },
    NoRoom { id: ItemId, qty: u32 },
}

/// Per-item (buy, sell) prices. Absent entries are not tradeable.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct PriceList {
    prices: HashMap<ItemId, (i64, i64)>,
}

impl PriceList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, id: impl Into<ItemId>, buy: i64, sell: i64) {
        self.prices.insert(id.into(), (buy.max(0), sell.max(0)));
    }

    pub fn buy_price(&self, id: &ItemId) -> Option<i64> {
        self.prices.get(id).map(|p| p.0)
    }

    pub fn sell_price(&self, id: &ItemId) -> Option<i64> {
        self.prices.get(id).map(|p| p.1)
    }
}

/// Buy `qty` into the bag, debiting `funds`. Atomic.
pub fn buy(
    reg: &ItemRegistry,
    prices: &PriceList,
    inv: &mut SlotInventory,
    funds: &mut i64,
    id: &ItemId,
    qty: u32,
) -> Result<(), TradeError> {
    let price = prices
        .buy_price(id)
        .ok_or_else(|| TradeError::UnknownPrice(id.clone()))?;
    if qty == 0 {
        return Ok(());
    }
    let total = price.checked_mul(qty as i64).unwrap_or(i64::MAX);
    if *funds < total {
        return Err(TradeError::NoFunds {
            need: total,
            have: *funds,
        });
    }
    if reg.def(id).is_none() {
        return Err(TradeError::UnknownPrice(id.clone()));
    }
    if inv.count_free(reg, id) < qty {
        return Err(TradeError::NoRoom {
            id: id.clone(),
            qty,
        });
    }
    *funds = funds.saturating_sub(total);
    let left = inv.insert(reg, ItemStack::new(id.clone(), qty));
    debug_assert_eq!(left, 0);
    inv.record(InventoryEvent::Bought {
        id: id.clone(),
        qty,
        price: total,
    });
    Ok(())
}

/// Sell `qty` from the bag, crediting `funds`. Atomic.
pub fn sell(
    _reg: &ItemRegistry,
    prices: &PriceList,
    inv: &mut SlotInventory,
    funds: &mut i64,
    id: &ItemId,
    qty: u32,
) -> Result<(), TradeError> {
    let price = prices
        .sell_price(id)
        .ok_or_else(|| TradeError::UnknownPrice(id.clone()))?;
    let have = inv.count(id);
    if have < qty {
        return Err(TradeError::NoStock {
            id: id.clone(),
            need: qty,
            have,
        });
    }
    if qty == 0 {
        return Ok(());
    }
    inv.remove_all(id, qty);
    let total = price.checked_mul(qty as i64).unwrap_or(i64::MAX);
    *funds = funds.saturating_add(total);
    inv.record(InventoryEvent::Sold {
        id: id.clone(),
        qty,
        price: total,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemDef;

    fn setup() -> (ItemRegistry, PriceList, SlotInventory) {
        let mut r = ItemRegistry::new();
        r.register(ItemDef {
            id: ItemId::from("potion"),
            name: "Potion".into(),
            description: String::new(),
            stackable: true,
            max_stack: 5,
            weight: 0.5,
            value: 10,
            cells_w: 1,
            cells_h: 1,
            tags: vec![],
        });
        let mut p = PriceList::new();
        p.set("potion", 12, 6);
        (r, p, SlotInventory::new(2))
    }

    #[test]
    fn buy_sell_roundtrip() {
        let (r, p, mut inv) = setup();
        let mut funds = 30;
        buy(&r, &p, &mut inv, &mut funds, &ItemId::from("potion"), 2).unwrap();
        assert_eq!(funds, 6);
        assert_eq!(inv.count(&ItemId::from("potion")), 2);
        sell(&r, &p, &mut inv, &mut funds, &ItemId::from("potion"), 1).unwrap();
        assert_eq!(funds, 12);
    }

    #[test]
    fn failures_atomic() {
        let (r, p, mut inv) = setup();
        let mut funds = 5;
        assert!(buy(&r, &p, &mut inv, &mut funds, &ItemId::from("potion"), 1).is_err());
        assert_eq!(funds, 5);
        assert!(sell(&r, &p, &mut inv, &mut funds, &ItemId::from("potion"), 1).is_err());
    }

    #[test]
    fn zero_qty_is_noop_not_free_item() {
        let (r, p, mut inv) = setup();
        let mut funds = 0;
        buy(&r, &p, &mut inv, &mut funds, &ItemId::from("potion"), 0).unwrap();
        assert_eq!(funds, 0);
        assert_eq!(inv.count(&ItemId::from("potion")), 0);
        sell(&r, &p, &mut inv, &mut funds, &ItemId::from("potion"), 0).unwrap();
        assert_eq!(funds, 0);
    }
}
