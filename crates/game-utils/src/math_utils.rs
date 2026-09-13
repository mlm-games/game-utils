use glam::{Vec2, Vec3};

pub struct MathUtils;

impl MathUtils {
    pub fn smooth_damp(
        current: f32,
        target: f32,
        current_velocity: f32,
        smooth_time: f32,
        delta: f32,
    ) -> (f32, f32) {
        if !current.is_finite() || !target.is_finite() || !current_velocity.is_finite() {
            return (target, 0.0);
        }
        if !smooth_time.is_finite() || !delta.is_finite() || delta <= 0.0 {
            return (current, current_velocity);
        }
        let smooth_time = smooth_time.max(0.0001);
        let omega = 2.0 / smooth_time;
        let x = omega * delta;

        let exp = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
        let change = current - target;
        let temp = (current_velocity + omega * change) * delta;
        let new_velocity = (current_velocity - omega * temp) * exp;
        let output = target + (change + temp) * exp;
        (output, new_velocity)
    }

    pub fn approach(current: f32, target: f32, rate: f32) -> f32 {
        if !current.is_finite() || !target.is_finite() || !rate.is_finite() || rate < 0.0 {
            return current;
        }
        if current < target {
            (current + rate).min(target)
        } else {
            (current - rate).max(target)
        }
    }

    pub fn wave(from: f32, to: f32, duration: f32, offset: f32, time_secs: f32) -> f32 {
        if !duration.is_finite() || duration <= 0.0 {
            return from;
        }
        if !from.is_finite() || !to.is_finite() || !offset.is_finite() || !time_secs.is_finite() {
            return from;
        }
        let t = (time_secs + offset) / duration;
        from + (to - from) * (t * std::f32::consts::TAU).sin().mul_add(0.5, 0.5)
    }

    pub fn smooth_damp_vec2(
        current: Vec2,
        target: Vec2,
        velocity: &mut Vec2,
        smooth_time: f32,
        delta: f32,
    ) -> Vec2 {
        let (x, vx) = Self::smooth_damp(current.x, target.x, velocity.x, smooth_time, delta);
        let (y, vy) = Self::smooth_damp(current.y, target.y, velocity.y, smooth_time, delta);
        *velocity = Vec2::new(vx, vy);
        Vec2::new(x, y)
    }

    /// Per-axis approach: each axis moves up to `rate` toward the target,
    /// so diagonal travel covers up to √2 × `rate` per call. When `rate`
    /// is a speed, use Euclidean `move_towards`-style math instead.
    pub fn approach_vec2(current: Vec2, target: Vec2, rate: f32) -> Vec2 {
        Vec2::new(
            Self::approach(current.x, target.x, rate),
            Self::approach(current.y, target.y, rate),
        )
    }

    pub fn smooth_damp_vec3(
        current: Vec3,
        target: Vec3,
        velocity: &mut Vec3,
        smooth_time: f32,
        delta: f32,
    ) -> Vec3 {
        let (x, vx) = Self::smooth_damp(current.x, target.x, velocity.x, smooth_time, delta);
        let (y, vy) = Self::smooth_damp(current.y, target.y, velocity.y, smooth_time, delta);
        let (z, vz) = Self::smooth_damp(current.z, target.z, velocity.z, smooth_time, delta);
        *velocity = Vec3::new(vx, vy, vz);
        Vec3::new(x, y, z)
    }

    /// Cubic ease-out 0..1. Shared by hitstop recovery (Bevy + repame
    /// sim-time) so both ease with the same curve.
    pub fn ease_out_cubic(t: f32) -> f32 {
        let u = t.clamp(0.0, 1.0) - 1.0;
        u * u * u + 1.0
    }

    /// Ease the camera toward its follow target.
    ///
    /// Moves `current` toward `target` with exponential smoothing at `speed`
    /// world units per second: fast when far away, settling gently without
    /// overshooting. Large `speed` values approach a snap; the motion is
    /// frame-rate independent for a fixed `dt`.
    ///
    /// - `speed <= 0` (or non-finite) snaps directly to `target`.
    /// - `dt <= 0` holds `current` (a paused frame never moves the camera).
    pub fn smooth_toward_vec2(current: Vec2, target: Vec2, speed: f32, dt: f32) -> Vec2 {
        if dt <= 0.0 {
            return current;
        }
        if speed <= 0.0 || !speed.is_finite() {
            return target;
        }
        let t = 1.0 - (-speed * dt).exp();
        current + (target - current) * t
    }

    /// Back-eased pop-in scale (overshoots past 1.0, settles at 1.0).
    /// Renderer-free twin of the `game-utils-bevy` juice pop-in curve:
    /// same math, `glam` in/out, no `Transform` writes.
    pub fn pop_scale(t: f32) -> f32 {
        let overshoot = 1.70158;
        let t2 = t.clamp(0.0, 1.0) - 1.0;
        t2 * t2 * ((overshoot + 1.0) * t2 + overshoot) + 1.0
    }

    /// Squash-and-stretch XY scale: ramps to `amount` in the first half,
    /// relaxes back to 1.0 in the second half. Renderer-free twin of the
    /// `game-utils-bevy` squash-stretch curve.
    pub fn squash_stretch_xy(t: f32, amount: Vec2) -> Vec2 {
        let t = t.clamp(0.0, 1.0);
        if t < 0.5 {
            let u = t / 0.5;
            Vec2::new(1.0 + (amount.x - 1.0) * u, 1.0 + (amount.y - 1.0) * u)
        } else {
            let u = (t - 0.5) / 0.5;
            amount + (Vec2::ONE - amount) * u
        }
    }

    /// Bounce scale: peaks at 30% of the duration, relaxes to 1.0.
    /// Renderer-free twin of the `game-utils-bevy` bounce-scale curve.
    pub fn bounce_wave(t: f32, peak: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t < 0.3 {
            1.0 + (peak - 1.0) * (t / 0.3)
        } else {
            peak + (1.0 - peak) * ((t - 0.3) / 0.7)
        }
    }

    /// Decaying sinusoidal shake offset for `elapsed_secs` of wall time.
    /// Renderer-free twin of the `game-utils-bevy` shake curve.
    pub fn shake_offset(elapsed_secs: f32, intensity: f32, decay: f32) -> Vec2 {
        let d = decay.clamp(0.0, 1.0);
        Vec2::new(
            (elapsed_secs * 50.0).sin() * intensity * d,
            (elapsed_secs * 47.0).cos() * intensity * d,
        )
    }

    /// Overwrite a velocity with a directional knockback impulse.
    pub fn knockback(velocity: &mut Vec2, dir: Vec2, force: f32) {
        *velocity = dir.normalize_or_zero() * force;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_smooth_damp() {
        let (out, vel) = MathUtils::smooth_damp(0.0, 10.0, 0.0, 0.1, 0.016);
        assert!((out - 0.414).abs() < 0.01, "out={out}");
        assert!((vel - 46.47).abs() < 0.5, "vel={vel}");
        // non-finite guard golden
        let (o, v) = MathUtils::smooth_damp(f32::NAN, 10.0, 0.0, 0.1, 0.016);
        assert_eq!(o, 10.0);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn golden_approach() {
        assert_eq!(MathUtils::approach(0.0, 10.0, 2.0), 2.0);
        assert_eq!(MathUtils::approach(9.0, 10.0, 2.0), 10.0);
        assert_eq!(MathUtils::approach(10.0, 0.0, 2.0), 8.0);
        assert!(MathUtils::approach(f32::NAN, 10.0, 2.0).is_nan());
    }

    #[test]
    fn golden_wave() {
        assert!((MathUtils::wave(0.0, 10.0, 2.0, 0.0, 0.5) - 10.0).abs() < 1e-5);
        assert!((MathUtils::wave(0.0, 10.0, 2.0, 0.0, 1.0) - 5.0).abs() < 1e-5);
        assert_eq!(MathUtils::wave(0.0, 10.0, 0.0, 0.0, 1.0), 0.0);
    }

    #[test]
    fn golden_ease_out_cubic() {
        assert_eq!(MathUtils::ease_out_cubic(0.0), 0.0);
        assert_eq!(MathUtils::ease_out_cubic(1.0), 1.0);
        assert!(MathUtils::ease_out_cubic(0.5) > 0.5);
    }

    #[test]
    fn feel_curves_hold_shape() {
        assert!((MathUtils::pop_scale(1.0) - 1.0).abs() < 1e-4);
        assert!(MathUtils::pop_scale(0.5) > 1.0);
        let end = MathUtils::squash_stretch_xy(1.0, Vec2::new(1.3, 0.7));
        assert!((end - Vec2::ONE).length() < 1e-6);
        assert!((MathUtils::bounce_wave(1.0, 1.5) - 1.0).abs() < 1e-6);
        assert!(MathUtils::bounce_wave(0.15, 1.5) > 1.0);
        let off = MathUtils::shake_offset(1.0, 10.0, 0.0);
        assert_eq!(off, Vec2::ZERO);
        let mut v = Vec2::ZERO;
        MathUtils::knockback(&mut v, Vec2::X, 5.0);
        assert_eq!(v, Vec2::new(5.0, 0.0));
    }

    #[test]
    fn smooth_toward_converges() {
        let target = Vec2::new(100.0, 0.0);
        let p1 = MathUtils::smooth_toward_vec2(Vec2::ZERO, target, 5.0, 0.016);
        assert!(p1.x > 0.0 && p1.x < 100.0);
        let mut p = Vec2::ZERO;
        for _ in 0..600 {
            p = MathUtils::smooth_toward_vec2(p, target, 5.0, 0.016);
        }
        assert!((p - target).length() < 0.01, "got {p:?}");
        assert_eq!(
            MathUtils::smooth_toward_vec2(Vec2::ONE, target, 0.0, 0.016),
            target
        );
        assert_eq!(
            MathUtils::smooth_toward_vec2(Vec2::ONE, target, 5.0, 0.0),
            Vec2::ONE
        );
    }
}
