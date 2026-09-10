//! Sim-side time scales for repame games: pause, freeze-frames,
//! slow-motion, and hitstop dips.
//!
//! All effect state advances on unscaled [`SimTime`] fixed steps, so
//! behaviour is deterministic and headless-testable. Unlike Bevy's
//! `Time<Virtual>` (where `TimeScalePlugin` writes the global clock and
//! all gameplay slows automatically), `repame-sim` keeps `SimTime` fixed:
//! [`TimeScaleControl`] is a dial, and each gameplay system applies it
//! explicitly. That is the Godot `process_mode` split, done by hand:
//!
//! ```ignore
//! // Gameplay systems: scaled time, gated on pause/freeze.
//! fn step_movement(ctrl: Res<TimeScaleControl>, time: Res<SimTime>, ...) {
//!     if !ctrl.should_step() {
//!         return;
//!     }
//!     let dt = ctrl.scaled_delta(&time);
//!     // ... integrate with `dt` ...
//! }
//!
//! // Recovery/UI systems (hitstop, transitions, trauma decay): raw fixed
//! // dt, so the effect never stretches its own recovery.
//! fn tick_fx(time: Res<SimTime>, ...) {
//!     let dt = time.delta_secs;
//! }
//! ```
//!
//! [`TimeScaleControl::effective_speed`] is the single multiplier the game
//! applies to its own virtual clock or fixed-step scaling; rendering/audio
//! read it as plain data.
//!
//! Feel intensity ([`FeelIntensity`]) honors NT-style options-menu sliders:
//! `freezeframes` scales hitstop recovery (0 disables the dip outright),
//! `screenshake` is applied game-side at trauma call sites
//! (`trauma.add(amount * intensity.screenshake)`).

use bevy_ecs::prelude::*;
use repame_sim::{Sim, SimTime};

use crate::feel::ease_out_cubic;

/// Options-menu feel multipliers (NT "FREEZE FRAMES" / "SCREENSHAKE").
///
/// In nt-recreated-bevy these settings were stored but never consumed;
/// here they are honored: [`HitStop`] recovery consults `freezeframes`
/// every tick, and games multiply trauma amounts by `screenshake`.
#[derive(Resource, Debug, Clone)]
pub struct FeelIntensity {
    /// Hitstop recovery-duration multiplier. `1.0` = full effect,
    /// `0.0` = hitstop dips disabled (trigger is a no-op visually).
    pub freezeframes: f32,
    /// Screen-shake magnitude multiplier, applied game-side at each
    /// `trauma.add(amount * screenshake)` call site. `1.0` = full shake.
    pub screenshake: f32,
}

impl Default for FeelIntensity {
    fn default() -> Self {
        Self {
            freezeframes: 1.0,
            screenshake: 1.0,
        }
    }
}

/// Single owner of virtual-time speed + pause.
#[derive(Resource, Debug, Clone)]
pub struct TimeScaleControl {
    /// App-level pause (settings/pause menu). Freezes virtual time.
    pub paused: bool,
    /// Hard freeze-frame effect. Freezes virtual time while active.
    pub freeze_active: bool,
    /// Multiplicative scale while slow-motion is active (`1.0` when idle).
    pub slow_mo_scale: f32,
    /// Multiplicative scale while hitstop is recovering (`1.0` when idle).
    pub hitstop_scale: f32,
}

impl Default for TimeScaleControl {
    fn default() -> Self {
        Self {
            paused: false,
            freeze_active: false,
            slow_mo_scale: 1.0,
            hitstop_scale: 1.0,
        }
    }
}

impl TimeScaleControl {
    /// Multiplicative virtual-time speed combining all active feel scales.
    pub fn effective_speed(&self) -> f32 {
        let s = self.slow_mo_scale.max(0.01) * self.hitstop_scale.max(0.01);
        s.clamp(0.01, 32.0)
    }

    /// True when virtual time should not advance at all.
    pub fn frozen(&self) -> bool {
        self.paused || self.freeze_active
    }

    /// Pause-gate for gameplay systems: `false` while paused or
    /// freeze-framed. Recovery/UI systems ignore this and tick on raw
    /// fixed dt instead.
    pub fn should_step(&self) -> bool {
        !self.frozen()
    }

    /// Gameplay delta: fixed step scaled by [`Self::effective_speed`],
    /// or `0.0` while [`Self::frozen`]. This is the actuator — the value
    /// gameplay integration actually consumes.
    pub fn scaled_delta(&self, time: &SimTime) -> f32 {
        if self.frozen() {
            0.0
        } else {
            time.delta_secs * self.effective_speed()
        }
    }
}

/// Brief dip in virtual-time speed after a big hit.
///
/// Mirrors the Godot `Engine.time_scale = 0.05 -> 1.0` trick: time keeps
/// flowing, just slower, and recovers smoothly. Ticks on unscaled fixed
/// dt so the dip itself does not stretch its own recovery (gameplay
/// slowdown comes from consumers using
/// [`TimeScaleControl::scaled_delta`], not from this timer).
#[derive(Resource, Debug, Clone)]
pub struct HitStop {
    /// Whether a dip is currently active.
    pub active: bool,
    /// Current virtual-time speed factor (eased to 1.0).
    pub scale: f32,
    /// Initial dip scale applied on trigger.
    pub start_scale: f32,
    /// Recovery progress in seconds.
    pub elapsed: f32,
    /// Recovery length in seconds.
    pub duration: f32,
}

impl Default for HitStop {
    fn default() -> Self {
        Self {
            active: false,
            scale: 1.0,
            start_scale: 1.0,
            elapsed: 0.0,
            duration: 0.0,
        }
    }
}

impl HitStop {
    /// Dip virtual time to `scale` immediately, then recover to normal
    /// speed over `recover_secs` sim seconds (scaled by
    /// [`FeelIntensity::freezeframes`] at tick time). Strongest wins:
    /// a weaker-or-equal call mid-freeze is ignored, like the Bevy twin.
    pub fn trigger(&mut self, scale: f32, recover_secs: f32) {
        let scale = scale.clamp(0.01, 1.0);
        if self.active && scale >= self.scale {
            return;
        }
        self.start_scale = scale;
        self.scale = self.start_scale;
        self.elapsed = 0.0;
        self.duration = recover_secs.max(0.0);
        self.active = true;
    }

    pub fn cancel(&mut self) {
        self.active = false;
        self.scale = 1.0;
    }

    fn tick(&mut self, dt: f32, freezeframes: f32) -> f32 {
        if !self.active {
            self.scale = 1.0;
            return 1.0;
        }
        // Intensity rescales the stored raw duration every tick, so a
        // mid-recovery slider change takes effect immediately; 0
        // finishes the dip at once (effect disabled).
        let duration = (self.duration * freezeframes.max(0.0)).max(0.0);
        if duration <= 0.0 {
            self.active = false;
            self.scale = 1.0;
            return 1.0;
        }
        self.elapsed += dt;
        let t = (self.elapsed / duration).clamp(0.0, 1.0);
        self.scale = self.start_scale + (1.0 - self.start_scale) * ease_out_cubic(t);
        if t >= 1.0 {
            self.active = false;
            self.scale = 1.0;
        }
        self.scale.max(0.01)
    }
}

/// Sustained slow-motion window (e.g. kill-cam, focus mode).
#[derive(Resource, Debug, Clone)]
pub struct SlowMotion {
    pub active: bool,
    pub scale: f32,
    pub elapsed: f32,
    pub duration: f32,
}

impl Default for SlowMotion {
    fn default() -> Self {
        Self {
            active: false,
            scale: 1.0,
            elapsed: 0.0,
            duration: 0.0,
        }
    }
}

impl SlowMotion {
    /// Strongest wins, like [`HitStop::trigger`].
    pub fn start(&mut self, scale: f32, duration: f32) {
        let scale = scale.clamp(0.01, 1.0);
        if self.active && scale >= self.scale {
            return;
        }
        self.scale = scale;
        self.elapsed = 0.0;
        self.duration = duration.max(0.0);
        self.active = true;
    }

    pub fn cancel(&mut self) {
        self.active = false;
        self.scale = 1.0;
    }

    fn tick(&mut self, dt: f32) -> f32 {
        if !self.active {
            self.scale = 1.0;
            return 1.0;
        }
        self.elapsed += dt;
        if self.elapsed >= self.duration {
            self.active = false;
            self.scale = 1.0;
            return 1.0;
        }
        self.scale.clamp(0.01, 1.0)
    }
}

fn tick_time_effects(
    time: Res<SimTime>,
    intensity: Res<FeelIntensity>,
    mut ctrl: ResMut<TimeScaleControl>,
    mut hitstop: ResMut<HitStop>,
    mut slow: ResMut<SlowMotion>,
) {
    let dt = time.delta_secs;
    ctrl.hitstop_scale = hitstop.tick(dt, intensity.freezeframes);
    ctrl.slow_mo_scale = slow.tick(dt);
}

/// Insert [`TimeScaleControl`], [`HitStop`], [`SlowMotion`] and
/// [`FeelIntensity`]. Call once at boot.
pub fn init_resources(world: &mut World) {
    world.init_resource::<TimeScaleControl>();
    world.init_resource::<HitStop>();
    world.init_resource::<SlowMotion>();
    world.init_resource::<FeelIntensity>();
}

/// Register per-step effect ticking on a [`Sim`].
pub fn register_systems(sim: &mut Sim) {
    sim.add_system(tick_time_effects);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_speed_combines_scales() {
        let ctrl = TimeScaleControl {
            slow_mo_scale: 0.5,
            hitstop_scale: 0.5,
            ..Default::default()
        };
        assert!((ctrl.effective_speed() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn hitstop_recovers_on_sim_ticks() {
        let mut sim = Sim::with_default_step();
        init_resources(&mut sim.world);
        register_systems(&mut sim);
        sim.world.resource_mut::<HitStop>().trigger(0.1, 0.2);
        for _ in 0..30 {
            sim.tick();
        }
        let hs = sim.world.resource::<HitStop>();
        assert!(!hs.active);
        let ctrl = sim.world.resource::<TimeScaleControl>();
        assert!((ctrl.hitstop_scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn slow_motion_expires() {
        let mut sim = Sim::with_default_step();
        init_resources(&mut sim.world);
        register_systems(&mut sim);
        sim.world.resource_mut::<SlowMotion>().start(0.3, 0.05);
        for _ in 0..10 {
            sim.tick();
        }
        assert!(!sim.world.resource::<SlowMotion>().active);
    }

    #[test]
    fn scaled_delta_gates_on_freeze_and_scales() {        let time = SimTime {
            elapsed_secs: 0.0,
            delta_secs: 1.0 / 60.0,
        };
        let ctrl = TimeScaleControl {
            slow_mo_scale: 0.5,
            hitstop_scale: 0.5,
            ..Default::default()
        };
        assert!(ctrl.should_step());
        assert!((ctrl.scaled_delta(&time) - 0.25 / 60.0).abs() < 1e-6);
        let paused = TimeScaleControl {
            paused: true,
            ..Default::default()
        };
        assert!(!paused.should_step());
        assert_eq!(paused.scaled_delta(&time), 0.0);
        let frozen = TimeScaleControl {
            freeze_active: true,
            ..Default::default()
        };
        assert!(!frozen.should_step());
        assert_eq!(frozen.scaled_delta(&time), 0.0);
    }

    #[test]
    fn freezeframes_zero_disables_hitstop() {
        let mut sim = Sim::with_default_step();
        init_resources(&mut sim.world);
        register_systems(&mut sim);
        sim.world.resource_mut::<FeelIntensity>().freezeframes = 0.0;
        sim.world.resource_mut::<HitStop>().trigger(0.1, 0.2);
        sim.tick();
        assert!(!sim.world.resource::<HitStop>().active);
        assert!((sim.world.resource::<TimeScaleControl>().hitstop_scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn freezeframes_half_recovers_twice_as_fast() {
        let mut full = Sim::with_default_step();
        init_resources(&mut full.world);
        register_systems(&mut full);
        let mut half = Sim::with_default_step();
        init_resources(&mut half.world);
        register_systems(&mut half);
        half.world.resource_mut::<FeelIntensity>().freezeframes = 0.5;
        full.world.resource_mut::<HitStop>().trigger(0.1, 0.2);
        half.world.resource_mut::<HitStop>().trigger(0.1, 0.2);
        for _ in 0..7 {
            full.tick();
            half.tick();
        }
        // 0.2 s at 60 Hz is 12 ticks; halved it finishes in 6.
        assert!(!half.world.resource::<HitStop>().active);
        assert!(full.world.resource::<HitStop>().active);
    }

    #[test]
    fn feel_intensity_defaults_to_full() {
        let intensity = FeelIntensity::default();
        assert_eq!((intensity.freezeframes, intensity.screenshake), (1.0, 1.0));
    }
}
