//! Integer cell coordinates (2D + minimal 3D).

use serde::{Deserialize, Serialize};

/// 4-neighborhood offsets (N, E, S, W).
pub const DIRS4: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
/// 8-neighborhood offsets (cardinals first, then diagonals).
pub const DIRS8: [(i32, i32); 8] = [
    (0, -1),
    (1, 0),
    (0, 1),
    (-1, 0),
    (1, -1),
    (1, 1),
    (-1, 1),
    (-1, -1),
];

/// 2D cell coordinate. `Ord` compares x first, then y.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
}

impl GridPos {
    pub const ZERO: Self = Self { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn neighbors4(self) -> [Self; 4] {
        DIRS4.map(|(dx, dy)| Self::new(self.x + dx, self.y + dy))
    }

    pub fn neighbors8(self) -> [Self; 8] {
        DIRS8.map(|(dx, dy)| Self::new(self.x + dx, self.y + dy))
    }

    pub fn manhattan(self, o: Self) -> u32 {
        self.x.abs_diff(o.x) + self.y.abs_diff(o.y)
    }

    pub fn chebyshev(self, o: Self) -> u32 {
        self.x.abs_diff(o.x).max(self.y.abs_diff(o.y))
    }

    pub fn euclid(self, o: Self) -> f32 {
        let dx = (self.x - o.x) as f32;
        let dy = (self.y - o.y) as f32;
        dx.hypot(dy)
    }

    /// One king-move toward `o` (or self when already there).
    pub fn step_toward(self, o: Self) -> Self {
        Self::new(
            self.x + (o.x - self.x).signum(),
            self.y + (o.y - self.y).signum(),
        )
    }

    /// Rotate clockwise by `turns` quarter-turns (for shaped footprints).
    pub fn rotated(self, turns: u8) -> Self {
        match turns % 4 {
            0 => self,
            1 => Self::new(-self.y, self.x),
            2 => Self::new(-self.x, -self.y),
            _ => Self::new(self.y, -self.x),
        }
    }

    /// Bresenham line, endpoints inclusive. For LOS checks.
    pub fn line_to(self, o: Self) -> Vec<Self> {
        let mut cells = Vec::new();
        let (mut x, mut y) = (self.x, self.y);
        let dx = (o.x - x).abs();
        let dy = -(o.y - y).abs();
        let sx = (o.x - x).signum();
        let sy = (o.y - y).signum();
        let mut err = dx + dy;
        loop {
            cells.push(Self::new(x, y));
            if x == o.x && y == o.y {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
        cells
    }
}

impl core::ops::Add for GridPos {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}

impl core::ops::Sub for GridPos {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}

/// 3D cell coordinate (voxel layers, stacked tilemaps).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct GridPos3 {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl GridPos3 {
    pub const ZERO: Self = Self { x: 0, y: 0, z: 0 };

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn neighbors6(self) -> [Self; 6] {
        [
            Self::new(self.x + 1, self.y, self.z),
            Self::new(self.x - 1, self.y, self.z),
            Self::new(self.x, self.y + 1, self.z),
            Self::new(self.x, self.y - 1, self.z),
            Self::new(self.x, self.y, self.z + 1),
            Self::new(self.x, self.y, self.z - 1),
        ]
    }

    pub fn manhattan(self, o: Self) -> u32 {
        self.x.abs_diff(o.x) + self.y.abs_diff(o.y) + self.z.abs_diff(o.z)
    }

    pub fn xy(self) -> GridPos {
        GridPos::new(self.x, self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighborhoods() {
        assert_eq!(GridPos::ZERO.neighbors4().len(), 4);
        assert_eq!(GridPos::ZERO.neighbors8().len(), 8);
        assert!(GridPos::ZERO.neighbors4().contains(&GridPos::new(1, 0)));
    }

    #[test]
    fn distances() {
        let (a, b) = (GridPos::new(0, 0), GridPos::new(3, 4));
        assert_eq!(a.manhattan(b), 7);
        assert_eq!(a.chebyshev(b), 4);
        assert!((a.euclid(b) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn rotation_cycles() {
        let p = GridPos::new(2, 1);
        assert_eq!(p.rotated(4), p);
        assert_eq!(p.rotated(1).rotated(3), p);
        assert_eq!(p.rotated(1), GridPos::new(-1, 2));
    }

    #[test]
    fn bresenham_endpoints_and_length() {
        let line = GridPos::new(0, 0).line_to(GridPos::new(3, 0));
        assert_eq!(line.len(), 4);
        assert_eq!(*line.last().unwrap(), GridPos::new(3, 0));
        let diag = GridPos::new(0, 0).line_to(GridPos::new(2, 2));
        assert_eq!(diag.len(), 3);
        assert_eq!(
            diag,
            vec![GridPos::new(0, 0), GridPos::new(1, 1), GridPos::new(2, 2)]
        );
    }

    #[test]
    fn step_toward_moves_one() {
        assert_eq!(
            GridPos::new(0, 0).step_toward(GridPos::new(5, -3)),
            GridPos::new(1, -1)
        );
        assert_eq!(GridPos::ZERO.step_toward(GridPos::ZERO), GridPos::ZERO);
    }
}
