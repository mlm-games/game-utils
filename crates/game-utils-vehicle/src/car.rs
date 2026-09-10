//! Full configurable car: engine, gearbox, suspension, tires, aero,
//! stability, TCS/ABS, nitrous. One [`CarConfig`] of plain data;
//! tweak it with [`StatMods`](crate::tuning::StatMods). Step in fixed
//! step with any [`GroundProbe`](crate::ground::GroundProbe).

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::body::{BodyState, RigidConfig};
use crate::drivetrain::{CenterDiff, Clutch, Differential, GearState, Gearbox};
use crate::engine::{EngineConfig, EngineOutput};
use crate::ground::GroundProbe;
use crate::input::VehicleInput;
use crate::steering::SteeringConfig;
use crate::wheel::{AbsConfig, TcsConfig, WheelConfig, WheelState, corner_masses};

/// Forced induction: first-order turbo spool adding torque.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InductionConfig {
    /// Peak added pressure ratio (1.0 = doubles torque).
    pub max_boost: f32,
    /// Spool rate (1/s).
    pub spool_rate: f32,
    /// Conversion efficiency.
    pub efficiency: f32,
}

impl Default for InductionConfig {
    fn default() -> Self {
        Self {
            max_boost: 0.8,
            spool_rate: 2.5,
            efficiency: 0.9,
        }
    }
}

/// One axle: wheels, torque split, anti-roll, vectoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxleConfig {
    /// Indices into [`CarConfig::wheels`].
    pub wheels: Vec<usize>,
    pub diff: Differential,
    /// Share of gearbox output (FWD 1/0, RWD 0/1, AWD 0.4/0.6).
    pub torque_split: f32,
    /// Anti-roll bar stiffness (N per unit compression difference).
    pub anti_roll: f32,
    /// Torque vectoring: axle-torque fraction sent to the outer wheel.
    pub vectoring: f32,
}

impl AxleConfig {
    pub fn new(wheels: Vec<usize>, diff: Differential, torque_split: f32) -> Self {
        Self {
            wheels,
            diff,
            torque_split,
            anti_roll: 4000.0,
            vectoring: 0.0,
        }
    }
}

/// Aero base plus optional wake-up devices.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AeroConfig {
    pub drag_coef: f32,
    pub frontal_area: f32,
    pub air_density: f32,
    /// Downforce coefficient: `F = coef * v^2` (N), pushing down.
    pub downforce_coef: f32,
    /// Front-axle downforce share (0..1), applied at axle centroids.
    pub front_downforce_share: f32,
    pub spoiler: Option<AeroDevice>,
    pub air_brake: Option<AeroDevice>,
}

/// Aero element active above a speed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AeroDevice {
    pub downforce_coef: f32,
    pub drag_coef: f32,
    pub area: f32,
    pub min_speed: f32,
}

impl Default for AeroConfig {
    fn default() -> Self {
        Self {
            drag_coef: 0.3,
            frontal_area: 2.0,
            air_density: 1.225,
            downforce_coef: 0.15,
            front_downforce_share: 0.45,
            spoiler: None,
            air_brake: None,
        }
    }
}

impl AeroConfig {
    /// Drag + downforce magnitudes (N) at `speed`.
    pub fn forces(&self, speed: f32, air_brake_on: bool) -> (f32, f32) {
        let q = 0.5 * self.air_density.max(0.0) * speed * speed;
        let mut drag = self.drag_coef.max(0.0) * self.frontal_area.max(0.0) * q;
        let mut down = self.downforce_coef.max(0.0) * q;
        let mut apply_device = |d: &AeroDevice| {
            if speed >= d.min_speed.max(0.0) {
                drag += d.drag_coef.max(0.0) * d.area.max(0.0) * q;
                down += d.downforce_coef.max(0.0) * d.area.max(0.0) * q;
            }
        };
        if let Some(s) = &self.spoiler {
            apply_device(s);
        }
        if air_brake_on && let Some(a) = &self.air_brake {
            apply_device(a);
        }
        (drag, down)
    }
}

/// Yaw damping + upright leveling.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StabilityConfig {
    pub enabled: bool,
    /// Yaw-rate damping (N m per rad/s) past `engage_slip`.
    pub yaw_strength: f32,
    pub yaw_engage_slip: f32,
    pub upright_spring: f32,
    pub upright_damping: f32,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            yaw_strength: 1500.0,
            yaw_engage_slip: 0.25,
            upright_spring: 8000.0,
            upright_damping: 1200.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NitrousConfig {
    /// Extra shaft torque (N m) at full spray.
    pub power: f32,
    /// Bottle capacity (seconds of spray).
    pub capacity: f32,
    /// Regen per second when not spraying.
    pub regen: f32,
}

impl Default for NitrousConfig {
    fn default() -> Self {
        Self {
            power: 150.0,
            capacity: 8.0,
            regen: 0.5,
        }
    }
}

/// Manual shift request for this step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GearShift {
    #[default]
    None,
    Up,
    Down,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarConfig {
    pub body: RigidConfig,
    /// Fraction of mass on the front axle (0..1).
    pub front_weight: f32,
    pub engine: EngineConfig,
    pub induction: Option<InductionConfig>,
    /// Engine braking torque (N m) at closed throttle.
    pub engine_braking: f32,
    pub engine_inertia: f32,
    pub gearbox: Gearbox,
    pub clutch_speed: f32,
    /// Clutch bite rpm (0 disables). Below it the clutch slips.
    pub clutch_bite_rpm: f32,
    /// Active center split (AWD); `None` = fixed axle splits.
    pub center_diff: Option<CenterDiff>,
    /// CG height above ground (m) for load transfer.
    pub cg_height: f32,
    pub wheels: Vec<WheelConfig>,
    pub axles: Vec<AxleConfig>,
    pub steering: SteeringConfig,
    /// Steering demand exponent (1 = linear).
    pub steer_exponent: f32,
    /// Throttle/brake pedal response rates (1/s toward demand).
    pub throttle_speed: f32,
    pub brake_speed: f32,
    pub max_brake_force: f32,
    pub max_handbrake_force: f32,
    pub aero: AeroConfig,
    pub stability: StabilityConfig,
    pub tcs: TcsConfig,
    pub abs: AbsConfig,
    pub nitrous: Option<NitrousConfig>,
    /// Sleep speed (m/s): park cleanly below it with no throttle.
    pub sleep_speed: f32,
}

impl Default for CarConfig {
    fn default() -> Self {
        // Four wheels: FL FR RL RR with mounts for a ~4.4 m car.
        let (fl, fr, rl, rr) = (
            Vec3::new(-0.8, 0.55, -1.4),
            Vec3::new(0.8, 0.55, -1.4),
            Vec3::new(-0.8, 0.55, 1.4),
            Vec3::new(0.8, 0.55, 1.4),
        );
        let front_wheel = |mount: Vec3| WheelConfig {
            mount,
            steered: true,
            driven: false,
            brake_bias: 0.35,
            ..WheelConfig::default()
        };
        let rear_wheel = |mount: Vec3| WheelConfig {
            mount,
            driven: true,
            brake_bias: 0.15,
            handbrake: true,
            ..WheelConfig::default()
        };
        Self {
            body: RigidConfig::default(),
            front_weight: 0.55,
            engine: EngineConfig::default(),
            induction: None,
            engine_braking: 25.0,
            engine_inertia: 0.4,
            gearbox: Gearbox::default(),
            clutch_speed: 6.0,
            clutch_bite_rpm: 0.0,
            center_diff: None,
            cg_height: 0.45,
            wheels: vec![
                front_wheel(fl),
                front_wheel(fr),
                rear_wheel(rl),
                rear_wheel(rr),
            ],
            axles: vec![
                AxleConfig::new(vec![0, 1], Differential::Open, 0.0),
                AxleConfig::new(
                    vec![2, 3],
                    Differential::LimitedSlip {
                        engage_torque: 400.0,
                        engage_ratio: 0.05,
                    },
                    1.0,
                ),
            ],
            steering: SteeringConfig::default(),
            steer_exponent: 1.0,
            throttle_speed: 8.0,
            brake_speed: 10.0,
            max_brake_force: 8000.0,
            max_handbrake_force: 16000.0,
            aero: AeroConfig::default(),
            stability: StabilityConfig::default(),
            tcs: TcsConfig::default(),
            abs: AbsConfig::default(),
            nitrous: None,
            sleep_speed: 0.15,
        }
    }
}

impl CarConfig {
    /// Ackermann corrections (left, right) from track/wheelbase/angle.
    pub fn ackermann(track: f32, wheelbase: f32, max_angle: f32) -> (f32, f32) {
        if max_angle.abs() < 1e-4 || wheelbase <= 0.0 {
            return (0.0, 0.0);
        }
        let t = (max_angle.tan() * track * 0.5 / wheelbase).clamp(-0.9, 0.9);
        (t, -t)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarState {
    pub body: BodyState,
    pub gear: GearState,
    pub clutch: Clutch,
    pub rpm: f32,
    pub boost_bar: f32,
    pub nitrous_left: f32,
    pub throttle: f32,
    pub brake: f32,
    pub wheels: Vec<WheelState>,
    /// Current front torque share (active center split).
    pub center_front_split: f32,
    /// Smoothed body acceleration (m/s^2, world) for load transfer.
    #[serde(default)]
    pub accel: Vec3,
    #[serde(default)]
    prev_vel: Vec3,
}

impl Default for CarState {
    fn default() -> Self {
        Self {
            body: BodyState::new(),
            gear: GearState::new(),
            clutch: Clutch::default(),
            rpm: 1000.0,
            boost_bar: 0.0,
            nitrous_left: 8.0,
            throttle: 0.0,
            brake: 0.0,
            wheels: vec![WheelState::default(); 4],
            center_front_split: 0.0,
            accel: Vec3::ZERO,
            prev_vel: Vec3::ZERO,
        }
    }
}

impl CarState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_wheels(mut self, n: usize) -> Self {
        self.wheels = vec![WheelState::default(); n];
        self
    }

    pub fn speed(&self) -> f32 {
        self.body.speed()
    }

    /// Respawn at `pos` facing `yaw`: motion zeroed, nitrous refilled.
    pub fn reset(&mut self, cfg: &CarConfig, pos: Vec3, yaw: f32) {
        self.body.pos = pos;
        self.body.vel = Vec3::ZERO;
        self.body.orient = glam::Quat::from_rotation_y(yaw);
        self.body.ang_vel = Vec3::ZERO;
        self.gear = GearState::new();
        self.clutch = Clutch::default();
        self.rpm = cfg.engine.idle_rpm;
        self.boost_bar = 0.0;
        self.nitrous_left = cfg.nitrous.map_or(0.0, |n| n.capacity.max(0.0));
        self.throttle = 0.0;
        self.brake = 0.0;
        self.center_front_split = cfg
            .center_diff
            .map_or(0.0, |c| c.base_front_split.clamp(0.0, 1.0));
        self.accel = Vec3::ZERO;
        self.prev_vel = Vec3::ZERO;
        if self.wheels.len() != cfg.wheels.len() {
            self.wheels
                .resize_with(cfg.wheels.len(), WheelState::default);
        }
        for w in &mut self.wheels {
            *w = WheelState::default();
        }
        self.settle_suspension(cfg);
    }

    /// Pre-compress to static equilibrium. Call once after spawning.
    pub fn settle_suspension(&mut self, cfg: &CarConfig) {
        if self.wheels.len() != cfg.wheels.len() {
            self.wheels
                .resize_with(cfg.wheels.len(), WheelState::default);
        }
        let corners = corner_masses(cfg.body.mass, cfg.front_weight);
        for (wi, wst) in self.wheels.iter_mut().enumerate() {
            let Some(wcfg) = cfg.wheels.get(wi) else {
                continue;
            };
            let corner = corners.get(wi).copied().unwrap_or(cfg.body.mass / 4.0);
            let travel = wcfg.suspension.travel.max(0.01);
            let k = wcfg.suspension.spring_rate.max(0.0);
            let preload = k * travel * wcfg.suspension.preload.clamp(0.0, 1.0);
            let c = if k * travel > 0.0 {
                ((corner * 9.81 - preload) / (k * travel)).clamp(0.0, 0.95)
            } else {
                0.0
            };
            wst.susp.compression = c;
            wst.susp.compression_vel = 0.0;
            wst.susp.grounded = c > 0.001;
        }
    }

    /// Fixed-step advance. Call [`CarState::settle_suspension`] once
    /// after spawning.
    pub fn step(
        &mut self,
        cfg: &CarConfig,
        input: &VehicleInput,
        shift: GearShift,
        probe: &impl GroundProbe,
        dt: f32,
    ) {
        if dt <= 0.0 {
            return;
        }
        let mut inp = *input;
        inp.clamp();
        if self.wheels.len() != cfg.wheels.len() {
            self.wheels
                .resize_with(cfg.wheels.len(), WheelState::default);
        }

        // Body acceleration estimate (world) for load transfer.
        let raw_accel = (self.body.vel - self.prev_vel) / dt.max(1e-4);
        self.prev_vel = self.body.vel;
        let blend = (8.0 * dt).min(1.0);
        self.accel += (raw_accel - self.accel) * blend;

        // Pedal response.
        self.throttle += ((inp.throttle.max(0.0) - self.throttle)
            * (cfg.throttle_speed * dt).min(1.0))
        .clamp(-1.0, 1.0);
        self.brake += ((inp.brake - self.brake) * (cfg.brake_speed * dt).min(1.0)).clamp(-1.0, 1.0);

        // Gears + clutch.
        if !cfg.gearbox.automatic {
            match shift {
                GearShift::Up => {
                    self.gear.shift_up(&cfg.gearbox);
                }
                GearShift::Down => {
                    self.gear.shift_down(&cfg.gearbox);
                }
                GearShift::None => {}
            }
        }
        self.gear
            .update(&cfg.gearbox, self.rpm, self.throttle, self.speed(), dt);
        self.clutch.engagement_speed = cfg.clutch_speed.max(0.5);
        self.clutch.bite_rpm = cfg.clutch_bite_rpm.max(0.0);
        self.clutch
            .update(inp.clutch, self.gear.is_shifting(), self.rpm, dt);

        // Induction spool.
        let rpm_norm = ((self.rpm - cfg.engine.idle_rpm)
            / (cfg.engine.max_rpm - cfg.engine.idle_rpm).max(1.0))
        .clamp(0.0, 1.0);
        if let Some(ind) = &cfg.induction {
            let target = ind.max_boost.max(0.0) * self.throttle * rpm_norm.min(1.0);
            self.boost_bar += (target - self.boost_bar) * (ind.spool_rate * dt).min(1.0);
        } else {
            self.boost_bar = 0.0;
        }

        // Engine + nitrous shaft torque.
        let eng: EngineOutput = cfg.engine.eval(self.throttle, self.rpm);
        let induction_mult =
            1.0 + self.boost_bar * cfg.induction.map_or(0.0, |i| i.efficiency.max(0.0));
        let mut shaft =
            eng.torque * induction_mult - cfg.engine_braking.max(0.0) * (1.0 - self.throttle);
        if let Some(nit) = &cfg.nitrous {
            if inp.boost > 0.0 && self.nitrous_left > 0.0 {
                let spray = (inp.boost * dt).min(self.nitrous_left);
                self.nitrous_left -= spray;
                shaft += nit.power.max(0.0) * inp.boost;
            } else {
                self.nitrous_left =
                    (self.nitrous_left + nit.regen.max(0.0) * dt).min(nit.capacity.max(0.0));
            }
        }
        let shaft = shaft.max(0.0) * self.clutch.engagement;
        let ratio = self.gear.ratio(&cfg.gearbox);

        // Axle splits (normalized). Active center diff chases rear slip.
        if let Some(center) = &cfg.center_diff {
            let front_slip = axle_slip_mean(self, cfg, 0);
            let rear_slip = axle_slip_mean(self, cfg, cfg.axles.len().saturating_sub(1));
            let target = center.base_front_split
                + (rear_slip - front_slip).clamp(-1.0, 1.0) * center.variable_range.max(0.0);
            self.center_front_split = center.update(self.center_front_split, target, dt);
        }
        let total_split: f32 = cfg.axles.iter().map(|a| a.torque_split.max(0.0)).sum();
        let norm_split = if total_split > 0.0 {
            1.0 / total_split
        } else {
            0.0
        };

        // Reflected drivetrain inertia at the wheels: the engine
        // flywheel seen through the gears (/ driven wheels). This is
        // what makes low-gear spin dynamics stable and gives engine
        // braking for free. Fades with the clutch.
        let driven_count = cfg.wheels.iter().filter(|w| w.driven).count().max(1) as f32;
        let reflected =
            cfg.engine_inertia.max(0.0) * ratio * ratio * self.clutch.engagement / driven_count;

        // Steering demand with exponent.
        let shaped = inp.steer.signum() * inp.steer.abs().powf(cfg.steer_exponent.max(0.2));

        // Corner masses with transfer. Axle 0 is front, wheel 0 is left.
        let corners = transfer_corners(self, cfg);
        let mut total_force = Vec3::ZERO;
        let mut total_torque = Vec3::ZERO;

        // Per-axle, per-wheel updates.
        let mut axle_spins: Vec<(f32, f32)> = Vec::new();
        for axle in &cfg.axles {
            let l = axle.wheels.first().copied().unwrap_or(0);
            let r = axle.wheels.get(1).copied().unwrap_or(l);
            axle_spins.push((
                self.wheels.get(l).map_or(0.0, |w| w.spin),
                self.wheels.get(r).map_or(0.0, |w| w.spin),
            ));
        }
        for (axle, (spin_l, spin_r)) in cfg.axles.iter().zip(axle_spins.iter()) {
            let axle_torque = shaft * ratio * axle.torque_split.max(0.0) * norm_split;
            let (t_l, t_r) = axle.diff.split(axle_torque, *spin_l, *spin_r);
            // Torque vectoring: shift axle torque to the outer wheel
            // with steering (positive `shaped` = right = left is outer).
            let vec_shift = (axle.vectoring.max(0.0) * shaped * axle_torque.abs() * 0.5)
                .clamp(-axle_torque.abs() * 0.5, axle_torque.abs() * 0.5);
            let torques = [t_l + vec_shift, t_r - vec_shift];
            // Anti-roll from current compressions.
            let comps: Vec<f32> = axle
                .wheels
                .iter()
                .map(|&i| self.wheels.get(i).map_or(0.0, |w| w.susp.compression))
                .collect();
            for (slot, &wi) in axle.wheels.iter().enumerate() {
                let (Some(wcfg), Some(wst)) = (cfg.wheels.get(wi), self.wheels.get_mut(wi)) else {
                    continue;
                };
                let wheel_torque = torques.get(slot).copied().unwrap_or(0.0);
                let brake_f = self.brake * cfg.max_brake_force.max(0.0) * wcfg.brake_bias.max(0.0);
                let hand_f = if wcfg.handbrake {
                    inp.handbrake * cfg.max_handbrake_force.max(0.0) * 0.5
                } else {
                    0.0
                };
                let corner = corners.get(wi).copied().unwrap_or(cfg.body.mass / 4.0);
                let spin_inertia =
                    wcfg.inertia.max(0.05) + if wcfg.driven { reflected } else { 0.0 };
                // Inline the wheel update with explicit frame (shared).
                let out = wheel_step(
                    wst,
                    wcfg,
                    &self.body,
                    &cfg.steering,
                    shaped,
                    wheel_torque,
                    brake_f,
                    hand_f,
                    &cfg.tcs,
                    &cfg.abs,
                    probe,
                    corner,
                    spin_inertia,
                    dt,
                );
                if let Some(o) = out {
                    total_force += o.force;
                    total_torque += (o.point - self.body.pos).cross(o.force);
                }
            }
            // Axle anti-roll torque (roll axis = body Z... forward).
            if comps.len() >= 2 {
                let transfer = axle.anti_roll.max(0.0) * (comps[0] - comps[1]) * 0.5;
                let fwd = self.body.forward();
                total_torque += fwd * transfer * 0.5;
            }
        }
        // Reverse gear creep when selected with negative throttle is
        // already in `ratio`; nothing extra needed.

        // Aero: drag at com, downforce split across axle centroids.
        let speed = self.speed();
        let (drag, down) = cfg.aero.forces(speed, self.brake > 0.3);
        if speed > 0.01 {
            total_force += -self.body.vel.normalize_or_zero() * drag;
        }
        let share = cfg.aero.front_downforce_share.clamp(0.0, 1.0);
        let up = -self.body.up();
        let (front_c, rear_c) = axle_centroids(self, cfg);
        total_force += up * down * share + up * down * (1.0 - share);
        total_torque += (front_c - self.body.pos).cross(up * down * share);
        total_torque += (rear_c - self.body.pos).cross(up * down * (1.0 - share));
        // Gravity always applies; suspension/aero push back.
        total_force += Vec3::new(0.0, -9.81 * cfg.body.mass.max(1.0), 0.0);

        // Stability: yaw damping + upright leveling.
        if cfg.stability.enabled {
            let local_ang = self.body.orient.inverse() * self.body.ang_vel;
            let yaw_excess = (local_ang.y.abs() - cfg.stability.yaw_engage_slip.max(0.0)).max(0.0);
            if yaw_excess > 0.0 {
                let damp = self.body.orient
                    * Vec3::new(
                        0.0,
                        -local_ang.y.signum() * yaw_excess * cfg.stability.yaw_strength.max(0.0),
                        0.0,
                    );
                total_torque += damp;
            }
            let up = self.body.up();
            let tilt = Vec3::Y.cross(up);
            if tilt.length() > 1e-4 {
                let level = tilt.normalize_or_zero() * cfg.stability.upright_spring.max(0.0)
                    - self.body.ang_vel * cfg.stability.upright_damping.max(0.0);
                total_torque += level;
            }
        }

        self.body
            .integrate(&cfg.body, total_force, total_torque, dt);

        // Sleep: kill residual creep so parked cars rest exactly.
        // Skipped under any throttle demand (including reverse).
        if cfg.sleep_speed > 0.0 && inp.throttle == 0.0 && self.speed() < cfg.sleep_speed {
            let s = (1.0 - 10.0 * dt).max(0.0);
            self.body.vel *= s;
            self.body.ang_vel *= s;
            if self.speed() < cfg.sleep_speed * 0.25 {
                self.body.vel = Vec3::ZERO;
            }
        }

        // RPM: blend free-rev and gear-locked by clutch.
        let drive_spins: Vec<f32> = cfg
            .axles
            .iter()
            .flat_map(|a| a.wheels.iter())
            .filter_map(|&i| {
                let driven = cfg.wheels.get(i).is_some_and(|w| w.driven);
                self.wheels
                    .get(i)
                    .map(|w| if driven { w.spin } else { 0.0 })
            })
            .collect();
        let mean_spin = if drive_spins.is_empty() {
            0.0
        } else {
            drive_spins.iter().sum::<f32>() / drive_spins.len() as f32
        };
        // Wheel rad/s -> engine rpm through the total ratio.
        let locked_rpm = cfg.engine.idle_rpm
            + (mean_spin.abs() * ratio.abs() * 60.0 / std::f32::consts::TAU).max(0.0);
        let free_rpm =
            cfg.engine.idle_rpm + self.throttle * (cfg.engine.max_rpm - cfg.engine.idle_rpm);
        let target_rpm =
            locked_rpm * self.clutch.engagement + free_rpm * (1.0 - self.clutch.engagement);
        let rpm_rate =
            (cfg.engine.max_rpm - cfg.engine.idle_rpm) / cfg.engine_inertia.max(0.05) * dt;
        let diff = target_rpm - self.rpm;
        self.rpm += diff.clamp(-rpm_rate, rpm_rate);
        self.rpm = self.rpm.clamp(0.0, cfg.engine.max_rpm * 1.1);
    }
}

/// Mean driven-wheel |slip| of an axle.
fn axle_slip_mean(car: &CarState, cfg: &CarConfig, axle_idx: usize) -> f32 {
    let Some(axle) = cfg.axles.get(axle_idx) else {
        return 0.0;
    };
    let mut sum = 0.0;
    let mut n = 0;
    for &wi in &axle.wheels {
        if let (Some(wcfg), Some(wst)) = (cfg.wheels.get(wi), car.wheels.get(wi))
            && wcfg.driven
        {
            sum += wst.slip_ratio.abs();
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { sum / n as f32 }
}

/// Corner masses with accel-based load transfer. `axles[0]` is front,
/// `axle.wheels[0]` is left.
fn transfer_corners(car: &CarState, cfg: &CarConfig) -> Vec<f32> {
    let mut corners = vec![0.0; cfg.wheels.len()];
    for (i, b) in corner_masses(cfg.body.mass, cfg.front_weight)
        .iter()
        .enumerate()
    {
        if i < corners.len() {
            corners[i] = *b;
        }
    }
    let h = cfg.cg_height.max(0.0);
    if h <= 0.0 || cfg.axles.is_empty() {
        return corners;
    }
    let local_a = car.body.orient.inverse() * car.accel;
    let mass = cfg.body.mass.max(1.0);
    // Wheelbase from first/last axle centroids.
    let centroid_z = |axle: &AxleConfig| -> f32 {
        let zs: Vec<f32> = axle
            .wheels
            .iter()
            .filter_map(|&i| cfg.wheels.get(i).map(|w| w.mount.z))
            .collect();
        if zs.is_empty() {
            0.0
        } else {
            zs.iter().sum::<f32>() / zs.len() as f32
        }
    };
    let wheelbase = (centroid_z(&cfg.axles[cfg.axles.len() - 1]) - centroid_z(&cfg.axles[0]))
        .abs()
        .max(0.5);
    // Longitudinal: forward accel (+, since forward is -Z) loads rear.
    let long_total = mass * (-local_a.z) * h / wheelbase;
    for (axle_idx, axle) in cfg.axles.iter().enumerate() {
        let xs: Vec<f32> = axle
            .wheels
            .iter()
            .filter_map(|&i| cfg.wheels.get(i).map(|w| w.mount.x))
            .collect();
        if xs.len() < 2 {
            continue;
        }
        let track = (xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
            - xs.iter().cloned().fold(f32::INFINITY, f32::min))
        .max(0.5);
        let axle_mass: f32 = axle
            .wheels
            .iter()
            .map(|&i| corners.get(i).copied().unwrap_or(0.0))
            .sum();
        // Lateral: rightward accel loads the left side.
        let lat_total = axle_mass * local_a.x * h / track;
        let n = axle.wheels.len().max(1) as f32;
        for (slot, &wi) in axle.wheels.iter().enumerate() {
            if wi >= corners.len() {
                continue;
            }
            // Front axle sheds under accel, others share the gain.
            let long_each = if axle_idx == 0 {
                -long_total / n
            } else {
                long_total / n / (cfg.axles.len() - 1).max(1) as f32
            };
            let lat_each = if slot == 0 {
                lat_total / 2.0
            } else {
                -lat_total / 2.0
            };
            corners[wi] = (corners[wi] + long_each + lat_each).max(0.0);
        }
    }
    corners
}

/// World centroids of first/last axle mounts.
fn axle_centroids(car: &CarState, cfg: &CarConfig) -> (Vec3, Vec3) {
    let centroid = |axle_idx: usize| -> Vec3 {
        let Some(axle) = cfg.axles.get(axle_idx) else {
            return car.body.pos;
        };
        let mut sum = Vec3::ZERO;
        let mut n = 0;
        for &wi in &axle.wheels {
            if let Some(w) = cfg.wheels.get(wi) {
                sum += car.body.to_world(w.mount);
                n += 1;
            }
        }
        if n == 0 { car.body.pos } else { sum / n as f32 }
    };
    let last = cfg.axles.len().saturating_sub(1);
    (centroid(0), centroid(last))
}

/// Per-wheel update with explicit corner mass and spin inertia.
#[allow(clippy::too_many_arguments)]
fn wheel_step(
    wst: &mut WheelState,
    wcfg: &WheelConfig,
    body: &BodyState,
    steering: &SteeringConfig,
    shaped: f32,
    wheel_torque: f32,
    brake_f: f32,
    hand_f: f32,
    tcs: &TcsConfig,
    abs: &AbsConfig,
    probe: &impl GroundProbe,
    corner: f32,
    spin_inertia: f32,
    dt: f32,
) -> Option<crate::wheel::WheelForce> {
    let target = if wcfg.steered {
        let base = steering.target_angle_at(shaped, body.forward_speed().abs()) * wcfg.steer_ratio;
        base * (1.0 + wcfg.ackermann * shaped.signum() * shaped.abs())
    } else {
        0.0
    };
    wst.steer = steering.step(wst.steer, target, dt);
    let cos = wst.steer.cos();
    let sin = wst.steer.sin();
    let fwd = body.forward();
    let right = body.right();
    let wheel_fwd = fwd * cos + right * sin;
    let wheel_right = right * cos - fwd * sin;
    let down = -body.up();
    let mount_world = body.to_world(wcfg.mount);
    let ray_len = wcfg.suspension.rest_length + wcfg.suspension.travel + wcfg.tire.radius;
    let hit = probe.probe(mount_world, down, ray_len);
    wst.update_with_mass(
        wcfg,
        body,
        wheel_fwd,
        wheel_right,
        down,
        mount_world,
        ray_len,
        hit,
        corner,
        spin_inertia,
        wheel_torque,
        brake_f,
        hand_f,
        tcs,
        abs,
        dt,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::FlatGround;

    fn driving_input() -> VehicleInput {
        VehicleInput {
            throttle: 1.0,
            ..VehicleInput::neutral()
        }
    }

    #[test]
    fn car_launches_and_shifts() {
        let cfg = CarConfig::default();
        let mut car = CarState::new();
        // Body sits so mounts (y=0.55, ray ~1.03) reach the plane.
        car.body.pos.y = 0.0;
        car.settle_suspension(&cfg);
        for _ in 0..900 {
            car.step(
                &cfg,
                &driving_input(),
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
        }
        assert!(car.speed() > 5.0, "speed = {}", car.speed());
        assert!(car.gear.gear > 1, "gear = {}", car.gear.gear);
        assert!(car.rpm > cfg.engine.idle_rpm);
    }

    #[test]
    fn car_brakes_to_stop() {
        let cfg = CarConfig::default();
        let mut car = CarState::new();
        car.settle_suspension(&cfg);
        for _ in 0..300 {
            car.step(
                &cfg,
                &driving_input(),
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
        }
        assert!(car.speed() > 3.0);
        let brake = VehicleInput {
            brake: 1.0,
            ..VehicleInput::neutral()
        };
        for _ in 0..600 {
            car.step(
                &cfg,
                &brake,
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
        }
        assert!(car.speed() < 0.5, "speed = {}", car.speed());
    }

    #[test]
    fn car_reset_respawns() {
        let cfg = CarConfig::default();
        let mut car = CarState::new();
        car.settle_suspension(&cfg);
        for _ in 0..120 {
            car.step(
                &cfg,
                &driving_input(),
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
        }
        assert!(car.speed() > 1.0);
        car.reset(&cfg, Vec3::new(10.0, 0.5, -20.0), 1.0);
        assert_eq!(car.speed(), 0.0);
        assert_eq!(car.body.pos, Vec3::new(10.0, 0.5, -20.0));
        assert_eq!(car.gear.gear, 1);
        assert_eq!(car.throttle, 0.0);
        // Facing yaw=1.0 about +Y from -Z forward.
        let expected = (glam::Quat::from_rotation_y(1.0) * Vec3::NEG_Z).normalize();
        assert!((car.body.forward() - expected).length() < 1e-5);
    }

    #[test]
    fn car_load_transfers_under_braking() {
        let cfg = CarConfig::default();
        let mut car = CarState::new();
        car.settle_suspension(&cfg);
        for _ in 0..300 {
            car.step(
                &cfg,
                &driving_input(),
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
        }
        let front_rest: f32 = car.wheels[0].load + car.wheels[1].load;
        let brake = VehicleInput {
            brake: 1.0,
            ..VehicleInput::neutral()
        };
        for _ in 0..60 {
            car.step(
                &cfg,
                &brake,
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
        }
        let front_braking: f32 = car.wheels[0].load + car.wheels[1].load;
        assert!(
            front_braking > front_rest,
            "front {front_braking} should exceed cruise {front_rest}"
        );
    }

    #[test]
    fn car_vectoring_yaws_more() {
        let mut cfg = CarConfig::default();
        for axle in &mut cfg.axles {
            axle.vectoring = 0.6;
        }
        let plain = CarConfig::default();
        let mut a = CarState::new();
        let mut b = CarState::new();
        a.settle_suspension(&plain);
        b.settle_suspension(&cfg);
        // Rolling turn with drive.
        let turn = VehicleInput {
            throttle: 0.6,
            steer: 0.8,
            ..VehicleInput::neutral()
        };
        for _ in 0..240 {
            a.step(
                &plain,
                &turn,
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
            b.step(
                &cfg,
                &turn,
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
        }
        // Vectoring car rotates more for the same input.
        let yaw_a = (a.body.forward() - Vec3::NEG_Z).length();
        let yaw_b = (b.body.forward() - Vec3::NEG_Z).length();
        assert!(yaw_b > yaw_a, "vec {yaw_b} vs plain {yaw_a}");
    }

    #[test]
    fn car_manual_reverse() {
        let mut cfg = CarConfig::default();
        cfg.gearbox.automatic = false;
        let mut car = CarState::new();
        car.gear = GearState::neutral();
        car.settle_suspension(&cfg);
        let flat = FlatGround::new(0.0);
        car.step(&cfg, &VehicleInput::neutral(), GearShift::Down, &flat, 1.0);
        assert_eq!(car.gear.gear, -1);
        for _ in 0..300 {
            car.step(
                &cfg,
                &driving_input(),
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
        }
        assert!(
            car.body.forward_speed() < -0.5,
            "fwd = {}",
            car.body.forward_speed()
        );
    }

    #[test]
    fn car_nitrous_adds_speed() {
        let cfg = CarConfig {
            nitrous: Some(NitrousConfig::default()),
            ..CarConfig::default()
        };
        let plain = CarConfig::default();
        let mut a = CarState::new();
        let mut b = CarState::new();
        a.settle_suspension(&plain);
        b.settle_suspension(&cfg);
        let gas = driving_input();
        let spray = VehicleInput {
            boost: 1.0,
            ..driving_input()
        };
        for _ in 0..300 {
            a.step(
                &plain,
                &gas,
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
            b.step(
                &cfg,
                &spray,
                GearShift::None,
                &FlatGround::new(0.0),
                1.0 / 60.0,
            );
        }
        assert!(b.speed() > a.speed());
        assert!(b.nitrous_left < 8.0);
    }

    #[test]
    fn ackermann_helper() {
        let (l, r) = CarConfig::ackermann(1.6, 2.7, 0.6);
        assert!(l > 0.0 && r < 0.0);
        assert_eq!(CarConfig::ackermann(1.6, 2.7, 0.0), (0.0, 0.0));
    }

    #[test]
    fn aero_forces_grow_with_speed() {
        let cfg = AeroConfig::default();
        let (d0, _) = cfg.forces(0.0, false);
        let (d1, w1) = cfg.forces(30.0, false);
        assert_eq!(d0, 0.0);
        assert!(d1 > 0.0 && w1 > 0.0);
    }
}
