use bevy::prelude::*;

#[derive(Component)]
pub struct HoverScale {
    pub normal: Vec3,
    pub hovered: Vec3,
    pub speed: f32,
}

impl Default for HoverScale {
    fn default() -> Self {
        Self {
            normal: Vec3::ONE,
            hovered: Vec3::splat(1.08),
            speed: 14.0,
        }
    }
}

#[derive(Component)]
pub struct Typewriter {
    pub full_text: String,
    pub visible_chars: usize,
    pub timer: Timer,
    pub finished: bool,
}

impl Typewriter {
    pub fn new(full_text: impl Into<String>, secs_per_char: f32) -> Self {
        Self {
            full_text: full_text.into(),
            visible_chars: 0,
            timer: Timer::from_seconds(secs_per_char.max(0.001), TimerMode::Once),
            finished: false,
        }
    }
}

#[derive(Component)]
pub struct NumberCounter {
    pub from: f32,
    pub to: f32,
    pub current: f32,
    pub timer: Timer,
    pub finished: bool,
}

impl NumberCounter {
    pub fn new(from: f32, to: f32, duration_secs: f32) -> Self {
        Self {
            from,
            to,
            current: from,
            timer: Timer::from_seconds(duration_secs.max(0.0), TimerMode::Once),
            finished: false,
        }
    }
}

pub struct UiEffectsPlugin;
impl Plugin for UiEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (hover_scale_system, typewriter_system, number_counter_system),
        );
    }
}

fn hover_scale_system(time: Res<Time>, mut q: Query<(&Interaction, &HoverScale, &mut Transform)>) {
    for (interaction, hs, mut tf) in &mut q {
        let target = match *interaction {
            Interaction::Hovered | Interaction::Pressed => hs.hovered,
            _ => hs.normal,
        };
        if tf.scale != target {
            tf.scale = tf
                .scale
                .lerp(target, (hs.speed * time.delta_secs()).min(1.0));
        }
    }
}

fn typewriter_system(time: Res<Time>, mut q: Query<(&mut Text, &mut Typewriter)>) {
    for (mut text, mut tw) in &mut q {
        if tw.finished {
            continue;
        }
        tw.timer.tick(time.delta());
        if tw.timer.just_finished() {
            let total_chars = tw.full_text.chars().count();
            tw.visible_chars = (tw.visible_chars + 1).min(total_chars);
            text.0 = tw.full_text.chars().take(tw.visible_chars).collect();
            if tw.visible_chars >= total_chars {
                tw.finished = true;
            } else {
                tw.timer.reset();
            }
        }
    }
}

fn number_counter_system(time: Res<Time>, mut q: Query<(&mut Text, &mut NumberCounter)>) {
    for (mut text, mut nc) in &mut q {
        if nc.finished {
            continue;
        }
        nc.timer.tick(time.delta());
        let t = nc.timer.fraction().clamp(0.0, 1.0);
        nc.current = nc.from + (nc.to - nc.from) * t;
        text.0 = format!("{:.0}", nc.current);
        if nc.timer.just_finished() {
            nc.current = nc.to;
            nc.finished = true;
        }
    }
}
