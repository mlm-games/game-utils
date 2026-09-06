//! Damage-over-time ticks (burning, poison, bleed).

use serde::{Deserialize, Serialize};

/// One periodic damage source. `ticks_left = u32::MAX` burns forever.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Dot {
    pub kind: String,
    pub per_tick: f32,
    pub interval: f32,
    pub acc: f32,
    pub ticks_left: u32,
    pub source: String,
}

impl Dot {
    pub fn new(kind: impl Into<String>, per_tick: f32, interval: f32, ticks: u32) -> Self {
        Self {
            kind: kind.into(),
            per_tick,
            interval: interval.max(0.01),
            acc: 0.0,
            ticks_left: ticks,
            source: String::new(),
        }
    }

    /// Merge another instance of the same ailment: damage sums,
    /// ticks refresh upward, interval takes the faster.
    pub fn merge(&mut self, o: &Dot) {
        self.per_tick += o.per_tick;
        self.ticks_left = self.ticks_left.max(o.ticks_left);
        self.interval = self.interval.min(o.interval);
    }
}

/// Live DoT set. `tick` returns (kind, amount, source) per fired tick.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Dots {
    dots: Vec<Dot>,
}

impl Dots {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, dot: Dot) {
        self.dots.push(dot);
    }

    /// Add, merging into a same-kind live instance when present.
    pub fn merge(&mut self, dot: Dot) {
        match self.dots.iter_mut().find(|d| d.kind == dot.kind) {
            Some(cur) => cur.merge(&dot),
            None => self.dots.push(dot),
        }
    }

    pub fn clear_kind(&mut self, kind: &str) {
        self.dots.retain(|d| d.kind != kind);
    }

    pub fn tick(&mut self, dt: f32) -> Vec<(String, f32, String)> {
        let mut out = Vec::new();
        self.dots.retain_mut(|d| {
            if d.ticks_left == 0 {
                return false;
            }
            d.acc += dt;
            while d.acc >= d.interval && d.ticks_left > 0 {
                d.acc -= d.interval;
                d.ticks_left -= 1;
                out.push((d.kind.clone(), d.per_tick, d.source.clone()));
            }
            d.ticks_left > 0
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_then_expires() {
        let mut d = Dots::new();
        d.add(Dot::new("burn", 5.0, 1.0, 3));
        assert_eq!(d.tick(1.0).len(), 1);
        assert_eq!(d.tick(2.5).len(), 2);
        assert!(d.tick(10.0).is_empty());
    }

    #[test]
    fn merge_sums_damage_refreshes_ticks() {
        let mut d = Dots::new();
        d.merge(Dot::new("burn", 5.0, 1.0, 2));
        d.merge(Dot::new("burn", 3.0, 1.0, 5));
        assert_eq!(d.tick(1.0), vec![("burn".to_string(), 8.0, String::new())]);
        assert_eq!(d.dots[0].ticks_left, 4);
    }

    #[test]
    fn clear_kind_removes() {
        let mut d = Dots::new();
        d.add(Dot::new("burn", 5.0, 1.0, 9));
        d.add(Dot::new("poison", 2.0, 1.0, 9));
        d.clear_kind("burn");
        let ticks = d.tick(1.0);
        assert_eq!(ticks.len(), 1);
        assert_eq!(ticks[0].0, "poison");
    }
}
