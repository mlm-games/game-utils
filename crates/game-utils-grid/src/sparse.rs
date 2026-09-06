//! Unbounded sparse cell storage.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::pos::GridPos;

/// HashMap-backed grid for unbounded worlds and overlays.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct SparseGrid<T> {
    cells: HashMap<GridPos, T>,
}

impl<T> SparseGrid<T> {
    pub fn new() -> Self {
        Self {
            cells: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn get(&self, p: GridPos) -> Option<&T> {
        self.cells.get(&p)
    }

    pub fn contains(&self, p: GridPos) -> bool {
        self.cells.contains_key(&p)
    }

    pub fn set(&mut self, p: GridPos, v: T) -> Option<T> {
        self.cells.insert(p, v)
    }

    pub fn remove(&mut self, p: GridPos) -> Option<T> {
        self.cells.remove(&p)
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&GridPos, &T)> {
        self.cells.iter()
    }

    /// (min, max) corners, or None when empty.
    pub fn bounds(&self) -> Option<(GridPos, GridPos)> {
        let mut it = self.cells.keys();
        let first = *it.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in it {
            lo.x = lo.x.min(p.x);
            lo.y = lo.y.min(p.y);
            hi.x = hi.x.max(p.x);
            hi.y = hi.y.max(p.y);
        }
        Some((lo, hi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_remove_bounds() {
        let mut g = SparseGrid::new();
        assert!(g.is_empty());
        g.set(GridPos::new(-5, 7), "a");
        g.set(GridPos::new(3, -2), "b");
        assert_eq!(g.len(), 2);
        assert_eq!(g.bounds(), Some((GridPos::new(-5, -2), GridPos::new(3, 7))));
        assert_eq!(g.remove(GridPos::new(-5, 7)), Some("a"));
        assert!(!g.contains(GridPos::new(-5, 7)));
    }

    #[test]
    fn empty_bounds_none() {
        let g: SparseGrid<u8> = SparseGrid::new();
        assert_eq!(g.bounds(), None);
    }
}
