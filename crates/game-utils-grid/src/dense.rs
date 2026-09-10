//! Fixed-size dense tile layer.

use serde::{Deserialize, Serialize};

use crate::pos::GridPos;

/// Row-major `w` x `h` grid. Indexing is bounds-checked via `Option`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct DenseGrid<T: Clone> {
    pub w: u32,
    pub h: u32,
    cells: Vec<T>,
}

impl<T: Clone> DenseGrid<T> {
    pub fn new(w: u32, h: u32, fill: T) -> Self {
        Self {
            w,
            h,
            cells: vec![fill; (w * h) as usize],
        }
    }

    pub fn from_fn(w: u32, h: u32, mut f: impl FnMut(GridPos) -> T) -> Self {
        let mut cells = Vec::with_capacity((w * h) as usize);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                cells.push(f(GridPos::new(x, y)));
            }
        }
        Self { w, h, cells }
    }

    pub fn in_bounds(&self, p: GridPos) -> bool {
        p.x >= 0 && p.y >= 0 && (p.x as u32) < self.w && (p.y as u32) < self.h
    }

    fn idx(&self, p: GridPos) -> Option<usize> {
        self.in_bounds(p)
            .then(|| (p.y as u32 * self.w + p.x as u32) as usize)
    }

    pub fn get(&self, p: GridPos) -> Option<&T> {
        self.idx(p).map(|i| &self.cells[i])
    }

    pub fn get_mut(&mut self, p: GridPos) -> Option<&mut T> {
        self.idx(p).map(|i| &mut self.cells[i])
    }

    /// Returns false when out of bounds (no-op).
    pub fn set(&mut self, p: GridPos, v: T) -> bool {
        match self.idx(p) {
            Some(i) => {
                self.cells[i] = v;
                true
            }
            None => false,
        }
    }

    pub fn fill(&mut self, v: T) {
        self.cells.fill(v);
    }

    pub fn pos_of(&self, i: usize) -> Option<GridPos> {
        if self.w == 0 {
            return None;
        }
        (i < self.cells.len())
            .then(|| GridPos::new(i as i32 % self.w as i32, i as i32 / self.w as i32))
    }

    pub fn iter(&self) -> impl Iterator<Item = (GridPos, &T)> {
        let w = self.w.max(1) as i32;
        self.cells
            .iter()
            .enumerate()
            .map(move |(i, v)| (GridPos::new(i as i32 % w, i as i32 / w), v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (GridPos, &mut T)> {
        let w = self.w.max(1) as i32;
        self.cells
            .iter_mut()
            .enumerate()
            .map(move |(i, v)| (GridPos::new(i as i32 % w, i as i32 / w), v))
    }

    /// In-bounds 4-neighbors with values.
    pub fn surrounding4(&self, p: GridPos) -> Vec<(GridPos, &T)> {
        p.neighbors4()
            .into_iter()
            .filter_map(|n| self.get(n).map(|v| (n, v)))
            .collect()
    }

    /// In-bounds 8-neighbors with values.
    pub fn surrounding8(&self, p: GridPos) -> Vec<(GridPos, &T)> {
        p.neighbors8()
            .into_iter()
            .filter_map(|n| self.get(n).map(|v| (n, v)))
            .collect()
    }

    pub fn count(&self, mut pred: impl FnMut(&T) -> bool) -> usize {
        self.cells.iter().filter(|v| pred(v)).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_oob() {
        let mut g = DenseGrid::new(3, 2, 0u8);
        assert!(g.set(GridPos::new(2, 1), 9));
        assert_eq!(g.get(GridPos::new(2, 1)), Some(&9));
        assert!(!g.set(GridPos::new(3, 0), 1));
        assert_eq!(g.get(GridPos::new(-1, 0)), None);
    }

    #[test]
    fn from_fn_and_iter_roundtrip() {
        let g = DenseGrid::from_fn(2, 2, |p| p.x + p.y * 10);
        assert_eq!(g.iter().count(), 4);
        for (p, v) in g.iter() {
            assert_eq!(*v, p.x + p.y * 10);
        }
    }

    #[test]
    fn surrounding_clipped_at_edge() {
        let g = DenseGrid::new(2, 2, 1u8);
        assert_eq!(g.surrounding4(GridPos::ZERO).len(), 2);
        assert_eq!(g.surrounding8(GridPos::ZERO).len(), 3);
        assert_eq!(g.surrounding8(GridPos::new(5, 5)).len(), 0);
    }
}
