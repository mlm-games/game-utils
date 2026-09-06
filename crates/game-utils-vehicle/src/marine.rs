//! Displacement hulls on [`BodyState`]: multi-point buoyancy, water
//! drag, rudder, thrust. Sample points above water go quiet.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::body::{BodyState, RigidConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoatConfig {
    pub body: RigidConfig,
    /// Still-water height (m, world Y).
    pub water_height: f32,
    /// Hull sample offsets (body-local). Four corners is typical.
    pub sample_points: Vec<Vec3>,
    /// Buoyancy spring per point (N/m of submersion).
    pub buoyancy: f32,
    /// Water damping per point (N per m/s).
    pub water_damping: f32,
    /// Hull drag coefficients (forward, lateral).
    pub drag_forward: f32,
    pub drag_lateral: f32,
    /// Max thrust (N) at full throttle.
    pub max_thrust: f32,
    /// Rudder yaw moment (N m per unit demand at speed).
    pub rudder_moment: f32,
    /// Reference speed where the rudder fully bites (m/s).
    pub rudder_speed: f32,
    pub angular_damping: f32,
}

impl Default for BoatConfig {
    fn default() -> Self {
        Self {
            body: RigidConfig {
                mass: 800.0,
                inertia: Vec3::new(2500.0, 3000.0, 4000.0),
                com_offset: Vec3::ZERO,
            },
            water_height: 0.0,
            sample_points: vec![
                Vec3::new(-0.9, -0.2, -1.8),
                Vec3::new(0.9, -0.2, -1.8),
                Vec3::new(-0.9, -0.2, 1.8),
                Vec3::new(0.9, -0.2, 1.8),
            ],
            buoyancy: 12000.0,
            water_damping: 2500.0,
            drag_forward: 400.0,
            drag_lateral: 2500.0,
            max_thrust: 4000.0,
            rudder_moment: 6000.0,
            rudder_speed: 4.0,
            angular_damping: 1.5,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct BoatControls {
    /// -1 full reverse, +1 full ahead.
    pub throttle: f32,
    /// Rudder demand [-1, 1].
    pub rudder: f32,
}

impl BoatControls {
    pub fn clamp(&mut self) {
        self.throttle = self.throttle.clamp(-1.0, 1.0);
        self.rudder = self.rudder.clamp(-1.0, 1.0);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BoatState {
    pub body: BodyState,
    /// Mean submersion depth across sample points (m).
    pub submersion: f32,
}

impl BoatState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn speed(&self) -> f32 {
        self.body.speed()
    }

    pub fn step(&mut self, cfg: &BoatConfig, controls: &BoatControls, dt: f32) {
        if dt <= 0.0 || cfg.sample_points.is_empty() {
            return;
        }
        let mut ctl = *controls;
        ctl.clamp();

        let mut force = Vec3::ZERO;
        let mut torque = Vec3::ZERO;
        let mut depth_sum = 0.0;
        let n = cfg.sample_points.len() as f32;
        for local in &cfg.sample_points {
            let world = self.body.to_world(*local);
            let depth = cfg.water_height - world.y;
            if depth <= 0.0 {
                continue;
            }
            depth_sum += depth;
            // Buoyancy + vertical water damping at the sample point.
            let r = world - self.body.pos;
            let v_point = self.body.vel + self.body.ang_vel.cross(r);
            let lift = Vec3::Y * depth * cfg.buoyancy.max(0.0)
                - Vec3::Y * v_point.y * cfg.water_damping.max(0.0);
            force += lift;
            torque += r.cross(lift);
        }
        self.submersion = depth_sum / n;

        // Hull drag in body frame (only the submerged part bites).
        let wet = (self.submersion / 0.4).clamp(0.0, 1.0);
        let local_v = self.body.local_velocity();
        let drag_local = Vec3::new(
            -local_v.x * cfg.drag_lateral.max(0.0),
            0.0,
            -local_v.z * cfg.drag_forward.max(0.0),
        ) * wet.max(0.15);
        force += self.body.dir_to_world(drag_local);

        // Thrust + rudder (both fade when dry).
        let fwd_speed = self.body.forward_speed();
        force += self.body.forward() * ctl.throttle * cfg.max_thrust.max(0.0) * wet.max(0.2);
        let bite = (fwd_speed.abs() / cfg.rudder_speed.max(0.5)).clamp(0.0, 1.0);
        torque += self.body.up()
            * ctl.rudder
            * cfg.rudder_moment.max(0.0)
            * bite
            * wet.max(0.2)
            * fwd_speed.signum().max(0.1);

        // Gravity + angular damping.
        force += Vec3::new(0.0, -9.81 * cfg.body.mass.max(1.0), 0.0);
        torque += -self.body.ang_vel * cfg.angular_damping.max(0.0) * cfg.body.mass.max(1.0);

        self.body.integrate(&cfg.body, force, torque, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boat_floats_level() {
        let cfg = BoatConfig::default();
        let mut b = BoatState::new();
        b.body.pos.y = 0.0;
        let idle = BoatControls::default();
        for _ in 0..1200 {
            b.step(&cfg, &idle, 1.0 / 60.0);
        }
        // Settles at the waterline, upright, still.
        assert!(b.body.pos.y.abs() < 0.3, "y = {}", b.body.pos.y);
        assert!(b.body.up().y > 0.95);
        assert!(b.body.speed() < 0.5, "v = {}", b.body.speed());
        assert!(b.submersion > 0.0);
    }

    #[test]
    fn boat_thrust_and_rudder() {
        let cfg = BoatConfig::default();
        let mut b = BoatState::new();
        b.body.pos.y = 0.0;
        let go = BoatControls {
            throttle: 1.0,
            rudder: 0.0,
        };
        for _ in 0..600 {
            b.step(&cfg, &go, 1.0 / 60.0);
        }
        assert!(
            b.body.forward_speed() > 2.0,
            "fwd = {}",
            b.body.forward_speed()
        );
        let turn = BoatControls {
            throttle: 0.5,
            rudder: 1.0,
        };
        let yaw0 = b.body.ang_vel.length();
        for _ in 0..300 {
            b.step(&cfg, &turn, 1.0 / 60.0);
        }
        assert!(b.body.ang_vel.length() > yaw0);
    }

    #[test]
    fn boat_rights_after_heel() {
        let cfg = BoatConfig::default();
        let mut b = BoatState::new();
        b.body.pos.y = 0.0;
        // Heeled 20 degrees.
        b.body.orient = glam::Quat::from_rotation_z(0.35) * b.body.orient;
        let idle = BoatControls::default();
        for _ in 0..1200 {
            b.step(&cfg, &idle, 1.0 / 60.0);
        }
        assert!(b.body.up().y > 0.9, "up = {}", b.body.up());
    }
}
