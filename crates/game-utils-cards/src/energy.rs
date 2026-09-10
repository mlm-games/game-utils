use serde::{Deserialize, Serialize};

/// Play cost. `Fixed` adjusts flat and clamps at zero; `X` spends
/// everything; `Scaled` grows with a game quantity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cost {
    #[serde(rename = "fixed")]
    Fixed(i32),
    #[serde(rename = "x")]
    X,
    #[serde(rename = "scaled")]
    Scaled {
        base: i32,
        per_unit: i32,
        units: u32,
    },
}

impl Default for Cost {
    fn default() -> Self {
        Self::Fixed(0)
    }
}

impl Cost {
    pub fn fixed(v: i32) -> Self {
        Self::Fixed(v)
    }

    pub fn x() -> Self {
        Self::X
    }

    pub fn scaled(base: i32, per_unit: i32, units: u32) -> Self {
        Self::Scaled {
            base,
            per_unit,
            units,
        }
    }

    pub fn is_x(&self) -> bool {
        matches!(self, Self::X)
    }

    /// Raw (unadjusted) value. `X` reports 0 since it spends everything.
    pub fn raw(&self) -> i32 {
        match self {
            Self::Fixed(v) => *v,
            Self::X => 0,
            Self::Scaled {
                base,
                per_unit,
                units,
            } => base + per_unit * (*units as i32),
        }
    }

    /// Value after flat adjustments (discounts are negative), clamped at 0.
    pub fn effective(&self, adjustments: &[i32]) -> i32 {
        if self.is_x() {
            return 0;
        }
        let adj: i32 = adjustments.iter().sum();
        (self.raw() + adj).max(0)
    }

    /// Value after flat adjustments and a multiplier, clamped at 0.
    /// A negative multiplier can't drive the result below 0 (paying a
    /// negative cost would credit resources downstream).
    pub fn effective_scaled(&self, adjustments: &[i32], multiplier: f32) -> i32 {
        if self.is_x() {
            return 0;
        }
        (((self.effective(adjustments) as f32) * multiplier).round() as i32).max(0)
    }
}

/// Single named resource pool (energy, mana, action points, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePool {
    pub current: i32,
    pub max: i32,
}

/// Convenience alias; `EnergyPool` and `ResourcePool` are the same type.
pub type EnergyPool = ResourcePool;

impl ResourcePool {
    pub fn new(max: i32) -> Self {
        Self { current: max, max }
    }

    pub fn with_current(max: i32, current: i32) -> Self {
        Self {
            current: current.clamp(0, max.max(0)),
            max: max.max(0),
        }
    }

    pub fn can_pay(&self, cost: &Cost, adjustments: &[i32]) -> bool {
        if cost.is_x() {
            return self.current > 0;
        }
        self.current >= cost.effective(adjustments)
    }

    /// Pay `cost`. False (unmutated) if unaffordable; `X` drains all.
    pub fn pay(&mut self, cost: &Cost, adjustments: &[i32]) -> bool {
        if cost.is_x() {
            if self.current <= 0 {
                return false;
            }
            self.current = 0;
            return true;
        }
        let eff = cost.effective(adjustments);
        if self.current < eff {
            return false;
        }
        self.current -= eff;
        true
    }

    pub fn refill(&mut self) {
        self.current = self.max;
    }

    pub fn gain(&mut self, amount: i32) {
        self.current = (self.current + amount).clamp(0, self.max);
    }

    pub fn add_max(&mut self, delta: i32) {
        self.max = (self.max + delta).max(0);
        self.current = self.current.min(self.max);
    }
}

impl Default for ResourcePool {
    fn default() -> Self {
        Self::new(3)
    }
}

/// Multi-resource wallet with game-defined keys. Single-resource
/// games should use [`ResourcePool`] instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceMap {
    current: std::collections::HashMap<String, i32>,
    max: std::collections::HashMap<String, i32>,
}

impl ResourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_max(&mut self, key: &str, max: i32) {
        let max = max.max(0);
        self.max.insert(key.to_string(), max);
        let cur = self.current.entry(key.to_string()).or_insert(max);
        *cur = (*cur).clamp(0, max);
    }

    pub fn set(&mut self, key: &str, current: i32) {
        let max = self.max.get(key).copied().unwrap_or(i32::MAX);
        self.current.insert(key.to_string(), current.clamp(0, max));
    }

    pub fn get(&self, key: &str) -> i32 {
        self.current.get(key).copied().unwrap_or(0)
    }

    pub fn max_of(&self, key: &str) -> i32 {
        self.max.get(key).copied().unwrap_or(0)
    }

    pub fn can_pay(&self, costs: &[(&str, i32)]) -> bool {
        costs.iter().all(|(k, v)| self.get(k) >= (*v).max(0))
    }

    /// Pay all parts atomically, or nothing on failure.
    pub fn pay_all(&mut self, costs: &[(&str, i32)]) -> bool {
        if !self.can_pay(costs) {
            return false;
        }
        for (k, v) in costs {
            let cur = self.get(k);
            self.current.insert(k.to_string(), cur - (*v).max(0));
        }
        true
    }

    pub fn gain(&mut self, key: &str, amount: i32) {
        let max = self.max.get(key).copied().unwrap_or(i32::MAX);
        let cur = self.get(key);
        self.current
            .insert(key.to_string(), (cur + amount).clamp(0, max));
    }

    pub fn refill(&mut self, key: &str) {
        if let Some(max) = self.max.get(key).copied() {
            self.current.insert(key.to_string(), max);
        }
    }

    pub fn refill_all(&mut self) {
        let keys: Vec<String> = self.max.keys().cloned().collect();
        for k in keys {
            self.refill(&k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_effective() {
        assert_eq!(Cost::Fixed(3).effective(&[]), 3);
        assert_eq!(Cost::Fixed(3).effective(&[-1]), 2);
        assert_eq!(Cost::Fixed(1).effective(&[-5]), 0);
        assert!(Cost::X.is_x());
        assert_eq!(Cost::scaled(2, 1, 3).raw(), 5);
        assert_eq!(Cost::scaled(2, 1, 3).effective(&[-1]), 4);
        assert_eq!(Cost::Fixed(3).effective_scaled(&[], 0.5), 2);
        assert_eq!(Cost::Fixed(3).effective_scaled(&[], -1.0), 0);
    }

    #[test]
    fn resource_pool_pay() {
        let mut p = ResourcePool::new(4);
        assert!(p.can_pay(&Cost::Fixed(3), &[]));
        assert!(p.pay(&Cost::Fixed(3), &[]));
        assert_eq!(p.current, 1);
        assert!(!p.pay(&Cost::Fixed(3), &[]));
        assert_eq!(p.current, 1);
        p.refill();
        assert_eq!(p.current, 4);
        p.gain(-10);
        assert_eq!(p.current, 0);
    }

    #[test]
    fn resource_pool_x_cost() {
        let mut p = ResourcePool::with_current(3, 2);
        assert!(p.pay(&Cost::X, &[]));
        assert_eq!(p.current, 0);
        assert!(!p.pay(&Cost::X, &[]));
    }

    #[test]
    fn resource_map_atomic() {
        let mut m = ResourceMap::new();
        m.set_max("gold", 10);
        m.set_max("gems", 3);
        m.set("gold", 5);
        assert!(m.can_pay(&[("gold", 3), ("gems", 3)]));
        assert!(m.pay_all(&[("gold", 3), ("gems", 1)]));
        assert_eq!(m.get("gold"), 2);
        assert!(!m.pay_all(&[("gold", 3)]));
        assert_eq!(m.get("gold"), 2);
        m.refill_all();
        assert_eq!(m.get("gold"), 10);
    }
}
