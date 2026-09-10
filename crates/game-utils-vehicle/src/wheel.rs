//! Wheel: probe, spring, tire forces, spin dynamics, TCS/ABS.
/// Slip is substepped to quasi-static equilibrium each body step.
use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::body::BodyState;
use crate::ground::{GroundHit, GroundProbe};
use crate::steering::SteeringConfig;
use crate::suspension::{SuspensionConfig, SuspensionState, anti_roll_force};
use crate::tire::TireConfig;

/// Sign that treats exact zero as zero (unlike `f32::signum`,
/// which maps +0.0 to 1.0 and would drag parked wheels).
fn static_sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Traction control: scales drive torque toward the slip target.
/// Control is proportional (`target / slip`), and the cut itself is
/// rate-limited (hydraulics and engine torque can't change instantly),
/// which is also what keeps ABS/TCS from juddering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TcsConfig {
    pub enabled: bool,
    pub slip_target: f32,
    /// Cut response rate (1/s).
    pub response: f32,
}

impl Default for TcsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            slip_target: 0.12,
            response: 25.0,
        }
    }
}

/// Anti-lock braking: scales brake torque toward the slip target,
/// with the same rate-limited cut as [`TcsConfig`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AbsConfig {
    pub enabled: bool,
    pub slip_target: f32,
    /// Cut response rate (1/s).
    pub response: f32,
}

impl Default for AbsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            slip_target: 0.15,
            response: 25.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelConfig {
    pub tire: TireConfig,
    pub suspension: SuspensionConfig,
    /// Body-local mount point (suspension top).
    pub mount: Vec3,
    pub steered: bool,
    /// Multiplier on steer demand (1 = front, negative = rear steer).
    pub steer_ratio: f32,
    /// Per-wheel Ackermann correction (0 = none).
    pub ackermann: f32,
    pub driven: bool,
    /// Share of service-brake force.
    pub brake_bias: f32,
    pub handbrake: bool,
    /// Spin inertia (kg m^2).
    pub inertia: f32,
    /// Friction multiplier (assist cheat, 1 = none).
    pub friction_help: f32,
}

impl Default for WheelConfig {
    fn default() -> Self {
        Self {
            tire: TireConfig::default(),
            suspension: SuspensionConfig::default(),
            mount: Vec3::ZERO,
            steered: false,
            steer_ratio: 1.0,
            ackermann: 0.0,
            driven: false,
            brake_bias: 0.5,
            handbrake: false,
            inertia: 1.2,
            friction_help: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WheelState {
    /// Spin rate (rad/s, positive = rolling forward).
    pub spin: f32,
    pub steer: f32,
    pub susp: SuspensionState,
    pub slip_ratio: f32,
    pub slip_angle: f32,
    /// Last normal load (N).
    pub load: f32,
    pub surface: u32,
    /// 0..1 fraction of drive torque actually applied (1 = no TCS cut).
    pub tcs_activity: f32,
    /// 0..1 fraction of brake torque actually applied (1 = no ABS cut).
    pub abs_activity: f32,
}

/// Contact force result for body integration.
#[derive(Debug, Clone, Copy)]
pub struct WheelForce {
    pub force: Vec3,
    pub point: Vec3,
}

impl WheelState {
    /// Advance one wheel. Returns world-space contact force, or `None`
    /// airborne. Torques: post-diff drive, contact brake/handbrake.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        cfg: &WheelConfig,
        body: &BodyState,
        steering: &SteeringConfig,
        steer_demand: f32,
        drive_torque: f32,
        brake_force: f32,
        handbrake_force: f32,
        tcs: &TcsConfig,
        abs: &AbsConfig,
        probe: &impl GroundProbe,
        dt: f32,
    ) -> Option<WheelForce> {
        // Steering with Ackermann: inner wheel turns tighter.
        let target = if cfg.steered {
            let base = steering.target_angle(steer_demand) * cfg.steer_ratio;
            base * (1.0 + cfg.ackermann * steer_demand.signum() * steer_demand.abs())
        } else {
            0.0
        };
        self.steer = steering.step(self.steer, target, dt);

        let cos = self.steer.cos();
        let sin = self.steer.sin();
        let fwd = body.forward();
        let right = body.right();
        let wheel_fwd = fwd * cos + right * sin;
        let wheel_right = right * cos - fwd * sin;

        // Suspension probe along body-down from the mount.
        let down = -body.up();
        let mount_world = body.to_world(cfg.mount);
        let ray_len = cfg.suspension.rest_length + cfg.suspension.travel + cfg.tire.radius;
        let hit: Option<GroundHit> = probe.probe(mount_world, down, ray_len);

        // Standalone default: a quarter of a nominal 1200 kg body.
        // Car-level code should prefer `update_with_mass` with real
        // weight distribution (see `corner_masses`).
        self.update_with_mass(
            cfg,
            body,
            wheel_fwd,
            wheel_right,
            down,
            mount_world,
            ray_len,
            hit,
            300.0,
            cfg.inertia,
            drive_torque,
            brake_force,
            handbrake_force,
            tcs,
            abs,
            dt,
        )
    }

    /// [`WheelState::update`] with explicit corner mass and spin
    /// inertia (wheel + reflected drivetrain).
    #[allow(clippy::too_many_arguments)]
    pub fn update_with_mass(
        &mut self,
        cfg: &WheelConfig,
        body: &BodyState,
        wheel_fwd: Vec3,
        wheel_right: Vec3,
        down: Vec3,
        _mount_world: Vec3,
        ray_len: f32,
        hit: Option<GroundHit>,
        corner_mass: f32,
        spin_inertia: f32,
        drive_torque: f32,
        brake_force: f32,
        handbrake_force: f32,
        tcs: &TcsConfig,
        abs: &AbsConfig,
        dt: f32,
    ) -> Option<WheelForce> {
        let spring_force = self.susp.update(
            &cfg.suspension,
            hit.map(|h| h.distance - cfg.tire.radius),
            ray_len - cfg.tire.radius,
            corner_mass,
            dt,
        );
        let Some(h) = hit else {
            self.slip_ratio = 0.0;
            self.slip_angle = 0.0;
            self.load = 0.0;
            self.tcs_activity = 1.0;
            self.abs_activity = 1.0;
            // Spin decays slowly in the air.
            self.spin *= (1.0 - 0.2 * dt.max(0.0)).max(0.0);
            return None;
        };
        if !self.susp.grounded {
            self.load = 0.0;
            return None;
        }
        self.surface = h.surface;
        // Normal load from the spring along the ground normal.
        let load = spring_force;
        self.load = load;

        // Contact-point velocity (rigid-body transport).
        let contact = h.point;
        let r = contact - body.pos;
        let v_contact = body.vel + body.ang_vel.cross(r);
        let vx = v_contact.dot(wheel_fwd);
        let vy = v_contact.dot(wheel_right);

        // Substepped: spin dynamics are far stiffer than the body.
        let radius = cfg.tire.radius;
        let inertia = spin_inertia.max(0.05);
        let help = cfg.friction_help.max(0.0);
        let substeps = 5;
        let sdt = dt.max(0.0) / substeps as f32;
        let mut fx = 0.0;
        let mut fy = 0.0;
        // NOTE: activity persists across steps; do not reset.
        for _ in 0..substeps {
            // Slip measures with guarded denominators. Longitudinal
            // slip is relative to whichever is larger (road or tire
            // speed) so launches don't read as infinite slip.
            let rolling = self.spin * radius;
            let denom = vx.abs().max(rolling.abs()).max(1.0);
            self.slip_ratio = (rolling - vx) / denom;
            self.slip_angle = (-vy / vx.abs().max(2.0)).clamp(-2.0, 2.0);

            // TCS: proportional scale toward the target slip, applied
            // through a rate-limited cut.
            let tcs_target =
                if tcs.enabled && drive_torque > 0.0 && self.slip_ratio > tcs.slip_target.max(0.01)
                {
                    (tcs.slip_target.max(0.01) / self.slip_ratio).clamp(0.0, 1.0)
                } else {
                    1.0
                };
            let tcs_step = tcs.response.max(1.0) * sdt;
            self.tcs_activity += (tcs_target - self.tcs_activity).clamp(-tcs_step, tcs_step);
            let drive = drive_torque * self.tcs_activity;
            // ABS: proportional scale toward the target slip, same
            // rate-limited cut. Handbrake torque bypasses ABS (it is
            // meant to lock).
            let abs_target =
                if abs.enabled && brake_force > 0.0 && self.slip_ratio < -abs.slip_target.max(0.01)
                {
                    (abs.slip_target.max(0.01) / -self.slip_ratio).clamp(0.0, 1.0)
                } else {
                    1.0
                };
            let abs_step = abs.response.max(1.0) * sdt;
            self.abs_activity += (abs_target - self.abs_activity).clamp(-abs_step, abs_step);
            let brake = brake_force * self.abs_activity + handbrake_force;

            // Tire contact force.
            let (raw_fx, raw_fy) =
                cfg.tire
                    .force(self.slip_ratio, self.slip_angle, load, h.surface);
            fx = raw_fx * help;
            fy = raw_fy * help;

            // Spin dynamics: drive - brake - contact - rolling resist.
            // Static signs: stopped wheels feel no resistance torque.
            let spin_sign = static_sign(self.spin);
            let brake_torque = brake * radius * spin_sign;
            let rolling_torque = cfg.tire.rolling_force(load, h.surface) * radius * spin_sign;
            let hand_lock =
                cfg.handbrake && handbrake_force > 0.0 && vx.abs() < 1.0 && drive <= 0.0;
            if hand_lock {
                self.spin = 0.0;
            } else if sdt > 0.0 {
                let dw = (drive - brake_torque - fx * radius - rolling_torque) / inertia * sdt;
                let next = self.spin + dw;
                // Static brake hold: don't let brakes reverse the spin.
                if brake > 0.0 && drive <= 0.0 && next.signum() != self.spin.signum() {
                    self.spin = 0.0;
                } else {
                    self.spin = next;
                }
            }
        }

        let force = wheel_fwd * fx + wheel_right * fy + (-down) * spring_force;
        Some(WheelForce {
            force,
            point: contact,
        })
    }
}

/// Corner masses from static weight distribution: [FL, FR, RL, RR]
/// in kg. Games with live aero/load transfer adjust per step; this is
/// the honest starting point.
pub fn corner_masses(mass: f32, front_share: f32) -> [f32; 4] {
    let share = front_share.clamp(0.0, 1.0);
    let front = mass * share * 0.5;
    let rear = mass * (1.0 - share) * 0.5;
    [front, front, rear, rear]
}

/// Axle anti-roll application: returns (left_add, right_add) spring
/// force corrections for a wheel pair.
pub fn axle_anti_roll(stiffness: f32, left_comp: f32, right_comp: f32) -> (f32, f32) {
    let t = anti_roll_force(stiffness, left_comp, right_comp);
    (t, -t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::FlatGround;

    fn test_wheel() -> (WheelConfig, WheelState) {
        (
            WheelConfig {
                driven: true,
                ..WheelConfig::default()
            },
            WheelState::default(),
        )
    }

    #[test]
    fn wheel_drives_from_standstill() {
        let (cfg, mut w) = test_wheel();
        let body = BodyState::new();
        let steering = SteeringConfig::without_falloff(0.6, 10.0);
        let probe = FlatGround::new(0.0);
        // Mount high so the ray reaches the ground plane at y=0.
        let mut cfg = cfg;
        cfg.mount = Vec3::new(0.0, 0.5, 0.0);
        let out = w
            .update(
                &cfg,
                &body,
                &steering,
                0.0,
                200.0,
                0.0,
                0.0,
                &TcsConfig {
                    enabled: false,
                    ..TcsConfig::default()
                },
                &AbsConfig::default(),
                &probe,
                0.016,
            )
            .expect("should touch ground");
        assert!(out.force.length() > 0.0);
        assert!(w.load > 0.0);
        assert!(w.spin > 0.0);
    }

    #[test]
    fn wheel_airborne_returns_none() {
        let (cfg, mut w) = test_wheel();
        let mut body = BodyState::new();
        body.pos.y = 50.0;
        let steering = SteeringConfig::default();
        let probe = FlatGround::new(0.0);
        let out = w.update(
            &cfg,
            &body,
            &steering,
            0.0,
            200.0,
            0.0,
            0.0,
            &TcsConfig::default(),
            &AbsConfig::default(),
            &probe,
            0.016,
        );
        assert!(out.is_none());
        assert_eq!(w.load, 0.0);
    }

    #[test]
    fn tcs_cuts_burnout() {
        let (cfg, mut w) = test_wheel();
        let body = BodyState::new();
        let steering = SteeringConfig::default();
        let probe = FlatGround::new(0.0);
        let mut cfg = cfg;
        cfg.mount = Vec3::new(0.0, 0.5, 0.0);
        // Huge torque from standstill must spin up, then trigger TCS.
        for _ in 0..10 {
            w.update(
                &cfg,
                &body,
                &steering,
                0.0,
                5000.0,
                0.0,
                0.0,
                &TcsConfig::default(),
                &AbsConfig::default(),
                &probe,
                0.016,
            );
        }
        assert!(w.tcs_activity < 1.0, "tcs = {}", w.tcs_activity);
    }
}
