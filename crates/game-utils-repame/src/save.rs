//! Sim-side save helpers over the engine-agnostic
//! [`game_utils::save::SaveManager`].
//!
//! Thin [`Resource`] wrapper plus load/save helpers. Autosave timers and
//! hotkeys live game-side (drive from `ShellHooks::on_frame` or
//! `repame-input` bindings).

use std::ops::{Deref, DerefMut};

use game_utils::save::{SaveManager, Versioned};
use game_utils::storage::{FsStorage, Storage};
use bevy_ecs::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Bevy-agnostic [`SaveManager`] as a sim resource. Generic over the
/// storage backend (defaults to [`FsStorage`]): pass a wasm `ropfs` sync
/// (`localStorage`) or custom backend via [`SaveResource::new_with_storage`] /
/// [`SaveResource::from_manager`].
#[derive(Resource, Clone)]
pub struct SaveResource<S: Storage = FsStorage>(pub SaveManager<S>);

impl SaveResource<FsStorage> {
    pub fn new(
        qualifier: &'static str,
        org: &'static str,
        app: &'static str,
        file_name: &'static str,
        current_version: u32,
    ) -> Self {
        Self(SaveManager::new(
            qualifier,
            org,
            app,
            file_name,
            current_version,
        ))
    }
}

impl<S: Storage> SaveResource<S> {
    /// Build with an explicit storage backend (wasm `ropfs` sync shim, in-memory,
    /// encrypted, …). The plain [`SaveResource::new`] keeps working for
    /// native `fs` games.
    pub fn new_with_storage(
        qualifier: &'static str,
        org: &'static str,
        app: &'static str,
        file_name: &'static str,
        current_version: u32,
        storage: S,
    ) -> Self {
        Self(SaveManager::new_with_storage(
            qualifier,
            org,
            app,
            file_name,
            current_version,
            storage,
        ))
    }

    /// Wrap an already-configured manager.
    pub fn from_manager(manager: SaveManager<S>) -> Self {
        Self(manager)
    }

    /// Load `T`, falling back to `Default` on any error.
    pub fn load_or_default<T>(&self) -> T
    where
        T: DeserializeOwned + Versioned + Default,
    {
        self.0.load::<T>()
    }

    pub fn save_now<T>(&self, data: &T) -> SaveResult
    where
        T: Serialize,
    {
        self.0.save(data)
    }

    /// Version-stamped save: sets `data.version = current_version` before
    /// serializing. Prefer this for `Versioned` types: raw `save_now`
    /// writes whatever stamp `data` carries, so the next load migrates
    /// from the wrong base.
    pub fn save_now_versioned<T>(&self, data: &mut T) -> SaveResult
    where
        T: Serialize + Versioned,
    {
        self.0.save_versioned(data)
    }
}

impl<S: Storage> Deref for SaveResource<S> {
    type Target = SaveManager<S>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S: Storage> DerefMut for SaveResource<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Insert the save manager and the loaded data `T` as resources.
pub fn register_save<T, S: Storage>(world: &mut World, manager: SaveResource<S>)
where
    T: Resource + DeserializeOwned + Versioned + Default,
{
    let data: T = manager.load_or_default();
    world.insert_resource(data);
    world.insert_resource(manager);
}

/// anyhow-free result alias for games that don't depend on anyhow.
pub type SaveResult = Result<(), String>;

#[cfg(test)]
mod tests {
    use super::*;
    use game_utils::storage::MemoryStorage;
    use serde::Deserialize;

    #[derive(Resource, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Dummy {
        #[serde(default)]
        version: u32,
        value: u32,
    }

    impl Versioned for Dummy {
        fn version(&self) -> u32 {
            self.version
        }
        fn set_version(&mut self, v: u32) {
            self.version = v;
        }
    }

    #[test]
    fn register_save_with_custom_storage() {
        let mut world = World::new();
        let manager = SaveResource::new_with_storage(
            "com",
            "testorg",
            "testapp_custom_storage",
            "save.ron",
            1,
            MemoryStorage::default(),
        );
        register_save::<Dummy, _>(&mut world, manager);
        assert_eq!(world.resource::<Dummy>().value, 0);
        world
            .resource::<SaveResource<MemoryStorage>>()
            .save_now(&Dummy {
                version: 1,
                value: 3,
            })
            .unwrap();
        let loaded: Dummy = world
            .resource::<SaveResource<MemoryStorage>>()
            .load_or_default();
        assert_eq!(loaded.value, 3);
    }
}
