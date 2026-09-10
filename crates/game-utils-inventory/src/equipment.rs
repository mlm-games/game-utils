//! Named equipment slots with tag-gated sockets.

use serde::{Deserialize, Serialize};

use crate::events::InventoryEvent;
use crate::item::{ItemRegistry, ItemStack};
use crate::slots::SlotInventory;

/// Equip failure.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EquipError {
    UnknownSlot,
    EmptySlot,
    RejectedTag,
    NoRoom,
}

/// One socket: empty `accepts` takes anything.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct EquipSlot {
    pub name: String,
    pub accepts: Vec<String>,
    pub stack: Option<ItemStack>,
}

impl EquipSlot {
    pub fn new(name: impl Into<String>, accepts: Vec<String>) -> Self {
        Self {
            name: name.into(),
            accepts,
            stack: None,
        }
    }

    fn accepts_def(&self, reg: &ItemRegistry, stack: &ItemStack) -> bool {
        if self.accepts.is_empty() {
            return true;
        }
        match reg.def(&stack.def) {
            Some(def) => self.accepts.iter().any(|t| def.has_tag(t)),
            None => false,
        }
    }
}

/// Worn loadout (hands, armor, trinkets). Displaced gear flows back
/// into the bag; equipping fails when the bag cannot take it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Equipment {
    slots: Vec<EquipSlot>,
    #[serde(skip, default = "_events")]
    events: Vec<InventoryEvent>,
}

fn _events() -> Vec<InventoryEvent> {
    Vec::new()
}

impl Equipment {
    pub fn new(slots: Vec<EquipSlot>) -> Self {
        Self {
            slots,
            events: Vec::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&EquipSlot> {
        self.slots.iter().find(|s| s.name == name)
    }

    pub fn drain_events(&mut self) -> Vec<InventoryEvent> {
        core::mem::take(&mut self.events)
    }

    /// Move `bag[idx]` into `slot`, returning the old occupant to the bag.
    /// All-or-nothing: fails before mutating when the bag cannot take
    /// the displaced occupant.
    pub fn equip(
        &mut self,
        reg: &ItemRegistry,
        bag: &mut SlotInventory,
        idx: usize,
        slot: &str,
    ) -> Result<(), EquipError> {
        let si = self
            .slots
            .iter()
            .position(|s| s.name == slot)
            .ok_or(EquipError::UnknownSlot)?;
        let stack = bag.get(idx).cloned().ok_or(EquipError::EmptySlot)?;
        if !self.slots[si].accepts_def(reg, &stack) {
            return Err(EquipError::RejectedTag);
        }
        let taken = bag.remove(idx, u32::MAX).ok_or(EquipError::EmptySlot)?;
        let old = self.slots[si].stack.replace(taken);
        if let Some(prev) = old {
            if bag.free_for(reg, &prev.def, prev.condition) < prev.qty {
                let back = self.slots[si].stack.replace(prev);
                let _ = bag.insert(reg, back.expect("just equipped"));
                return Err(EquipError::NoRoom);
            }
            let left = bag.insert(reg, prev);
            debug_assert_eq!(left, 0);
        }
        let id = self.slots[si].stack.as_ref().unwrap().def.clone();
        self.events.push(InventoryEvent::Equipped {
            slot: slot.into(),
            id,
        });
        Ok(())
    }

    /// Move `slot` contents back into the bag. Fails before mutating
    /// when the bag has no room.
    pub fn unequip(
        &mut self,
        reg: &ItemRegistry,
        bag: &mut SlotInventory,
        slot: &str,
    ) -> Result<(), EquipError> {
        let si = self
            .slots
            .iter()
            .position(|s| s.name == slot)
            .ok_or(EquipError::UnknownSlot)?;
        let stack = self.slots[si].stack.clone().ok_or(EquipError::EmptySlot)?;
        if bag.free_for(reg, &stack.def, stack.condition) < stack.qty {
            return Err(EquipError::NoRoom);
        }
        let stack = self.slots[si].stack.take().unwrap();
        let id = stack.def.clone();
        let left = bag.insert(reg, stack);
        debug_assert_eq!(left, 0);
        self.events.push(InventoryEvent::Unequipped {
            slot: slot.into(),
            id,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemDef, ItemId};

    fn reg() -> ItemRegistry {
        let mut r = ItemRegistry::new();
        let mk = |id: &str, tags: &[&str]| ItemDef {
            id: ItemId::from(id),
            name: id.into(),
            description: String::new(),
            stackable: false,
            max_stack: 1,
            weight: 1.0,
            value: 1,
            cells_w: 1,
            cells_h: 1,
            tags: tags.iter().map(|s| s.to_string()).collect(),
        };
        r.register(mk("sword", &["weapon"]));
        r.register(mk("helm", &["armor"]));
        r
    }

    fn gear() -> Equipment {
        Equipment::new(vec![
            EquipSlot::new("hand", vec!["weapon".into()]),
            EquipSlot::new("head", vec!["armor".into()]),
        ])
    }

    #[test]
    fn equip_rejects_wrong_socket() {
        let r = reg();
        let mut bag = SlotInventory::new(2);
        bag.insert(&r, ItemStack::new("helm", 1));
        let mut gear = gear();
        assert_eq!(
            gear.equip(&r, &mut bag, 0, "hand"),
            Err(EquipError::RejectedTag)
        );
        assert!(gear.equip(&r, &mut bag, 0, "head").is_ok());
        assert!(bag.is_empty());
    }

    #[test]
    fn full_bag_swap_uses_freed_slot() {
        let r = reg();
        let mut bag = SlotInventory::new(1);
        bag.insert(&r, ItemStack::new("sword", 1));
        let mut gear = gear();
        gear.equip(&r, &mut bag, 0, "hand").unwrap();
        bag.insert(&r, ItemStack::new("sword", 1));
        assert!(gear.equip(&r, &mut bag, 0, "hand").is_ok());
        assert_eq!(bag.count(&ItemId::from("sword")), 1);
        assert_eq!(
            gear.get("hand").unwrap().stack.as_ref().unwrap().def,
            ItemId::from("sword")
        );
    }

    #[test]
    fn equip_swaps_back_to_bag() {
        let r = reg();
        let mut bag = SlotInventory::new(2);
        bag.insert(&r, ItemStack::new("sword", 1));
        let mut gear = gear();
        gear.equip(&r, &mut bag, 0, "hand").unwrap();
        bag.insert(&r, ItemStack::new("helm", 1));
        // Helm cannot go in hand; sword stays.
        assert!(gear.equip(&r, &mut bag, 0, "hand").is_err());
        assert_eq!(
            gear.get("hand").unwrap().stack.as_ref().unwrap().def,
            ItemId::from("sword")
        );
    }

    #[test]
    fn unequip_roundtrip_and_full_bag() {
        let r = reg();
        let mut bag = SlotInventory::new(1);
        bag.insert(&r, ItemStack::new("sword", 1));
        let mut gear = gear();
        gear.equip(&r, &mut bag, 0, "hand").unwrap();
        assert!(gear.unequip(&r, &mut bag, "hand").is_ok());
        assert_eq!(bag.count(&ItemId::from("sword")), 1);
        // Full bag blocks unequip without losing the item.
        gear.equip(&r, &mut bag, 0, "hand").unwrap();
        bag.insert(&r, ItemStack::new("helm", 1));
        assert_eq!(gear.unequip(&r, &mut bag, "hand"), Err(EquipError::NoRoom));
        assert_eq!(
            gear.get("hand").unwrap().stack.as_ref().unwrap().def,
            ItemId::from("sword")
        );
    }
}
