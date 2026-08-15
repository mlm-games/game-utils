use bevy::prelude::*;
use rand::RngExt;

use crate::time_scale::TimeScaleControl;

#[derive(Resource, Default)]
pub struct Trauma(pub f32);

/// Tuning for the spring-mass impulse shake.
#[derive(Resource)]
pub struct ImpactShakeConfig {
    /// Spring stiffness (acceleration per unit of offset).
    pub stiffness: f32,
    /// Spring damping (velocity retention per frame).
    pub damping: f32,
    /// Impulse-to-velocity gain.
    pub amplitude: f32,
    /// Fatigue decay time constant (seconds).
    pub fatigue_tau: f32,
    /// Fatigue reference scale for the damping curve.
    pub fatigue_ref: f32,
    /// Impulses below this size don't build fatigue.
    pub fatigue_free_impulse: f32,
    /// Floor for the fatigue damping scale (never fully muted).
    pub min_scale: f32,
    /// Max total shake offset (units), clamps runaway springs.
    pub max_offset: f32,
    /// Max accumulated impulse per frame.
    pub max_added_vel: f32,
}

impl Default for ImpactShakeConfig {
    fn default() -> Self {
        Self {
            stiffness: 1.0,
            damping: 0.5,
            amplitude: 10.0,
            fatigue_tau: 0.4,
            fatigue_ref: 150.0,
            fatigue_free_impulse: 40.0,
            min_scale: 0.35,
            max_offset: 1500.0,
            max_added_vel: 32000.0,
        }
    }
}

/// Spring-mass state for the impulse shake. One resource serves the whole app;
/// the single camera-shake system in this module sums it with the trauma jitter.
#[derive(Resource, Default)]
pub struct ImpactShake {
    /// Impulses queued by `trigger` since the last spring integration.
    pub added_velocity: Vec2,
    /// Current spring velocity.
    pub velocity: Vec2,
    /// Current camera offset applied on top of the base transform.
    pub offset: Vec2,
    /// Accumulated fatigue (decays exponentially).
    pub fatigue: f32,
}

impl ImpactShake {
    /// Add a directional impulse. `dir` packs strength into its length (Godot:
    /// `G.main.screen_shake(dir * damage * 40.0)`); direction is jittered by ±22.5°
    /// and the accumulated impulse is clamped.
    pub fn trigger(&mut self, dir: Vec2, cfg: &ImpactShakeConfig) {
        if dir.length_squared() <= 0.0 {
            return;
        }
        let jitter =
            rand::rng().random_range(-std::f32::consts::PI / 8.0..std::f32::consts::PI / 8.0);
        let rotated = Vec2::new(
            dir.x * jitter.cos() - dir.y * jitter.sin(),
            dir.x * jitter.sin() + dir.y * jitter.cos(),
        );
        self.added_velocity += -rotated;
        let max = cfg.max_added_vel;
        if self.added_velocity.length_squared() > max * max {
            self.added_velocity = self.added_velocity.normalize() * max;
        }
    }

    /// Integrate one step of the spring, returning the offset to apply this frame.
    pub fn step(&mut self, delta: f32, cfg: &ImpactShakeConfig) -> Vec2 {
        // Fatigue decays over time; the damping curve scales new impulses.
        self.fatigue *= (-delta / cfg.fatigue_tau).exp();
        let impulse = self.added_velocity.length().sqrt();
        let damp_scale = (cfg.min_scale).max(cfg.fatigue_ref / (cfg.fatigue_ref + self.fatigue));
        self.velocity +=
            self.added_velocity.normalize_or_zero() * impulse * damp_scale * cfg.amplitude;
        // Small hits don't build fatigue, so rare big bursts keep full impact.
        self.fatigue += (impulse - cfg.fatigue_free_impulse).max(0.0);
        self.added_velocity = Vec2::ZERO;

        self.offset += self.velocity;
        if self.offset.length_squared() > cfg.max_offset * cfg.max_offset {
            self.offset = self.offset.normalize() * cfg.max_offset;
            self.velocity *= 0.5;
        }
        self.velocity = -self.offset * cfg.stiffness + self.velocity * cfg.damping;
        self.offset
    }
}

#[derive(Resource)]
pub struct FlashWhite {
    pub amount: f32,
    pub timer: Timer,
}

impl Default for FlashWhite {
    fn default() -> Self {
        Self {
            amount: 0.0,
            timer: Timer::from_seconds(0.0, TimerMode::Once),
        }
    }
}

#[derive(Resource)]
pub struct FreezeFrame {
    pub active: bool,
    pub timer: Timer,
}

impl Default for FreezeFrame {
    fn default() -> Self {
        Self {
            active: false,
            timer: Timer::from_seconds(0.0, TimerMode::Once),
        }
    }
}

#[derive(Resource, Default)]
pub struct ChromaticAberration(pub f32);

/// Feel tuning for the trauma shake and chromatic decay systems. Mirrors the Godot
/// template's `shake_camera(camera, strength, duration)`/`camera_shake(intensity,
/// duration, decay)` params: consumers override the resource once to pick their feel.
#[derive(Resource)]
pub struct ScreenEffectsConfig {
    pub shake_magnitude_2d: f32,
    pub shake_magnitude_3d: f32,
    pub rotation_jitter_2d: f32,
    pub rotation_jitter_3d: f32,
    pub trauma_decay: f32,
    pub chromatic_decay: f32,
}

impl Default for ScreenEffectsConfig {
    fn default() -> Self {
        Self {
            shake_magnitude_2d: 12.0,
            shake_magnitude_3d: 0.35,
            rotation_jitter_2d: 0.05,
            rotation_jitter_3d: 0.02,
            trauma_decay: 1.5,
            chromatic_decay: 2.0,
        }
    }
}

#[derive(Component, Clone, Copy)]
pub struct CameraBase {
    pub translation: Vec3,
    pub rotation: f32,
}

#[derive(Component, Clone, Copy)]
pub struct CameraBase3d {
    pub translation: Vec3,
    pub rotation: Quat,
}

/// System set for camera-offset feel (trauma shake, impact shake...). Camera
/// follow rigs schedule their base-writing before this set so the shake reads
/// this frame's rest position; impulse-style shake chips in after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CameraShakeSet;

pub struct ScreenEffects;

impl ScreenEffects {
    pub fn add_trauma(trauma: &mut Trauma, amount: f32) {
        trauma.0 = (trauma.0 + amount).clamp(0.0, 1.0);
    }

    pub fn freeze_frame(freeze: &mut FreezeFrame, duration: f32) {
        freeze.timer = Timer::from_seconds(duration, TimerMode::Once);
        freeze.active = true;
    }

    pub fn flash_white(flash: &mut FlashWhite, duration: f32) {
        flash.amount = 1.0;
        flash.timer = Timer::from_seconds(duration, TimerMode::Once);
    }

    pub fn chromatic_pulse(chrom: &mut ChromaticAberration, strength: f32) {
        chrom.0 = chrom.0.max(strength);
    }

    /// Kick the spring-mass camera shake with a directional impulse. `dir` is the
    /// world-space direction the hit came *from* (the camera kicks away from it).
    pub fn add_impulse(shake: &mut ImpactShake, cfg: &ImpactShakeConfig, dir: Vec2) {
        shake.trigger(dir, cfg);
    }
}

pub struct ScreenEffectsPlugin;
impl Plugin for ScreenEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Trauma>()
            .init_resource::<ImpactShake>()
            .init_resource::<ImpactShakeConfig>()
            .init_resource::<FlashWhite>()
            .init_resource::<FreezeFrame>()
            .init_resource::<ChromaticAberration>()
            .init_resource::<ScreenEffectsConfig>()
            .add_systems(
                Update,
                (
                    apply_camera_shake.in_set(CameraShakeSet),
                    tick_flash,
                    tick_freeze,
                    tick_chromatic,
                )
                    .chain(),
            );
    }
}

fn tick_chromatic(
    time: Res<Time>,
    cfg: Res<ScreenEffectsConfig>,
    mut chrom: ResMut<ChromaticAberration>,
) {
    chrom.0 = (chrom.0 - cfg.chromatic_decay * time.delta_secs()).max(0.0);
}

fn apply_camera_shake(
    time: Res<Time>,
    cfg: Res<ScreenEffectsConfig>,
    shake_cfg: Res<ImpactShakeConfig>,
    mut trauma: ResMut<Trauma>,
    mut shake: ResMut<ImpactShake>,
    mut q2: Query<(&mut Transform, &CameraBase), (With<Camera2d>, Without<Camera3d>)>,
    mut q3: Query<(&mut Transform, &CameraBase3d), (With<Camera3d>, Without<Camera2d>)>,
) {
    let dt = time.delta_secs();

    // Advance the spring-mass impulse shake.
    let spring = if dt > 0.0 {
        shake.step(dt, &shake_cfg)
    } else {
        shake.offset
    };

    let mut rng = rand::rng();
    let t = trauma.0;
    let shake_pow = t * t;
    for (mut tf, base) in &mut q2 {
        let offset = if shake_pow > 0.001 {
            let mag = shake_pow * cfg.shake_magnitude_2d;
            let ox = rng.random_range(-mag..mag);
            let oy = rng.random_range(-mag..mag);
            let rot = rng.random_range(-cfg.rotation_jitter_2d..cfg.rotation_jitter_2d) * shake_pow;
            tf.rotation = Quat::from_rotation_z(base.rotation + rot);
            Vec3::new(spring.x + ox, spring.y + oy, 0.0)
        } else {
            tf.rotation = Quat::from_rotation_z(base.rotation);
            Vec3::new(spring.x, spring.y, 0.0)
        };
        tf.translation = base.translation + offset;
    }
    for (mut tf, base) in &mut q3 {
        let offset = if shake_pow > 0.001 {
            let mag = shake_pow * cfg.shake_magnitude_3d;
            let ox = rng.random_range(-mag..mag);
            let oy = rng.random_range(-mag..mag);
            let oz = rng.random_range(-mag..mag);
            let rot_j =
                rng.random_range(-cfg.rotation_jitter_3d..cfg.rotation_jitter_3d) * shake_pow;
            tf.rotation = base.rotation * Quat::from_rotation_z(rot_j);
            Vec3::new(spring.x + ox, spring.y + oy, oz)
        } else {
            tf.rotation = base.rotation;
            Vec3::new(spring.x, spring.y, 0.0)
        };
        tf.translation = base.translation + offset;
    }
    trauma.0 = (trauma.0 - cfg.trauma_decay * dt).max(0.0);
}

fn tick_flash(real: Res<Time<Real>>, mut flash: ResMut<FlashWhite>) {
    if flash.amount <= 0.0 {
        return;
    }
    flash.timer.tick(real.delta());
    let t = flash.timer.fraction();
    flash.amount = 1.0 - t;
    if flash.timer.just_finished()
        || flash.timer.elapsed_secs() >= flash.timer.duration().as_secs_f32()
    {
        flash.amount = 0.0;
    }
}

fn tick_freeze(
    real: Res<Time<Real>>,
    mut freeze: ResMut<FreezeFrame>,
    mut ctrl: ResMut<TimeScaleControl>,
) {
    if !freeze.active {
        ctrl.freeze_active = false;
        return;
    }
    ctrl.freeze_active = true;
    freeze.timer.tick(real.delta());
    if freeze.timer.just_finished() {
        freeze.active = false;
        ctrl.freeze_active = false;
    }
}
