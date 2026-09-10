//! Seeded room placement, cellular automata, and drunkard-walk tunnels.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::dense::DenseGrid;
use crate::pos::GridPos;

/// Integer rect: top-left cell + size in cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rect {
    pub pos: GridPos,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            pos: GridPos::new(x, y),
            w,
            h,
        }
    }

    pub fn center(&self) -> GridPos {
        GridPos::new(self.pos.x + self.w / 2, self.pos.y + self.h / 2)
    }

    /// Overlap test with an extra margin ring.
    pub fn overlaps(&self, o: &Rect, margin: i32) -> bool {
        self.pos.x - margin < o.pos.x + o.w
            && self.pos.x + self.w + margin > o.pos.x
            && self.pos.y - margin < o.pos.y + o.h
            && self.pos.y + self.h + margin > o.pos.y
    }

    pub fn cells(&self) -> Vec<GridPos> {
        let mut out = Vec::with_capacity((self.w * self.h).max(0) as usize);
        for y in self.pos.y..self.pos.y + self.h {
            for x in self.pos.x..self.pos.x + self.w {
                out.push(GridPos::new(x, y));
            }
        }
        out
    }
}

/// Room placement parameters.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RoomSpec {
    pub area_w: u32,
    pub area_h: u32,
    pub count: usize,
    pub min_w: u32,
    pub min_h: u32,
    pub max_w: u32,
    pub max_h: u32,
    pub margin: i32,
    pub attempts: u32,
}

/// Random non-overlapping rooms inside the spec area.
/// Each room gets `attempts` tries before it is skipped.
pub fn place_rooms(rng: &mut impl Rng, spec: RoomSpec) -> Vec<Rect> {
    let mut rooms = Vec::new();
    for _ in 0..spec.count {
        for _ in 0..spec.attempts.max(1) {
            let w = rng.random_range(spec.min_w..=spec.max_w.max(spec.min_w)) as i32;
            let h = rng.random_range(spec.min_h..=spec.max_h.max(spec.min_h)) as i32;
            if w > spec.area_w as i32 || h > spec.area_h as i32 {
                continue;
            }
            let r = Rect::new(
                rng.random_range(0..=(spec.area_w as i32 - w)),
                rng.random_range(0..=(spec.area_h as i32 - h)),
                w,
                h,
            );
            if rooms.iter().all(|o: &Rect| !r.overlaps(o, spec.margin)) {
                rooms.push(r);
                break;
            }
        }
    }
    rooms
}

/// One cellular-automata pass. `birth`/`survive` list live-neighbor counts
/// (e.g. birth [5..=8], survive [4..=8] smooths caves).
pub fn cellular_step(
    grid: &DenseGrid<bool>,
    birth: &[u8],
    survive: &[u8],
    diagonal: bool,
) -> DenseGrid<bool> {
    DenseGrid::from_fn(grid.w, grid.h, |p| {
        let mut n = 0u8;
        for q in p.neighbors8() {
            let alive = grid.get(q).copied().unwrap_or(true);
            if alive && (diagonal || q.x == p.x || q.y == p.y) {
                n += 1;
            }
        }
        let alive = grid.get(p).copied().unwrap_or(false);
        if alive {
            survive.contains(&n)
        } else {
            birth.contains(&n)
        }
    })
}

/// Drunkard walk: carve `set_to` along `steps` random king-moves from
/// `from` (inclusive), clamped in bounds. Returns the end cell.
pub fn tunnel_walk(
    rng: &mut impl Rng,
    grid: &mut DenseGrid<bool>,
    mut from: GridPos,
    steps: usize,
    set_to: bool,
) -> GridPos {
    grid.set(from, set_to);
    for _ in 0..steps {
        let (dx, dy) = crate::pos::DIRS8[rng.random_range(0..crate::pos::DIRS8.len())];
        from = GridPos::new(
            (from.x + dx).clamp(0, grid.w as i32 - 1),
            (from.y + dy).clamp(0, grid.h as i32 - 1),
        );
        grid.set(from, set_to);
    }
    from
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    fn rng() -> SmallRng {
        SmallRng::seed_from_u64(7)
    }

    fn spec() -> RoomSpec {
        RoomSpec {
            area_w: 40,
            area_h: 40,
            count: 8,
            min_w: 3,
            min_h: 3,
            max_w: 8,
            max_h: 8,
            margin: 1,
            attempts: 20,
        }
    }

    #[test]
    fn exact_fit_room_is_placed() {
        let spec = RoomSpec {
            area_w: 10,
            area_h: 10,
            count: 2,
            min_w: 10,
            min_h: 10,
            max_w: 10,
            max_h: 10,
            margin: 0,
            attempts: 5,
        };
        let rooms = place_rooms(&mut rng(), spec);
        assert_eq!(rooms.len(), 1);
        assert_eq!((rooms[0].w, rooms[0].h), (10, 10));
    }

    #[test]
    fn rooms_fit_and_spread() {
        let rooms = place_rooms(&mut rng(), spec());
        assert!(!rooms.is_empty());
        for r in &rooms {
            assert!(r.pos.x >= 0 && r.pos.y >= 0);
            assert!(r.pos.x + r.w <= 40 && r.pos.y + r.h <= 40);
        }
        for (i, a) in rooms.iter().enumerate() {
            for b in &rooms[i + 1..] {
                assert!(!a.overlaps(b, 1));
            }
        }
    }

    #[test]
    fn rooms_deterministic() {
        let a = place_rooms(&mut rng(), spec());
        let b = place_rooms(&mut rng(), spec());
        assert_eq!(a, b);
    }

    #[test]
    fn cellular_kills_lonely() {
        let mut g = DenseGrid::new(5, 5, false);
        g.set(GridPos::new(2, 2), true);
        let next = cellular_step(&g, &[5, 6, 7, 8], &[4, 5, 6, 7, 8], true);
        assert!(!next.get(GridPos::new(2, 2)).copied().unwrap());
    }

    #[test]
    fn walk_carves_and_stays_inside() {
        let mut g = DenseGrid::new(10, 10, false);
        let end = tunnel_walk(&mut rng(), &mut g, GridPos::new(5, 5), 50, true);
        assert!(g.in_bounds(end));
        assert!(g.count(|v| *v) > 1);
    }
}
