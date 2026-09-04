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

pub use storage::{EncryptedStorage, FsStorage, MemoryStorage, Storage};
#[cfg(target_arch = "wasm32")]
pub use storage::OpfsStorage;
pub use typed_id::{AchievementId, CategoryId, CodexId, StatId, TypedId, UnlockId};
