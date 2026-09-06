//! Engine-free agent AI: state machines, steering, perception, nav graphs.
//!
//! Waypoint graph search delegates to the external `pathfinding` crate;
//! tile grids live in `game-utils-grid`. Fight behavior (targeting,
//! telegraphs, wave direction) lives in `game-utils-combat`.

pub mod fsm;
pub mod navgraph;
pub mod perception;
pub mod steering;

pub use fsm::{Fsm, FsmEvent, Transition};
pub use navgraph::{NavGraph, smooth_path};
pub use perception::{Senses, Stimuli, Stimulus, Tracker};
pub use steering::{
    PathCursor, PathMode, align, arrive, cohesion, evade, flee, follow_path, pursue, seek,
    separation, wander,
};
