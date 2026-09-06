//! Timed stat effects with stacking (`source/key/value/duration/max_stacks`).

use serde::{Deserialize, Serialize};

/// One effect instance. `duration <= 0` means permanent.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Buff {
    pub key: String,
    pub value: f32,
    pub duration: f32,
    pub stacks: u32,
    pub max_stacks: u32,
    pub source: String,
}

impl Buff {
    pub fn new(key: impl Into<String>, value: f32, duration: f32) -> Self {
        Self {
            key: key.into(),
            value,
            duration,
            stacks: 1,
            max_stacks: 1,
            source: String::new(),
        }
    }

    pub fn stacked(mut self, max: u32) -> Self {
        self.max_stacks = max.max(1);
        self
    }

    pub fn sourced(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }
}

/// Live effect set. Same keys merge (stacks clamp, duration refreshes
/// upward); sources are labels only.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct BuffList {
    buffs: Vec<Buff>,
}

impl BuffList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, incoming: Buff) {
        match self.buffs.iter_mut().find(|b| b.key == incoming.key) {
            Some(cur) => {
                cur.stacks =
                    (cur.stacks + incoming.stacks).min(cur.max_stacks.max(incoming.max_stacks));
                cur.max_stacks = cur.max_stacks.max(incoming.max_stacks);
                cur.duration = cur.duration.max(incoming.duration);
                cur.value = incoming.value;
            }
            None => self.buffs.push(incoming),
        }
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let n = self.buffs.len();
        self.buffs.retain(|b| b.key != key);
        self.buffs.len() != n
    }

    /// Sum of value x stacks for `key`.
    pub fn sum(&self, key: &str) -> f32 {
        self.buffs
            .iter()
            .filter(|b| b.key == key)
            .map(|b| b.value * b.stacks as f32)
            .sum()
    }

    pub fn has(&self, key: &str) -> bool {
        self.buffs.iter().any(|b| b.key == key)
    }

    /// Advance timers; returns expired keys.
    pub fn tick(&mut self, dt: f32) -> Vec<String> {
        let mut out = Vec::new();
        self.buffs.retain_mut(|b| {
            if b.duration > 0.0 {
                b.duration -= dt;
                if b.duration <= 0.0 {
                    out.push(b.key.clone());
                    return false;
                }
            }
            true
        });
        out
    }
}

/// Scale a duration by status resistance (0..1).
pub fn resist_duration(duration: f32, resistance: f32) -> f32 {
    (duration * (1.0 - resistance.clamp(0.0, 1.0))).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_clamp_and_refresh() {
        let mut l = BuffList::new();
        l.add(Buff::new("might", 10.0, 3.0).stacked(3));
        l.add(Buff::new("might", 10.0, 5.0).stacked(3));
        l.add(Buff::new("might", 10.0, 5.0).stacked(3));
        l.add(Buff::new("might", 10.0, 5.0).stacked(3));
        assert_eq!(l.sum("might"), 30.0);
        assert_eq!(l.buffs[0].duration, 5.0);
    }

    #[test]
    fn expiry_and_permanent() {
        let mut l = BuffList::new();
        l.add(Buff::new("haste", 1.0, 1.0));
        l.add(Buff::new("aura", 2.0, 0.0));
        assert_eq!(l.tick(1.5), vec!["haste".to_string()]);
        assert!(l.has("aura"));
        assert!(l.remove("aura"));
    }
}
