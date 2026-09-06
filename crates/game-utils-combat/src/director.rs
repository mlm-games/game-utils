//! Wave director: escalating budgets, spawn queues, alive caps.

use std::collections::{HashMap, VecDeque};

use glam::Vec2;
use rand::distr::weighted::WeightedIndex;
use rand::prelude::Distribution;
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

/// One spawnable kind: budget cost and per-wave cap.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SpawnEntry {
    pub id: String,
    pub cost: u32,
    pub max: u32,
    pub weight: f32,
}

impl SpawnEntry {
    pub fn new(id: impl Into<String>, cost: u32, max: u32, weight: f32) -> Self {
        Self {
            id: id.into(),
            cost: cost.max(1),
            max: max.max(1),
            weight,
        }
    }
}

/// Budget curve: `base * (1 + growth)^(wave - 1)`.
pub fn budget_for(wave: u32, base: u32, growth: f32) -> u32 {
    (base as f32 * (1.0 + growth.max(0.0)).powi(wave.saturating_sub(1) as i32)) as u32
}

/// Spend `budget` on weighted entries (per-entry caps respected).
/// Returns spawn ids in pick order.
pub fn build_wave(rng: &mut impl Rng, budget: u32, entries: &[SpawnEntry]) -> Vec<String> {
    let mut out = Vec::new();
    let mut spent = HashMap::new();
    let mut left = budget;
    loop {
        let mut ids = Vec::new();
        let mut weights = Vec::new();
        for e in entries {
            if e.weight <= 0.0 || e.cost > left {
                continue;
            }
            if spent.get(e.id.as_str()).copied().unwrap_or(0) >= e.max {
                continue;
            }
            ids.push(e.id.as_str());
            weights.push(e.weight.max(f32::MIN_POSITIVE));
        }
        if ids.is_empty() {
            break;
        }
        let Ok(dist) = WeightedIndex::new(weights) else {
            break;
        };
        let id = ids[dist.sample(rng)];
        let cost = entries.iter().find(|e| e.id == id).unwrap().cost;
        *spent.entry(id).or_insert(0) += 1;
        left -= cost;
        out.push(id.to_owned());
    }
    out
}

/// True when `pos` is past the cull radius (despawn far agents).
pub fn should_despawn(pos: Vec2, center: Vec2, cull_radius: f32) -> bool {
    pos.distance(center) > cull_radius.max(0.0)
}

/// Sample a spawn point on the ring [`min_r`, `max_r`] around `center`.
/// Rejects points where `blocked` holds (walls, water); None when all
/// `tries` fail so the game can defer the spawn.
pub fn pick_spawn(
    rng: &mut impl Rng,
    center: Vec2,
    min_r: f32,
    max_r: f32,
    blocked: &impl Fn(Vec2) -> bool,
    tries: u32,
) -> Option<Vec2> {
    let (lo, hi) = (min_r.max(0.0), max_r.max(min_r.max(0.0)));
    for _ in 0..tries.max(1) {
        let a = rng.random_range(0.0..core::f32::consts::TAU);
        let r = if hi <= lo {
            lo
        } else {
            rng.random_range(lo..=hi)
        };
        let p = center + Vec2::new(a.cos(), a.sin()) * r;
        if !blocked(p) {
            return Some(p);
        }
    }
    None
}
/// Director changes, drained by game code.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum DirectorEvent {
    WaveStarted { wave: u32, spawns: usize },
    WaveCleared { wave: u32 },
}

/// Live director: trickles the queue on an interval, capped by alive.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Director {
    pub wave: u32,
    pub alive: u32,
    pub max_alive: u32,
    pub interval: f32,
    queue: VecDeque<String>,
    acc: f32,
    #[serde(skip, default = "_events")]
    events: Vec<DirectorEvent>,
}

fn _events() -> Vec<DirectorEvent> {
    Vec::new()
}

impl Director {
    pub fn new(max_alive: u32, interval: f32) -> Self {
        Self {
            wave: 0,
            alive: 0,
            max_alive: max_alive.max(1),
            interval: interval.max(0.0),
            queue: VecDeque::new(),
            acc: 0.0,
            events: Vec::new(),
        }
    }

    pub fn drain_events(&mut self) -> Vec<DirectorEvent> {
        core::mem::take(&mut self.events)
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Build and open the next wave.
    pub fn next_wave(
        &mut self,
        rng: &mut impl Rng,
        base: u32,
        growth: f32,
        entries: &[SpawnEntry],
    ) {
        self.wave += 1;
        self.queue = build_wave(rng, budget_for(self.wave, base, growth), entries)
            .into_iter()
            .collect();
        self.acc = 0.0;
        self.events.push(DirectorEvent::WaveStarted {
            wave: self.wave,
            spawns: self.queue.len(),
        });
    }

    pub fn notify_spawned(&mut self, n: u32) {
        self.alive += n;
    }

    pub fn notify_killed(&mut self, n: u32) {
        self.alive = self.alive.saturating_sub(n);
        if self.queue.is_empty() && self.alive == 0 && self.wave > 0 {
            self.events
                .push(DirectorEvent::WaveCleared { wave: self.wave });
        }
    }

    /// Advance the trickle timer. Returns ids to spawn now (alive-capped).
    pub fn tick(&mut self, dt: f32) -> Vec<String> {
        let mut out = Vec::new();
        if self.queue.is_empty() {
            return out;
        }
        let room = self.max_alive.saturating_sub(self.alive) as usize;
        if self.interval <= 0.0 {
            // Instant waves drain the queue at once (up to the alive cap).
            for _ in 0..room {
                match self.queue.pop_front() {
                    Some(id) => out.push(id),
                    None => break,
                }
            }
            return out;
        }
        self.acc += dt.max(0.0);
        while self.acc >= self.interval
            && !self.queue.is_empty()
            && self.alive + (out.len() as u32) < self.max_alive
        {
            self.acc -= self.interval;
            out.push(self.queue.pop_front().unwrap());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    fn rng() -> SmallRng {
        SmallRng::seed_from_u64(9)
    }

    fn entries() -> Vec<SpawnEntry> {
        vec![
            SpawnEntry::new("grunt", 1, 99, 3.0),
            SpawnEntry::new("brute", 3, 2, 1.0),
        ]
    }

    #[test]
    fn budget_grows() {
        assert_eq!(budget_for(1, 10, 0.5), 10);
        assert_eq!(budget_for(2, 10, 0.5), 15);
        assert_eq!(budget_for(3, 10, 0.5), 22);
    }

    #[test]
    fn wave_respects_caps() {
        let w = build_wave(&mut rng(), 100, &entries());
        assert!(w.iter().filter(|s| *s == "brute").count() <= 2);
        assert!(!w.is_empty());
    }

    #[test]
    fn trickle_and_clear() {
        let mut d = Director::new(4, 1.0);
        d.next_wave(&mut rng(), 6, 0.0, &entries());
        assert_eq!(d.wave, 1);
        let first = d.tick(1.0);
        assert_eq!(first.len(), 1);
        d.notify_spawned(1);
        d.notify_killed(1);
        while d.queued() > 0 {
            let ids = d.tick(10.0);
            d.notify_spawned(ids.len() as u32);
            d.notify_killed(ids.len() as u32);
        }
        assert!(
            d.drain_events()
                .contains(&DirectorEvent::WaveCleared { wave: 1 })
        );
    }

    #[test]
    fn spawn_ring_and_cull() {
        let c = Vec2::ZERO;
        let open = |_: Vec2| false;
        let p = pick_spawn(&mut rng(), c, 10.0, 20.0, &open, 8).unwrap();
        assert!((10.0..=20.0).contains(&p.distance(c)));
        assert!(!should_despawn(p, c, 100.0));
        assert!(should_despawn(p, c, 5.0));
        let shut = |_: Vec2| true;
        assert_eq!(pick_spawn(&mut rng(), c, 10.0, 20.0, &shut, 4), None);
    }
}
