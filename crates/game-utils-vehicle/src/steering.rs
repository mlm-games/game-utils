use serde::{Deserialize, Serialize};

/// Steering geometry. Limit halves linearly up to `falloff_speed`;
/// 0 disables falloff.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SteeringConfig {
    pub max_angle_rad: f32,
    pub steer_speed: f32,
    pub falloff_speed: f32,
}

impl Default for SteeringConfig {
    fn default() -> Self {
        Self {
            max_angle_rad: 0.65,
            steer_speed: 5.0,
            falloff_speed: 30.0,
        }
    }
}

impl SteeringConfig {
    pub fn without_falloff(max_angle_rad: f32, steer_speed: f32) -> Self {
        Self {
            max_angle_rad,
            steer_speed,
            falloff_speed: 0.0,
        }
    }

    /// Angle limit at the given forward speed.
    pub fn limit_at(&self, speed: f32) -> f32 {
        if self.falloff_speed <= 0.0 {
            return self.max_angle_rad;
        }
        let t = (speed / self.falloff_speed).clamp(0.0, 1.0);
        self.max_angle_rad * (1.0 - 0.5 * t)
    }

    pub fn target_angle(&self, input_steer: f32) -> f32 {
        input_steer.clamp(-1.0, 1.0) * self.max_angle_rad
    }

    pub fn target_angle_at(&self, input_steer: f32, speed: f32) -> f32 {
        input_steer.clamp(-1.0, 1.0) * self.limit_at(speed)
    }

    /// Rate-limited approach of `current` toward `target`.
    pub fn step(&self, current: f32, target: f32, dt: f32) -> f32 {
        if dt <= 0.0 {
            return current;
        }
        let max_step = self.steer_speed.max(0.0) * dt;
        let diff = target - current;
        if diff.abs() <= max_step {
            target
        } else {
            current + max_step * diff.signum()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steering_target_and_step() {
        let c = SteeringConfig::default();
        assert!((c.target_angle(1.0) - c.max_angle_rad).abs() < 1e-6);
        assert!((c.target_angle(-1.0) + c.max_angle_rad).abs() < 1e-6);
        let s = c.step(0.0, c.max_angle_rad, 0.1);
        assert!(s > 0.0 && s <= c.max_angle_rad);
        assert_eq!(c.step(0.2, 0.2, 0.1), 0.2);
        assert_eq!(c.step(0.1, 0.9, 0.0), 0.1);
    }

    #[test]
    fn steering_falloff() {
        let c = SteeringConfig::default();
        assert!((c.limit_at(0.0) - c.max_angle_rad).abs() < 1e-6);
        assert!((c.limit_at(c.falloff_speed) - c.max_angle_rad * 0.5).abs() < 1e-6);
        assert!((c.limit_at(1e6) - c.max_angle_rad * 0.5).abs() < 1e-6);
        let flat = SteeringConfig::without_falloff(0.5, 5.0);
        assert_eq!(flat.limit_at(100.0), 0.5);
    }
}
