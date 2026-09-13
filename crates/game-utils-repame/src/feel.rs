//! Renderer-free game feel math + a `Recoil` sim component.
//!
//! Port of the `game-utils-bevy` juice/game-feel easing curves without any
//! `bevy_sprite`/`bevy_transform` types: everything operates on `glam`
//! vectors, and games add the returned offsets to their viewport snapshot.
//! Deterministic on fixed steps; headless-testable.

use bevy_ecs::prelude::*;
use game_utils::math_utils::MathUtils;
use glam::{Vec2, Vec3};
use repame_sim::{Sim, SimTime};

/// Cubic ease-out 0..1. Delegates to the canonical
/// [`MathUtils::ease_out_cubic`](game_utils::math_utils::MathUtils::ease_out_cubic)
/// so Bevy hitstop and repame hitstop ease with the same curve.
pub fn ease_out_cubic(t: f32) -> f32 {
    MathUtils::ease_out_cubic(t)
}

/// Back-eased pop-in scale (overshoots past 1.0, settles at 1.0).
/// Canonical [`MathUtils::pop_scale`](game_utils::math_utils::MathUtils::pop_scale).
pub fn pop_scale(t: f32) -> f32 {
    MathUtils::pop_scale(t)
}

/// Squash-and-stretch XY scale. Canonical
/// [`MathUtils::squash_stretch_xy`](game_utils::math_utils::MathUtils::squash_stretch_xy).
pub fn squash_stretch_xy(t: f32, amount: Vec2) -> Vec2 {
    MathUtils::squash_stretch_xy(t, amount)
}

/// Bounce scale. Canonical
/// [`MathUtils::bounce_wave`](game_utils::math_utils::MathUtils::bounce_wave).
pub fn bounce_wave(t: f32, peak: f32) -> f32 {
    MathUtils::bounce_wave(t, peak)
}

/// Decaying sinusoidal shake offset. Canonical
/// [`MathUtils::shake_offset`](game_utils::math_utils::MathUtils::shake_offset).
pub fn shake_offset(elapsed_secs: f32, intensity: f32, decay: f32) -> Vec2 {
    MathUtils::shake_offset(elapsed_secs, intensity, decay)
}

/// Overwrite a velocity with a directional knockback impulse. Canonical
/// [`MathUtils::knockback`](game_utils::math_utils::MathUtils::knockback).
pub fn knockback(velocity: &mut Vec2, dir: Vec2, force: f32) {
    MathUtils::knockback(velocity, dir, force)
}

/// Recoil kick on an entity: applied additively on top of gameplay
/// translation, then undone as it eases out (so movement never fights it).
#[derive(Component, Debug, Clone, Copy)]
pub struct Recoil {
    pub offset: Vec2,
    pub elapsed: f32,
    pub duration: f32,
    /// Offset applied last step; systems subtract it before re-applying.
    pub last_applied: Vec2,
}

impl Recoil {
    pub fn new(dir: Vec2, strength: f32, duration: f32) -> Self {
        Self {
            offset: dir.normalize_or_zero() * strength,
            elapsed: 0.0,
            duration: duration.max(f32::EPSILON),
            last_applied: Vec2::ZERO,
        }
    }

    /// Current applied offset for this step (quart ease-out).
    pub fn current(&self) -> Vec2 {
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - t).powi(4);
        self.offset * (1.0 - ease)
    }

    /// Advance; returns `true` when finished and ready for removal.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        self.elapsed >= self.duration
    }
}

/// Progress-only recoil ticking: advances every [`Recoil`] and removes
/// finished ones. This is the system support for the math-only path —
/// [`Recoil::current`] plus the game-owned position — so a `Recoil` ticks
/// and is removed with no position holder required.
pub fn tick_recoils(
    time: Res<SimTime>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Recoil)>,
) {
    for (e, mut recoil) in &mut query {
        if recoil.tick(time.delta_secs) {
            commands.entity(e).remove::<Recoil>();
        }
    }
}

/// Register recoil ticking on a [`Sim`].
pub fn register_systems(sim: &mut Sim) {
    sim.add_system(tick_recoils);
}

/// 3D convenience: extend a 2D feel offset onto a world position.
pub fn apply_xy(pos: Vec3, offset: Vec2) -> Vec3 {
    pos + offset.extend(0.0)
}

/// Scroll limits in world units. Disabled by default.
///
/// The clamp applies to the camera follow target only: pass the target
/// through [`apply_limits`] before writing it into the camera, and keep
/// momentary displacement (trauma shake) in a separate offset so shake
/// bypasses the clamp by design.
#[derive(Clone, Copy, Debug)]
pub struct CameraLimits {
    /// Smallest visible center `x`. Defaults to `0.0`.
    pub left: f32,
    /// Smallest visible center `y`. Defaults to `0.0`.
    pub top: f32,
    /// Largest visible center `x`. Defaults to `0.0`.
    pub right: f32,
    /// Largest visible center `y`. Defaults to `0.0`.
    pub bottom: f32,
    /// Master switch. Defaults to `false` (no clamping).
    pub enabled: bool,
}

impl Default for CameraLimits {
    fn default() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            enabled: false,
        }
    }
}

/// Clamp a follow target into scroll limits.
///
/// Returns `center` unchanged when limits are disabled or when a pair is
/// inverted (`left > right` is normalized, never a trap). Each axis clamps
/// independently, so a corner target slides along the clamped edge.
///
/// ```rust
/// use game_utils_repame::feel::{CameraLimits, apply_limits};
///
/// let limits = CameraLimits { left: 100.0, top: 100.0, right: 700.0, bottom: 500.0, enabled: true };
/// assert_eq!(apply_limits([50.0, 300.0], limits), [100.0, 300.0]);
/// assert_eq!(apply_limits([400.0, 300.0], limits), [400.0, 300.0]);
/// ```
pub fn apply_limits(center: [f32; 2], limits: CameraLimits) -> [f32; 2] {
    if !limits.enabled {
        return center;
    }
    [
        center[0].clamp(limits.left.min(limits.right), limits.left.max(limits.right)),
        center[1].clamp(limits.top.min(limits.bottom), limits.top.max(limits.bottom)),
    ]
}

/// Ease the camera toward its follow target.
///
/// Moves `current` toward `target` with exponential smoothing at `speed`
/// world units per second: fast when far away, settling gently without
/// overshooting. Large `speed` values approach a snap; the motion is
/// frame-rate independent for a fixed `dt`. Delegates to
/// [`MathUtils::smooth_toward_vec2`](game_utils::math_utils::MathUtils::smooth_toward_vec2).
///
/// - `speed <= 0` (or non-finite) snaps directly to `target`.
/// - `dt <= 0` holds `current` (a paused frame never moves the camera).
///
/// ```rust
/// use game_utils_repame::feel::smooth_toward;
///
/// let p = smooth_toward([0.0, 0.0], [100.0, 0.0], 5.0, 0.016);
/// assert!(p[0] > 0.0 && p[0] < 100.0);
/// ```
pub fn smooth_toward(current: [f32; 2], target: [f32; 2], speed: f32, dt: f32) -> [f32; 2] {
    let out = MathUtils::smooth_toward_vec2(
        Vec2::new(current[0], current[1]),
        Vec2::new(target[0], target[1]),
        speed,
        dt,
    );
    [out.x, out.y]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-only position holder for the applier test below. Not public:
    /// real games apply [`Recoil::current`] to their own positions.
    #[derive(Component, Debug, Clone, Copy, Default)]
    struct FeelPosition(Vec2);

    /// Test-only applier mirroring the supported game-side pattern
    /// (subtract last, tick, apply current) without double-borrowing.
    fn tick_recoil_positions(
        time: Res<SimTime>,
        mut commands: Commands,
        mut query: Query<(Entity, &mut Recoil, &mut FeelPosition)>,
    ) {
        for (e, mut recoil, mut pos) in &mut query {
            pos.0 -= recoil.last_applied;
            let finished = recoil.tick(time.delta_secs);
            let applied = recoil.current();
            if finished {
                commands.entity(e).remove::<Recoil>();
                recoil.last_applied = Vec2::ZERO;
            } else {
                pos.0 += applied;
                recoil.last_applied = applied;
            }
        }
    }

    #[test]
    fn pop_settles_at_one() {
        assert!((pop_scale(1.0) - 1.0).abs() < 1e-4);
        assert!(pop_scale(0.5) > 1.0);
    }

    #[test]
    fn squash_returns_to_one() {
        let end = squash_stretch_xy(1.0, Vec2::new(1.3, 0.7));
        assert!((end - Vec2::ONE).length() < 1e-6);
    }

    #[test]
    fn delegation_matches_canonical_math() {
        // Thin wrappers: pin agreement with `MathUtils`, shape goldens
        // live upstream in `game-utils`.
        assert_eq!(pop_scale(0.5), MathUtils::pop_scale(0.5));
        assert_eq!(
            squash_stretch_xy(0.25, Vec2::new(1.3, 0.7)),
            MathUtils::squash_stretch_xy(0.25, Vec2::new(1.3, 0.7))
        );
        assert_eq!(bounce_wave(0.15, 1.5), MathUtils::bounce_wave(0.15, 1.5));
        assert_eq!(
            shake_offset(1.0, 10.0, 0.5),
            MathUtils::shake_offset(1.0, 10.0, 0.5)
        );
    }

    #[test]
    fn limits_clamp_center_not_offset() {
        let limits = CameraLimits {
            left: 100.0,
            top: 100.0,
            right: 700.0,
            bottom: 500.0,
            enabled: true,
        };
        assert_eq!(apply_limits([50.0, 300.0], limits), [100.0, 300.0]);
        assert_eq!(apply_limits([400.0, 900.0], limits), [400.0, 500.0]);
        assert_eq!(apply_limits([400.0, 300.0], limits), [400.0, 300.0]);
        assert_eq!(
            apply_limits(
                [50.0, 50.0],
                CameraLimits {
                    enabled: false,
                    ..limits
                }
            ),
            [50.0, 50.0]
        );
    }

    #[test]
    fn smoothing_converges_without_overshoot() {
        let target = [100.0, 0.0];
        let p1 = smooth_toward([0.0, 0.0], target, 5.0, 0.016);
        assert!(p1[0] > 0.0 && p1[0] < 100.0, "eases forward, got {p1:?}");
        let mut p = [0.0, 0.0];
        for _ in 0..600 {
            p = smooth_toward(p, target, 5.0, 0.016);
        }
        assert!((p[0] - 100.0).abs() < 1e-2, "settles, got {p:?}");
        assert_eq!(smooth_toward([1.0, 2.0], target, 0.0, 0.016), target);
        assert_eq!(smooth_toward([1.0, 2.0], target, 5.0, 0.0), [1.0, 2.0]);
    }

    #[test]
    fn recoil_applier_eases_out_and_finishes() {
        let mut sim = Sim::with_default_step();
        let e = sim
            .world
            .spawn((Recoil::new(Vec2::X, 10.0, 0.05), FeelPosition(Vec2::ZERO)))
            .id();
        sim.add_system(tick_recoil_positions);
        for _ in 0..10 {
            sim.tick();
        }
        assert!(sim.world.get::<Recoil>(e).is_none());
        let p = sim.world.get::<FeelPosition>(e).unwrap().0;
        assert!(p.length() < 1e-4);
    }

    #[test]
    fn math_only_recoil_ticks_and_is_removed() {
        // No FeelPosition: the supported path (Recoil::current applied to
        // game-owned positions) still gets progress + removal.
        let mut sim = Sim::with_default_step();
        let e = sim.world.spawn(Recoil::new(Vec2::X, 10.0, 0.05)).id();
        register_systems(&mut sim);
        for _ in 0..10 {
            sim.tick();
        }
        assert!(sim.world.get::<Recoil>(e).is_none());
    }
}
