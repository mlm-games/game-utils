//! Waypoint graphs over caller-owned ids and positions.
//!
//! Search delegates to the external `pathfinding` crate (A*); costs are
//! quantized to millis internally because the crate needs `Ord` costs.
//! Tile grids live in `game-utils-grid`.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use glam::Vec2;
use pathfinding::directed::astar::astar;
use serde::{Deserialize, Serialize};

/// Greedy LOS shortcutting: from each point, jump to the farthest
/// later point with a clear segment. Straightens grid/A* zigzags.
/// `blocked(a, b)` reports segment obstruction (walls, water).
pub fn smooth_path(points: &[Vec2], blocked: &impl Fn(Vec2, Vec2) -> bool) -> Vec<Vec2> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    let mut i = 0;
    while i + 1 < points.len() {
        let mut j = points.len() - 1;
        while j > i + 1 && blocked(points[i], points[j]) {
            j -= 1;
        }
        out.push(points[j]);
        i = j;
    }
    out
}

/// Waypoint graph: positions plus directed weighted edges.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct NavGraph {
    pos: HashMap<u32, Vec2>,
    edges: HashMap<u32, Vec<(u32, f32)>>,
}

impl NavGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, id: u32, at: Vec2) {
        self.pos.insert(id, at);
        self.edges.entry(id).or_default();
    }

    pub fn remove_node(&mut self, id: u32) -> bool {
        if self.pos.remove(&id).is_none() {
            return false;
        }
        self.edges.remove(&id);
        for outs in self.edges.values_mut() {
            outs.retain(|(to, _)| *to != id);
        }
        true
    }

    /// Directed edge (cost clamped positive).
    pub fn connect(&mut self, from: u32, to: u32, cost: f32) {
        if self.pos.contains_key(&from) && self.pos.contains_key(&to) {
            let outs = self.edges.entry(from).or_default();
            outs.retain(|(t, _)| *t != to);
            outs.push((to, cost.max(0.001)));
        }
    }

    pub fn connect_bi(&mut self, a: u32, b: u32, cost: f32) {
        self.connect(a, b, cost);
        self.connect(b, a, cost);
    }

    pub fn position(&self, id: u32) -> Option<Vec2> {
        self.pos.get(&id).copied()
    }

    pub fn nearest(&self, at: Vec2) -> Option<u32> {
        self.pos
            .iter()
            .min_by(|a, b| {
                a.1.distance_squared(at)
                    .partial_cmp(&b.1.distance_squared(at))
                    .unwrap_or(core::cmp::Ordering::Equal)
            })
            .map(|(id, _)| *id)
    }

    fn successors(&self, id: &u32) -> Vec<(u32, u32)> {
        self.edges
            .get(id)
            .map(|outs| {
                outs.iter()
                    .map(|(to, c)| (*to, (c * 1000.0) as u32))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Node-id path plus true (unquantized) cost, via external A*.
    pub fn find_path(&self, start: u32, goal: u32) -> Option<(Vec<u32>, f32)> {
        if !self.pos.contains_key(&start) || !self.pos.contains_key(&goal) {
            return None;
        }
        let goal_pos = self.pos[&goal];
        let res = astar(
            &start,
            |id| self.successors(id),
            |id| (self.pos[id].distance(goal_pos) * 1000.0) as u32,
            |id| *id == goal,
        )?;
        let cost: f32 = res
            .0
            .windows(2)
            .map(|w| {
                self.edges
                    .get(&w[0])
                    .and_then(|outs| outs.iter().find(|(to, _)| *to == w[1]))
                    .map(|(_, c)| *c)
                    .unwrap_or(0.0)
            })
            .sum();
        Some((res.0, cost))
    }

    /// All nodes within `budget` of `start`, mapped to true cost.
    /// Local Dijkstra: the external crate has no ranged fill.
    pub fn reachable_within(&self, start: u32, budget: f32) -> HashMap<u32, f32> {
        let mut best: HashMap<u32, f32> = HashMap::new();
        if !self.pos.contains_key(&start) || budget < 0.0 {
            return best;
        }
        let mut heap = BinaryHeap::new();
        heap.push((Reverse(0u32), start));
        best.insert(start, 0.0);
        while let Some((Reverse(_), cur)) = heap.pop() {
            let cur_cost = best[&cur];
            if let Some(outs) = self.edges.get(&cur) {
                for (to, c) in outs {
                    let next = cur_cost + *c;
                    if next <= budget && next < best.get(to).copied().unwrap_or(f32::INFINITY) {
                        best.insert(*to, next);
                        heap.push((Reverse((next * 1000.0) as u32), *to));
                    }
                }
            }
        }
        best
    }

    /// Nearest of several goals in one A* run: (goal, path, true cost).
    /// The external search accepts any goal, so this costs one pass.
    pub fn find_nearest_goal(&self, start: u32, goals: &[u32]) -> Option<(u32, Vec<u32>, f32)> {
        if !self.pos.contains_key(&start) || goals.is_empty() {
            return None;
        }
        // Unknown goals would poison the heuristic (u32::MAX overflows
        // the external search); drop them, fail when none remain.
        let goals: Vec<u32> = goals
            .iter()
            .copied()
            .filter(|g| self.pos.contains_key(g))
            .collect();
        if goals.is_empty() {
            return None;
        }
        let in_set = |id: &u32| goals.contains(id);
        let res = astar(
            &start,
            |id| self.successors(id),
            |id| {
                goals
                    .iter()
                    .filter_map(|g| {
                        self.pos
                            .get(g)
                            .map(|p| (self.pos[id].distance(*p) * 1000.0) as u32)
                    })
                    .min()
                    .unwrap_or(u32::MAX)
            },
            in_set,
        )?;
        let goal = *res.0.last().unwrap();
        let cost: f32 = res
            .0
            .windows(2)
            .map(|w| {
                self.edges
                    .get(&w[0])
                    .and_then(|outs| outs.iter().find(|(to, _)| *to == w[1]))
                    .map(|(_, c)| *c)
                    .unwrap_or(0.0)
            })
            .sum();
        Some((goal, res.0, cost))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> NavGraph {
        let mut g = NavGraph::new();
        g.add_node(0, Vec2::ZERO);
        g.add_node(1, Vec2::X * 10.0);
        g.add_node(2, Vec2::X * 20.0);
        g.add_node(3, Vec2::Y * 10.0);
        g.connect_bi(0, 1, 10.0);
        g.connect_bi(1, 2, 10.0);
        g.connect(0, 3, 50.0);
        g
    }

    #[test]
    fn shortest_path_prefers_cheap() {
        let g = graph();
        let (path, cost) = g.find_path(0, 2).unwrap();
        assert_eq!(path, vec![0, 1, 2]);
        assert!((cost - 20.0).abs() < 1e-6);
        assert!(g.find_path(0, 99).is_none());
    }

    #[test]
    fn directed_edges_respected() {
        let g = graph();
        // 3 -> 0 has no edge.
        assert!(g.find_path(3, 0).is_none());
    }

    #[test]
    fn reachable_cutoff() {
        let g = graph();
        let r = g.reachable_within(0, 15.0);
        assert!(r.contains_key(&0) && r.contains_key(&1));
        assert!(!r.contains_key(&2));
        assert!(!r.contains_key(&3));
    }

    #[test]
    fn nearest_and_remove() {
        let mut g = graph();
        assert_eq!(g.nearest(Vec2::X * 9.0), Some(1));
        assert!(g.remove_node(1));
        assert!(g.find_path(0, 2).is_none());
        assert!(!g.remove_node(1));
    }

    #[test]
    fn nearest_goal_single_pass() {
        let g = graph();
        let (goal, path, cost) = g.find_nearest_goal(0, &[2, 3]).unwrap();
        assert_eq!((goal, path), (2, vec![0, 1, 2]));
        assert!((cost - 20.0).abs() < 1e-6);
        assert!(g.find_nearest_goal(0, &[]).is_none());
        assert!(g.find_nearest_goal(0, &[99]).is_none());
    }

    #[test]
    fn smooth_shortcuts_open_space() {
        let pts = vec![Vec2::ZERO, Vec2::X, Vec2::X * 2.0, Vec2::X * 3.0];
        let open = |_: Vec2, _: Vec2| false;
        assert_eq!(smooth_path(&pts, &open), vec![Vec2::ZERO, Vec2::X * 3.0]);
        let shut = |_: Vec2, _: Vec2| true;
        assert_eq!(smooth_path(&pts, &shut).len(), 4);
    }
}
