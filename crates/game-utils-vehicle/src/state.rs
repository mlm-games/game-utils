//! Kinematic bicycle model. Simple single-track, documented units;
//! for prototyping, AI lookahead, and reconciliation.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::engine::{EngineConfig, EngineOutput};
use crate::input::VehicleInput;
use crate::steering::SteeringConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleConfig {
    /// Vehicle mass in kg.
    pub mass: f32,
    /// Wheelbase in meters.
    pub wheelbase: f32,
    pub engine: EngineConfig,
    pub steering: SteeringConfig,
    /// Peak service-brake force in newtons.
    pub max_brake_force: f32,
    /// Extra decel force in newtons at full handbrake.
    pub handbrake_force: f32,
    /// Drivetrain ratio: shaft torque (N m) -> wheel force (N).
    /// ~9 for a car. Reverse reuses it.
    pub drive_ratio: f32,
    /// Quadratic aerodynamic drag coefficient: `F = coef * v^2` (N).
    pub drag_coef: f32,
    /// Linear rolling resistance coefficient: `F = coef * v` (N).
    pub rolling_coef: f32,
    /// Engine rpm gained per m/s of vehicle speed (clutch-out coupling).
    pub rpm_per_speed: f32,
}

impl Default for VehicleConfig {
    fn default() -> Self {
        Self {
            mass: 1500.0,
            wheelbase: 2.7,
            engine: EngineConfig::default(),
            steering: SteeringConfig::default(),
            max_brake_force: 8000.0,
            handbrake_force: 6000.0,
            drive_ratio: 9.0,
            drag_coef: 0.3,
            rolling_coef: 20.0,
            rpm_per_speed: 20.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleState {
    pub pos: Vec2,
    pub vel: Vec2,
    pub heading_rad: f32,
    pub steer_rad: f32,
    pub rpm: f32,
}

impl Default for VehicleState {
    fn default() -> Self {
        Self {
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            heading_rad: 0.0,
            steer_rad: 0.0,
            rpm: 800.0,
        }
    }
}

impl VehicleState {
    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    /// Signed forward speed (positive = moving along heading).
    pub fn forward_speed(&self) -> f32 {
        let forward = Vec2::new(self.heading_rad.cos(), self.heading_rad.sin());
        self.vel.dot(forward)
    }

    pub fn is_moving(&self) -> bool {
        self.speed() > 0.1
    }

    /// Advance the simulation by `dt` seconds, updating in place.
    /// Non-positive `dt` is a no-op.
    pub fn step(&mut self, cfg: &VehicleConfig, input: &VehicleInput, dt: f32) {
        *self = self.predicted(cfg, input, dt);
    }

    /// Predicted state `dt` ahead, leaving `self` untouched.
    pub fn predicted(&self, cfg: &VehicleConfig, input: &VehicleInput, dt: f32) -> Self {
        if dt <= 0.0 {
            return self.clone();
        }
        let mut inp = *input;
        inp.clamp();

        let speed = self.speed();
        let target_steer = cfg.steering.target_angle_at(inp.steer, speed);
        let steer = cfg.steering.step(self.steer_rad, target_steer, dt);

        // Analog clutch slips drive: 0 = engaged, 1 = fully disengaged.
        let engagement = 1.0 - inp.clutch;
        let eng: EngineOutput = cfg.engine.eval(inp.throttle.max(0.0), self.rpm);
        let reverse = (-inp.throttle.min(0.0)) * cfg.engine.max_torque * cfg.drive_ratio;
        let drive_force = eng.torque * cfg.drive_ratio * engagement;

        let forward = Vec2::new(self.heading_rad.cos(), self.heading_rad.sin());
        let mass = cfg.mass.max(1.0);
        let drag = cfg.drag_coef * speed * speed + cfg.rolling_coef * speed;
        let brake_force = inp.brake * cfg.max_brake_force + inp.handbrake * cfg.handbrake_force;

        // Brakes oppose motion; drive pushes along heading.
        let motion_dir = if speed > 1e-4 {
            self.vel / speed
        } else {
            forward
        };
        let mut accel =
            forward * (drive_force - reverse) / mass - motion_dir * (drag + brake_force) / mass;
        // Brakes never reverse the vehicle by themselves: if the
        // brake-only update would flip or kill motion, stop dead.
        let vel_candidate = self.vel + accel * dt;
        if brake_force > 0.0
            && drive_force <= 0.0
            && reverse <= 0.0
            && vel_candidate.dot(self.vel) <= 0.0
        {
            accel = -self.vel / dt;
        }

        let vel = self.vel + accel * dt;
        let pos = self.pos + vel * dt;

        // Bicycle heading update from forward motion.
        let fwd_speed = vel.dot(forward);
        let heading = if fwd_speed.abs() > 0.1 {
            let turn_rate = steer.tan() * fwd_speed / cfg.wheelbase.max(0.1);
            self.heading_rad + turn_rate * dt
        } else {
            self.heading_rad
        };

        let rpm = (eng.rpm + vel.length() * cfg.rpm_per_speed)
            .clamp(cfg.engine.idle_rpm, cfg.engine.max_rpm);

        Self {
            pos,
            vel,
            heading_rad: heading,
            steer_rad: steer,
            rpm,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driving_input() -> VehicleInput {
        VehicleInput {
            throttle: 1.0,
            ..VehicleInput::neutral()
        }
    }

    #[test]
    fn step_moves_forward() {
        let cfg = VehicleConfig::default();
        let mut s = VehicleState {
            rpm: 3000.0,
            ..Default::default()
        };
        s.step(&cfg, &driving_input(), 0.1);
        assert!(s.speed() > 0.0);
        assert!(s.forward_speed() > 0.0);
    }

    #[test]
    fn predicted_leaves_self_untouched() {
        let cfg = VehicleConfig::default();
        let s = VehicleState::default();
        let future = s.predicted(&cfg, &driving_input(), 1.0);
        assert_eq!(s.speed(), 0.0);
        assert!(future.speed() > 0.0);
    }

    #[test]
    fn braking_stops_without_reversing() {
        let cfg = VehicleConfig::default();
        let mut s = VehicleState {
            vel: Vec2::new(10.0, 0.0),
            ..Default::default()
        };
        let brake = VehicleInput {
            brake: 1.0,
            ..VehicleInput::neutral()
        };
        for _ in 0..600 {
            s.step(&cfg, &brake, 1.0 / 60.0);
        }
        assert!(s.speed() < 0.05, "speed = {}", s.speed());
        assert!(s.forward_speed() >= -0.01);
    }

    #[test]
    fn reverse_throttle_moves_backward() {
        let cfg = VehicleConfig::default();
        let mut s = VehicleState::default();
        let rev = VehicleInput {
            throttle: -1.0,
            ..VehicleInput::neutral()
        };
        for _ in 0..120 {
            s.step(&cfg, &rev, 1.0 / 60.0);
        }
        assert!(s.forward_speed() < -0.5);
    }

    #[test]
    fn clutch_slips_drive() {
        let cfg = VehicleConfig::default();
        let engaged = VehicleState {
            rpm: 3000.0,
            ..Default::default()
        }
        .predicted(&cfg, &driving_input(), 0.5);
        let slipped = VehicleState {
            rpm: 3000.0,
            ..Default::default()
        }
        .predicted(
            &cfg,
            &VehicleInput {
                clutch: 1.0,
                ..driving_input()
            },
            0.5,
        );
        assert!(engaged.speed() > slipped.speed());
    }

    #[test]
    fn steering_changes_heading() {
        let cfg = VehicleConfig::default();
        let s = VehicleState {
            vel: Vec2::new(5.0, 0.0),
            ..Default::default()
        };
        let input = VehicleInput {
            throttle: 0.5,
            steer: 1.0,
            ..VehicleInput::neutral()
        };
        let s2 = s.predicted(&cfg, &input, 0.1);
        assert!(s2.heading_rad != s.heading_rad);
        assert!(s2.steer_rad != 0.0);
    }

    #[test]
    fn nonpositive_dt_is_noop() {
        let cfg = VehicleConfig::default();
        let s = VehicleState {
            vel: Vec2::new(3.0, 1.0),
            ..Default::default()
        };
        assert_eq!(s.predicted(&cfg, &driving_input(), 0.0).vel, s.vel);
    }
}
