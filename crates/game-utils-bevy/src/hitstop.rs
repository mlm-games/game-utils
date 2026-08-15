use bevy::prelude::*;

use crate::time_scale::TimeScaleControl;

/// Brief dip in virtual-time speed after a big hit.
///
/// Mirrors the Godot `Engine.time_scale = 0.05 -> 1.0`. Distinct from a hard freeze-frame: time keeps
/// flowing, just slower, and recovers smoothly.
#[derive(Resource)]
pub struct HitStop {
    /// Whether a dip is currently active.
    pub active: bool,
    /// Virtual-time speed factor while recovering (set on trigger, eased to 1.0).
    pub scale: f32,
    /// Recovery timer, runs on real time so it is unaffected by the dip itself.
    pub recover: Timer,
    /// Initial dip scale applied immediately on trigger.
    pub start_scale: f32,
}

impl Default for HitStop {
    fn default() -> Self {
        Self {
            active: false,
            scale: 1.0,
            recover: Timer::from_seconds(0.0, TimerMode::Once),
            start_scale: 1.0,
        }
    }
}

impl HitStop {
    /// Dip virtual time to `scale` immediately, then recover to normal speed over
    /// `recover_secs` real seconds. (need to extract out trans)
    pub fn trigger(&mut self, scale: f32, recover_secs: f32) {
        self.start_scale = scale.clamp(0.01, 1.0);
        self.scale = self.start_scale;
        self.recover = Timer::from_seconds(recover_secs.max(0.0), TimerMode::Once);
        self.active = true;
    }

    pub fn cancel(&mut self) {
        self.active = false;
        self.scale = 1.0;
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let u = t - 1.0;
    u * u * u + 1.0
}

fn tick_hitstop(
    real: Res<Time<Real>>,
    mut hs: ResMut<HitStop>,
    mut ctrl: ResMut<TimeScaleControl>,
) {
    if !hs.active {
        ctrl.hitstop_scale = 1.0;
        return;
    }
    hs.recover.tick(real.delta());
    let t = hs.recover.fraction().clamp(0.0, 1.0);
    hs.scale = hs.start_scale + (1.0 - hs.start_scale) * ease_out_cubic(t);
    ctrl.hitstop_scale = hs.scale.max(0.01);
    if hs.recover.just_finished() {
        hs.active = false;
        ctrl.hitstop_scale = 1.0;
    }
}

pub struct HitStopPlugin;

impl Plugin for HitStopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HitStop>()
            .add_systems(Update, tick_hitstop);
    }
}
