//! Circuit centerlines: arclength progress, lateral offset,
//! start/finish laps. For arenas use [`race`](crate::race) gates.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::ai::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackConfig {
    pub path: Path,
    /// Laps to finish.
    pub laps: u32,
    /// Seconds of backward progress before a wrong-way event.
    pub wrong_way_secs: f32,
}

impl Default for TrackConfig {
    fn default() -> Self {
        Self {
            path: Path::new(Vec::new(), true),
            laps: 3,
            wrong_way_secs: 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TrackEvent {
    Lap { lap: u32, lap_time: f32 },
    Finished { total_time: f32, best_lap: f32 },
    WrongWay,
    BackOnTrack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackState {
    /// Arclength along the centerline (m).
    pub s: f32,
    /// Signed lateral offset from the centerline (m).
    pub lateral: f32,
    pub lap: u32,
    pub race_time: f32,
    pub lap_start: f32,
    pub last_lap: Option<f32>,
    pub best_lap: Option<f32>,
    /// Distance travelled (m), for HUDs and tiebreaks.
    pub distance: f32,
    pub finished: bool,
    wrong_way_timer: f32,
    wrong_way_active: bool,
    lap_armed: bool,
    prev_s: Option<f32>,
    prev_pos: Option<Vec3>,
}

impl Default for TrackState {
    fn default() -> Self {
        Self::new()
    }
}

impl TrackState {
    pub fn new() -> Self {
        Self {
            s: 0.0,
            lateral: 0.0,
            lap: 0,
            race_time: 0.0,
            lap_start: 0.0,
            last_lap: None,
            best_lap: None,
            distance: 0.0,
            finished: false,
            wrong_way_timer: 0.0,
            wrong_way_active: false,
            lap_armed: false,
            prev_s: None,
            prev_pos: None,
        }
    }

    /// Restart the run (menus, restarts, new heats).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Overall progress in meters. Compare across racers.
    pub fn progress(&self, cfg: &TrackConfig) -> f32 {
        self.lap as f32 * cfg.path.length() + self.s
    }

    /// Feed position; returns events fired this step.
    pub fn update(&mut self, cfg: &TrackConfig, pos: Vec3, dt: f32) -> Vec<TrackEvent> {
        let mut events = Vec::new();
        let total = cfg.path.length();
        if self.finished || total <= 0.0 {
            return events;
        }
        let dt = dt.max(0.0);
        self.race_time += dt;
        if let Some(prev) = self.prev_pos {
            self.distance += pos.distance(prev);
        }
        self.prev_pos = Some(pos);

        let (s, closest, tangent) = match cfg.path.project(pos) {
            Some(v) => v,
            None => return events,
        };
        self.s = s;
        self.lateral = (closest - pos).cross(tangent).y;

        if dt > 0.0 {
            if let Some(prev) = self.prev_s {
                // Arclength delta with wraparound handling.
                let mut ds = s - prev;
                if ds > total * 0.5 {
                    ds -= total;
                } else if ds < -total * 0.5 {
                    ds += total;
                }
                // Wrong-way: sustained backward progress.
                if ds < -0.01 {
                    self.wrong_way_timer += dt;
                    if !self.wrong_way_active && self.wrong_way_timer >= cfg.wrong_way_secs.max(0.1)
                    {
                        self.wrong_way_active = true;
                        events.push(TrackEvent::WrongWay);
                    }
                } else {
                    self.wrong_way_timer = 0.0;
                    if self.wrong_way_active {
                        self.wrong_way_active = false;
                        events.push(TrackEvent::BackOnTrack);
                    }
                }
                // Lap: cross s = 0 forward after traveling past halfway
                // (hysteresis against start-line jitter).
                if s > total * 0.5 {
                    self.lap_armed = true;
                }
                if self.lap_armed && prev > total * 0.75 && s < total * 0.25 {
                    self.lap_armed = false;
                    self.lap += 1;
                    let lap_time = self.race_time - self.lap_start;
                    self.last_lap = Some(lap_time);
                    self.best_lap = Some(self.best_lap.map_or(lap_time, |b: f32| b.min(lap_time)));
                    self.lap_start = self.race_time;
                    if self.lap >= cfg.laps.max(1) {
                        self.finished = true;
                        events.push(TrackEvent::Finished {
                            total_time: self.race_time,
                            best_lap: self.best_lap.unwrap_or(lap_time),
                        });
                    } else {
                        events.push(TrackEvent::Lap {
                            lap: self.lap,
                            lap_time,
                        });
                    }
                }
            }
            self.prev_s = Some(s);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circuit() -> TrackConfig {
        let mut pts = Vec::new();
        for i in 0..16 {
            let a = i as f32 / 16.0 * std::f32::consts::TAU;
            pts.push(Vec3::new(a.cos() * 50.0, 0.0, a.sin() * 20.0));
        }
        TrackConfig {
            path: Path::new(pts, true),
            laps: 1,
            wrong_way_secs: 1.0,
        }
    }

    #[test]
    fn track_lap_on_circuit() {
        let cfg = circuit();
        let mut st = TrackState::new();
        let mut finished = false;
        let mut laps = 0;
        // Walk the whole ellipse once, starting just past the line so
        // the crossing arms and fires exactly once.
        for i in 0..=64 {
            let a = i as f32 / 64.0 * std::f32::consts::TAU;
            let pos = Vec3::new(a.cos() * 50.0, 0.0, a.sin() * 20.0);
            for e in st.update(&cfg, pos, 0.25) {
                match e {
                    TrackEvent::Finished { .. } => finished = true,
                    TrackEvent::Lap { .. } => laps += 1,
                    _ => {}
                }
            }
        }
        assert!(finished);
        assert_eq!(st.lap, 1);
        assert_eq!(laps, 0);
        assert!(st.distance > 100.0);
        assert!(st.progress(&cfg) > 0.0);
    }

    #[test]
    fn track_wrong_way() {
        let cfg = circuit();
        let mut st = TrackState::new();
        let mut saw = false;
        // Walk backward from the start line.
        let mut a = 0.0f32;
        for _ in 0..80 {
            a -= 0.02;
            let pos = Vec3::new(a.cos() * 50.0, 0.0, a.sin() * 20.0);
            for e in st.update(&cfg, pos, 0.1) {
                if e == TrackEvent::WrongWay {
                    saw = true;
                }
            }
        }
        assert!(saw);
    }
}
