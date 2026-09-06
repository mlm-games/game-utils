//! Atomic crafting: consume inputs, produce outputs, or fail clean.

use serde::{Deserialize, Serialize};

use crate::events::InventoryEvent;
use crate::item::{ItemId, ItemRegistry, ItemStack};
use crate::slots::SlotInventory;

/// Craft failure.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CraftError {
    UnknownItem(ItemId),
    Missing { id: ItemId, need: u32, have: u32 },
    NoOutputRoom { id: ItemId, qty: u32 },
}

/// One recipe: N inputs -> M outputs.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub inputs: Vec<(ItemId, u32)>,
    pub outputs: Vec<(ItemId, u32)>,
}

impl Recipe {
    pub fn new(
        id: impl Into<String>,
        inputs: Vec<(ItemId, u32)>,
        outputs: Vec<(ItemId, u32)>,
    ) -> Self {
        Self {
            id: id.into(),
            inputs,
            outputs,
        }
    }
}

/// True when inputs are present and outputs fit (for `times` runs).
pub fn can_craft(reg: &ItemRegistry, inv: &SlotInventory, recipe: &Recipe, times: u32) -> bool {
    if times == 0 {
        return true;
    }
    for (id, qty) in &recipe.inputs {
        if reg.def(id).is_none() || inv.count(id) < qty * times {
            return false;
        }
    }
    // Simulate consumption, then check output room.
    let mut sim = inv.clone();
    for (id, qty) in &recipe.inputs {
        sim.remove_all(id, qty * times);
    }
    for (id, qty) in &recipe.outputs {
        if reg.def(id).is_none() || sim.count_free(reg, id) < qty * times {
            return false;
        }
    }
    true
}

/// Run `times` crafts atomically. Returns Err without mutating on failure.
pub fn craft(
    reg: &ItemRegistry,
    inv: &mut SlotInventory,
    recipe: &Recipe,
    times: u32,
) -> Result<(), CraftError> {
    if times == 0 {
        return Ok(());
    }
    for (id, qty) in &recipe.inputs {
        if reg.def(id).is_none() {
            return Err(CraftError::UnknownItem(id.clone()));
        }
        let have = inv.count(id);
        if have < qty * times {
            return Err(CraftError::Missing {
                id: id.clone(),
                need: qty * times,
                have,
            });
        }
    }
    // Output room after simulated consumption.
    let mut sim = inv.clone();
    for (id, qty) in &recipe.inputs {
        sim.remove_all(id, qty * times);
    }
    for (id, qty) in &recipe.outputs {
        let Some(_) = reg.def(id) else {
            return Err(CraftError::UnknownItem(id.clone()));
        };
        if sim.count_free(reg, id) < qty * times {
            return Err(CraftError::NoOutputRoom {
                id: id.clone(),
                qty: qty * times,
            });
        }
    }
    for (id, qty) in &recipe.inputs {
        inv.remove_all(id, qty * times);
    }
    for (id, qty) in &recipe.outputs {
        let left = inv.insert(reg, ItemStack::new(id.clone(), qty * times));
        debug_assert_eq!(left, 0);
        inv.record(InventoryEvent::Crafted {
            id: id.clone(),
            qty: qty * times,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemDef;

    fn setup() -> (ItemRegistry, SlotInventory, Recipe) {
        let mut r = ItemRegistry::new();
        let mk = |id: &str| ItemDef {
            id: ItemId::from(id),
            name: id.into(),
            description: String::new(),
            stackable: true,
            max_stack: 99,
            weight: 1.0,
            value: 1,
            cells_w: 1,
            cells_h: 1,
            tags: vec![],
        };
        r.register(mk("wood"));
        r.register(mk("plank"));
        let mut inv = SlotInventory::new(4);
        inv.insert(&r, ItemStack::new("wood", 4));
        let recipe = Recipe::new(
            "planks",
            vec![(ItemId::from("wood"), 2)],
            vec![(ItemId::from("plank"), 3)],
        );
        (r, inv, recipe)
    }

    #[test]
    fn craft_consumes_and_produces() {
        let (r, mut inv, recipe) = setup();
        assert!(can_craft(&r, &inv, &recipe, 2));
        craft(&r, &mut inv, &recipe, 2).unwrap();
        assert_eq!(inv.count(&ItemId::from("wood")), 0);
        assert_eq!(inv.count(&ItemId::from("plank")), 6);
    }

    #[test]
    fn craft_missing_is_atomic() {
        let (r, mut inv, recipe) = setup();
        assert!(!can_craft(&r, &inv, &recipe, 3));
        assert!(craft(&r, &mut inv, &recipe, 3).is_err());
        assert_eq!(inv.count(&ItemId::from("wood")), 4);
    }

    #[test]
    fn craft_no_room_is_atomic() {
        let (r, _, recipe) = setup();
        let mut tiny = SlotInventory::new(1);
        tiny.insert(&r, ItemStack::new("wood", 4));
        assert_eq!(
            craft(&r, &mut tiny, &recipe, 1),
            Err(CraftError::NoOutputRoom {
                id: ItemId::from("plank"),
                qty: 3
            })
        );
        // Wood untouched (outputs need 1 slot, wood occupies the only one).
        assert_eq!(tiny.count(&ItemId::from("wood")), 4);
    }
}
