//! Compact boolean layer (solidity, fog, occupancy).

use serde::{Deserialize, Serialize};

use crate::pos::GridPos;

/// Bit-packed `w` x `h` flags. Backs collision layers and shaped occupancy.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BitGrid {
    pub w: u32,
    pub h: u32,
    bits: Vec<u64>,
}

impl BitGrid {
    pub fn new(w: u32, h: u32) -> Self {
        let n = (w as usize * h as usize).div_ceil(64);
        Self {
            w,
            h,
            bits: vec![0; n],
        }
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h
    }

    fn bit(&self, x: i32, y: i32) -> Option<usize> {
        self.in_bounds(x, y)
            .then_some(y as usize * self.w as usize + x as usize)
    }

    pub fn get(&self, x: i32, y: i32) -> bool {
        match self.bit(x, y) {
            Some(i) => self.bits[i / 64] >> (i % 64) & 1 == 1,
            None => false,
        }
    }

    pub fn get_pos(&self, p: GridPos) -> bool {
        self.get(p.x, p.y)
    }

    /// No-op when out of bounds; returns false then.
    pub fn set(&mut self, x: i32, y: i32, v: bool) -> bool {
        match self.bit(x, y) {
            Some(i) => {
                if v {
                    self.bits[i / 64] |= 1 << (i % 64);
                } else {
                    self.bits[i / 64] &= !(1 << (i % 64));
                }
                true
            }
            None => false,
        }
    }

    pub fn set_pos(&mut self, p: GridPos, v: bool) -> bool {
        self.set(p.x, p.y, v)
    }

    pub fn clear(&mut self) {
        self.bits.fill(0);
    }

    pub fn count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// True when every cell of the `w` x `h` rect at (x, y) is clear.
    /// Rects hanging off the edge count as blocked.
    pub fn region_free(&self, x: i32, y: i32, w: u32, h: u32) -> bool {
        if x < 0 || y < 0 || (x as u32) + w > self.w || (y as u32) + h > self.h {
            return false;
        }
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                if self.get(x + dx, y + dy) {
                    return false;
                }
            }
        }
        true
    }

    pub fn set_region(&mut self, x: i32, y: i32, w: u32, h: u32, v: bool) -> bool {
        if x < 0 || y < 0 || (x as u32) + w > self.w || (y as u32) + h > self.h {
            return false;
        }
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                self.set(x + dx, y + dy, v);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_oob_false() {
        let mut g = BitGrid::new(4, 4);
        assert!(g.set(1, 1, true));
        assert!(g.get(1, 1));
        assert!(!g.get(9, 9));
        assert!(!g.set(9, 9, true));
    }

    #[test]
    fn region_ops() {
        let mut g = BitGrid::new(4, 4);
        assert!(g.region_free(0, 0, 2, 2));
        assert!(g.set_region(0, 0, 2, 2, true));
        assert!(!g.region_free(1, 1, 2, 2));
        assert!(!g.region_free(3, 3, 2, 2));
        assert!(g.set_region(0, 0, 2, 2, false));
        assert!(g.region_free(0, 0, 4, 4));
    }

    #[test]
    fn popcount() {
        let mut g = BitGrid::new(10, 10);
        assert_eq!(g.count(), 0);
        g.set_region(0, 0, 3, 3, true);
        assert_eq!(g.count(), 9);
        g.set(0, 0, false);
        assert_eq!(g.count(), 8);
    }
}
