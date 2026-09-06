//! Quest log plus dialogue runtime bridge (bubbles-dialogue).

pub mod dialogue;
pub mod quest;

pub use quest::{
    Kind, ObjectiveDef, QuestDef, QuestEvent, QuestLog, QuestState, Rewards, Stage, StartError,
    render,
};
