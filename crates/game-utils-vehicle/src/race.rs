//! Lap/checkpoint tracking from position samples. Model-agnostic;
//! countdowns and placements stay game-side.

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Checkpoint gate: crossing within `radius` advances.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Checkpoint {
    pub pos: Vec2,
    pub radius: f32,
}

impl Checkpoint {
    pub fn new(pos: Vec2, radius: f32) -> Self {
        Self {
            pos,
            radius: radius.max(0.0),
        }
    }

    pub fn reached(&self, pos: Vec2) -> bool {
        pos.distance(self.pos) <= self.radius
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaceConfig {
    /// Gates in order; last one completes the lap.
    pub checkpoints: Vec<Checkpoint>,
    /// Laps to finish.
    pub laps: u32,
    /// Seconds of sustained backward motion before a wrong-way event.
    pub wrong_way_secs: f32,
}

impl Default for RaceConfig {
    fn default() -> Self {
        Self {
            checkpoints: Vec::new(),
            laps: 3,
            wrong_way_secs: 2.0,
        }
    }
}

/// Events from [`RaceState::update`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RaceEvent {
    Checkpoint { index: usize },
    Lap { lap: u32, lap_time: f32 },
    Finished { total_time: f32, best_lap: f32 },
    WrongWay,
    BackOnTrack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaceState {
    /// Completed laps.
    pub lap: u32,
    /// Next gate index.
    pub next: usize,
    pub race_time: f32,
    pub lap_start: f32,
    pub last_lap: Option<f32>,
    pub best_lap: Option<f32>,
    /// Split (gate) times of the current lap.
    pub splits: Vec<f32>,
    /// Distance travelled (m), for HUDs and tiebreaks.
    pub distance: f32,
    pub finished: bool,
    wrong_way_timer: f32,
    wrong_way_active: bool,
    prev_pos: Option<Vec2>,
}

impl Default for RaceState {
    fn default() -> Self {
        Self::new()
    }
}

impl RaceState {
    pub fn new() -> Self {
        Self {
            lap: 0,
            next: 0,
            race_time: 0.0,
            lap_start: 0.0,
            last_lap: None,
            best_lap: None,
            splits: Vec::new(),
            distance: 0.0,
            finished: false,
            wrong_way_timer: 0.0,
            wrong_way_active: false,
            prev_pos: None,
        }
    }

    /// Restart the run (menus, restarts, new heats).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Overall progress in gates (lap * gates + next). Compare across
    /// racers for live positions.
    pub fn progress(&self, gate_count: usize) -> u32 {
        self.lap * gate_count.max(1) as u32 + self.next as u32
    }

    /// Feed the current position; returns events fired this step.
    /// Non-positive `dt` still checks gates but advances no timers.
    pub fn update(&mut self, cfg: &RaceConfig, pos: Vec2, dt: f32) -> Vec<RaceEvent> {
        let mut events = Vec::new();
        if self.finished || cfg.checkpoints.is_empty() {
            return events;
        }
        let dt = dt.max(0.0);
        self.race_time += dt;
        let moved = match self.prev_pos {
            Some(prev) => {
                self.distance += pos.distance(prev);
                pos - prev
            }
            None => Vec2::ZERO,
        };
        self.prev_pos = Some(pos);

        // Wrong-way: sustained motion away from the next gate.
        let gate = &cfg.checkpoints[self.next % cfg.checkpoints.len()];
        let to_gate = gate.pos - pos;
        if dt > 0.0
            && to_gate.length() > gate.radius
            && moved.length() > 0.5 * dt
            && moved.normalize_or_zero().dot(to_gate.normalize_or_zero()) < -0.3
        {
            self.wrong_way_timer += dt;
            if !self.wrong_way_active && self.wrong_way_timer >= cfg.wrong_way_secs.max(0.1) {
                self.wrong_way_active = true;
                events.push(RaceEvent::WrongWay);
            }
        } else {
            self.wrong_way_timer = 0.0;
            if self.wrong_way_active {
                self.wrong_way_active = false;
                events.push(RaceEvent::BackOnTrack);
            }
        }

        if gate.reached(pos) {
            let gate_index = self.next % cfg.checkpoints.len();
            self.splits.push(self.race_time - self.lap_start);
            self.next += 1;
            events.push(RaceEvent::Checkpoint { index: gate_index });
            if self.next.is_multiple_of(cfg.checkpoints.len()) {
                let lap_time = self.race_time - self.lap_start;
                self.lap += 1;
                self.last_lap = Some(lap_time);
                self.best_lap = Some(self.best_lap.map_or(lap_time, |b: f32| b.min(lap_time)));
                self.lap_start = self.race_time;
                self.splits.clear();
                if self.lap >= cfg.laps.max(1) {
                    self.finished = true;
                    events.push(RaceEvent::Finished {
                        total_time: self.race_time,
                        best_lap: self.best_lap.unwrap_or(lap_time),
                    });
                } else {
                    events.push(RaceEvent::Lap {
                        lap: self.lap,
                        lap_time,
                    });
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight_cfg() -> RaceConfig {
        RaceConfig {
            checkpoints: vec![
                Checkpoint::new(Vec2::new(10.0, 0.0), 2.0),
                Checkpoint::new(Vec2::new(20.0, 0.0), 2.0),
            ],
            laps: 2,
            wrong_way_secs: 1.0,
        }
    }

    #[test]
    fn race_gates_laps_finish() {
        let cfg = straight_cfg();
        let mut s = RaceState::new();
        let mut pos = Vec2::ZERO;
        let mut finished = false;
        let mut laps = 0;
        // Shuttle along x to pass gates twice.
        let mut dir = 1.0f32;
        for _ in 0..400 {
            pos.x += dir * 1.0;
            if pos.x > 22.0 {
                dir = -1.0;
            }
            if pos.x < -2.0 {
                dir = 1.0;
            }
            for e in s.update(&cfg, pos, 0.1) {
                match e {
                    RaceEvent::Lap { .. } => laps += 1,
                    RaceEvent::Finished { .. } => finished = true,
                    _ => {}
                }
            }
            if finished {
                break;
            }
        }
        assert!(finished);
        assert_eq!(s.lap, 2);
        assert_eq!(laps, 1);
        assert!(s.best_lap.is_some());
        assert!(s.distance > 0.0);
    }

    #[test]
    fn race_wrong_way_fires_and_clears() {
        let cfg = straight_cfg();
        let mut s = RaceState::new();
        let mut saw_wrong = false;
        let mut saw_back = false;
        let mut pos = Vec2::ZERO;
        // Drive away from the first gate.
        for _ in 0..30 {
            pos.x -= 1.0;
            for e in s.update(&cfg, pos, 0.1) {
                if e == RaceEvent::WrongWay {
                    saw_wrong = true;
                }
            }
        }
        assert!(saw_wrong);
        // Head back.
        for _ in 0..40 {
            pos.x += 1.0;
            for e in s.update(&cfg, pos, 0.1) {
                if e == RaceEvent::BackOnTrack {
                    saw_back = true;
                }
            }
        }
        assert!(saw_back);
    }

    #[test]
    fn race_no_gates_no_events() {
        let cfg = RaceConfig::default();
        let mut s = RaceState::new();
        assert!(s.update(&cfg, Vec2::new(5.0, 5.0), 0.1).is_empty());
        assert_eq!(s.progress(0), 0);
    }
}
