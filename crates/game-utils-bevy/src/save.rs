use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use bevy::prelude::*;
use game_utils::save::Versioned;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Bevy resource wrapper around the bevy-agnostic [`game_utils::save::SaveManager`].
#[derive(Resource, Clone)]
pub struct SaveManager(pub game_utils::save::SaveManager);

impl SaveManager {
    pub fn new(
        qualifier: &'static str,
        org: &'static str,
        app: &'static str,
        file_name: &'static str,
        current_version: u32,
    ) -> Self {
        Self(game_utils::save::SaveManager::new(
            qualifier,
            org,
            app,
            file_name,
            current_version,
        ))
    }
}

impl Deref for SaveManager {
    type Target = game_utils::save::SaveManager;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SaveManager {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Loads the generic save type `T` as a resource and installs F5/F9 save/load hotkeys.
pub struct SavePlugin<T> {
    pub manager: SaveManager,
    _marker: PhantomData<T>,
}

impl<T> SavePlugin<T> {
    pub fn new(manager: SaveManager) -> Self {
        Self {
            manager,
            _marker: PhantomData,
        }
    }
}

impl<T> Plugin for SavePlugin<T>
where
    T: Resource + Clone + Serialize + DeserializeOwned + Versioned + Default,
{
    fn build(&self, app: &mut App) {
        let data = self.manager.load::<T>();
        app.insert_resource(data)
            .insert_resource(self.manager.clone())
            .add_systems(Update, hotkeys::<T>);
    }
}

fn hotkeys<T>(
    keys: Res<ButtonInput<KeyCode>>,
    save: Res<T>,
    manager: Res<SaveManager>,
    mut commands: Commands,
) where
    T: Resource + Serialize + DeserializeOwned + Versioned + Default,
{
    if keys.just_pressed(KeyCode::F5) {
        if let Err(e) = manager.save(&*save) {
            bevy::log::warn!("Save failed: {e}");
        } else {
            bevy::log::info!("Game saved");
        }
    }
    if keys.just_pressed(KeyCode::F9) {
        let loaded = manager.load::<T>();
        commands.insert_resource(loaded);
        bevy::log::info!("Game loaded");
    }
}
