//! 2D steering behaviors returning desired velocities (glam).
//!
//! Games integrate: `pos += clamp(desired, max_speed) * dt`. All fns are
//! pure; `wander` takes rng for heading jitter.

use glam::Vec2;
use rand::{Rng, RngExt};

/// Full-speed beeline at `target`.
pub fn seek(pos: Vec2, target: Vec2, max_speed: f32) -> Vec2 {
    let d = target - pos;
    if d.length_squared() <= f32::EPSILON || max_speed <= 0.0 {
        return Vec2::ZERO;
    }
    d.normalize() * max_speed
}

/// Full-speed run from `threat`.
pub fn flee(pos: Vec2, threat: Vec2, max_speed: f32) -> Vec2 {
    seek(threat, pos, max_speed)
}

/// Seek that brakes inside `slowing` (proportional slowdown).
pub fn arrive(pos: Vec2, target: Vec2, max_speed: f32, slowing: f32) -> Vec2 {
    let d = target - pos;
    let dist = d.length();
    if dist <= f32::EPSILON || max_speed <= 0.0 {
        return Vec2::ZERO;
    }
    let speed = if slowing > 0.0 {
        max_speed * (dist / slowing).min(1.0)
    } else {
        max_speed
    };
    d / dist * speed
}

/// Seek where the target will be (`lead` scales target velocity).
pub fn pursue(pos: Vec2, target: Vec2, target_vel: Vec2, max_speed: f32, lead: f32) -> Vec2 {
    seek(pos, target + target_vel * lead.max(0.0), max_speed)
}

/// Flee where the threat will be.
pub fn evade(pos: Vec2, threat: Vec2, threat_vel: Vec2, max_speed: f32, lead: f32) -> Vec2 {
    flee(pos, threat + threat_vel * lead.max(0.0), max_speed)
}

/// Jittered heading: rotate `dir` by a random angle up to `jitter` radians.
pub fn wander(rng: &mut impl Rng, dir: Vec2, jitter: f32) -> Vec2 {
    if dir.length_squared() <= f32::EPSILON {
        return Vec2::X;
    }
    let a = rng.random_range(-jitter.abs()..=jitter.abs());
    let (s, c) = a.sin_cos();
    Vec2::new(dir.x * c - dir.y * s, dir.x * s + dir.y * c).normalize()
}

/// Push from crowded neighbors: sum of (away / dist) inside `radius`.
pub fn separation(pos: Vec2, others: &[Vec2], radius: f32, max: f32) -> Vec2 {
    let mut out = Vec2::ZERO;
    if radius <= 0.0 {
        return out;
    }
    for o in others {
        let d = pos - *o;
        let dist = d.length();
        if dist > f32::EPSILON && dist < radius {
            out += d / (dist * dist);
        }
    }
    if out.length() > max && max > 0.0 {
        out = out.normalize() * max;
    }
    out
}

/// Steer toward the local average heading.
pub fn align(vel: Vec2, others: &[Vec2], max_speed: f32) -> Vec2 {
    if others.is_empty() || max_speed <= 0.0 {
        return Vec2::ZERO;
    }
    let avg = others.iter().sum::<Vec2>() / others.len() as f32;
    if avg.length_squared() <= f32::EPSILON {
        return Vec2::ZERO;
    }
    avg.normalize() * max_speed - vel
}

/// Steer toward the local center of mass.
pub fn cohesion(pos: Vec2, others: &[Vec2], max_speed: f32) -> Vec2 {
    if others.is_empty() {
        return Vec2::ZERO;
    }
    seek(
        pos,
        others.iter().sum::<Vec2>() / others.len() as f32,
        max_speed,
    )
}

/// Waypoint traversal end behavior.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PathMode {
    /// Stop at the last point.
    #[default]
    Once,
    /// Restart from the first point.
    Loop,
    /// Reverse direction at each end.
    PingPong,
}

/// Cursor over a waypoint polyline. Advance with [`follow_path`].
#[derive(Clone, PartialEq, Debug)]
pub struct PathCursor {
    pub points: Vec<Vec2>,
    pub idx: usize,
    /// Arrival radius per waypoint.
    pub radius: f32,
    pub mode: PathMode,
    dir: i32,
    done: bool,
}

impl PathCursor {
    pub fn new(points: Vec<Vec2>, radius: f32, mode: PathMode) -> Self {
        Self {
            points,
            idx: 0,
            radius: radius.max(0.01),
            mode,
            dir: 1,
            done: false,
        }
    }

    pub fn target(&self) -> Option<Vec2> {
        if self.done {
            return None;
        }
        self.points.get(self.idx).copied()
    }

    pub fn finished(&self) -> bool {
        self.done
    }

    fn advance(&mut self) {
        let n = self.points.len();
        if n <= 1 {
            self.done = true;
            return;
        }
        match self.mode {
            PathMode::Once => {
                if self.idx + 1 >= n {
                    self.done = true;
                } else {
                    self.idx += 1;
                }
            }
            PathMode::Loop => self.idx = (self.idx + 1) % n,
            PathMode::PingPong => {
                let next = self.idx as i32 + self.dir;
                if next >= n as i32 || next < 0 {
                    self.dir = -self.dir;
                    self.idx = (self.idx as i32 + self.dir).clamp(0, n as i32 - 1) as usize;
                } else {
                    self.idx = next as usize;
                }
            }
        }
    }
}

/// Seek the cursor target, advancing inside `radius`. Returns
/// (desired velocity, finished). Legs past the reached point arrive.
pub fn follow_path(c: &mut PathCursor, pos: Vec2, max_speed: f32) -> (Vec2, bool) {
    let Some(t) = c.target() else {
        return (Vec2::ZERO, true);
    };
    if pos.distance(t) <= c.radius {
        c.advance();
        let Some(t2) = c.target() else {
            return (Vec2::ZERO, true);
        };
        return (arrive(pos, t2, max_speed, c.radius * 4.0), c.finished());
    }
    (seek(pos, t, max_speed), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    #[test]
    fn seek_full_speed() {
        let v = seek(Vec2::ZERO, Vec2::X * 10.0, 5.0);
        assert!((v.length() - 5.0).abs() < 1e-6);
        assert_eq!(seek(Vec2::ZERO, Vec2::ZERO, 5.0), Vec2::ZERO);
    }

    #[test]
    fn arrive_brakes() {
        let far = arrive(Vec2::ZERO, Vec2::X * 10.0, 5.0, 2.0);
        assert!((far.length() - 5.0).abs() < 1e-6);
        let near = arrive(Vec2::ZERO, Vec2::X * 1.0, 5.0, 2.0);
        assert!((near.length() - 2.5).abs() < 1e-6);
    }

    #[test]
    fn pursue_leads_target() {
        let p = pursue(Vec2::ZERO, Vec2::X * 10.0, Vec2::X * 5.0, 5.0, 1.0);
        assert!(p.x > 0.0 && (p.length() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn wander_stays_unit() {
        let mut rng = SmallRng::seed_from_u64(1);
        for _ in 0..20 {
            let w = wander(&mut rng, Vec2::X, 0.5);
            assert!((w.length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn separation_pushes_away() {
        let s = separation(Vec2::ZERO, &[Vec2::X * 0.5], 2.0, 10.0);
        assert!(s.x < 0.0);
        assert_eq!(
            separation(Vec2::ZERO, &[Vec2::X * 5.0], 2.0, 10.0),
            Vec2::ZERO
        );
    }

    #[test]
    fn flock_helpers() {
        let others = [Vec2::X, Vec2::X];
        assert!(align(Vec2::ZERO, &others, 4.0).x > 0.0);
        assert!(cohesion(Vec2::ZERO, &others, 4.0).x > 0.0);
        assert_eq!(align(Vec2::ZERO, &[], 4.0), Vec2::ZERO);
    }

    #[test]
    fn path_once_then_done() {
        let mut c = PathCursor::new(vec![Vec2::X * 10.0, Vec2::X * 20.0], 1.0, PathMode::Once);
        let (v, done) = follow_path(&mut c, Vec2::ZERO, 5.0);
        assert!(v.x > 0.0 && !done);
        let (_, done) = follow_path(&mut c, Vec2::X * 10.0, 5.0);
        assert!(!done && c.idx == 1);
        let (v, done) = follow_path(&mut c, Vec2::X * 20.0, 5.0);
        assert!(done && v == Vec2::ZERO);
    }

    #[test]
    fn path_loop_and_pingpong() {
        let mut c = PathCursor::new(vec![Vec2::ZERO, Vec2::X], 10.0, PathMode::Loop);
        follow_path(&mut c, Vec2::ZERO, 1.0);
        assert_eq!(c.idx, 1);
        follow_path(&mut c, Vec2::X, 1.0);
        assert_eq!(c.idx, 0);
        let mut p = PathCursor::new(vec![Vec2::ZERO, Vec2::X], 10.0, PathMode::PingPong);
        follow_path(&mut p, Vec2::ZERO, 1.0);
        assert_eq!((p.idx, p.finished()), (1, false));
        follow_path(&mut p, Vec2::X, 1.0);
        assert_eq!(p.idx, 0);
    }
}
