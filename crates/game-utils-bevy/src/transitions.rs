use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CircleWipeDirection {
    /// The black circle grows from the center outwards to cover the scene.
    #[default]
    Expand,
    /// The black circle shrinks from the edges towards the center to cover the scene.
    Contract,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransitionKind {
    #[default]
    Fade,
    CircleWipe(CircleWipeDirection),
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionPhase {
    #[default]
    Idle,
    Covering,
    Uncovering,
}

/// Drives a fade/circle-wipe between app states. Generic over the game's state type.
#[derive(Resource)]
pub struct Transition<S: FreelyMutableState> {
    pub active: bool,
    pub kind: TransitionKind,
    pub phase: TransitionPhase,
    pub progress: f32,
    pub speed: f32,
    pub pending_state: Option<S>,
    pub overlay_alpha: f32,
    pub circle_progress: f32,
    pub block_input: bool,
}

impl<S: FreelyMutableState> Default for Transition<S> {
    fn default() -> Self {
        Self {
            active: false,
            kind: TransitionKind::Fade,
            phase: TransitionPhase::Idle,
            progress: 0.0,
            speed: 2.5,
            pending_state: None,
            overlay_alpha: 0.0,
            circle_progress: 0.0,
            block_input: false,
        }
    }
}

impl<S: FreelyMutableState> Transition<S> {
    pub fn begin_to_state(&mut self, next: S) {
        self.active = true;
        self.phase = TransitionPhase::Covering;
        self.progress = 0.0;
        self.pending_state = Some(next);
        self.kind = TransitionKind::Fade;
        self.block_input = true;
    }

    pub fn begin_to_state_with(&mut self, next: S, kind: TransitionKind) {
        self.active = true;
        self.phase = TransitionPhase::Covering;
        self.progress = 0.0;
        self.pending_state = Some(next);
        self.kind = kind;
        self.block_input = true;
    }

    pub fn circle_wipe_progress(&self) -> f32 {
        if matches!(self.kind, TransitionKind::CircleWipe(_)) {
            match self.phase {
                TransitionPhase::Covering => self.progress,
                TransitionPhase::Uncovering => 1.0 - self.progress,
                TransitionPhase::Idle => 0.0,
            }
        } else {
            0.0
        }
    }
}

pub struct Transitions;

impl Transitions {
    pub fn change_scene_with<S: FreelyMutableState>(transition: &mut Transition<S>, next: S) {
        transition.begin_to_state(next);
    }
}

pub struct TransitionsPlugin<S: FreelyMutableState>(PhantomData<S>);

impl<S: FreelyMutableState> Default for TransitionsPlugin<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S: FreelyMutableState> Plugin for TransitionsPlugin<S> {
    fn build(&self, app: &mut App) {
        app.init_resource::<Transition<S>>().add_systems(
            Update,
            (tick_transition::<S>, clear_blocked_inputs::<S>).chain(),
        );
    }
}

fn tick_transition<S: FreelyMutableState>(
    real: Res<Time<Real>>,
    mut tr: ResMut<Transition<S>>,
    mut next_state: ResMut<NextState<S>>,
) {
    if !tr.active {
        tr.overlay_alpha = 0.0;
        tr.circle_progress = (tr.circle_progress - 2.0 * real.delta_secs()).max(0.0);
        tr.block_input = false;
        return;
    }
    let dt = real.delta_secs() * tr.speed;
    match tr.phase {
        TransitionPhase::Covering => {
            tr.progress = (tr.progress + dt).min(1.0);
            let p = tr.progress;
            update_visuals(&mut tr, p, true);
            if tr.progress >= 1.0 {
                if let Some(s) = tr.pending_state.take() {
                    next_state.set(s);
                }
                tr.phase = TransitionPhase::Uncovering;
                tr.progress = 0.0;
            }
        }
        TransitionPhase::Uncovering => {
            tr.progress = (tr.progress + dt).min(1.0);
            let p = tr.progress;
            update_visuals(&mut tr, p, false);
            if tr.progress >= 1.0 {
                tr.active = false;
                tr.phase = TransitionPhase::Idle;
                tr.overlay_alpha = 0.0;
                tr.circle_progress = 0.0;
                tr.block_input = false;
            }
        }
        TransitionPhase::Idle => {}
    }
}

fn update_visuals<S: FreelyMutableState>(tr: &mut Transition<S>, t: f32, covering: bool) {
    match tr.kind {
        TransitionKind::Fade => {
            tr.overlay_alpha = if covering { t } else { 1.0 - t };
            tr.circle_progress = 0.0;
        }
        TransitionKind::CircleWipe(dir) => {
            let radius = match dir {
                CircleWipeDirection::Expand => t,
                CircleWipeDirection::Contract => 1.0 - t,
            };
            tr.circle_progress = if covering { radius } else { 1.0 - radius };
            tr.overlay_alpha = if covering {
                (t * 1.2).min(1.0)
            } else {
                ((1.0 - t) * 1.2).min(1.0)
            };
        }
    }
}

pub fn input_blocked<S: FreelyMutableState>(tr: Res<Transition<S>>) -> bool {
    tr.block_input
}

/// Clears queued key/mouse presses while a transition blocks input, so a press
/// during the wipe/fade can't leak into the new state.
fn clear_blocked_inputs<S: FreelyMutableState>(
    tr: Res<Transition<S>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    if !tr.block_input {
        return;
    }
    keys.reset_all();
    mouse.reset_all();
}
