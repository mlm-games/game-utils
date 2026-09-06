//! Fixed-wing flight on [`BodyState`]: thrust, stall lift, and
//! pressure-scaled controls. -Z forward; pitch/roll/yaw in [-1, 1].

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::body::{BodyState, RigidConfig};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlaneConfig {
    pub body: RigidConfig,
    /// Max static thrust (N) at full throttle.
    pub max_thrust: f32,
    /// Wing area (m^2).
    pub wing_area: f32,
    /// Lift coefficient slope region cap.
    pub lift_coef: f32,
    /// Zero-lift incidence (rad, usually slightly negative).
    pub zero_lift_angle: f32,
    /// Weathervane stability per radian of misalignment (0 = neutral).
    pub pitch_stability: f32,
    pub yaw_stability: f32,
    /// Stall angle of attack (rad), measured from zero-lift.
    pub stall_angle: f32,
    pub drag_coef: f32,
    pub frontal_area: f32,
    pub air_density: f32,
    /// Control moment rates (rad/s^2 per unit demand at full authority).
    pub pitch_rate: f32,
    pub roll_rate: f32,
    pub yaw_rate: f32,
    /// Rotary damping per unit dynamic pressure.
    pub rotary_damping: f32,
    /// Baseline angular damping (1/s, mass-scaled).
    pub angular_damping: f32,
}

impl Default for PlaneConfig {
    fn default() -> Self {
        Self {
            body: RigidConfig {
                mass: 1000.0,
                inertia: Vec3::new(3000.0, 4000.0, 5000.0),
                com_offset: Vec3::ZERO,
            },
            max_thrust: 9000.0,
            wing_area: 16.0,
            lift_coef: 1.3,
            zero_lift_angle: -0.08,
            pitch_stability: 0.35,
            yaw_stability: 0.25,
            stall_angle: 0.28,
            drag_coef: 0.05,
            frontal_area: 2.0,
            air_density: 1.225,
            pitch_rate: 2.2,
            roll_rate: 3.5,
            yaw_rate: 0.8,
            rotary_damping: 0.3,
            angular_damping: 1.2,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct PlaneControls {
    pub throttle: f32,
    pub pitch: f32,
    pub roll: f32,
    pub yaw: f32,
}

impl PlaneControls {
    pub fn clamp(&mut self) {
        self.throttle = self.throttle.clamp(0.0, 1.0);
        self.pitch = self.pitch.clamp(-1.0, 1.0);
        self.roll = self.roll.clamp(-1.0, 1.0);
        self.yaw = self.yaw.clamp(-1.0, 1.0);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaneState {
    pub body: BodyState,
}

impl PlaneState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn speed(&self) -> f32 {
        self.body.speed()
    }

    /// Angle of attack (rad). Positive with airflow from below.
    pub fn angle_of_attack(&self) -> f32 {
        let v = self.body.local_velocity();
        let fwd = -v.z;
        if fwd.abs() < 0.5 {
            return 0.0;
        }
        (-v.y / fwd).atan()
    }

    pub fn step(&mut self, cfg: &PlaneConfig, controls: &PlaneControls, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let mut ctl = *controls;
        ctl.clamp();
        let v = self.body.vel;
        let speed = v.length();
        let q = 0.5 * cfg.air_density.max(0.0) * speed * speed;

        // Thrust along the nose.
        let mut force = self.body.forward() * ctl.throttle * cfg.max_thrust.max(0.0);

        // Lift along body-up from effective incidence (aoa minus
        // zero-lift), fading past stall; induced drag spikes too.
        let aoa = self.angle_of_attack() - cfg.zero_lift_angle;
        let stall = cfg.stall_angle.max(0.05);
        let lift_scale = if aoa.abs() <= stall {
            (aoa / stall).clamp(-1.0, 1.0)
        } else {
            // Post-stall: partial lift, sign preserved.
            (stall / aoa.abs()).clamp(0.0, 1.0) * aoa.signum()
        };
        let lift = lift_scale * cfg.lift_coef.max(0.0) * q * cfg.wing_area.max(0.0);
        force += self.body.up() * lift;

        // Parasitic + stall drag opposing motion.
        let stall_drag = if aoa.abs() > stall {
            (aoa.abs() - stall) * 4.0
        } else {
            0.0
        };
        let drag = (cfg.drag_coef.max(0.0) * cfg.frontal_area.max(0.0) + stall_drag) * q;
        if speed > 0.01 {
            force += -v.normalize_or_zero() * drag;
        }
        // Gravity.
        force += Vec3::new(0.0, -9.81 * cfg.body.mass.max(1.0), 0.0);

        // Body axes: X = pitch axis, Y = yaw axis, Z = roll axis.
        // Control authority scales with dynamic pressure (no airflow =
        // no control), plus plain angular damping.
        let authority = (q / 500.0).clamp(0.0, 1.0);
        let local_moment = Vec3::new(
            ctl.pitch * cfg.pitch_rate,
            ctl.yaw * cfg.yaw_rate,
            ctl.roll * cfg.roll_rate,
        ) * authority
            * cfg.body.mass.max(1.0);
        // Weathervane stability: nose follows the airflow. Faded out
        // at low forward speed so tumbles don't pump energy.
        let lv = self.body.local_velocity();
        let fwd_factor = (lv.z.abs() / 5.0).clamp(0.0, 1.0);
        let beta = if lv.z.abs() > 0.5 {
            (lv.x / -lv.z).atan()
        } else {
            0.0
        };
        let stab = Vec3::new(
            -aoa * cfg.pitch_stability.max(0.0),
            -beta * cfg.yaw_stability.max(0.0),
            0.0,
        ) * q
            * cfg.wing_area.max(0.0)
            * fwd_factor;
        let local_w = self.body.orient.inverse() * self.body.ang_vel;
        let damp = -local_w
            * (cfg.angular_damping.max(0.0) * cfg.body.mass.max(1.0) * 0.5
                + q * cfg.wing_area.max(0.0) * cfg.rotary_damping.max(0.0));
        let torque = self.body.orient * (local_moment + stab + damp);

        self.body.integrate(&cfg.body, force, torque, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cruise() -> (PlaneConfig, PlaneState) {
        let cfg = PlaneConfig::default();
        let mut p = PlaneState::new();
        // Start at flying speed, level.
        p.body.vel = Vec3::new(0.0, 0.0, -60.0);
        (cfg, p)
    }

    #[test]
    fn plane_pull_up_climbs() {
        let (cfg, mut p) = cruise();
        // Brief pull, then release: nose comes up and altitude gains.
        let pull = PlaneControls {
            throttle: 0.6,
            pitch: 0.5,
            ..PlaneControls::default()
        };
        for _ in 0..45 {
            p.step(&cfg, &pull, 1.0 / 60.0);
        }
        let coast = PlaneControls {
            throttle: 0.6,
            ..PlaneControls::default()
        };
        let y0 = p.body.pos.y;
        for _ in 0..255 {
            p.step(&cfg, &coast, 1.0 / 60.0);
        }
        // Gentle maneuver, no lawn dart: roughly holds altitude.
        assert!(p.body.pos.y > y0 - 30.0, "y = {} (y0 = {y0})", p.body.pos.y);
        assert!(p.body.pos.y.is_finite());
        assert!(p.speed() < 400.0);
        assert!(p.body.orient.is_normalized());
    }

    #[test]
    fn plane_settles_after_disturbance() {
        let (cfg, mut p) = cruise();
        // Brief shove, then release: weathervane + damping must
        // recover without tumbling.
        let push = PlaneControls {
            throttle: 0.4,
            pitch: -0.4,
            ..PlaneControls::default()
        };
        for _ in 0..30 {
            p.step(&cfg, &push, 1.0 / 60.0);
        }
        let coast = PlaneControls {
            throttle: 0.4,
            ..PlaneControls::default()
        };
        for _ in 0..600 {
            p.step(&cfg, &coast, 1.0 / 60.0);
        }
        assert!(p.body.pos.y.is_finite());
        assert!(p.body.up().y > 0.0, "up = {}", p.body.up());
        assert!(p.body.ang_vel.length() < 2.0, "w = {}", p.body.ang_vel);
    }

    #[test]
    fn plane_stalls_when_slow() {
        let (cfg, mut p) = cruise();
        p.body.vel = Vec3::new(0.0, 0.0, -5.0);
        let idle = PlaneControls::default();
        for _ in 0..300 {
            p.step(&cfg, &idle, 1.0 / 60.0);
        }
        // Slow + no thrust: sinks.
        assert!(p.body.vel.y < -1.0, "vy = {}", p.body.vel.y);
    }

    #[test]
    fn plane_controls_need_airflow() {
        let (cfg, mut p) = cruise();
        p.body.vel = Vec3::ZERO;
        let ctl = PlaneControls {
            roll: 1.0,
            ..PlaneControls::default()
        };
        p.step(&cfg, &ctl, 0.1);
        assert!(p.body.ang_vel.length() < 0.05);
    }
}
