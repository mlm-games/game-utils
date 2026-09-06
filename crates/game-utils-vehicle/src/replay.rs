//! Ghost/replay traces: fixed-step samples, interpolated playback.

use glam::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sample {
    pub t: f32,
    pub pos: Vec2,
    pub heading_rad: f32,
    pub speed: f32,
    /// Control snapshot for input-accurate ghosts and validation.
    #[serde(default)]
    pub steer: f32,
    #[serde(default)]
    pub throttle: f32,
    #[serde(default)]
    pub brake: f32,
    #[serde(default)]
    pub boost: f32,
    /// Engine speed (rpm) when the source has one, else 0.
    #[serde(default)]
    pub rpm: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    /// Fixed interval between samples (seconds).
    pub dt: f32,
    samples: Vec<Sample>,
    /// Oldest samples are dropped past this count (0 = unbounded).
    pub max_samples: usize,
    dropped: u64,
}

impl Trace {
    pub fn new(dt: f32) -> Self {
        Self {
            dt: dt.max(1e-4),
            samples: Vec::new(),
            max_samples: 0,
            dropped: 0,
        }
    }

    pub fn bounded(dt: f32, max_samples: usize) -> Self {
        Self {
            dt: dt.max(1e-4),
            samples: Vec::new(),
            max_samples,
            dropped: 0,
        }
    }

    pub fn record(&mut self, t: f32, pos: Vec2, heading_rad: f32, speed: f32) {
        self.record_full(t, pos, heading_rad, speed, 0.0, 0.0, 0.0, 0.0, 0.0);
    }

    /// Record with a control snapshot (see [`Sample`]).
    #[allow(clippy::too_many_arguments)]
    pub fn record_full(
        &mut self,
        t: f32,
        pos: Vec2,
        heading_rad: f32,
        speed: f32,
        steer: f32,
        throttle: f32,
        brake: f32,
        boost: f32,
        rpm: f32,
    ) {
        if self.max_samples > 0 {
            while self.samples.len() >= self.max_samples {
                self.samples.remove(0);
                self.dropped += 1;
            }
        }
        self.samples.push(Sample {
            t,
            pos,
            heading_rad,
            speed,
            steer,
            throttle,
            brake,
            boost,
            rpm,
        });
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn duration(&self) -> f32 {
        match (self.samples.first(), self.samples.last()) {
            (Some(a), Some(b)) => (b.t - a.t).max(0.0),
            _ => 0.0,
        }
    }

    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Interpolated sample at `t` (clamped ends, `None` if empty).
    pub fn sample_at(&self, t: f32) -> Option<Sample> {
        let n = self.samples.len();
        if n == 0 {
            return None;
        }
        if t <= self.samples[0].t {
            return Some(self.samples[0]);
        }
        if t >= self.samples[n - 1].t {
            return Some(self.samples[n - 1]);
        }
        // Binary search the surrounding pair.
        let mut lo = 0;
        let mut hi = n - 1;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.samples[mid].t <= t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let a = &self.samples[lo];
        let b = &self.samples[hi];
        let span = (b.t - a.t).max(f32::EPSILON);
        let f = ((t - a.t) / span).clamp(0.0, 1.0);
        Some(Sample {
            t,
            pos: a.pos.lerp(b.pos, f),
            heading_rad: lerp_angle(a.heading_rad, b.heading_rad, f),
            speed: a.speed + (b.speed - a.speed) * f,
            steer: a.steer + (b.steer - a.steer) * f,
            throttle: a.throttle + (b.throttle - a.throttle) * f,
            brake: a.brake + (b.brake - a.brake) * f,
            boost: a.boost + (b.boost - a.boost) * f,
            rpm: a.rpm + (b.rpm - a.rpm) * f,
        })
    }
}

/// Shortest-path angle interpolation.
pub fn lerp_angle(a: f32, b: f32, f: f32) -> f32 {
    let d = (b - a) % std::f32::consts::TAU;
    let d = if d > std::f32::consts::PI {
        d - std::f32::consts::TAU
    } else if d < -std::f32::consts::PI {
        d + std::f32::consts::TAU
    } else {
        d
    };
    a + d * f.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn trace_record_and_interpolate() {
        let mut tr = Trace::new(0.1);
        tr.record(0.0, Vec2::ZERO, 0.0, 0.0);
        tr.record(0.1, Vec2::new(10.0, 0.0), 0.0, 10.0);
        assert_eq!(tr.len(), 2);
        assert!((tr.duration() - 0.1).abs() < 1e-6);
        let mid = tr.sample_at(0.05).unwrap();
        assert!((mid.pos.x - 5.0).abs() < 1e-4);
        assert!((mid.speed - 5.0).abs() < 1e-4);
        // Clamped ends.
        assert_eq!(tr.sample_at(-1.0).unwrap().pos, Vec2::ZERO);
        assert_eq!(tr.sample_at(99.0).unwrap().pos, Vec2::new(10.0, 0.0));
        assert!(Trace::new(0.1).sample_at(0.0).is_none());
    }

    #[test]
    fn trace_bounded_drops_oldest() {
        let mut tr = Trace::bounded(0.1, 2);
        tr.record(0.0, Vec2::ZERO, 0.0, 0.0);
        tr.record(0.1, Vec2::ONE, 0.0, 1.0);
        tr.record(0.2, Vec2::splat(2.0), 0.0, 2.0);
        assert_eq!(tr.len(), 2);
        assert_eq!(tr.dropped(), 1);
        assert_eq!(tr.samples()[0].pos, Vec2::ONE);
    }

    #[test]
    fn lerp_angle_takes_short_path() {
        let a = lerp_angle(0.0, 2.0 * PI - 0.2, 0.5);
        assert!(a < 0.0, "went the long way: {a}");
        assert!((lerp_angle(1.0, 2.0, 0.5) - 1.5).abs() < 1e-6);
    }
}
