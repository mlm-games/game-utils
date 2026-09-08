//! Repame (repose stack) game-feel utilities over `repame-sim`.
//!
//! Sim-side counterpart to `game-utils-bevy` (which is frozen for
//! Bevy/`repose-bevy` games). Everything here steps on [`repame_sim::Sim`]
//! fixed ticks with `glam` math — no renderer, window, or audio types.
//!
//! Rendering/audio/UI are delegated, not reimplemented:
//! - sprites/viewport → `repame-sprite` (snapshot in, pixels out)
//! - particles/trauma/flash/transitions → `repame-fx`
//! - sound/music → `repame-audio`
//! - widgets/shell → `repose-ui` / `repame-shell`
//!
//! Quick start:
//!
//! ```rust
//! use game_utils_repame::{init_time_resources, register_systems};
//! use repame_sim::Sim;
//!
//! let mut sim = Sim::with_default_step();
//! init_time_resources(&mut sim.world);
//! register_systems(&mut sim);
//! ```
//!
//! Setup honesty: [`init_resources`] only covers the arg-free time
//! resources ([`sim_time`]). `pooling` needs a type parameter + `max_size`
//! (`EntityPool::<M>::new`), while `save`/`i18n`/`loading` take arguments
//! — so those each have their own `register_*` constructor which the game
//! calls explicitly. See [`init_time_resources`], the gap-free name.

pub mod feel;
pub mod i18n;
pub mod loading;
pub mod pooling;
pub mod save;
pub mod sim_time;

pub use feel::{Recoil, knockback};
pub use i18n::{I18nStrings, register_i18n};
pub use loading::{LoadingProgress, register_loading};
pub use pooling::{DEFAULT_MAX_SIZE, EntityPool, ObjectPool, PoolHidden, scrub_dead};
pub use save::{SaveResource, SaveResult, register_save};
pub use sim_time::{FeelIntensity, HitStop, SlowMotion, TimeScaleControl};

pub use game_utils::{
    achievements as core_achievements, codex as core_codex, i18n as core_i18n,
    math_utils, profiles as core_profiles, save as core_save,
    save_store as core_save_store, stats as core_stats, unlock as core_unlock,
    weighted as core_weighted,
};

use bevy_ecs::prelude::World;
use repame_sim::Sim;

/// Insert the arg-free time resources ([`sim_time`]). Call once at boot
/// before first tick. Prefer this honest name: `pooling` needs a type
/// parameter + `max_size`, and `save`/`i18n`/`loading` take constructor
/// arguments, so those use their own `register_*` fns instead of being
/// silently skipped here.
pub fn init_time_resources(world: &mut World) {
    sim_time::init_resources(world);
}

/// Insert all resources. Call once at boot before first tick.
///
/// Currently identical to [`init_time_resources`]; only `sim_time` has
/// arg-free resources. Kept for compatibility.
#[deprecated(since = "0.1.2", note = "use `init_time_resources`; pooling/save/i18n/loading need their own `register_*` calls")]
pub fn init_resources(world: &mut World) {
    init_time_resources(world);
}

/// Register all arg-free per-step systems on a [`Sim`]: time-effect
/// ticking ([`sim_time`]) + recoil ticking ([`feel`]). `pooling` is a
/// resource-only helper (no systems); `save`/`i18n`/`loading` are plain
/// data resources.
pub fn register_systems(sim: &mut Sim) {
    sim_time::register_systems(sim);
    feel::register_systems(sim);
}
