use bevy::prelude::*;
use rand::RngExt;

use crate::juice::Particle;

#[derive(Component)]
pub struct DamageNumber {
    pub timer: Timer,
    pub velocity: Vec2,
    pub gravity: f32,
}

/// Feel tuning for [`VfxSpawner::spawn_damage_number_cfg`]. Mirrors the Godot
/// template's approach of exposing tuneables as defaults the consumer can override.
#[derive(Clone, Copy)]
pub struct DamageNumberConfig {
    pub lifetime: f32,
    pub spread: f32,
    pub rise_speed: f32,
    pub gravity: f32,
    pub font_size: f32,
}

impl Default for DamageNumberConfig {
    fn default() -> Self {
        Self {
            lifetime: 1.0,
            spread: 20.0,
            rise_speed: 80.0,
            gravity: 120.0,
            font_size: 28.0,
        }
    }
}

#[derive(Component)]
pub struct TrailEmitter {
    pub timer: Timer,
    pub interval: f32,
    pub ghost_lifetime: f32,
}

#[derive(Component)]
pub struct TrailGhost {
    pub timer: Timer,
}

pub struct VfxSpawner;

impl VfxSpawner {
    pub fn spawn_damage_number(commands: &mut Commands, amount: i32, pos: Vec2, color: Color) {
        Self::spawn_damage_number_cfg(commands, amount, pos, color, DamageNumberConfig::default());
    }

    pub fn spawn_damage_number_cfg(
        commands: &mut Commands,
        amount: i32,
        pos: Vec2,
        color: Color,
        cfg: DamageNumberConfig,
    ) {
        commands.spawn((
            Text2d::new(amount.to_string()),
            TextFont {
                font_size: FontSize::Px(cfg.font_size),
                ..default()
            },
            TextColor(color),
            TextLayout::default(),
            Transform::from_translation(pos.extend(50.0)),
            DamageNumber {
                timer: Timer::from_seconds(cfg.lifetime, TimerMode::Once),
                velocity: Vec2::new(
                    rand::rng().random_range(-cfg.spread..cfg.spread),
                    cfg.rise_speed,
                ),
                gravity: cfg.gravity,
            },
        ));
    }

    pub fn spawn_burst(
        commands: &mut Commands,
        pos: Vec2,
        count: usize,
        color: Color,
        speed_range: (f32, f32),
    ) {
        let mut rng = rand::rng();
        for _ in 0..count {
            let angle = rng.random_range(0.0..std::f32::consts::TAU);
            let speed = rng.random_range(speed_range.0..speed_range.1);
            let vel = Vec2::new(angle.cos(), angle.sin()) * speed;
            commands.spawn((
                Sprite {
                    color,
                    custom_size: Some(Vec2::splat(rng.random_range(3.0..7.0))),
                    ..default()
                },
                Transform::from_translation(pos.extend(5.0)),
                Particle {
                    lifetime: Timer::from_seconds(rng.random_range(0.4..0.9), TimerMode::Once),
                    velocity: vel,
                    start_color: color,
                },
            ));
        }
    }

    pub fn create_trail(
        commands: &mut Commands,
        entity: Entity,
        interval: f32,
        ghost_lifetime: f32,
    ) {
        commands.entity(entity).insert(TrailEmitter {
            timer: Timer::from_seconds(interval, TimerMode::Repeating),
            interval,
            ghost_lifetime,
        });
    }
}

pub struct VfxPlugin;
impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (animate_damage_numbers, emit_trails, animate_trail_ghosts),
        );
    }
}

fn animate_damage_numbers(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut TextColor, &mut DamageNumber)>,
) {
    for (e, mut tf, mut color, mut dn) in &mut q {
        dn.timer.tick(time.delta());
        tf.translation += (dn.velocity * time.delta_secs()).extend(0.0);
        dn.velocity.y -= dn.gravity * time.delta_secs();
        let a = (1.0 - dn.timer.fraction()).clamp(0.0, 1.0);
        color.0.set_alpha(a);
        if dn.timer.just_finished() {
            commands.entity(e).despawn();
        }
    }
}

fn emit_trails(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(&Transform, &mut TrailEmitter)>,
) {
    for (tf, mut emitter) in &mut q {
        emitter.timer.tick(time.delta());
        if emitter.timer.just_finished() {
            commands.spawn((
                Sprite {
                    color: Color::srgba(1.0, 1.0, 1.0, 0.6),
                    custom_size: Some(Vec2::splat(16.0)),
                    ..default()
                },
                Transform::from_translation(tf.translation.truncate().extend(1.0)),
                TrailGhost {
                    timer: Timer::from_seconds(emitter.ghost_lifetime, TimerMode::Once),
                },
            ));
        }
    }
}

fn animate_trail_ghosts(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Sprite, &mut TrailGhost)>,
) {
    for (e, mut sprite, mut ghost) in &mut q {
        ghost.timer.tick(time.delta());
        let a = (1.0 - ghost.timer.fraction()).clamp(0.0, 1.0);
        sprite.color.set_alpha(a);
        if ghost.timer.just_finished() {
            commands.entity(e).despawn();
        }
    }
}
