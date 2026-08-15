use bevy::prelude::*;

/// Single owner of `Time<Virtual>` relative speed + pause.
#[derive(Resource, Default)]
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

impl TimeScaleControl {
    /// Multiplicative virtual-time speed combining all active feel scales.
    pub fn effective_speed(&self) -> f32 {
        let s = self.slow_mo_scale.max(0.01) * self.hitstop_scale.max(0.01);
        s.clamp(0.01, 1.0)
    }
}

pub struct TimeScalePlugin;

impl Plugin for TimeScalePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TimeScaleControl>()
            .add_systems(Last, apply_time_scale);
    }
}

fn apply_time_scale(ctrl: Res<TimeScaleControl>, mut virtual_time: ResMut<Time<Virtual>>) {
    if ctrl.paused || ctrl.freeze_active {
        if !virtual_time.is_paused() {
            virtual_time.pause();
        }
        virtual_time.set_relative_speed(1.0);
    } else {
        if virtual_time.is_paused() {
            virtual_time.unpause();
        }
        let speed = ctrl.effective_speed();
        if (virtual_time.relative_speed() - speed).abs() > 1e-6 {
            virtual_time.set_relative_speed(speed);
        }
    }
}
