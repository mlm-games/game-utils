# game-utils

Reusable game utility crates extracted from the [`my-ecosystem-bevy`](https://github.com/mlm-games/my-ecosystem-template-bevy) template.

The workspace is split into two crates so the generic pieces can be used without pulling in Bevy:

| Crate | Description |
| --- | --- |
| [`game-utils`](crates/game-utils/) | Bevy-agnostic: math helpers (`glam`), generic save manager (RON + platform data dir), crash-safe save store (atomic temp+rename writes, `.bak` rotation, corruption quarantine), weighted random / stable sampling, stats aggregation + unlock conditions, achievement registry, i18n (Fluent) |
| [`game-utils-bevy`](crates/game-utils-bevy/) | Bevy plugins: audio channels + pooled positional SFX with per-frame collapse + music fade, game feel, juice, vfx, screen effects (2D + 3D), post-processing, transitions, UI effects, entity pooling |

## game-utils (bevy-agnostic)

- `math_utils` — `MathUtils` with `smooth_damp`, `approach`, `wave` (uses `glam` so the types are the same ones Bevy uses).
- `save` — `SaveManager` persists any `Serialize` data to RON. Data types implement `Versioned` for version migration. Works without Bevy.
- `i18n` — `LocaleResources` parses Fluent (`.ftl`) strings into a key/value map for the current locale.

## game-utils-bevy

Bundled into a single `EcosystemPlugin<S>` (generic over your app's state type) plus `I18nPlugin`
(configured with your translation keys and embedded FTL) and `SavePlugin<T>` (configured with your
save data type).

```rust
use bevy::prelude::*;
use game_utils_bevy::{
    EcosystemPlugin, I18nPlugin,
    save::{SaveManager, SavePlugin},
    transitions::Transition,
};
use game_utils::save::Versioned;
use serde::{Deserialize, Serialize};

#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash)]
enum AppState { #[default] Menu, Playing }

#[derive(Resource, Clone, Serialize, Deserialize)]
struct SaveData {
    #[serde(default)]
    version: u32,
    best_score: u32,
}
impl Versioned for SaveData {
    fn version(&self) -> u32 { self.version }
    fn set_version(&mut self, v: u32) { self.version = v; }
}
impl Default for SaveData {
    fn default() -> Self { Self { version: 1, best_score: 0 } }
}

const KEYS: &[&str] = &["app-title", "start"];
const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("assets/locales/en/main.ftl")),
    ("es", include_str!("assets/locales/es/main.ftl")),
];

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .init_state::<AppState>()
        .add_plugins(EcosystemPlugin::<AppState>::new(I18nPlugin::new(KEYS, LOCALES)))
        .add_plugins(SavePlugin::<SaveData>::new(SaveManager::new(
            "com", "mlm-games", "my-game", "save.ron", 1,
        )))
        // ...
        .run();
}
```

## License

Dual-licensed under either MIT or Apache-2.0, at your option.
