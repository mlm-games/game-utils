//! Sparse board occupancy for cards on cells. Optional bounds;
/// placement rules stay game-side.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Board cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cell(pub i32, pub i32);

impl Cell {
    pub fn new(x: i32, y: i32) -> Self {
        Self(x, y)
    }

    /// Manhattan neighbors (orthogonal only).
    pub fn neighbors_4(self) -> [Cell; 4] {
        [
            Cell(self.0 + 1, self.1),
            Cell(self.0 - 1, self.1),
            Cell(self.0, self.1 + 1),
            Cell(self.0, self.1 - 1),
        ]
    }
}

/// Inclusive rectangular bounds, or unbounded when `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub min: Cell,
    pub max: Cell,
}

impl Bounds {
    pub fn new(min: Cell, max: Cell) -> Self {
        Self { min, max }
    }

    pub fn size(w: i32, h: i32) -> Self {
        Self {
            min: Cell(0, 0),
            max: Cell(w.max(1) - 1, h.max(1) - 1),
        }
    }

    pub fn contains(&self, cell: Cell) -> bool {
        cell.0 >= self.min.0 && cell.0 <= self.max.0 && cell.1 >= self.min.1 && cell.1 <= self.max.1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceError {
    Occupied,
    OutOfBounds,
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Occupied => write!(f, "cell is occupied"),
            Self::OutOfBounds => write!(f, "cell is out of bounds"),
        }
    }
}

impl std::error::Error for PlaceError {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Board<T> {
    bounds: Option<Bounds>,
    cells: HashMap<Cell, T>,
}

impl<T> Board<T> {
    pub fn new() -> Self {
        Self {
            bounds: None,
            cells: HashMap::new(),
        }
    }

    pub fn bounded(bounds: Bounds) -> Self {
        Self {
            bounds: Some(bounds),
            cells: HashMap::new(),
        }
    }

    pub fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn get(&self, cell: Cell) -> Option<&T> {
        self.cells.get(&cell)
    }

    pub fn is_free(&self, cell: Cell) -> bool {
        if self.bounds.is_some_and(|b| !b.contains(cell)) {
            return false;
        }
        !self.cells.contains_key(&cell)
    }

    /// Place an item. Failures return it with the reason.
    pub fn place(&mut self, cell: Cell, item: T) -> Result<(), (T, PlaceError)> {
        if self.bounds.is_some_and(|b| !b.contains(cell)) {
            return Err((item, PlaceError::OutOfBounds));
        }
        if self.cells.contains_key(&cell) {
            return Err((item, PlaceError::Occupied));
        }
        self.cells.insert(cell, item);
        Ok(())
    }

    pub fn remove(&mut self, cell: Cell) -> Option<T> {
        self.cells.remove(&cell)
    }

    /// Move an occupant. No-op unless source is full and dest is free.
    pub fn move_cell(&mut self, from: Cell, to: Cell) -> bool {
        if !self.cells.contains_key(&from) || !self.is_free(to) {
            return false;
        }
        if let Some(item) = self.cells.remove(&from) {
            self.cells.insert(to, item);
            return true;
        }
        false
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, Cell, T> {
        self.cells.iter()
    }

    /// All occupants orthogonally adjacent to `cell`.
    pub fn neighbors_of(&self, cell: Cell) -> Vec<(Cell, &T)> {
        cell.neighbors_4()
            .into_iter()
            .filter_map(|c| self.get(c).map(|t| (c, t)))
            .collect()
    }

    pub fn clear(&mut self) -> Vec<(Cell, T)> {
        std::mem::take(&mut self.cells).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_place_move_remove() {
        let mut b = Board::bounded(Bounds::size(3, 3));
        assert!(b.place(Cell(0, 0), "a").is_ok());
        assert_eq!(b.place(Cell(0, 0), "b"), Err(("b", PlaceError::Occupied)));
        assert_eq!(
            b.place(Cell(9, 9), "c"),
            Err(("c", PlaceError::OutOfBounds))
        );
        assert!(b.move_cell(Cell(0, 0), Cell(1, 1)));
        assert!(!b.move_cell(Cell(0, 0), Cell(1, 1)));
        assert_eq!(b.get(Cell(1, 1)), Some(&"a"));
        assert_eq!(b.remove(Cell(1, 1)), Some("a"));
        assert!(b.is_empty());
    }

    #[test]
    fn board_unbounded_and_neighbors() {
        let mut b = Board::new();
        b.place(Cell(-5, 100), 1).unwrap();
        b.place(Cell(-4, 100), 2).unwrap();
        let n = b.neighbors_of(Cell(-5, 100));
        assert_eq!(n.len(), 1);
        assert_eq!(b.len(), 2);
        assert_eq!(b.clear().len(), 2);
    }
}
