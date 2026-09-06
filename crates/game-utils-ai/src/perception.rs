//! Sight/hearing checks plus contact memory with forget timers.

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Sense tuning. `fov_cos` is the facing-dot cutoff (use 2.0 for
/// omnidirectional). `forget_time <= 0` never forgets.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Senses {
    pub sight_range: f32,
    pub fov_cos: f32,
    pub hearing_range: f32,
    pub forget_time: f32,
}

impl Senses {
    pub fn new(sight_range: f32, fov_cos: f32, hearing_range: f32, forget_time: f32) -> Self {
        Self {
            sight_range: sight_range.max(0.0),
            fov_cos,
            hearing_range: hearing_range.max(0.0),
            forget_time,
        }
    }

    /// True when `target` is in range, inside the FOV cone, and the
    /// caller-supplied `blocked(eye, target)` LOS check passes.
    pub fn sees(
        &self,
        eye: Vec2,
        facing: Vec2,
        target: Vec2,
        blocked: &impl Fn(Vec2, Vec2) -> bool,
    ) -> bool {
        let d = target - eye;
        let dist = d.length();
        if dist > self.sight_range || dist <= f32::EPSILON {
            return dist <= f32::EPSILON;
        }
        if self.fov_cos <= 1.0
            && facing.length_squared() > f32::EPSILON
            && d.normalize().dot(facing.normalize()) < self.fov_cos
        {
            return false;
        }
        !blocked(eye, target)
    }

    /// True when a noise at `at` with loudness `gain` is audible.
    pub fn hears(&self, ear: Vec2, at: Vec2, gain: f32) -> bool {
        ear.distance(at) <= self.hearing_range * gain.max(0.0)
    }
}

/// Last-known-position memory. `update` with each check; `known`
/// returns None once the contact is forgotten.
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Tracker {
    last: Option<Vec2>,
    age: f32,
}

impl Tracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, dt: f32, seen: Option<Vec2>, forget_time: f32) {
        match seen {
            Some(p) => {
                self.last = Some(p);
                self.age = 0.0;
            }
            None => self.age += dt.max(0.0),
        }
        if forget_time > 0.0 && self.age >= forget_time {
            self.last = None;
        }
    }

    pub fn known(&self) -> Option<Vec2> {
        self.last
    }

    pub fn age(&self) -> f32 {
        self.age
    }
}

/// One noise event (explosions, gunshots, alarms). `gain` scales the
/// hearing radius; `ttl` bounds how long it lingers.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Stimulus {
    pub pos: Vec2,
    pub gain: f32,
    pub ttl: f32,
}

impl Stimulus {
    pub fn new(pos: Vec2, gain: f32, ttl: f32) -> Self {
        Self {
            pos,
            gain: gain.max(0.0),
            ttl: ttl.max(0.0),
        }
    }
}

/// Live noise bus. Games push combat/explosion noise; agents query
/// [`Stimuli::heard`] with their own ears.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Stimuli {
    list: Vec<Stimulus>,
}

impl Stimuli {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, s: Stimulus) {
        if s.ttl > 0.0 && s.gain > 0.0 {
            self.list.push(s);
        }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Age all stimuli, dropping the expired.
    pub fn tick(&mut self, dt: f32) {
        let dt = dt.max(0.0);
        self.list.retain_mut(|s| {
            s.ttl -= dt;
            s.ttl > 0.0
        });
    }

    /// Positions audible from `ear` under `senses`.
    pub fn heard(&self, senses: &Senses, ear: Vec2) -> Vec<Vec2> {
        self.list
            .iter()
            .filter(|s| senses.hears(ear, s.pos, s.gain))
            .map(|s| s.pos)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn senses() -> Senses {
        Senses::new(10.0, 0.0, 5.0, 3.0)
    }

    #[test]
    fn sees_cone_and_los() {
        let s = senses();
        let open = |_: Vec2, _: Vec2| false;
        assert!(s.sees(Vec2::ZERO, Vec2::X, Vec2::X * 5.0, &open));
        assert!(!s.sees(Vec2::ZERO, Vec2::X, Vec2::NEG_X * 5.0, &open));
        assert!(!s.sees(Vec2::ZERO, Vec2::X, Vec2::X * 50.0, &open));
        let shut = |_: Vec2, _: Vec2| true;
        assert!(!s.sees(Vec2::ZERO, Vec2::X, Vec2::X * 5.0, &shut));
    }

    #[test]
    fn omni_when_fov_wide() {
        let s = Senses::new(10.0, 2.0, 0.0, 0.0);
        let open = |_: Vec2, _: Vec2| false;
        assert!(s.sees(Vec2::ZERO, Vec2::X, Vec2::NEG_Y * 5.0, &open));
    }

    #[test]
    fn hearing_scales_with_gain() {
        let s = senses();
        assert!(s.hears(Vec2::ZERO, Vec2::X * 4.0, 1.0));
        assert!(!s.hears(Vec2::ZERO, Vec2::X * 4.0, 0.5));
    }

    #[test]
    fn tracker_forgets() {
        let mut t = Tracker::new();
        t.update(0.0, Some(Vec2::X), 3.0);
        assert_eq!(t.known(), Some(Vec2::X));
        t.update(2.0, None, 3.0);
        assert!(t.known().is_some());
        t.update(2.0, None, 3.0);
        assert_eq!(t.known(), None);
    }

    #[test]
    fn stimuli_heard_then_expire() {
        let mut b = Stimuli::new();
        b.push(Stimulus::new(Vec2::X * 4.0, 1.0, 1.0));
        b.push(Stimulus::new(Vec2::X * 40.0, 1.0, 1.0));
        b.push(Stimulus::new(Vec2::Y, 0.0, 1.0));
        assert_eq!(b.len(), 2);
        assert_eq!(b.heard(&senses(), Vec2::ZERO), vec![Vec2::X * 4.0]);
        b.tick(2.0);
        assert!(b.is_empty());
    }
}
