//! Renderer-free game feel math + a `Recoil` sim component.
//!
//! Port of the `game-utils-bevy` juice/game-feel easing curves without any
//! `bevy_sprite`/`bevy_transform` types: everything operates on `glam`
//! vectors, and games add the returned offsets to their viewport snapshot.
//! Deterministic on fixed steps; headless-testable.

use bevy_ecs::prelude::*;
use glam::{Vec2, Vec3};
use repame_sim::{Sim, SimTime};

/// Cubic ease-out 0..1.
pub fn ease_out_cubic(t: f32) -> f32 {
    let u = (t.clamp(0.0, 1.0)) - 1.0;
    u * u * u + 1.0
}

/// Back-eased pop-in scale (overshoots past 1.0, settles at 1.0).
pub fn pop_scale(t: f32) -> f32 {
    let overshoot = 1.70158;
    let t2 = t.clamp(0.0, 1.0) - 1.0;
    t2 * t2 * ((overshoot + 1.0) * t2 + overshoot) + 1.0
}

/// Squash-and-stretch XY scale: ramps to `amount` in the first half,
/// relaxes back to 1.0 in the second half.
pub fn squash_stretch_xy(t: f32, amount: Vec2) -> Vec2 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        let u = t / 0.5;
        Vec2::new(
            1.0 + (amount.x - 1.0) * u,
            1.0 + (amount.y - 1.0) * u,
        )
    } else {
        let u = (t - 0.5) / 0.5;
        amount + (Vec2::ONE - amount) * u
    }
}

/// Bounce scale: peaks at 30% of the duration, relaxes to 1.0.
pub fn bounce_wave(t: f32, peak: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.3 {
        1.0 + (peak - 1.0) * (t / 0.3)
    } else {
        peak + (1.0 - peak) * ((t - 0.3) / 0.7)
    }
}

/// Decaying sinusoidal shake offset for `elapsed_secs` of wall time.
pub fn shake_offset(elapsed_secs: f32, intensity: f32, decay: f32) -> Vec2 {
    let d = decay.clamp(0.0, 1.0);
    Vec2::new(
        (elapsed_secs * 50.0).sin() * intensity * d,
        (elapsed_secs * 47.0).cos() * intensity * d,
    )
}

/// Overwrite a velocity with a directional knockback impulse.
pub fn knockback(velocity: &mut Vec2, dir: Vec2, force: f32) {
    *velocity = dir.normalize_or_zero() * force;
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

/// 2D position holder for recoil demo/testing. Not part of the supported
/// API: real games apply [`Recoil::current`] to their own snapshot
/// positions instead. Only reachable as `feel::FeelPosition` (no
/// top-level re-export).
#[doc(hidden)]
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct FeelPosition(pub Vec2);

/// Progress-only recoil ticking: advances every [`Recoil`] that has no
/// [`FeelPosition`] attached and removes finished ones. This is the
/// system support for the math-only path — [`Recoil::current`] plus the
/// game-owned position — so a `Recoil` on an entity without a position
/// holder still ticks and is still removed.
pub fn tick_recoils(
    time: Res<SimTime>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Recoil), Without<FeelPosition>>,
) {
    for (e, mut recoil) in &mut query {
        if recoil.tick(time.delta_secs) {
            commands.entity(e).remove::<Recoil>();
        }
    }
}

/// Simpler exact variant used by games: advances recoil and folds the
/// delta into [`FeelPosition`] without double-borrowing the query.
///
/// Opt-in demo path only — requires [`FeelPosition`]. The two systems are
/// disjoint (`Without<FeelPosition>` vs a required [`FeelPosition`]), so
/// plain [`Recoil`] entities are handled by [`tick_recoils`] instead of
/// being skipped.
pub fn tick_recoil_positions(
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

/// Register recoil ticking on a [`Sim`]: progress-only [`tick_recoils`]
/// chained before the opt-in [`tick_recoil_positions`] applier, so
/// step→apply order is guaranteed.
pub fn register_systems(sim: &mut Sim) {
    sim.add_chained_systems((tick_recoils, tick_recoil_positions).chain());
}

/// 3D convenience: extend a 2D feel offset onto a world position.
pub fn apply_xy(pos: Vec3, offset: Vec2) -> Vec3 {
    pos + offset.extend(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn recoil_eases_out_and_finishes() {
        let mut sim = Sim::with_default_step();
        let e = sim
            .world
            .spawn((
                Recoil::new(Vec2::X, 10.0, 0.05),
                FeelPosition(Vec2::ZERO),
            ))
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
