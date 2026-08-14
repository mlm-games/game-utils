//! Damped camera follow with aim lookahead and zoom lerping.

use bevy::prelude::*;

use crate::screen_effects::{CameraBase, CameraShakeSet};

/// Frame-rate-independent lerp factor equivalent to Godot's per-physics-frame
/// `lerp(a, b, weight)` at 60 tps.
fn framed_lerp(weight: f32, dt: f32) -> f32 {
    1.0 - (1.0 - weight).powf((dt * 60.0).max(0.0))
}

#[derive(Component)]
pub struct CameraFollow {
    /// Entity to track (usually the player). If `None`, the camera stays put but
    /// zoom smoothing still applies.
    pub target: Option<Entity>,
    /// How tightly the camera chases the target each frame.
    pub follow_weight: f32,
    /// How quickly the smoothed aim point catches up.
    pub aim_weight: f32,
    /// Pull of the aim point toward the target.
    pub aim_pull: f32,
    /// World-space aim point (mouse/stick target). `None` centers on the target.
    pub aim_point: Option<Vec2>,
    /// Smoothed aim point accumulated across frames.
    pub smooth_aim: Vec2,
    /// Zoom target, as `OrthographicProjection::scale` (higher = zoomed out).
    pub base_scale: f32,
    pub zoom_speed: f32,
}

impl Default for CameraFollow {
    fn default() -> Self {
        Self {
            target: None,
            follow_weight: 0.2,
            aim_weight: 0.1,
            aim_pull: 0.2,
            aim_point: None,
            smooth_aim: Vec2::ZERO,
            base_scale: 1.0,
            zoom_speed: 0.02,
        }
    }
}

impl CameraFollow {
    pub fn new(target: Entity) -> Self {
        Self {
            target: Some(target),
            ..default()
        }
    }

    /// Point the aim lookahead at a world position each frame (e.g. mouse).
    pub fn set_aim(&mut self, world_pos: Vec2) {
        self.aim_point = Some(world_pos);
    }
}

pub struct CameraFollowPlugin;

impl Plugin for CameraFollowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, camera_follow_system.before(CameraShakeSet));
    }
}

fn camera_follow_system(
    time: Res<Time>,
    targets: Query<&GlobalTransform>,
    mut q: Query<
        (
            &mut Transform,
            &mut CameraFollow,
            &mut Projection,
            Option<&mut CameraBase>,
        ),
        With<Camera2d>,
    >,
) {
    let dt = time.delta_secs();
    for (mut tf, mut follow, mut projection, base) in &mut q {
        if let Some(target) = follow.target
            && let Ok(gt) = targets.get(target)
        {
            let target_pos = gt.translation().truncate();
            let desired_aim = follow.aim_point.unwrap_or(target_pos);
            follow.smooth_aim = follow
                .smooth_aim
                .lerp(desired_aim, framed_lerp(follow.aim_weight, dt));
            let center = target_pos.lerp(follow.smooth_aim, framed_lerp(follow.aim_pull, dt));
            tf.translation = center.extend(tf.translation.z);
            if let Some(mut base) = base {
                base.translation = tf.translation;
                base.rotation = 0.0;
            }
        }
        if let Projection::Orthographic(ortho) = projection.as_mut() {
            ortho.scale = ortho
                .scale
                .lerp(follow.base_scale, framed_lerp(follow.zoom_speed, dt));
        }
    }
}
