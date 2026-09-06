//! Per-entity hit flash driven on `Sprite.color` (and UI). Ported from Pathogenic's
//! player-damage flash: the sprite/UI tints toward a flash color, then eases back
//! to its original color. Add via [`HitFlash::apply`]; the system removes the
//! component when the flash finishes.

use bevy_app::prelude::*;
use bevy_color::{Color, Mix};
use bevy_ecs::prelude::*;
use bevy_sprite::Sprite;
use bevy_text::TextColor;
use bevy_time::{Time, Timer, TimerMode};
use bevy_ui::BackgroundColor;

#[derive(Component)]
pub struct HitFlash {
    pub color: Color,
    pub value: f32,
    pub timer: Timer,
    pub original: Option<Color>,
}

impl HitFlash {
    pub fn new(color: Color, duration_secs: f32) -> Self {
        Self::with_value(color, duration_secs, 0.85)
    }

    pub fn with_value(color: Color, duration_secs: f32, value: f32) -> Self {
        Self {
            color,
            value: value.clamp(0.0, 4.0),
            timer: Timer::from_seconds(duration_secs, TimerMode::Once),
            original: None,
        }
    }

    pub fn for_damage(color: Color, damage: f32) -> Self {
        let d = (damage / 300.0).clamp(0.08, 0.16);
        Self::new(color, d)
    }

    pub fn apply(commands: &mut Commands, entity: Entity, color: Color, duration_secs: f32) {
        commands
            .entity(entity)
            .insert(Self::new(color, duration_secs));
    }
}

pub struct HitFlashPlugin;

impl Plugin for HitFlashPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, tick_hit_flash);
    }
}

fn tick_hit_flash(
    time: Res<Time>,
    mut commands: Commands,
    mut sprites: Query<(Entity, &mut Sprite, &mut HitFlash)>,
    mut bgs: Query<(Entity, &mut BackgroundColor, &mut HitFlash), Without<Sprite>>,
    mut texts: Query<
        (Entity, &mut TextColor, &mut HitFlash),
        (Without<Sprite>, Without<BackgroundColor>),
    >,
) {
    for (e, mut sprite, mut hf) in &mut sprites {
        if hf.original.is_none() {
            hf.original = Some(sprite.color);
        }
        hf.timer.tick(time.delta());
        let t = hf.timer.fraction().clamp(0.0, 1.0);
        // Cubic ease-out decay: full strength at t=0, falling to 0 at t=1.
        let strength = hf.value * (1.0 - t).powi(3);
        let base = hf.original.unwrap_or(Color::WHITE);
        sprite.color = base.mix(&hf.color, strength);
        if hf.timer.just_finished() {
            sprite.color = hf.original.unwrap_or(Color::WHITE);
            commands.entity(e).remove::<HitFlash>();
        }
    }
    for (e, mut bg, mut hf) in &mut bgs {
        if hf.original.is_none() {
            hf.original = Some(bg.0);
        }
        hf.timer.tick(time.delta());
        let t = hf.timer.fraction().clamp(0.0, 1.0);
        let strength = hf.value * (1.0 - t).powi(3);
        let base = hf.original.unwrap_or(Color::WHITE);
        bg.0 = base.mix(&hf.color, strength);
        if hf.timer.just_finished() {
            bg.0 = hf.original.unwrap_or(Color::WHITE);
            commands.entity(e).remove::<HitFlash>();
        }
    }
    for (e, mut text_color, mut hf) in &mut texts {
        if hf.original.is_none() {
            hf.original = Some(text_color.0);
        }
        hf.timer.tick(time.delta());
        let t = hf.timer.fraction().clamp(0.0, 1.0);
        let strength = hf.value * (1.0 - t).powi(3);
        let base = hf.original.unwrap_or(Color::WHITE);
        text_color.0 = base.mix(&hf.color, strength);
        if hf.timer.just_finished() {
            text_color.0 = hf.original.unwrap_or(Color::WHITE);
            commands.entity(e).remove::<HitFlash>();
        }
    }
}
