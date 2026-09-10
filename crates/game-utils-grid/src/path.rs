//! A*, flood fill, and distance fields over caller-owned solidity.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use serde::{Deserialize, Serialize};

use crate::pos::GridPos;

/// Diagonal movement rule (mirrors common tilemap pathfinders).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum DiagonalMode {
    /// 4-directional only.
    #[default]
    Never,
    /// 8-directional, diagonals cut corners.
    Always,
    /// 8-directional, diagonal needs both orthogonal neighbors free.
    NoCornerCut,
}

/// A* distance estimate. Zero turns A* into Dijkstra.
///
/// Must never overestimate the cheapest step, or A* returns suboptimal
/// paths. Pair with the movement rule and minimum step cost:
/// - `Never` (4-dir) + unit costs → `Manhattan` (the default pairing).
/// - `Always`/`NoCornerCut` (8-dir) + unit costs → `Chebyshev`
///   (`Manhattan` estimates 2 for a 1-cost diagonal, `Euclid` 1.41).
/// - Diagonal cost ≥ √2 (corner-cut penalty) → `Euclid` is admissible.
/// When in doubt use `Zero` (Dijkstra: always optimal, just slower).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Heuristic {
    #[default]
    Manhattan,
    Chebyshev,
    Euclid,
    Zero,
}

/// A* tunables.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct AstarConfig {
    pub diagonal: DiagonalMode,
    pub heuristic: Heuristic,
    /// Abort after this many pops (0 = unlimited).
    pub max_visits: u32,
}

impl Default for AstarConfig {
    fn default() -> Self {
        Self {
            diagonal: DiagonalMode::Never,
            heuristic: Heuristic::Manhattan,
            max_visits: 0,
        }
    }
}

fn estimate(h: Heuristic, a: GridPos, b: GridPos) -> f32 {
    match h {
        Heuristic::Manhattan => a.manhattan(b) as f32,
        Heuristic::Chebyshev => a.chebyshev(b) as f32,
        Heuristic::Euclid => a.euclid(b),
        Heuristic::Zero => 0.0,
    }
}

fn moves(mode: DiagonalMode) -> &'static [(i32, i32)] {
    match mode {
        DiagonalMode::Never => &crate::pos::DIRS4,
        _ => &crate::pos::DIRS8,
    }
}

// `f32` has no `Ord`; quantize for the heap. 1e3 resolution is plenty.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key(i64);

/// Shortest path from `start` to `goal` (both inclusive), or None.
/// `passable` must hold for start; goal is never expanded past.
pub fn astar(
    start: GridPos,
    goal: GridPos,
    passable: &impl Fn(GridPos) -> bool,
    step_cost: &impl Fn(GridPos, GridPos) -> f32,
    cfg: AstarConfig,
) -> Option<Vec<GridPos>> {
    if start == goal {
        return passable(start).then_some(vec![start]);
    }
    if !passable(start) || !passable(goal) {
        return None;
    }
    let mut open = BinaryHeap::new();
    let mut g_score: HashMap<GridPos, f32> = HashMap::new();
    let mut came: HashMap<GridPos, GridPos> = HashMap::new();
    g_score.insert(start, 0.0);
    open.push((Reverse(Key(0)), start));
    let mut visits = 0u32;

    while let Some((_, cur)) = open.pop() {
        visits += 1;
        if cfg.max_visits > 0 && visits > cfg.max_visits {
            return None;
        }
        if cur == goal {
            let mut path = vec![cur];
            while let Some(p) = came.get(path.last().unwrap()) {
                path.push(*p);
            }
            path.reverse();
            return Some(path);
        }
        let cur_g = g_score[&cur];
        for (dx, dy) in moves(cfg.diagonal) {
            let nxt = GridPos::new(cur.x + dx, cur.y + dy);
            if !passable(nxt) {
                continue;
            }
            if cfg.diagonal == DiagonalMode::NoCornerCut
                && *dx != 0
                && *dy != 0
                && (!passable(GridPos::new(cur.x + dx, cur.y))
                    || !passable(GridPos::new(cur.x, cur.y + dy)))
            {
                continue;
            }
            let g = cur_g + step_cost(cur, nxt).max(0.0);
            if g < g_score.get(&nxt).copied().unwrap_or(f32::INFINITY) {
                g_score.insert(nxt, g);
                came.insert(nxt, cur);
                let f = g + estimate(cfg.heuristic, nxt, goal);
                open.push((Reverse(Key((f * 1000.0) as i64)), nxt));
            }
        }
    }
    None
}

/// Path to `goal`, or to the cheapest passable neighbor of it when the
/// goal itself is blocked (furniture, chairs, doors). Returns None only
/// when nothing adjacent is reachable.
pub fn astar_adjacent(
    start: GridPos,
    goal: GridPos,
    passable: &impl Fn(GridPos) -> bool,
    step_cost: &impl Fn(GridPos, GridPos) -> f32,
    cfg: AstarConfig,
) -> Option<Vec<GridPos>> {
    if let Some(p) = astar(start, goal, passable, step_cost, cfg) {
        return Some(p);
    }
    fn cost_of(p: &[GridPos], step_cost: &impl Fn(GridPos, GridPos) -> f32) -> f32 {
        p.windows(2).map(|w| step_cost(w[0], w[1]).max(0.0)).sum()
    }
    let mut best: Option<(Vec<GridPos>, f32)> = None;
    for n in goal.neighbors8() {
        if n == start || !passable(n) {
            continue;
        }
        if let Some(p) = astar(start, n, passable, step_cost, cfg) {
            let c = cost_of(&p, step_cost);
            if best.as_ref().is_none_or(|(_, bc)| c < *bc) {
                best = Some((p, c));
            }
        }
    }
    best.map(|(p, _)| p)
}

/// Flood fill from `start` up to `max_depth` steps. Maps cell -> depth.
pub fn bfs_fill(
    start: GridPos,
    max_depth: u32,
    diagonal: bool,
    passable: &impl Fn(GridPos) -> bool,
) -> HashMap<GridPos, u32> {
    let mut out = HashMap::new();
    if !passable(start) {
        return out;
    }
    let mut frontier = vec![start];
    out.insert(start, 0);
    let mut depth = 0u32;
    while !frontier.is_empty() && depth < max_depth {
        depth += 1;
        let mut next = Vec::new();
        for cur in frontier {
            let it: &[(i32, i32)] = if diagonal {
                &crate::pos::DIRS8
            } else {
                &crate::pos::DIRS4
            };
            for (dx, dy) in it {
                let nxt = GridPos::new(cur.x + dx, cur.y + dy);
                if out.contains_key(&nxt) || !passable(nxt) {
                    continue;
                }
                out.insert(nxt, depth);
                next.push(nxt);
            }
        }
        frontier = next;
    }
    out
}

/// Multi-source step-distance to the nearest goal (unit cost).
/// Unreached passable cells are absent; bound the region via `passable`.
pub fn distance_field(
    goals: &[GridPos],
    diagonal: bool,
    passable: &impl Fn(GridPos) -> bool,
) -> HashMap<GridPos, u32> {
    let mut out = HashMap::new();
    let mut frontier: Vec<GridPos> = goals.iter().copied().filter(|g| passable(*g)).collect();
    for g in &frontier {
        out.insert(*g, 0);
    }
    let mut depth = 0u32;
    while !frontier.is_empty() {
        depth += 1;
        let mut next = Vec::new();
        for cur in frontier {
            let it: &[(i32, i32)] = if diagonal {
                &crate::pos::DIRS8
            } else {
                &crate::pos::DIRS4
            };
            for (dx, dy) in it {
                let nxt = GridPos::new(cur.x + dx, cur.y + dy);
                if out.contains_key(&nxt) || !passable(nxt) {
                    continue;
                }
                out.insert(nxt, depth);
                next.push(nxt);
            }
        }
        frontier = next;
        if depth > 1_000_000 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dense::DenseGrid;

    fn open_grid(w: u32, h: u32, walls: &[(i32, i32)]) -> DenseGrid<bool> {
        let mut g = DenseGrid::new(w, h, true);
        for (x, y) in walls {
            g.set(GridPos::new(*x, *y), false);
        }
        g
    }

    fn unit(_a: GridPos, _b: GridPos) -> f32 {
        1.0
    }

    #[test]
    fn straight_path_4dir() {
        let g = open_grid(5, 1, &[]);
        let pass = |p: GridPos| g.get(p).copied().unwrap_or(false);
        let p = astar(
            GridPos::new(0, 0),
            GridPos::new(4, 0),
            &pass,
            &unit,
            AstarConfig::default(),
        )
        .unwrap();
        assert_eq!(p.len(), 5);
    }

    #[test]
    fn detours_around_wall() {
        let g = open_grid(3, 3, &[(1, 0), (1, 1)]);
        let pass = |p: GridPos| g.get(p).copied().unwrap_or(false);
        let p = astar(
            GridPos::new(0, 0),
            GridPos::new(2, 0),
            &pass,
            &unit,
            AstarConfig::default(),
        )
        .unwrap();
        assert!(p.len() > 3);
        assert_eq!(*p.last().unwrap(), GridPos::new(2, 0));
    }

    #[test]
    fn unreachable_none() {
        let g = open_grid(3, 1, &[(1, 0)]);
        let pass = |p: GridPos| g.get(p).copied().unwrap_or(false);
        assert!(
            astar(
                GridPos::new(0, 0),
                GridPos::new(2, 0),
                &pass,
                &unit,
                AstarConfig::default()
            )
            .is_none()
        );
    }

    #[test]
    fn adjacent_goal_fallback() {
        let g = open_grid(3, 3, &[(1, 1)]);
        let pass = |p: GridPos| g.get(p).copied().unwrap_or(false);
        let p = astar_adjacent(
            GridPos::new(0, 0),
            GridPos::new(1, 1),
            &pass,
            &unit,
            AstarConfig::default(),
        )
        .unwrap();
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn diagonal_modes_differ() {
        let g = open_grid(3, 3, &[]);
        let pass = |p: GridPos| g.get(p).copied().unwrap_or(false);
        let four = astar(
            GridPos::ZERO,
            GridPos::new(2, 2),
            &pass,
            &unit,
            AstarConfig::default(),
        )
        .unwrap();
        let cfg = AstarConfig {
            diagonal: DiagonalMode::Always,
            ..Default::default()
        };
        let eight = astar(GridPos::ZERO, GridPos::new(2, 2), &pass, &unit, cfg).unwrap();
        assert_eq!(four.len(), 5);
        assert_eq!(eight.len(), 3);
    }

    #[test]
    fn corner_cut_blocked() {
        let g = open_grid(2, 2, &[(1, 0), (0, 1)]);
        let pass = |p: GridPos| g.get(p).copied().unwrap_or(false);
        let cfg = AstarConfig {
            diagonal: DiagonalMode::NoCornerCut,
            ..Default::default()
        };
        assert!(astar(GridPos::ZERO, GridPos::new(1, 1), &pass, &unit, cfg).is_none());
        let cfg2 = AstarConfig {
            diagonal: DiagonalMode::Always,
            ..Default::default()
        };
        assert!(astar(GridPos::ZERO, GridPos::new(1, 1), &pass, &unit, cfg2).is_some());
    }

    #[test]
    fn fill_depth_and_field() {
        let g = open_grid(5, 5, &[]);
        let pass = |p: GridPos| g.get(p).copied().unwrap_or(false);
        let fill = bfs_fill(GridPos::new(2, 2), 1, false, &pass);
        assert_eq!(fill.len(), 5);
        let field = distance_field(&[GridPos::ZERO], false, &pass);
        assert_eq!(field[&GridPos::new(2, 0)], 2);
    }

    #[test]
    fn weighted_cost_prefers_cheap() {
        let g = open_grid(3, 3, &[]);
        let pass = |p: GridPos| g.get(p).copied().unwrap_or(false);
        let cost = |_a: GridPos, b: GridPos| if b.y == 1 { 10.0 } else { 1.0 };
        let cfg = AstarConfig {
            diagonal: DiagonalMode::Always,
            heuristic: Heuristic::Zero,
            ..Default::default()
        };
        let p = astar(GridPos::new(0, 1), GridPos::new(2, 1), &pass, &cost, cfg).unwrap();
        assert!(!p.iter().any(|c| c.y == 1 && c.x == 1));
    }
}
