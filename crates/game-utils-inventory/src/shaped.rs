//! Tetris-style shaped container: rect footprints with rotation.

use game_utils_grid::BitGrid;
use serde::{Deserialize, Serialize};

use crate::events::InventoryEvent;
use crate::item::{ItemId, ItemRegistry, ItemStack};

/// Placement failure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlaceError {
    UnknownItem,
    OutOfBounds,
    Blocked,
}

/// One placed stack: cell origin + clockwise quarter-turns.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PlacedItem {
    pub stack: ItemStack,
    pub x: u8,
    pub y: u8,
    pub rot: u8,
}

/// Fixed `w` x `h` cell bag. Stacks occupy footprints, no stacking.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ShapedInventory {
    pub w: u8,
    pub h: u8,
    items: Vec<PlacedItem>,
    #[serde(skip, default = "_occ")]
    occ: BitGrid,
    #[serde(skip, default = "_events")]
    events: Vec<InventoryEvent>,
}

fn _occ() -> BitGrid {
    BitGrid::new(0, 0)
}

fn _events() -> Vec<InventoryEvent> {
    Vec::new()
}

/// Footprint dims after rotation (odd turns swap w/h).
pub fn footprint(cells_w: u8, cells_h: u8, rot: u8) -> (u32, u32) {
    if rot.is_multiple_of(2) {
        (cells_w as u32, cells_h as u32)
    } else {
        (cells_h as u32, cells_w as u32)
    }
}

impl ShapedInventory {
    pub fn new(w: u8, h: u8) -> Self {
        Self {
            w,
            h,
            items: Vec::new(),
            occ: BitGrid::new(w as u32, h as u32),
            events: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<&PlacedItem> {
        self.items.get(idx)
    }

    pub fn drain_events(&mut self) -> Vec<InventoryEvent> {
        core::mem::take(&mut self.events)
    }

    fn rebuild(&mut self, reg: &ItemRegistry) {
        self.occ = BitGrid::new(self.w as u32, self.h as u32);
        for it in &self.items {
            if let Some(def) = reg.def(&it.stack.def) {
                let (fw, fh) = footprint(def.cells_w, def.cells_h, it.rot);
                self.occ.set_region(it.x as i32, it.y as i32, fw, fh, true);
            }
        }
    }

    /// Repair a stale occupancy grid. `occ` is `#[serde(skip)]` (footprints
    /// need the registry to rebuild), so a deserialized bag starts with an
    /// empty grid that would let `place` overlap everything. Any writer
    /// path calls this first.
    fn ensure_occ(&mut self, reg: &ItemRegistry) {
        if self.occ.w != self.w as u32 || self.occ.h != self.h as u32 {
            self.rebuild(reg);
            return;
        }
        let stale = if self.items.is_empty() {
            self.occ.count() != 0
        } else if self.occ.count() == 0 {
            true
        } else {
            self.items.iter().any(|it| {
                reg.def(&it.stack.def).is_some_and(|def| {
                    let (fw, fh) = footprint(def.cells_w, def.cells_h, it.rot);
                    (0..fh).any(|dy| {
                        (0..fw).any(|dx| {
                            !self
                                .occ
                                .get(it.x as i32 + dx as i32, it.y as i32 + dy as i32)
                        })
                    })
                })
            })
        };
        if stale {
            self.rebuild(reg);
        }
    }

    /// Place at an explicit cell + rotation.
    pub fn place(
        &mut self,
        reg: &ItemRegistry,
        stack: ItemStack,
        x: u8,
        y: u8,
        rot: u8,
    ) -> Result<usize, PlaceError> {
        self.ensure_occ(reg);
        let def = reg.def(&stack.def).ok_or(PlaceError::UnknownItem)?;
        let (fw, fh) = footprint(def.cells_w, def.cells_h, rot % 4);
        if x as u32 + fw > self.w as u32 || y as u32 + fh > self.h as u32 {
            return Err(PlaceError::OutOfBounds);
        }
        if !self.occ.region_free(x as i32, y as i32, fw, fh) {
            return Err(PlaceError::Blocked);
        }
        let id = stack.def.clone();
        let qty = stack.qty;
        self.occ.set_region(x as i32, y as i32, fw, fh, true);
        self.items.push(PlacedItem {
            stack,
            x,
            y,
            rot: rot % 4,
        });
        self.events.push(InventoryEvent::Inserted { id, qty });
        Ok(self.items.len() - 1)
    }

    /// First-fit auto-place across rotations. Returns (idx, x, y, rot).
    pub fn insert(
        &mut self,
        reg: &ItemRegistry,
        stack: ItemStack,
    ) -> Result<(usize, u8, u8, u8), PlaceError> {
        self.ensure_occ(reg);
        let spot = self.find_space(reg, &stack.def).ok_or_else(|| {
            if reg.def(&stack.def).is_none() {
                PlaceError::UnknownItem
            } else {
                PlaceError::Blocked
            }
        })?;
        let idx = self.place(reg, stack, spot.0, spot.1, spot.2)?;
        Ok((idx, spot.0, spot.1, spot.2))
    }

    pub fn find_space(&self, reg: &ItemRegistry, id: &ItemId) -> Option<(u8, u8, u8)> {
        let def = reg.def(id)?;
        for rot in 0..4u8 {
            let (fw, fh) = footprint(def.cells_w, def.cells_h, rot);
            for y in 0..=(self.h as u32).saturating_sub(fh) {
                for x in 0..=(self.w as u32).saturating_sub(fw) {
                    if self.occ.region_free(x as i32, y as i32, fw, fh) {
                        return Some((x as u8, y as u8, rot));
                    }
                }
            }
        }
        None
    }

    pub fn remove(&mut self, reg: &ItemRegistry, idx: usize) -> Option<ItemStack> {
        if idx >= self.items.len() {
            return None;
        }
        let it = self.items.remove(idx);
        self.rebuild(reg);
        self.events.push(InventoryEvent::Removed {
            id: it.stack.def.clone(),
            qty: it.stack.qty,
        });
        Some(it.stack)
    }

    /// Move to a new cell/rotation. Restores the old spot on failure.
    pub fn move_item(
        &mut self,
        reg: &ItemRegistry,
        idx: usize,
        x: u8,
        y: u8,
        rot: u8,
    ) -> Result<(), PlaceError> {
        let Some(it) = self.items.get(idx).cloned() else {
            return Err(PlaceError::Blocked);
        };
        self.ensure_occ(reg);
        self.items.remove(idx);
        self.rebuild(reg);
        match self.place(reg, it.stack.clone(), x, y, rot) {
            Ok(_) => {
                // `place` pushed at the end; keep original order stable-ish.
                let last = self.items.pop().unwrap();
                self.items.insert(idx.min(self.items.len()), last);
                self.events.push(InventoryEvent::Moved {
                    id: it.stack.def,
                    from: idx,
                    to: idx,
                });
                Ok(())
            }
            Err(e) => {
                self.items.insert(idx.min(self.items.len()), it);
                self.rebuild(reg);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemDef;

    fn reg() -> ItemRegistry {
        let mut r = ItemRegistry::new();
        let mk = |id: &str, w: u8, h: u8| ItemDef {
            id: ItemId::from(id),
            name: id.into(),
            description: String::new(),
            stackable: false,
            max_stack: 1,
            weight: 1.0,
            value: 1,
            cells_w: w,
            cells_h: h,
            tags: vec![],
        };
        r.register(mk("potion", 1, 1));
        r.register(mk("sword", 1, 3));
        r
    }

    #[test]
    fn place_and_overlap_rejected() {
        let r = reg();
        let mut inv = ShapedInventory::new(4, 4);
        inv.place(&r, ItemStack::new("sword", 1), 0, 0, 0).unwrap();
        assert_eq!(
            inv.place(&r, ItemStack::new("potion", 1), 0, 0, 0),
            Err(PlaceError::Blocked)
        );
        assert!(inv.place(&r, ItemStack::new("potion", 1), 1, 0, 0).is_ok());
    }

    #[test]
    fn rotation_fits_narrow_gap() {
        let r = reg();
        let mut inv = ShapedInventory::new(3, 1);
        // 1x3 sword only fits rotated.
        assert_eq!(
            inv.place(&r, ItemStack::new("sword", 1), 0, 0, 0),
            Err(PlaceError::OutOfBounds)
        );
        assert!(inv.place(&r, ItemStack::new("sword", 1), 0, 0, 1).is_ok());
    }

    #[test]
    fn auto_insert_full() {
        let r = reg();
        let mut inv = ShapedInventory::new(1, 1);
        inv.insert(&r, ItemStack::new("potion", 1)).unwrap();
        assert_eq!(
            inv.insert(&r, ItemStack::new("potion", 1)).unwrap_err(),
            PlaceError::Blocked
        );
    }

    #[test]
    fn move_restores_on_failure() {
        let r = reg();
        let mut inv = ShapedInventory::new(3, 3);
        inv.place(&r, ItemStack::new("sword", 1), 0, 0, 0).unwrap();
        inv.place(&r, ItemStack::new("potion", 1), 2, 2, 0).unwrap();
        assert!(inv.move_item(&r, 1, 0, 0, 0).is_err());
        assert_eq!(inv.len(), 2);
        assert!(inv.move_item(&r, 1, 2, 0, 0).is_ok());
    }

    #[test]
    fn occupancy_survives_serde_round_trip() {
        let r = reg();
        let mut inv = ShapedInventory::new(4, 4);
        inv.place(&r, ItemStack::new("sword", 1), 0, 0, 0).unwrap();
        let json = serde_json::to_string(&inv).unwrap();
        let mut loaded: ShapedInventory = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.place(&r, ItemStack::new("potion", 1), 0, 0, 0),
            Err(PlaceError::Blocked)
        );
        assert!(
            loaded
                .place(&r, ItemStack::new("potion", 1), 1, 0, 0)
                .is_ok()
        );
    }
}
