//! Weight capacity with upgradeable max (encumbrance).

use serde::{Deserialize, Serialize};

use crate::item::ItemRegistry;
use crate::slots::SlotInventory;

/// Total bag weight.
pub fn total_weight(reg: &ItemRegistry, inv: &SlotInventory) -> f32 {
    let mut w = 0.0;
    for i in 0..inv.len() {
        if let Some(st) = inv.get(i)
            && let Some(def) = reg.def(&st.def)
        {
            w += def.weight * st.qty as f32;
        }
    }
    w
}

/// True when `weight` exceeds `max`.
pub fn over_limit(weight: f32, max: f32) -> bool {
    weight > max
}

/// Tracked capacity. Bump `max` for bag upgrades.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Encumbrance {
    pub max: f32,
    pub current: f32,
}

impl Encumbrance {
    pub fn new(max: f32) -> Self {
        Self { max, current: 0.0 }
    }

    pub fn refresh(&mut self, reg: &ItemRegistry, inv: &SlotInventory) {
        self.current = total_weight(reg, inv);
    }

    pub fn is_over(&self) -> bool {
        over_limit(self.current, self.max)
    }

    /// Free headroom (may be negative when over).
    pub fn headroom(&self) -> f32 {
        self.max - self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemDef, ItemId, ItemStack};

    fn setup() -> (ItemRegistry, SlotInventory) {
        let mut r = ItemRegistry::new();
        r.register(ItemDef {
            id: ItemId::from("ore"),
            name: "Ore".into(),
            description: String::new(),
            stackable: true,
            max_stack: 99,
            weight: 10.0,
            value: 1,
            cells_w: 1,
            cells_h: 1,
            tags: vec![],
        });
        let mut inv = SlotInventory::new(4);
        inv.insert(&r, ItemStack::new("ore", 5));
        (r, inv)
    }

    #[test]
    fn weight_and_over() {
        let (r, inv) = setup();
        assert_eq!(total_weight(&r, &inv), 50.0);
        let mut e = Encumbrance::new(600.0);
        e.refresh(&r, &inv);
        assert!(!e.is_over());
        assert_eq!(e.headroom(), 550.0);
        e.max = 40.0;
        assert!(e.is_over());
    }
}
