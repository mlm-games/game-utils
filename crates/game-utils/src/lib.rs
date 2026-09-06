pub mod achievements;
pub mod codex;
pub mod i18n;
pub mod math_utils;
pub mod profiles;
pub mod save;
pub mod save_store;
pub mod stats;
pub mod storage;
pub mod typed_id;
pub mod unlock;
pub mod weighted;

// Genre crates re-exported behind features.
#[cfg(feature = "grid")]
pub use game_utils_grid as grid;
#[cfg(feature = "inventory")]
pub use game_utils_inventory as inventory;

#[cfg(target_arch = "wasm32")]
pub use storage::OpfsStorage;
pub use storage::{EncryptedStorage, FsStorage, MemoryStorage, Storage};
pub use typed_id::{AchievementId, CategoryId, CodexId, StatId, TypedId, UnlockId};
