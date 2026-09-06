//! Shared 3D rigid-body core: semi-implicit Euler over [`BodyState`].
//! Subsystems compute world-space forces; this applies them.

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Mass properties: mass (kg), diagonal inertia (kg m^2), and the
/// center of mass relative to the body origin (m).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RigidConfig {
    pub mass: f32,
    pub inertia: Vec3,
    pub com_offset: Vec3,
}

impl Default for RigidConfig {
    fn default() -> Self {
        Self {
            mass: 1200.0,
            inertia: Vec3::new(1500.0, 2200.0, 2200.0),
            com_offset: Vec3::ZERO,
        }
    }
}

/// World-space rigid-body state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BodyState {
    pub pos: Vec3,
    pub vel: Vec3,
    pub orient: Quat,
    /// Angular velocity in world frame (rad/s).
    pub ang_vel: Vec3,
}

impl Default for BodyState {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            orient: Quat::IDENTITY,
            ang_vel: Vec3::ZERO,
        }
    }
}

impl BodyState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Body axes in world space: +X right, +Y up, -Z forward.
    pub fn right(&self) -> Vec3 {
        self.orient * Vec3::X
    }

    pub fn up(&self) -> Vec3 {
        self.orient * Vec3::Y
    }

    pub fn forward(&self) -> Vec3 {
        self.orient * Vec3::NEG_Z
    }

    /// Velocity expressed in body frame.
    pub fn local_velocity(&self) -> Vec3 {
        self.orient.inverse() * self.vel
    }

    pub fn speed(&self) -> f32 {
        self.vel.length()
    }

    /// Signed forward speed (m/s).
    pub fn forward_speed(&self) -> f32 {
        self.vel.dot(self.forward())
    }

    /// Transform a body-local point to world space.
    pub fn to_world(&self, local: Vec3) -> Vec3 {
        self.pos + self.orient * local
    }

    /// Transform a body-local direction to world space.
    pub fn dir_to_world(&self, local: Vec3) -> Vec3 {
        self.orient * local
    }

    /// Apply `force` (N, world) at the body origin plus a pure
    /// `torque` (N m, world, about the center of mass) for `dt`
    /// seconds. Wheel (and other off-center) forces must already be
    /// folded into `torque` as `(point - pos).cross(force)` by the
    /// caller. Non-positive `dt` is a no-op.
    pub fn integrate(&mut self, cfg: &RigidConfig, force: Vec3, torque: Vec3, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        // Never let a bad subsystem poison the body with NaN/inf.
        let force = if force.is_finite() { force } else { Vec3::ZERO };
        let torque = if torque.is_finite() {
            torque
        } else {
            Vec3::ZERO
        };
        let mass = cfg.mass.max(1.0);
        let com = self.pos + self.orient * cfg.com_offset;
        // Torque about the center of mass.
        let r = self.pos - com;
        let total_torque = torque + r.cross(force);
        let inertia = Vec3::new(
            cfg.inertia.x.max(1.0),
            cfg.inertia.y.max(1.0),
            cfg.inertia.z.max(1.0),
        );
        // Diagonal inertia is body-aligned; rotate into world frame.
        let world_torque = self.orient.inverse() * total_torque;
        let local_alpha = world_torque / inertia;
        let alpha = self.orient * local_alpha;
        self.ang_vel += alpha * dt;
        // Orientation update via normalized quaternion step.
        let half_dt = 0.5 * dt;
        let dq = Quat::from_xyzw(
            self.ang_vel.x * half_dt,
            self.ang_vel.y * half_dt,
            self.ang_vel.z * half_dt,
            1.0,
        )
        .normalize();
        self.orient = (dq * self.orient).normalize();
        self.vel += force / mass * dt;
        self.pos += self.vel * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_falls_and_spins() {
        let cfg = RigidConfig::default();
        let mut b = BodyState::new();
        b.integrate(&cfg, Vec3::new(0.0, -9.81 * cfg.mass, 0.0), Vec3::ZERO, 1.0);
        assert!((b.vel.y + 9.81).abs() < 1e-4);
        assert!((b.pos.y + 9.81).abs() < 1e-4);
        b.integrate(&cfg, Vec3::ZERO, Vec3::new(0.0, 2200.0, 0.0), 1.0);
        assert!((b.ang_vel.y - 1.0).abs() < 1e-4);
    }

    #[test]
    fn body_offcenter_force_yaws() {
        let cfg = RigidConfig::default();
        let mut b = BodyState::new();
        let point = b.pos + Vec3::new(1.0, 0.0, 0.0);
        let force = Vec3::new(0.0, 0.0, -1000.0);
        let torque = (point - b.pos).cross(force);
        b.integrate(&cfg, force, torque, 0.016);
        // Force ahead of com about +X with -Z push yaws around Y.
        assert!(b.ang_vel.y.abs() > 0.0);
        assert!(b.orient.is_normalized());
    }

    #[test]
    fn body_axes_convention() {
        let b = BodyState::new();
        assert_eq!(b.forward(), Vec3::NEG_Z);
        assert_eq!(b.up(), Vec3::Y);
        assert_eq!(b.right(), Vec3::X);
    }
}
