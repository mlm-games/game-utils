//! Racing-line AI: pure-pursuit steering plus curvature braking over
//! a game-supplied [`Path`]. Returns steer demand and target speed.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// A drivable line: ordered points plus loop flag. Bake curves down
/// so segment projection stays smooth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Path {
    pub points: Vec<Vec3>,
    pub looped: bool,
}

impl Path {
    pub fn new(points: Vec<Vec3>, looped: bool) -> Self {
        Self { points, looped }
    }

    pub fn is_empty(&self) -> bool {
        self.points.len() < 2
    }

    /// Total length (m).
    pub fn length(&self) -> f32 {
        if self.points.len() < 2 {
            return 0.0;
        }
        let mut total = 0.0;
        for w in self.points.windows(2) {
            total += w[0].distance(w[1]);
        }
        if self.looped {
            total += self.points[self.points.len() - 1].distance(self.points[0]);
        }
        total
    }

    /// Closest point on the path: (arclength s, point, tangent).
    /// Linear scan; bake or spatially index for huge tracks.
    pub fn project(&self, pos: Vec3) -> Option<(f32, Vec3, Vec3)> {
        if self.points.len() < 2 {
            return None;
        }
        let n = self.points.len();
        let segs = if self.looped { n } else { n - 1 };
        let mut best: Option<(f32, f32, Vec3, Vec3)> = None; // (dist^2, s, point, tangent)
        let mut s = 0.0;
        for i in 0..segs {
            let a = self.points[i];
            let b = self.points[(i + 1) % n];
            let ab = b - a;
            let len = ab.length();
            if len < 1e-6 {
                continue;
            }
            let t = ((pos - a).dot(ab) / (len * len)).clamp(0.0, 1.0);
            let p = a + ab * t;
            let d2 = pos.distance_squared(p);
            if best.is_none_or(|(bd2, _, _, _)| d2 < bd2) {
                best = Some((d2, s + len * t, p, ab / len));
            }
            s += len;
        }
        best.map(|(_, bs, p, t)| (bs, p, t))
    }

    /// Point `ds` meters along the path from arclength `s` (wraps when
    /// looped, clamps otherwise).
    pub fn point_at(&self, s: f32, ds: f32) -> Option<Vec3> {
        if self.points.len() < 2 {
            return None;
        }
        let total = self.length();
        if total <= 0.0 {
            return None;
        }
        let mut target = s + ds;
        if self.looped {
            target = target.rem_euclid(total);
        } else {
            target = target.clamp(0.0, total);
        }
        let n = self.points.len();
        let segs = if self.looped { n } else { n - 1 };
        let mut acc = 0.0;
        for i in 0..segs {
            let a = self.points[i];
            let b = self.points[(i + 1) % n];
            let len = a.distance(b);
            if acc + len >= target {
                let t = if len > 1e-6 {
                    (target - acc) / len
                } else {
                    0.0
                };
                return Some(a + (b - a) * t.clamp(0.0, 1.0));
            }
            acc += len;
        }
        Some(self.points[n - 1])
    }

    /// Unsigned curvature (1/m) near arclength `s`, from the angle
    /// at the nearest vertex. Coarse paths give coarse answers -
    /// bake curves densely for smooth AI braking.
    pub fn curvature_at(&self, s: f32) -> f32 {
        let n = self.points.len();
        if n < 3 {
            return 0.0;
        }
        let anchor = match self.point_at(s, 0.0) {
            Some(p) => p,
            None => return 0.0,
        };
        // Nearest vertex (wrapping when looped).
        let mut best = 0;
        let mut best_d2 = f32::MAX;
        for (i, p) in self.points.iter().enumerate() {
            let d2 = anchor.distance_squared(*p);
            if d2 < best_d2 {
                best_d2 = d2;
                best = i;
            }
        }
        let prev = if best == 0 {
            if self.looped { n - 1 } else { return 0.0 }
        } else {
            best - 1
        };
        let next = if best + 1 >= n {
            if self.looped { 0 } else { return 0.0 }
        } else {
            best + 1
        };
        let v1 = self.points[best] - self.points[prev];
        let v2 = self.points[next] - self.points[best];
        let l1 = v1.length();
        let l2 = v2.length();
        if l1 < 1e-4 || l2 < 1e-4 {
            return 0.0;
        }
        let cos = (v1.dot(v2) / (l1 * l2)).clamp(-1.0, 1.0);
        cos.acos() / ((l1 + l2) * 0.5)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AiConfig {
    /// Pursuit distance (m) ahead on the line.
    pub lookahead: f32,
    /// Slow for curvature: v = sqrt(lat_accel / kappa).
    pub lateral_accel: f32,
    /// Absolute cap (m/s).
    pub top_speed: f32,
    /// Comfortable braking decel (m/s^2) for the braking-distance check.
    pub brake_accel: f32,
    /// Off-line slowdown per meter of lateral error (0 = none).
    pub off_line_penalty: f32,
    /// Track half-width for the off-line term (m).
    pub half_width: f32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            lookahead: 10.0,
            lateral_accel: 30.0,
            top_speed: 40.0,
            brake_accel: 8.0,
            off_line_penalty: 0.15,
            half_width: 7.0,
        }
    }
}

/// One AI decision: steer demand [-1, 1] and target speed (m/s).
#[derive(Debug, Clone, Copy)]
pub struct AiOutput {
    pub steer: f32,
    pub target_speed: f32,
    /// Lateral distance from the line (m, signed by side).
    pub lateral: f32,
}

impl AiOutput {
    /// Split speed error into pedals: full throttle below target,
    /// full brake when well above, coast in between.
    pub fn pedals(&self, speed: f32) -> (f32, f32) {
        let err = self.target_speed - speed;
        if err > 1.0 {
            (1.0, 0.0)
        } else if err < -2.0 {
            (0.0, 1.0)
        } else {
            (0.0, 0.0)
        }
    }
}

/// Pure-pursuit on `path`: steer toward the lookahead point, target
/// the slowest curvature-limited speed within braking distance.
pub fn pursue(cfg: &AiConfig, path: &Path, pos: Vec3, fwd: Vec3, speed: f32) -> Option<AiOutput> {
    let (s, closest, tangent) = path.project(pos)?;
    let to_path = closest - pos;
    // Signed lateral error (left positive, Y-up).
    let lateral = to_path.cross(fwd).y;
    let look = path.point_at(s, cfg.lookahead.max(1.0))?;
    let to_look = look - pos;
    let dist = to_look.length().max(0.001);
    // Signed angle from nose to lookahead: steer demand.
    let cross_y = fwd.cross(to_look / dist).y;
    let dot = fwd.dot(to_look / dist).clamp(-1.0, 1.0);
    let angle = cross_y.atan2(dot);
    let steer = (angle / 0.5).clamp(-1.0, 1.0);

    // Slowest corner within braking distance: for a corner `vc`
    // at distance `d`, the fastest we may go now is
    // sqrt(vc^2 + 2*a*d). Take the minimum over samples.
    let scan = (speed * speed / (2.0 * cfg.brake_accel.max(0.5))).max(cfg.lookahead);
    let mut target = cfg.top_speed.max(0.0);
    let mut d = 0.0;
    while d <= scan {
        let kappa = path.curvature_at(s + d);
        if kappa > 1e-5 {
            let corner = (cfg.lateral_accel.max(0.0) / kappa).sqrt();
            let allowed = (corner * corner + 2.0 * cfg.brake_accel.max(0.5) * d).sqrt();
            target = target.min(allowed);
        }
        d += 5.0;
    }
    // Off-line penalty.
    let off = (lateral.abs() / cfg.half_width.max(0.5)).clamp(0.0, 2.0);
    target *= 1.0 - (cfg.off_line_penalty.max(0.0) * off).min(0.6);
    let _ = tangent;
    Some(AiOutput {
        steer,
        target_speed: target.max(0.0),
        lateral,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oval() -> Path {
        // Looped ellipse, y=0.
        let mut pts = Vec::new();
        for i in 0..16 {
            let a = i as f32 / 16.0 * std::f32::consts::TAU;
            pts.push(Vec3::new(a.cos() * 50.0, 0.0, a.sin() * 20.0));
        }
        Path::new(pts, true)
    }

    #[test]
    fn path_project_and_point() {
        let p = oval();
        assert!(p.length() > 200.0);
        let (s, closest, _t) = p.project(Vec3::new(50.0, 0.0, 0.0)).unwrap();
        assert!(closest.distance(Vec3::new(50.0, 0.0, 0.0)) < 3.0);
        let ahead = p.point_at(s, 10.0).unwrap();
        assert!(ahead.distance(closest) > 5.0);
        assert!(Path::new(vec![], false).project(Vec3::ZERO).is_none());
    }

    #[test]
    fn pursuit_straight_fast_corner_slow() {
        let cfg = AiConfig::default();
        let path = oval();
        // Major-axis ends are the tight corners of the ellipse.
        let (st, _, _) = path.project(Vec3::new(0.0, 0.0, 20.0)).unwrap();
        let k_gentle = path.curvature_at(st);
        let (sc, _, _) = path.project(Vec3::new(50.0, 0.0, 0.0)).unwrap();
        let k_tight = path.curvature_at(sc);
        assert!(k_tight > k_gentle);
        let fwd = Vec3::new(0.0, 0.0, 1.0);
        let fast = pursue(&cfg, &path, Vec3::new(0.0, 0.0, -20.0), fwd, 30.0).unwrap();
        assert!(fast.target_speed > 10.0);
        assert!(fast.steer.abs() <= 1.0);
        let (thr, brk) = fast.pedals(5.0);
        assert_eq!((thr, brk), (1.0, 0.0));
        let (thr, brk) = fast.pedals(100.0);
        assert_eq!((thr, brk), (0.0, 1.0));
    }
}
