pub mod audio;
pub mod camera_follow;
pub mod center_pivot;
pub mod game_feel;
pub mod hit_flash;
pub mod hitstop;
pub mod i18n;
pub mod juice;
pub mod pooling;
pub mod post_process;
pub mod save;
pub mod screen_effects;
pub mod transitions;
pub mod ui_effects;
pub mod vfx;

pub use game_utils::{
    achievements as core_achievements, codex as core_codex, i18n as core_i18n, math_utils,
    profiles as core_profiles, save as core_save, save_store as core_save_store,
    stats as core_stats, unlock as core_unlock, weighted as core_weighted,
};

use std::marker::PhantomData;

use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;

/// Bundles all bevy-specific game-feel plugins.
///
/// Generic over the game's [`FreelyMutableState`] type so the bundled [`transitions::TransitionsPlugin`]
/// knows which state to drive. Configure i18n translations (keys + embedded FTL) via
/// [`i18n::I18nPlugin`] passed to [`EcosystemPlugin::new`].
pub struct EcosystemPlugin<S: FreelyMutableState> {
    pub i18n: i18n::I18nPlugin,
    _marker: PhantomData<S>,
}

impl<S: FreelyMutableState> EcosystemPlugin<S> {
    pub fn new(i18n: i18n::I18nPlugin) -> Self {
        Self {
            i18n,
            _marker: PhantomData,
        }
    }
}

impl<S: FreelyMutableState> Default for EcosystemPlugin<S> {
    fn default() -> Self {
        Self::new(i18n::I18nPlugin::default())
    }
}

impl<S: FreelyMutableState> Plugin for EcosystemPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            audio::AudioPlugin,
            camera_follow::CameraFollowPlugin,
            center_pivot::CenterPivotPlugin,
            game_feel::GameFeelPlugin,
            self.i18n.clone(),
            hit_flash::HitFlashPlugin,
            hitstop::HitStopPlugin,
            juice::JuicePlugin,
            post_process::ScreenEffectsPostProcessPlugin,
            screen_effects::ScreenEffectsPlugin,
            transitions::TransitionsPlugin::<S>::default(),
            ui_effects::UiEffectsPlugin,
            vfx::VfxPlugin,
        ));
    }
}
