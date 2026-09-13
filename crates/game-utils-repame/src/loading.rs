//! Sim-side loading progress: the engine-agnostic
//! [`LoadingProgress`](game_utils::loading::LoadingProgress) as a
//! `bevy_ecs` resource.
//!
//! The counter math lives in `game-utils` core (shared with Bevy games);
//! this module is only the `Resource` derive + [`register_loading`]
//! constructor for `repame-sim` worlds. No `bevy_asset` dependency.

use bevy_ecs::prelude::*;
use game_utils::loading::LoadingProgress;

/// `Resource` marker for the core counter, so games can
/// `world.init_resource` / `Res<LoadingProgress>` it directly.
#[derive(Resource, Debug, Clone, Default)]
pub struct LoadingResource(pub LoadingProgress);

impl LoadingResource {
    pub fn new(total: u32) -> Self {
        Self(LoadingProgress::new(total))
    }
}

impl std::ops::Deref for LoadingResource {
    type Target = LoadingProgress;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for LoadingResource {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Insert a fresh tracker expecting `total` loads.
pub fn register_loading(world: &mut World, total: u32) {
    world.insert_resource(LoadingResource::new(total));
}
