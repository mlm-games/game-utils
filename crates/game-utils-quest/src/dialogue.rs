//! Dialogue via bubbles-dialogue plus quest flag sync.
//!
//! Scripts stay in `.bub` sources compiled by bubbles; this module maps
//! quest state into dialogue variables so choices can gate on progress:
//! `$quest_<id>` is the stage number (0 inactive, 1 active, 2 completable,
//! 3 completed, 4 failed), `$quest_<id>_<objective>` the progress count,
//! `$quest_tracked` the tracked id (or empty).

pub use bubbles::{
    DialogueError, DialogueEvent, HashMapStorage, Runner, RunnerSnapshot, Value, VariableStorage,
    compile,
};

use crate::quest::{QuestLog, Stage};

/// Variable holding a quest stage (`$quest_<id>`).
pub fn quest_flag(id: &str) -> String {
    format!("quest_{id}")
}

/// Variable holding objective progress (`$quest_<id>_<objective>`).
pub fn objective_flag(id: &str, key: &str) -> String {
    format!("quest_{id}_{key}")
}

fn stage_number(s: Stage) -> f64 {
    match s {
        Stage::Inactive => 0.0,
        Stage::Active => 1.0,
        Stage::Completable => 2.0,
        Stage::Completed => 3.0,
        Stage::Failed => 4.0,
    }
}

/// Push quest stages, objective counts, and the tracked id into storage.
/// Call before `Runner::start` (and after quest changes mid-dialogue).
pub fn sync_quest_flags<S: VariableStorage>(
    log: &QuestLog,
    def: &crate::quest::QuestDef,
    storage: &mut S,
) {
    sync_flags(log, core::slice::from_ref(def), storage);
}

/// Sync flags for several quest defs (objective keys come from defs).
pub fn sync_flags<S: VariableStorage>(
    log: &QuestLog,
    defs: &[crate::quest::QuestDef],
    storage: &mut S,
) {
    for def in defs {
        let stage = log.stage(&def.id);
        storage.set(
            &format!("${}", quest_flag(&def.id)),
            Value::Number(stage_number(stage)),
        );
        if let Some(q) = log.get(&def.id) {
            for (i, obj) in def.objectives.iter().enumerate() {
                let n = q.progress.get(i).copied().unwrap_or(0) as f64;
                storage.set(
                    &format!("${}", objective_flag(&def.id, &obj.key)),
                    Value::Number(n),
                );
            }
        }
    }
    storage.set(
        "$quest_tracked",
        match log.tracked() {
            Some(id) => Value::Text(id.to_owned()),
            None => Value::Text(String::new()),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quest::{Kind, ObjectiveDef, QuestDef, Rewards};

    fn def() -> QuestDef {
        QuestDef {
            id: "rats".into(),
            name: "Rats!".into(),
            kind: Kind::Normal,
            priority: 0,
            requires: vec![],
            time_limit: None,
            repeatable: false,
            objectives: vec![ObjectiveDef {
                key: "kill_rat".into(),
                target: 3,
                description: String::new(),
            }],
            rewards: Rewards::default(),
        }
    }

    #[test]
    fn flags_mirror_log() {
        let d = def();
        let mut log = QuestLog::new();
        log.start(&d).unwrap();
        log.advance(&d, "kill_rat", 2);
        let mut storage = HashMapStorage::new();
        sync_quest_flags(&log, &d, &mut storage);
        assert_eq!(storage.get("$quest_rats"), Some(Value::Number(1.0)));
        assert_eq!(
            storage.get("$quest_rats_kill_rat"),
            Some(Value::Number(2.0))
        );
    }

    #[test]
    fn script_can_gate_on_flags() {
        let src = "title: Gate\n---\n<<if $quest_rats >= 1>>Rats it is.\n<<endif>>\nDone.\n===\n";
        let prog = compile(src).unwrap();
        let d = def();
        let mut log = QuestLog::new();
        log.start(&d).unwrap();
        let mut runner = Runner::new(prog, HashMapStorage::new());
        sync_quest_flags(&log, &d, runner.storage_mut());
        runner.start("Gate").unwrap();
        let mut lines = 0;
        while let Some(_ev) = runner.next_event().unwrap() {
            lines += 1;
        }
        assert!(lines > 0);
    }
}
