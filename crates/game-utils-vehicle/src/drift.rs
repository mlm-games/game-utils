use serde::{Deserialize, Serialize};

/// Drift charge tiers. Games map tiers to their own rewards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChargeLevel {
    #[default]
    None,
    Tier1,
    Tier2,
}

impl ChargeLevel {
    pub fn rank(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Tier1 => 1,
            Self::Tier2 => 2,
        }
    }
}

/// Tuning for drift charge buildup.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DriftConfig {
    /// Minimum forward speed to start a drift.
    pub min_speed: f32,
    /// Minimum |steer| (-1..1) to start a drift.
    pub min_steer: f32,
    /// Seconds of continuous drift to reach tier 1 / tier 2.
    pub tier1_time: f32,
    pub tier2_time: f32,
}

impl Default for DriftConfig {
    fn default() -> Self {
        Self {
            min_speed: 6.0,
            min_steer: 0.15,
            tier1_time: 1.0,
            tier2_time: 2.2,
        }
    }
}

/// Drift state machine: idle -> drifting -> released. Feed per-step
/// inputs; [`DriftState::release`] collects the tier and resets.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct DriftState {
    pub active: bool,
    pub direction: f32,
    pub charge: f32,
    pub level: ChargeLevel,
}

impl DriftState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_drifting(&self) -> bool {
        self.active
    }

    /// Advance one step: held drift control, steer demand, speed.
    pub fn update(&mut self, cfg: &DriftConfig, want_drift: bool, steer: f32, speed: f32, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        if !self.active {
            if want_drift && steer.abs() >= cfg.min_steer && speed >= cfg.min_speed {
                self.active = true;
                self.direction = steer.signum();
                self.charge = 0.0;
                self.level = ChargeLevel::None;
            }
            return;
        }
        if !want_drift {
            return;
        }
        self.charge += dt;
        if self.charge >= cfg.tier2_time {
            self.level = ChargeLevel::Tier2;
        } else if self.charge >= cfg.tier1_time {
            self.level = ChargeLevel::Tier1;
        }
    }

    /// End the drift, returning the earned tier and resetting.
    pub fn release(&mut self) -> ChargeLevel {
        let level = self.level;
        *self = Self::default();
        level
    }

    pub fn cancel(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_charges_through_tiers() {
        let cfg = DriftConfig::default();
        let mut d = DriftState::new();
        // Won't start below min speed.
        d.update(&cfg, true, 1.0, 0.0, 0.5);
        assert!(!d.is_drifting());
        d.update(&cfg, true, 1.0, 10.0, 0.1);
        assert!(d.is_drifting());
        assert_eq!(d.direction, 1.0);
        d.update(&cfg, true, 1.0, 10.0, cfg.tier1_time);
        assert_eq!(d.level, ChargeLevel::Tier1);
        d.update(&cfg, true, 1.0, 10.0, cfg.tier2_time);
        assert_eq!(d.level, ChargeLevel::Tier2);
        assert_eq!(d.release(), ChargeLevel::Tier2);
        assert!(!d.is_drifting());
    }

    #[test]
    fn drift_release_without_charge() {
        let mut d = DriftState::new();
        assert_eq!(d.release(), ChargeLevel::None);
    }
}
