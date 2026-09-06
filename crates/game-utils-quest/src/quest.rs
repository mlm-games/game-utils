//! Objectives, stages, tracked quest, rewards, and change events.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One counter objective (`key` reached `target`).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ObjectiveDef {
    pub key: String,
    pub target: u32,
    pub description: String,
}

/// Turn-in rewards. Item ids are game-side (pair with inventory crates).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Rewards {
    pub xp: u32,
    pub items: Vec<(String, u32)>,
    pub loot_table: Option<String>,
}

/// Quest flavor: normal log entries, passive world-state, rumors.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Kind {
    #[default]
    Normal,
    Passive,
    Rumor,
}

/// Static quest data.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct QuestDef {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub priority: i32,
    /// Completed quest ids required to start.
    pub requires: Vec<String>,
    /// Seconds to finish once Active (None = no limit).
    pub time_limit: Option<f32>,
    /// Completing re-arms instead of finishing (dailies).
    pub repeatable: bool,
    pub objectives: Vec<ObjectiveDef>,
    pub rewards: Rewards,
}

/// Lifecycle stage.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Stage {
    #[default]
    Inactive,
    Active,
    Completable,
    Completed,
    Failed,
}

/// Start refusal.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StartError {
    AlreadyPresent,
    RequiresUnmet(Vec<String>),
}

/// Live quest state.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct QuestState {
    pub def: String,
    pub stage: Stage,
    pub progress: Vec<u32>,
    /// Which ending/branch completed it (None until completed).
    pub outcome: Option<String>,
    pub time_left: Option<f32>,
}

/// Log changes, drained by UI/save code.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum QuestEvent {
    Started { id: String },
    ObjectiveDone { id: String, key: String },
    Completable { id: String },
    Completed { id: String, outcome: String },
    Failed { id: String },
    TrackedChanged { id: Option<String> },
}

/// Fill `{key}` slots from params (quest text generation).
pub fn render(template: &str, params: &[(&str, &str)]) -> String {
    let mut out = template.to_owned();
    for (k, v) in params {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// Quest log: start/advance/complete/fail plus one tracked quest.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct QuestLog {
    quests: HashMap<String, QuestState>,
    tracked: Option<String>,
    #[serde(skip, default = "_events")]
    events: Vec<QuestEvent>,
}

fn _events() -> Vec<QuestEvent> {
    Vec::new()
}

impl QuestLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: &str) -> Option<&QuestState> {
        self.quests.get(id)
    }

    pub fn stage(&self, id: &str) -> Stage {
        self.get(id).map(|q| q.stage).unwrap_or(Stage::Inactive)
    }

    pub fn tracked(&self) -> Option<&str> {
        self.tracked.as_deref()
    }

    pub fn drain_events(&mut self) -> Vec<QuestEvent> {
        core::mem::take(&mut self.events)
    }

    fn unmet(&self, def: &QuestDef) -> Vec<String> {
        def.requires
            .iter()
            .filter(|r| self.stage(r) != Stage::Completed)
            .cloned()
            .collect()
    }

    /// Begin a quest. Refuses duplicates and unmet prerequisites.
    pub fn start(&mut self, def: &QuestDef) -> Result<(), StartError> {
        if self.quests.contains_key(&def.id) {
            return Err(StartError::AlreadyPresent);
        }
        let unmet = self.unmet(def);
        if !unmet.is_empty() {
            return Err(StartError::RequiresUnmet(unmet));
        }
        let n = def.objectives.len();
        self.quests.insert(
            def.id.clone(),
            QuestState {
                def: def.id.clone(),
                stage: Stage::Active,
                progress: vec![0; n],
                outcome: None,
                time_left: def.time_limit,
            },
        );
        self.events.push(QuestEvent::Started { id: def.id.clone() });
        if n == 0 {
            self.mark_completable(&def.id);
        }
        Ok(())
    }

    /// Add `amount` to objective `key`. Returns completed keys.
    /// Finishing every objective flips the quest Completable.
    pub fn advance(&mut self, def: &QuestDef, key: &str, amount: u32) -> Vec<String> {
        let (done, finished) = match self.quests.get_mut(&def.id) {
            Some(q) if q.stage == Stage::Active => {
                let mut done = Vec::new();
                for (i, obj) in def.objectives.iter().enumerate() {
                    if obj.key == key && q.progress[i] < obj.target {
                        q.progress[i] = (q.progress[i] + amount).min(obj.target);
                        if q.progress[i] >= obj.target {
                            done.push(key.to_owned());
                        }
                    }
                }
                let finished = def
                    .objectives
                    .iter()
                    .enumerate()
                    .all(|(i, o)| q.progress[i] >= o.target);
                (done, finished)
            }
            _ => return Vec::new(),
        };
        for k in &done {
            self.events.push(QuestEvent::ObjectiveDone {
                id: def.id.clone(),
                key: k.clone(),
            });
        }
        if finished {
            self.mark_completable(&def.id);
        }
        done
    }

    fn mark_completable(&mut self, id: &str) {
        let active = self
            .quests
            .get(id)
            .is_some_and(|q| q.stage == Stage::Active);
        if active {
            self.quests.get_mut(id).unwrap().stage = Stage::Completable;
            self.events.push(QuestEvent::Completable { id: id.into() });
        }
    }

    /// Turn in a Completable quest with an outcome branch.
    /// Returns the rewards. Repeatables re-arm instead of finishing.
    pub fn complete(&mut self, def: &QuestDef, outcome: &str) -> Option<Rewards> {
        let q = self.quests.get_mut(&def.id)?;
        if q.stage != Stage::Completable {
            return None;
        }
        q.outcome = Some(outcome.into());
        self.events.push(QuestEvent::Completed {
            id: def.id.clone(),
            outcome: outcome.into(),
        });
        if def.repeatable {
            q.stage = Stage::Active;
            q.progress.fill(0);
            q.time_left = def.time_limit;
            self.events.push(QuestEvent::Started { id: def.id.clone() });
        } else {
            q.stage = Stage::Completed;
            if self.tracked.as_deref() == Some(&def.id) {
                self.tracked = None;
                self.events.push(QuestEvent::TrackedChanged { id: None });
            }
        }
        Some(def.rewards.clone())
    }

    /// Fail an Active quest.
    pub fn fail(&mut self, id: &str) -> bool {
        match self.quests.get_mut(id) {
            Some(q) if q.stage == Stage::Active => {
                q.stage = Stage::Failed;
                self.events.push(QuestEvent::Failed { id: id.into() });
                true
            }
            _ => false,
        }
    }

    /// Tick timed quests; expiries fail with outcome "timeout".
    pub fn tick(&mut self, dt: f32) {
        let mut expired = Vec::new();
        for (id, q) in self.quests.iter_mut() {
            if q.stage != Stage::Active {
                continue;
            }
            if let Some(t) = q.time_left.as_mut() {
                *t -= dt.max(0.0);
                if *t <= 0.0 {
                    q.stage = Stage::Failed;
                    q.outcome = Some("timeout".into());
                    expired.push(id.clone());
                }
            }
        }
        for id in expired {
            self.events.push(QuestEvent::Failed { id });
        }
    }

    /// Track one active quest (None clears).
    pub fn set_tracked(&mut self, id: Option<&str>) {
        let next = id.map(str::to_owned);
        if self.tracked != next {
            self.tracked = next.clone();
            self.events.push(QuestEvent::TrackedChanged { id: next });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> QuestDef {
        QuestDef {
            id: "rats".into(),
            name: "Rats!".into(),
            kind: Kind::Normal,
            priority: 0,
            requires: vec![],
            time_limit: None,
            repeatable: false,
            objectives: vec![
                ObjectiveDef {
                    key: "kill_rat".into(),
                    target: 3,
                    description: "Kill rats".into(),
                },
                ObjectiveDef {
                    key: "report".into(),
                    target: 1,
                    description: "Report back".into(),
                },
            ],
            rewards: Rewards {
                xp: 50,
                items: vec![("cheese".into(), 2)],
                loot_table: None,
            },
        }
    }

    #[test]
    fn full_lifecycle_with_outcome_and_rewards() {
        let d = def();
        let mut log = QuestLog::new();
        assert!(log.start(&d).is_ok());
        assert_eq!(log.start(&d), Err(StartError::AlreadyPresent));
        log.set_tracked(Some("rats"));
        log.advance(&d, "kill_rat", 3);
        log.advance(&d, "report", 1);
        assert_eq!(log.stage("rats"), Stage::Completable);
        let rw = log.complete(&d, "peaceful").unwrap();
        assert_eq!(rw.xp, 50);
        assert_eq!(rw.items, vec![("cheese".to_string(), 2)]);
        assert_eq!(
            log.get("rats").unwrap().outcome.as_deref(),
            Some("peaceful")
        );
        assert_eq!(log.tracked(), None);
    }

    #[test]
    fn requires_gate_start() {
        let d = def();
        let mut log = QuestLog::new();
        let mut nest = d.clone();
        nest.id = "nest".into();
        nest.requires = vec!["rats".into()];
        assert_eq!(
            log.start(&nest),
            Err(StartError::RequiresUnmet(vec!["rats".into()]))
        );
        log.start(&d).unwrap();
        log.advance(&d, "kill_rat", 3);
        log.advance(&d, "report", 1);
        log.complete(&d, "violent").unwrap();
        assert!(log.start(&nest).is_ok());
    }

    #[test]
    fn timed_quest_expires() {
        let mut d = def();
        d.time_limit = Some(10.0);
        let mut log = QuestLog::new();
        log.start(&d).unwrap();
        log.tick(4.0);
        assert_eq!(log.stage("rats"), Stage::Active);
        log.tick(9.0);
        assert_eq!(log.stage("rats"), Stage::Failed);
        assert_eq!(log.get("rats").unwrap().outcome.as_deref(), Some("timeout"));
    }

    #[test]
    fn repeatable_rearms() {
        let mut d = def();
        d.repeatable = true;
        let mut log = QuestLog::new();
        log.start(&d).unwrap();
        log.advance(&d, "kill_rat", 3);
        log.advance(&d, "report", 1);
        assert!(log.complete(&d, "again").is_some());
        assert_eq!(log.stage("rats"), Stage::Active);
        assert_eq!(log.get("rats").unwrap().progress, vec![0, 0]);
    }

    #[test]
    fn render_fills_slots() {
        assert_eq!(
            render(
                "Slay {n} rats in {where}",
                &[("n", "3"), ("where", "cellar")]
            ),
            "Slay 3 rats in cellar"
        );
    }
}
