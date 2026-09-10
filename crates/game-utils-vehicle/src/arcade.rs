//! Kinematic arcade model: scalar speed + heading, no gears or slip.
//! Steering is a yaw rate; surfaces scale behavior. Pair with
//! [`BoostPool`](crate::boost::BoostPool) / [`DriftState`](crate::drift::DriftState)
//! as needed - they stay separate on purpose.
use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::input::VehicleInput;

/// Surface response multipliers. 1.0 = nominal; lower models loose,
/// slick, or off-track ground.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SurfaceMod {
    pub accel_scale: f32,
    pub top_speed_scale: f32,
    pub turn_scale: f32,
}

impl Default for SurfaceMod {
    fn default() -> Self {
        Self::road()
    }
}

impl SurfaceMod {
    pub fn road() -> Self {
        Self {
            accel_scale: 1.0,
            top_speed_scale: 1.0,
            turn_scale: 1.0,
        }
    }

    pub fn scaled(accel: f32, top_speed: f32, turn: f32) -> Self {
        Self {
            accel_scale: accel.max(0.0),
            top_speed_scale: top_speed.max(0.0),
            turn_scale: turn.max(0.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArcadeConfig {
    /// Forward acceleration at full throttle (m/s^2).
    pub acceleration: f32,
    /// Reverse acceleration at full reverse (m/s^2).
    pub reverse_acceleration: f32,
    /// Braking decel (m/s^2).
    pub braking: f32,
    /// Top forward / reverse speeds (m/s).
    pub top_speed: f32,
    pub reverse_speed: f32,
    /// Yaw rate at full steer and full speed (rad/s).
    pub turn_rate: f32,
    /// How fast speed bleeds off with no input (1/s, exponential).
    pub coast_drag: f32,
}

impl Default for ArcadeConfig {
    fn default() -> Self {
        Self {
            acceleration: 12.0,
            reverse_acceleration: 6.0,
            braking: 18.0,
            top_speed: 28.0,
            reverse_speed: 8.0,
            turn_rate: 2.2,
            coast_drag: 0.6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArcadeState {
    pub pos: Vec2,
    pub heading_rad: f32,
    /// Signed scalar speed along heading (m/s).
    pub speed: f32,
}

impl ArcadeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn velocity(&self) -> Vec2 {
        Vec2::new(self.heading_rad.cos(), self.heading_rad.sin()) * self.speed
    }

    /// Advance by `dt`. `boost_speed_mult` / `boost_accel` come from a
    /// [`crate::boost::BoostPool`] when one is in play, else 1.0 / 0.0.
    /// Non-positive `dt` is a no-op.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        cfg: &ArcadeConfig,
        input: &VehicleInput,
        surface: &SurfaceMod,
        boost_speed_mult: f32,
        boost_accel: f32,
        dt: f32,
    ) {
        if dt <= 0.0 {
            return;
        }
        let mut inp = *input;
        inp.clamp();

        let top = cfg.top_speed * surface.top_speed_scale * boost_speed_mult.max(1.0);
        let rev_top = cfg.reverse_speed * surface.top_speed_scale;

        if inp.throttle > 0.0 {
            self.speed +=
                (inp.throttle * cfg.acceleration * surface.accel_scale + boost_accel) * dt;
        } else if inp.throttle < 0.0 {
            if self.speed > 0.5 {
                self.speed -= -inp.throttle * cfg.braking * surface.accel_scale * dt;
            } else {
                self.speed += inp.throttle * cfg.reverse_acceleration * surface.accel_scale * dt;
            }
        } else {
            self.speed -= self.speed * cfg.coast_drag.min(10.0) * dt;
            if self.speed.abs() < 0.01 {
                self.speed = 0.0;
            }
        }

        let brake_decel = inp.brake * cfg.braking * dt + inp.handbrake * cfg.braking * 1.5 * dt;
        if self.speed > 0.0 {
            self.speed = (self.speed - brake_decel).max(0.0);
        } else if self.speed < 0.0 {
            self.speed = (self.speed + brake_decel).min(0.0);
        }

        self.speed = self.speed.clamp(-rev_top.max(0.0), top.max(0.0));

        // Yaw authority grows with speed, saturating at ~40% of top.
        let authority = (self.speed.abs() / (top * 0.4).max(1.0)).clamp(0.0, 1.0);
        // Reversing flips the yaw direction, like real vehicles.
        let dir = if self.speed >= 0.0 { 1.0 } else { -1.0 };
        self.heading_rad += dir * inp.steer * cfg.turn_rate * surface.turn_scale * authority * dt;

        let forward = Vec2::new(self.heading_rad.cos(), self.heading_rad.sin());
        self.pos += forward * self.speed * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_throttle() -> VehicleInput {
        VehicleInput {
            throttle: 1.0,
            ..VehicleInput::neutral()
        }
    }

    #[test]
    fn arcade_accelerates_and_caps() {
        let cfg = ArcadeConfig::default();
        let mut s = ArcadeState::new();
        for _ in 0..600 {
            s.step(
                &cfg,
                &full_throttle(),
                &SurfaceMod::road(),
                1.0,
                0.0,
                1.0 / 60.0,
            );
        }
        assert!((s.speed - cfg.top_speed).abs() < 0.5, "speed = {}", s.speed);
    }

    #[test]
    fn arcade_surface_slows() {
        let cfg = ArcadeConfig::default();
        let mud = SurfaceMod::scaled(0.5, 0.5, 0.7);
        let mut road = ArcadeState::new();
        let mut muddy = ArcadeState::new();
        for _ in 0..600 {
            road.step(
                &cfg,
                &full_throttle(),
                &SurfaceMod::road(),
                1.0,
                0.0,
                1.0 / 60.0,
            );
            muddy.step(&cfg, &full_throttle(), &mud, 1.0, 0.0, 1.0 / 60.0);
        }
        assert!(muddy.speed < road.speed);
    }

    #[test]
    fn arcade_boost_raises_cap() {
        let cfg = ArcadeConfig::default();
        let mut s = ArcadeState::new();
        for _ in 0..1200 {
            s.step(
                &cfg,
                &full_throttle(),
                &SurfaceMod::road(),
                1.5,
                8.0,
                1.0 / 60.0,
            );
        }
        assert!(s.speed > cfg.top_speed);
    }

    #[test]
    fn arcade_turns_only_when_moving() {
        let cfg = ArcadeConfig::default();
        let steer = VehicleInput {
            steer: 1.0,
            ..VehicleInput::neutral()
        };
        let mut parked = ArcadeState::new();
        parked.step(&cfg, &steer, &SurfaceMod::road(), 1.0, 0.0, 0.5);
        assert_eq!(parked.heading_rad, 0.0);
        let mut rolling = ArcadeState {
            speed: 10.0,
            ..ArcadeState::new()
        };
        rolling.step(&cfg, &steer, &SurfaceMod::road(), 1.0, 0.0, 0.5);
        assert!(rolling.heading_rad > 0.0);
    }

    #[test]
    fn arcade_brake_holds_at_zero() {
        let cfg = ArcadeConfig::default();
        let brake = VehicleInput {
            brake: 1.0,
            ..VehicleInput::neutral()
        };
        let mut s = ArcadeState::new();
        s.step(&cfg, &brake, &SurfaceMod::road(), 1.0, 0.0, 1.0);
        assert_eq!(s.speed, 0.0);
    }
}
