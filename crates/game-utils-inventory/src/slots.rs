//! Fixed-size slot container: stack-first insert, swap, split, transfer.

use serde::{Deserialize, Serialize};

use crate::events::InventoryEvent;
use crate::item::{ItemId, ItemRegistry, ItemStack};

/// Slot bag. `None` slots are empty; events journal mutations.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SlotInventory {
    slots: Vec<Option<ItemStack>>,
    #[serde(skip, default = "_events")]
    events: Vec<InventoryEvent>,
}

fn _events() -> Vec<InventoryEvent> {
    Vec::new()
}

impl SlotInventory {
    pub fn new(size: usize) -> Self {
        Self {
            slots: vec![None; size],
            events: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn get(&self, idx: usize) -> Option<&ItemStack> {
        self.slots.get(idx).and_then(Option::as_ref)
    }

    /// Drain the change journal (UI/save polling).
    pub fn drain_events(&mut self) -> Vec<InventoryEvent> {
        core::mem::take(&mut self.events)
    }

    /// Append a domain event (crafting, trade, quest rewards).
    pub fn record(&mut self, ev: InventoryEvent) {
        self.events.push(ev);
    }

    /// Total units of `id` across slots.
    pub fn count(&self, id: &ItemId) -> u32 {
        self.slots
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|s| &s.def == id)
            .map(|s| s.qty)
            .sum()
    }

    /// Free room for `id` (open stack tops + empty slots).
    pub fn count_free(&self, reg: &ItemRegistry, id: &ItemId) -> u32 {
        self.free_for(reg, id, None)
    }

    /// Free room for an incoming stack (condition-aware merging).
    pub fn free_for(
        &self,
        reg: &ItemRegistry,
        id: &ItemId,
        cond: Option<crate::item::Condition>,
    ) -> u32 {
        let Some(def) = reg.def(id) else { return 0 };
        let lim = def.stack_limit();
        self.slots
            .iter()
            .map(|s| match s {
                None => lim,
                Some(st) if st.def == *id && st.condition == cond => lim.saturating_sub(st.qty),
                _ => 0,
            })
            .sum()
    }

    /// Insert, stacking first then filling empties. Returns leftover qty.
    pub fn insert(&mut self, reg: &ItemRegistry, mut stack: ItemStack) -> u32 {
        let Some(def) = reg.def(&stack.def) else {
            return stack.qty;
        };
        let lim = def.stack_limit();
        // Top up mergeable stacks.
        for slot in self.slots.iter_mut().filter_map(|s| s.as_mut()) {
            if stack.qty == 0 {
                break;
            }
            if slot.def == stack.def && slot.condition == stack.condition && slot.qty < lim {
                let room = lim - slot.qty;
                let take = room.min(stack.qty);
                slot.qty += take;
                stack.qty -= take;
                self.events.push(InventoryEvent::Merged {
                    id: slot.def.clone(),
                    qty: take,
                });
            }
        }
        // Fill empties.
        for slot in self.slots.iter_mut() {
            if stack.qty == 0 {
                break;
            }
            if slot.is_none() {
                let take = lim.min(stack.qty);
                let id = stack.def.clone();
                *slot = Some(ItemStack {
                    def: id.clone(),
                    qty: take,
                    condition: stack.condition,
                });
                stack.qty -= take;
                self.events.push(InventoryEvent::Inserted { id, qty: take });
            }
        }
        stack.qty
    }

    /// Insert into one slot: merges when compatible, occupies when empty.
    /// Returns leftover that did not fit (the input stack on mismatch).
    pub fn insert_at(
        &mut self,
        reg: &ItemRegistry,
        idx: usize,
        mut stack: ItemStack,
    ) -> Option<ItemStack> {
        let slot = self.slots.get_mut(idx)?;
        let lim = reg.def(&stack.def)?.stack_limit();
        match slot {
            None => {
                let take = lim.min(stack.qty);
                let id = stack.def.clone();
                *slot = Some(ItemStack {
                    def: id.clone(),
                    qty: take,
                    condition: stack.condition,
                });
                stack.qty -= take;
                self.events.push(InventoryEvent::Inserted { id, qty: take });
                (stack.qty > 0).then_some(stack)
            }
            Some(cur) if cur.def == stack.def && cur.condition == stack.condition => {
                let room = lim.saturating_sub(cur.qty);
                let take = room.min(stack.qty);
                cur.qty += take;
                stack.qty -= take;
                self.events.push(InventoryEvent::Merged {
                    id: cur.def.clone(),
                    qty: take,
                });
                (stack.qty > 0).then_some(stack)
            }
            _ => Some(stack),
        }
    }

    /// Remove up to `qty` from a slot.
    pub fn remove(&mut self, idx: usize, qty: u32) -> Option<ItemStack> {
        let cur = self.slots.get_mut(idx)?.as_mut()?;
        let take = qty.min(cur.qty);
        cur.qty -= take;
        let out = ItemStack {
            def: cur.def.clone(),
            qty: take,
            condition: cur.condition,
        };
        if cur.qty == 0 {
            *self.slots.get_mut(idx).unwrap() = None;
        }
        self.events.push(InventoryEvent::Removed {
            id: out.def.clone(),
            qty: take,
        });
        Some(out)
    }

    /// Remove up to `qty` units of `id` across slots. Returns removed.
    pub fn remove_all(&mut self, id: &ItemId, mut qty: u32) -> u32 {
        let mut removed = 0;
        for slot in self.slots.iter_mut() {
            if qty == 0 {
                break;
            }
            if let Some(cur) = slot.as_mut().filter(|s| &s.def == id) {
                let take = qty.min(cur.qty);
                cur.qty -= take;
                qty -= take;
                removed += take;
                if cur.qty == 0 {
                    *slot = None;
                }
            }
        }
        if removed > 0 {
            self.events.push(InventoryEvent::Removed {
                id: id.clone(),
                qty: removed,
            });
        }
        removed
    }

    pub fn swap(&mut self, a: usize, b: usize) -> bool {
        if a >= self.slots.len() || b >= self.slots.len() || a == b {
            return false;
        }
        self.slots.swap(a, b);
        self.events.push(InventoryEvent::Swapped { a, b });
        true
    }

    /// Split `qty` off slot `idx` into an empty slot. Returns the new idx.
    pub fn split(&mut self, idx: usize, qty: u32) -> Option<usize> {
        let cur = self.slots.get(idx)?.as_ref()?;
        if qty == 0 || qty >= cur.qty {
            return None;
        }
        let dst = self.slots.iter().position(Option::is_none)?;
        let src = self.slots[idx].as_mut().unwrap();
        src.qty -= qty;
        let id = src.def.clone();
        self.slots[dst] = Some(ItemStack {
            def: id.clone(),
            qty,
            condition: src.condition,
        });
        self.events.push(InventoryEvent::Split { id, qty });
        Some(dst)
    }

    /// Move up to `qty` from `self[idx]` into `dst`. Returns moved qty.
    /// Leftovers stay in the source slot.
    pub fn transfer(&mut self, reg: &ItemRegistry, idx: usize, dst: &mut Self, qty: u32) -> u32 {
        let Some(taken) = self.remove(idx, qty) else {
            return 0;
        };
        let (id, cond, before) = (taken.def.clone(), taken.condition, taken.qty);
        let leftover = dst.insert(reg, taken);
        let moved = before - leftover;
        if leftover > 0 {
            // Push back what did not fit (append path always has room:
            // the source slot just freed `before` units).
            let back = ItemStack {
                def: id,
                qty: leftover,
                condition: cond,
            };
            let _ = self.insert(reg, back);
        }
        moved
    }

    /// Merge stacks and group by id, preserving first-seen order.
    /// Returns stacks that no longer fit (e.g. the stack limit was
    /// lowered or more units were deserialized than slots hold) instead
    /// of silently deleting them; the caller decides (drop, stash, mail).
    pub fn compact(&mut self, reg: &ItemRegistry) -> Vec<ItemStack> {
        let mut order: Vec<ItemId> = Vec::new();
        let mut totals: Vec<(ItemId, Option<crate::item::Condition>, u32)> = Vec::new();
        for slot in self.slots.iter_mut() {
            if let Some(st) = slot.take() {
                if !order.contains(&st.def) {
                    order.push(st.def.clone());
                }
                match totals
                    .iter_mut()
                    .find(|(id, c, _)| *id == st.def && *c == st.condition)
                {
                    Some(e) => e.2 = e.2.saturating_add(st.qty),
                    None => totals.push((st.def, st.condition, st.qty)),
                }
            }
        }
        totals.sort_by_key(|(id, _, _)| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
        let mut i = 0;
        let mut overflow = Vec::new();
        for (id, cond, mut qty) in totals {
            let lim = reg.def(&id).map(|d| d.stack_limit()).unwrap_or(u32::MAX);
            let lim = lim.max(1);
            while qty > 0 && i < self.slots.len() {
                let take = lim.min(qty);
                self.slots[i] = Some(ItemStack {
                    def: id.clone(),
                    qty: take,
                    condition: cond,
                });
                qty -= take;
                i += 1;
            }
            if qty > 0 {
                overflow.push(ItemStack {
                    def: id,
                    qty,
                    condition: cond,
                });
            }
        }
        overflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemDef;

    fn reg() -> ItemRegistry {
        let mut r = ItemRegistry::new();
        let mk = |id: &str, stackable: bool, max: u32| ItemDef {
            id: ItemId::from(id),
            name: id.into(),
            description: String::new(),
            stackable,
            max_stack: max,
            weight: 2.0,
            value: 1,
            cells_w: 1,
            cells_h: 1,
            tags: vec![],
        };
        r.register(mk("herb", true, 7));
        r.register(mk("sword", false, 1));
        r
    }

    #[test]
    fn insert_stacks_then_spills() {
        let r = reg();
        let mut inv = SlotInventory::new(2);
        assert_eq!(inv.insert(&r, ItemStack::new("herb", 10)), 0);
        assert_eq!(inv.get(0).unwrap().qty, 7);
        assert_eq!(inv.get(1).unwrap().qty, 3);
        assert_eq!(inv.insert(&r, ItemStack::new("herb", 5)), 1);
    }

    #[test]
    fn non_stackable_one_per_slot() {
        let r = reg();
        let mut inv = SlotInventory::new(2);
        assert_eq!(inv.insert(&r, ItemStack::new("sword", 3)), 1);
        assert_eq!(inv.count(&ItemId::from("sword")), 2);
    }

    #[test]
    fn remove_split_swap() {
        let r = reg();
        let mut inv = SlotInventory::new(3);
        inv.insert(&r, ItemStack::new("herb", 7));
        let dst = inv.split(0, 3).unwrap();
        assert_eq!(inv.get(0).unwrap().qty, 4);
        assert_eq!(inv.get(dst).unwrap().qty, 3);
        assert!(inv.swap(0, dst));
        assert_eq!(inv.remove(0, 2).unwrap().qty, 2);
    }

    #[test]
    fn transfer_between_containers() {
        let r = reg();
        let mut a = SlotInventory::new(2);
        let mut b = SlotInventory::new(1);
        a.insert(&r, ItemStack::new("herb", 7));
        assert_eq!(a.transfer(&r, 0, &mut b, 7), 7);
        assert_eq!(b.count(&ItemId::from("herb")), 7);
        assert!(a.is_empty());
    }

    #[test]
    fn compact_groups_and_merges() {
        let r = reg();
        let mut inv = SlotInventory::new(4);
        inv.insert(&r, ItemStack::new("herb", 3));
        inv.insert(&r, ItemStack::new("sword", 1));
        inv.swap(1, 3);
        inv.compact(&r);
        assert_eq!(inv.get(0).unwrap().def, ItemId::from("herb"));
        assert_eq!(inv.get(1).unwrap().def, ItemId::from("sword"));
    }

    #[test]
    fn compact_returns_overflow_instead_of_deleting() {
        let r = reg();
        let mut inv = SlotInventory::new(1);
        inv.slots[0] = Some(ItemStack {
            def: ItemId::from("herb"),
            qty: 14,
            condition: None,
        });
        let overflow = inv.compact(&r);
        assert_eq!(inv.count(&ItemId::from("herb")), 7);
        assert_eq!(overflow.len(), 1);
        assert_eq!(overflow[0].qty, 7);
    }

    #[test]
    fn events_journaled() {
        let r = reg();
        let mut inv = SlotInventory::new(2);
        inv.insert(&r, ItemStack::new("herb", 2));
        let ev = inv.drain_events();
        assert_eq!(ev.len(), 1);
        assert!(inv.drain_events().is_empty());
    }
}
