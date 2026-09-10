use std::collections::HashMap;
use std::collections::VecDeque;

use bevy_app::prelude::*;
use bevy_asset::Handle;
use bevy_audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings};
use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Time, Timer, TimerMode};
use bevy_transform::components::Transform;
use rand::RngExt;

#[derive(Component)]
pub struct BaseVolume(pub f32);

/// Marker for a pooled one-shot SFX voice.
#[derive(Component, Default)]
pub struct SfxChannel;

/// Marker for the (single) music player.
#[derive(Component, Default)]
pub struct MusicChannel;

#[derive(Component, Default)]
pub struct UiChannel;

/// Volume settings, applied to every sink via `sync_channel_volumes`.
#[derive(Resource)]
pub struct AudioChannels {
    pub master: f32,
    pub sfx: f32,
    pub music: f32,
    pub ui: f32,
}

impl Default for AudioChannels {
    fn default() -> Self {
        Self {
            master: 1.0,
            sfx: 1.0,
            music: 0.8,
            ui: 1.0,
        }
    }
}

impl AudioChannels {
    pub fn sfx_volume(&self) -> f32 {
        self.master * self.sfx
    }

    pub fn music_volume(&self) -> f32 {
        self.master * self.music
    }

    pub fn ui_volume(&self) -> f32 {
        self.master * self.ui
    }
}

/// Fast-fade of a music sink's volume to a target.
#[derive(Component)]
pub struct MusicFade {
    pub from: f32,
    pub to: f32,
    pub timer: Timer,
}

impl MusicFade {
    pub fn new(from: f32, to: f32, duration_secs: f32) -> Self {
        Self {
            from,
            to,
            timer: Timer::from_seconds(duration_secs, TimerMode::Once),
        }
    }
}

/// Pooled one-shot SFX manager.
///
/// A fixed voice ring buffer replaces per-play node/playback churn. The same stream is
/// collapsed to ONE voice PER FRAME (closest-to-the-request wins) rather than capped by
/// concurrent voice count.
///
/// Reuse re-attaches the audio components (the documented way to restart a drained
/// sink), so the pool never churns entities.
#[derive(Resource)]
pub struct SfxPool {
    pub max_concurrent: usize,
    /// Round-robin ring of pooled voice entities, reuse order.
    voices: VecDeque<Entity>,
    /// (stream handle) -> (voice entity, squared distance of the first request this frame)
    frame_collapse: HashMap<Handle<AudioSource>, (Entity, f32)>,
    frame: u64,
}

impl Default for SfxPool {
    fn default() -> Self {
        Self::new(32)
    }
}

impl SfxPool {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            max_concurrent: max_concurrent.max(1),
            voices: VecDeque::new(),
            frame_collapse: HashMap::new(),
            frame: 0,
        }
    }

    /// Spawn the ring of idle voices up front so first-frame plays have zero latency.
    /// `PlaybackMode::Remove` drains the sink on finish but keeps the entity alive so it
    /// stays in the ring for reuse.
    pub fn prewarm(&mut self, commands: &mut Commands) {
        for _ in 0..self.max_concurrent {
            let e = commands
                .spawn((
                    PlaybackSettings::REMOVE.with_volume(bevy_audio::Volume::Linear(0.0)),
                    BaseVolume(0.0),
                    SfxChannel,
                ))
                .id();
            self.voices.push_back(e);
        }
    }

    fn start_frame(&mut self) {
        self.frame += 1;
        self.frame_collapse.clear();
    }

    /// Play a one-shot on a pooled voice. If the same stream was already requested this
    /// frame, collapse to the existing voice (moving it closer to a nearer request).
    /// `channels` stamps the bus gain at spawn: without it the sink plays at
    /// raw `volume` until `sync_channel_volumes` corrects it a frame later
    /// (one loud burst per one-shot at low master).
    pub fn play_sfx(
        &mut self,
        commands: &mut Commands,
        channels: &AudioChannels,
        handle: Handle<AudioSource>,
        pos: Vec3,
        listener: Vec3,
        volume: f32,
        pitch_var: f32,
    ) -> Entity {
        // Collapse duplicate requests of the same stream within one frame.
        if let Some((entity, dist_sq)) = self.frame_collapse.get(&handle).copied() {
            let new_dist = pos.distance_squared(listener);
            if new_dist < dist_sq {
                commands
                    .entity(entity)
                    .insert(Transform::from_translation(pos));
                self.frame_collapse.insert(handle, (entity, new_dist));
            }
            return entity;
        }

        if self.voices.is_empty() {
            // Not prewarming full ring here to avoid unbounded grow.
            let e = commands
                .spawn((
                    PlaybackSettings::REMOVE.with_volume(bevy_audio::Volume::Linear(0.0)),
                    BaseVolume(0.0),
                    SfxChannel,
                ))
                .id();
            self.voices.push_back(e);
        }
        let voice = self.voices.front().copied().expect("voices prewarmed");
        self.voices.pop_front();
        self.voices.push_back(voice);

        let mut pitch = 1.0;
        if pitch_var > 0.0 {
            let mut rng = rand::rng();
            pitch += rng.random_range(-pitch_var..pitch_var);
        }

        // Restart a drained/resting voice by removing the audio components and re-adding
        // them (AudioPlayer can't be re-used while still attached and playing).
        let mut ec = commands.entity(voice);
        ec.remove::<AudioPlayer<AudioSource>>();
        ec.remove::<PlaybackSettings>();
        ec.remove::<bevy_audio::SpatialAudioSink>();
        ec.remove::<AudioSink>();
        ec.insert((
            BaseVolume(volume),
            AudioPlayer::new(handle.clone()),
            PlaybackSettings::REMOVE
                .with_volume(bevy_audio::Volume::Linear(volume * channels.sfx_volume()))
                .with_speed(pitch)
                .with_spatial(true),
            Transform::from_translation(pos),
        ));
        self.frame_collapse
            .insert(handle, (voice, pos.distance_squared(listener)));
        voice
    }

    /// Play without positional/directional logic (UI, non-spatial). Varies pitch slightly.
    pub fn play_ui(
        &self,
        commands: &mut Commands,
        channels: &AudioChannels,
        handle: Handle<AudioSource>,
        volume: f32,
    ) {
        let mut rng = rand::rng();
        let pitch = 1.0 + rng.random_range(-0.05..0.05);
        commands.spawn((
            BaseVolume(volume),
            AudioPlayer::new(handle),
            PlaybackSettings::DESPAWN
                .with_volume(bevy_audio::Volume::Linear(volume * channels.ui_volume()))
                .with_speed(pitch),
            UiChannel,
        ));
    }

    pub fn capacity(&self) -> usize {
        self.max_concurrent
    }

    /// Stop and despawn every pooled voice.
    pub fn clear(&mut self, commands: &mut Commands) {
        for e in self.voices.drain(..) {
            commands.entity(e).despawn();
        }
        self.frame_collapse.clear();
    }
}

fn start_sfx_frame(
    mut pool: ResMut<SfxPool>,
    live: Query<Entity, With<SfxChannel>>,
    mut commands: Commands,
) {
    let live_set: std::collections::HashSet<_> = live.iter().collect();
    pool.voices.retain(|e| live_set.contains(e));
    pool.frame_collapse.retain(|_, (e, _)| live_set.contains(e));

    let missing = pool.max_concurrent.saturating_sub(pool.voices.len());
    for _ in 0..missing {
        let e = commands
            .spawn((
                PlaybackSettings::REMOVE.with_volume(bevy_audio::Volume::Linear(0.0)),
                BaseVolume(0.0),
                SfxChannel,
            ))
            .id();
        pool.voices.push_back(e);
    }
    pool.start_frame();
}

fn tick_music_fades(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut MusicFade, &mut AudioSink, &mut BaseVolume), With<MusicChannel>>,
    channels: Res<AudioChannels>,
) {
    for (e, mut fade, mut sink, mut base) in &mut q {
        fade.timer.tick(time.delta());
        let t = fade.timer.fraction().clamp(0.0, 1.0);
        // Cubic ease-out..
        let x = 1.0 - (1.0 - t).powi(3);
        let vol = fade.from + (fade.to - fade.from) * x;
        base.0 = vol;
        sink.set_volume(bevy_audio::Volume::Linear(vol * channels.music_volume()));
        if fade.timer.just_finished() {
            commands.entity(e).remove::<MusicFade>();
        }
    }
}

pub struct AudioM;

impl AudioM {
    /// Spawn volumes are bus-stamped at spawn (`sync_channel_volumes` only
    /// corrects them on a later frame, which bursts loud at low master).
    pub fn play_sfx(
        commands: &mut Commands,
        channels: &AudioChannels,
        handle: Handle<AudioSource>,
        volume: f32,
    ) {
        commands.spawn((
            BaseVolume(volume),
            AudioPlayer::new(handle),
            PlaybackSettings::DESPAWN
                .with_volume(bevy_audio::Volume::Linear(volume * channels.sfx_volume())),
            SfxChannel,
        ));
    }

    pub fn play_sfx_varied(
        commands: &mut Commands,
        channels: &AudioChannels,
        handle: Handle<AudioSource>,
        volume: f32,
        pitch_var: f32,
    ) {
        let mut rng = rand::rng();
        let pitch = 1.0 + rng.random_range(-pitch_var..pitch_var);
        commands.spawn((
            BaseVolume(volume),
            AudioPlayer::new(handle),
            PlaybackSettings::DESPAWN
                .with_volume(bevy_audio::Volume::Linear(volume * channels.sfx_volume()))
                .with_speed(pitch),
            SfxChannel,
        ));
    }

    pub fn play_music(
        commands: &mut Commands,
        channels: &AudioChannels,
        handle: Handle<AudioSource>,
        volume: f32,
        music_q: &Query<Entity, With<MusicChannel>>,
    ) {
        for e in music_q.iter() {
            commands.entity(e).despawn();
        }
        commands.spawn((
            BaseVolume(volume),
            AudioPlayer::new(handle),
            PlaybackSettings::LOOP
                .with_volume(bevy_audio::Volume::Linear(volume * channels.music_volume())),
            MusicChannel,
        ));
    }

    pub fn stop_music(commands: &mut Commands, music_q: &Query<Entity, With<MusicChannel>>) {
        for e in music_q.iter() {
            commands.entity(e).despawn();
        }
    }

    pub fn play_ui(
        commands: &mut Commands,
        channels: &AudioChannels,
        handle: Handle<AudioSource>,
        volume: f32,
    ) {
        commands.spawn((
            BaseVolume(volume),
            AudioPlayer::new(handle),
            PlaybackSettings::DESPAWN
                .with_volume(bevy_audio::Volume::Linear(volume * channels.ui_volume())),
            UiChannel,
        ));
    }
}

fn sync_channel_volumes(
    channels: Res<AudioChannels>,
    mut q: Query<
        (
            Option<&BaseVolume>,
            Option<&SfxChannel>,
            Option<&MusicChannel>,
            Option<&UiChannel>,
            Option<&mut AudioSink>,
            Option<&mut bevy_audio::SpatialAudioSink>,
        ),
        Without<MusicFade>,
    >,
) {
    for (base, sfx, music, ui, sink, spatial) in &mut q {
        let base = base.map(|b| b.0).unwrap_or(1.0);
        let bus = if sfx.is_some() {
            channels.sfx_volume()
        } else if music.is_some() {
            channels.music_volume()
        } else if ui.is_some() {
            channels.ui_volume()
        } else {
            channels.master
        };
        if let Some(mut s) = sink {
            s.set_volume(bevy_audio::Volume::Linear(base * bus));
        }
        if let Some(mut s) = spatial {
            s.set_volume(bevy_audio::Volume::Linear(base * bus));
        }
    }
}

pub struct AudioPlugin;
impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioChannels>()
            .init_resource::<SfxPool>()
            .add_systems(
                Startup,
                |mut commands: Commands, mut pool: ResMut<SfxPool>| {
                    pool.prewarm(&mut commands);
                },
            )
            .add_systems(
                Update,
                (start_sfx_frame, tick_music_fades, sync_channel_volumes),
            );
    }
}
