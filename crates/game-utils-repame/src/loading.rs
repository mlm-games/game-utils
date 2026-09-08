//! Sim-side loading progress: track asset/catalog loads as plain data.
//!
//! Producers bump [`LoadingProgress`] as loads complete; transitions gate
//! on [`LoadingProgress::is_ready`]. No `bevy_asset` dependency.

use bevy_ecs::prelude::*;

/// Fractional load tracker (`done` of `total`).
#[derive(Resource, Debug, Clone, Default)]
pub struct LoadingProgress {
    pub done: u32,
    pub total: u32,
}

impl LoadingProgress {
    pub fn new(total: u32) -> Self {
        Self { done: 0, total }
    }

    pub fn advance(&mut self) {
        self.done = self.done.saturating_add(1);
    }

    pub fn set(&mut self, done: u32, total: u32) {
        self.done = done.min(total);
        self.total = total;
    }

    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 1.0;
        }
        (self.done.min(self.total) as f32) / (self.total as f32)
    }

    pub fn is_ready(&self) -> bool {
        self.done >= self.total
    }
}

/// Insert a fresh tracker expecting `total` loads.
pub fn register_loading(world: &mut World, total: u32) {
    world.insert_resource(LoadingProgress::new(total));
}
