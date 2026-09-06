use bevy_app::prelude::*;
use bevy_asset::{AssetServer, UntypedHandle};
use bevy_ecs::prelude::*;
use bevy_state::prelude::*;
use bevy_state::state::FreelyMutableState;
use bevy_state::state::State;
use bevy_time::{Real, Time, Timer, TimerMode};

/// Progress of the current loading operation (0.0..1.0)
#[derive(Resource, Default, Clone)]
pub struct LoadingProgress(pub f32);

/// Tip / flavour text shown during loading (e.g. GenCont tips)
#[derive(Resource, Default, Clone)]
pub struct LoadingTip(pub String);

/// Configuration for a generic loading screen.
/// Generic over the app state `S`. The plugin will watch `LoadingProgress`
/// and a minimum timer before transitioning from `loading_state` to `target_state`.
#[derive(Resource, Clone)]
pub struct LoadingConfig<S: FreelyMutableState> {
    pub loading_state: S,
    pub target_state: S,
    pub min_secs: f32,
    pub tips: Vec<String>,
}

impl<S: FreelyMutableState + Clone> LoadingConfig<S> {
    pub fn new(loading_state: S, target_state: S) -> Self {
        Self {
            loading_state,
            target_state,
            min_secs: 0.5,
            tips: Vec::new(),
        }
    }

    pub fn with_min_secs(mut self, secs: f32) -> Self {
        self.min_secs = secs;
        self
    }

    pub fn with_tips(mut self, tips: Vec<String>) -> Self {
        self.tips = tips;
        self
    }
}

/// Tracks the minimum dwell time for the loading screen.
#[derive(Resource)]
struct LoadingTimer(Timer);

/// Generic loading plugin
pub struct LoadingPlugin<S: FreelyMutableState + Clone>(pub LoadingConfig<S>);

impl<S: FreelyMutableState + Clone + PartialEq + Eq + std::hash::Hash + std::fmt::Debug>
    LoadingPlugin<S>
{
    pub fn new(loading_state: S, target_state: S) -> Self {
        Self(LoadingConfig::new(loading_state, target_state))
    }

    pub fn with_config(config: LoadingConfig<S>) -> Self {
        Self(config)
    }
}

impl<S> Plugin for LoadingPlugin<S>
where
    S: FreelyMutableState + Clone + PartialEq + Eq + std::hash::Hash + std::fmt::Debug + States,
{
    fn build(&self, app: &mut App) {
        let config = self.0.clone();
        let loading_state = config.loading_state.clone();
        let min_secs = config.min_secs;
        app.insert_resource(config)
            .insert_resource(LoadingProgress(0.0))
            .init_resource::<LoadingTip>()
            .insert_resource(LoadingTimer(Timer::from_seconds(min_secs, TimerMode::Once)))
            .add_systems(OnEnter(loading_state.clone()), setup_loading::<S>)
            .add_systems(OnExit(loading_state), cleanup_loading)
            .add_systems(Update, tick_loading_generic::<S>);
    }
}

fn setup_loading<S: FreelyMutableState + Clone>(
    mut tip: ResMut<LoadingTip>,
    config: Res<LoadingConfig<S>>,
    mut timer: ResMut<LoadingTimer>,
) {
    timer.0 = Timer::from_seconds(config.min_secs, TimerMode::Once);
    if !config.tips.is_empty() {
        use rand::RngExt;
        let idx = rand::rng().random_range(0..config.tips.len());
        tip.0 = config.tips[idx].clone();
    } else {
        tip.0 = "LOADING...".to_string();
    }
}

fn cleanup_loading(mut progress: ResMut<LoadingProgress>) {
    progress.0 = 0.0;
}

fn tick_loading_generic<S: FreelyMutableState + Clone>(
    state: Res<State<S>>,
    time: Res<Time<Real>>,
    mut timer: ResMut<LoadingTimer>,
    progress: Res<LoadingProgress>,
    config: Res<LoadingConfig<S>>,
    mut next_state: ResMut<NextState<S>>,
) {
    if *state.get() != config.loading_state {
        return;
    }
    timer.0.tick(time.delta());
    if progress.0 >= 1.0 && timer.0.is_finished() {
        next_state.set(config.target_state.clone());
    }
}

/// Helper to compute progress from `AssetsLoading`-style handles.
///
/// Returns 0.0..1.0 based on how many handles are loaded.
pub fn assets_progress(handles: &[UntypedHandle], asset_server: &AssetServer) -> f32 {
    if handles.is_empty() {
        return 1.0;
    }
    let loaded = handles
        .iter()
        .filter(|h| asset_server.is_loaded_with_dependencies(h.id()))
        .count();
    loaded as f32 / handles.len() as f32
}
