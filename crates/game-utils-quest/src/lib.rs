//! Quest log plus dialogue runtime bridge (bubbles-dialogue).

pub mod dialogue;
pub mod quest;

pub use dialogue::{objective_flag, quest_flag, sync_flags, sync_quest_flags};
pub use quest::{
    Kind, ObjectiveDef, QuestDef, QuestEvent, QuestLog, QuestState, Rewards, Stage, StartError,
    render,
};
