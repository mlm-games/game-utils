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

/// Collapse repeated rows (`[(wood,3),(wood,3)]`) into one total per id so the
/// check, the simulation, and the consumption all agree. Rows whose scaled qty
/// is zero drop out, so they can neither be checked nor minted.
fn totals(rows: &[(ItemId, u32)], times: u32) -> Vec<(ItemId, u32)> {
    let mut out: Vec<(ItemId, u32)> = Vec::with_capacity(rows.len());
    for (id, qty) in rows {
        let n = qty.saturating_mul(times);
        if n == 0 {
            continue;
        }
        match out.iter_mut().find(|(existing, _)| existing == id) {
            Some(e) => e.1 = e.1.saturating_add(n),
            None => out.push((id.clone(), n)),
        }
    }
    out
}

/// Inventory state the recipe would leave behind, or the reason it cannot.
/// Output room is resolved by actually inserting into the simulation, so rows
/// that compete for the same free slots are accounted for together.
fn plan(
    reg: &ItemRegistry,
    inv: &SlotInventory,
    recipe: &Recipe,
    times: u32,
) -> Result<Vec<(ItemId, u32)>, CraftError> {
    let inputs = totals(&recipe.inputs, times);
    let outputs = totals(&recipe.outputs, times);

    for (id, need) in &inputs {
        if reg.def(id).is_none() {
            return Err(CraftError::UnknownItem(id.clone()));
        }
        let have = inv.count(id);
        if have < *need {
            return Err(CraftError::Missing {
                id: id.clone(),
                need: *need,
                have,
            });
        }
    }
    for id in &outputs {
        if reg.def(&id.0).is_none() {
            return Err(CraftError::UnknownItem(id.0.clone()));
        }
    }

    let mut sim = inv.clone();
    for (id, qty) in &inputs {
        sim.remove_all(id, *qty);
    }
    for (id, qty) in &outputs {
        if sim.insert(reg, ItemStack::new(id.clone(), *qty)) > 0 {
            return Err(CraftError::NoOutputRoom {
                id: id.clone(),
                qty: *qty,
            });
        }
    }
    Ok(outputs)
}

/// True when inputs are present and outputs fit (for `times` runs).
pub fn can_craft(reg: &ItemRegistry, inv: &SlotInventory, recipe: &Recipe, times: u32) -> bool {
    if times == 0 {
        return true;
    }
    plan(reg, inv, recipe, times).is_ok()
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
    let outputs = plan(reg, inv, recipe, times)?;
    let inputs = totals(&recipe.inputs, times);

    for (id, qty) in &inputs {
        inv.remove_all(id, *qty);
    }
    for (id, qty) in &outputs {
        let left = inv.insert(reg, ItemStack::new(id.clone(), *qty));
        debug_assert_eq!(left, 0);
        inv.record(InventoryEvent::Crafted {
            id: id.clone(),
            qty: *qty,
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
