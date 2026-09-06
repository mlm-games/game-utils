//! Gearbox, clutch, differentials. Torque path: engine -> clutch ->
//! gearbox -> center split -> axle diffs -> wheels.

use serde::{Deserialize, Serialize};

/// Gearbox: forward ratios, final drive, reverse, and shift behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gearbox {
    /// Forward gear ratios, index 0 = 1st.
    pub ratios: Vec<f32>,
    pub final_drive: f32,
    pub reverse_ratio: f32,
    /// Seconds a shift takes (torque cut while shifting).
    pub shift_time: f32,
    /// Minimum seconds between shift completions.
    pub shift_cooldown: f32,
    pub automatic: bool,
    pub upshift_rpm: f32,
    pub downshift_rpm: f32,
}

impl Default for Gearbox {
    fn default() -> Self {
        Self {
            ratios: vec![3.1, 2.2, 1.7, 1.3, 1.0, 0.8],
            final_drive: 3.7,
            reverse_ratio: 3.9,
            shift_time: 0.4,
            shift_cooldown: 0.5,
            automatic: true,
            upshift_rpm: 6500.0,
            downshift_rpm: 2500.0,
        }
    }
}

/// -1 = reverse, 0 = neutral, 1..=N = forward gears.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct GearState {
    pub gear: i32,
    pub shift_timer: f32,
    pub cooldown: f32,
}

impl GearState {
    pub fn new() -> Self {
        Self {
            gear: 1,
            shift_timer: 0.0,
            cooldown: 0.0,
        }
    }

    pub fn neutral() -> Self {
        Self {
            gear: 0,
            shift_timer: 0.0,
            cooldown: 0.0,
        }
    }

    pub fn is_shifting(&self) -> bool {
        self.shift_timer > 0.0
    }

    /// Total ratio (gear x final drive) at the current gear.
    /// Neutral and shifting evaluate to 0 (torque cut).
    pub fn ratio(&self, gearbox: &Gearbox) -> f32 {
        if self.is_shifting() || self.gear == 0 {
            return 0.0;
        }
        if self.gear < 0 {
            return -gearbox.reverse_ratio.max(0.0) * gearbox.final_drive.max(0.0);
        }
        let idx = (self.gear - 1) as usize;
        gearbox.ratios.get(idx).copied().unwrap_or(0.0).max(0.0) * gearbox.final_drive.max(0.0)
    }

    pub fn top_gear(&self, gearbox: &Gearbox) -> i32 {
        gearbox.ratios.len() as i32
    }

    fn request(&mut self, gearbox: &Gearbox, gear: i32) -> bool {
        let gear = gear.clamp(-1, self.top_gear(gearbox).max(1));
        if gear == self.gear || self.is_shifting() || self.cooldown > 0.0 {
            return false;
        }
        self.gear = gear;
        self.shift_timer = gearbox.shift_time.max(0.0);
        self.cooldown = gearbox.shift_time.max(0.0) + gearbox.shift_cooldown.max(0.0);
        true
    }

    pub fn shift_up(&mut self, gearbox: &Gearbox) -> bool {
        self.request(gearbox, self.gear + 1)
    }

    pub fn shift_down(&mut self, gearbox: &Gearbox) -> bool {
        self.request(gearbox, self.gear - 1)
    }

    /// Advance timers and run automatic shifting from rpm and throttle.
    /// `at_rest_throttle` selects 1st when nearly stopped.
    pub fn update(&mut self, gearbox: &Gearbox, rpm: f32, throttle: f32, speed: f32, dt: f32) {
        if dt > 0.0 {
            self.shift_timer = (self.shift_timer - dt).max(0.0);
            self.cooldown = (self.cooldown - dt).max(0.0);
        }
        if !gearbox.automatic || self.is_shifting() || self.cooldown > 0.0 {
            return;
        }
        if self.gear >= 1 {
            if rpm >= gearbox.upshift_rpm && self.gear < self.top_gear(gearbox) {
                self.request(gearbox, self.gear + 1);
            } else if rpm <= gearbox.downshift_rpm && self.gear > 1 {
                self.request(gearbox, self.gear - 1);
            }
        } else if self.gear <= 0 && throttle > 0.0 && speed < 0.5 {
            self.request(gearbox, 1);
        }
    }
}

/// Clutch engagement 0..1 approaching demand. `bite_rpm` caps
/// engagement below the friction point so launches slip instead of
/// stalling; 0 disables.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Clutch {
    pub engagement: f32,
    pub engagement_speed: f32,
    pub bite_rpm: f32,
}

impl Default for Clutch {
    fn default() -> Self {
        Self {
            engagement: 1.0,
            engagement_speed: 6.0,
            bite_rpm: 0.0,
        }
    }
}

impl Clutch {
    /// Update toward pedal demand (0 = engaged). Shifting forces
    /// disengagement; `rpm` feeds the bite zone.
    pub fn update(&mut self, pedal: f32, shifting: bool, rpm: f32, dt: f32) {
        let mut target = 1.0 - pedal.clamp(0.0, 1.0);
        if shifting {
            target = 0.0;
        }
        if self.bite_rpm > 0.0 {
            let bite =
                ((rpm - self.bite_rpm * 0.6) / (self.bite_rpm * 0.4).max(1.0)).clamp(0.0, 1.0);
            target = target.min(bite);
        }
        let step = self.engagement_speed.max(0.0) * dt.max(0.0);
        let diff = target - self.engagement;
        self.engagement += diff.clamp(-step, step);
    }
}

/// Active AWD center split: front share slews toward target within
/// base +- range at `response`/s.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CenterDiff {
    pub base_front_split: f32,
    pub variable_range: f32,
    pub response: f32,
}

impl Default for CenterDiff {
    fn default() -> Self {
        Self {
            base_front_split: 0.5,
            variable_range: 0.0,
            response: 2.0,
        }
    }
}

impl CenterDiff {
    /// Slew `current` toward `target`, clamped to the window.
    pub fn update(&self, current: f32, target: f32, dt: f32) -> f32 {
        let lo = (self.base_front_split - self.variable_range).clamp(0.0, 1.0);
        let hi = (self.base_front_split + self.variable_range).clamp(0.0, 1.0);
        let clamped = target.clamp(lo.min(hi), lo.max(hi));
        let step = self.response.max(0.0) * dt.max(0.0);
        current + (clamped - current).clamp(-step, step)
    }
}
/// Differential behavior between the two wheels of a driven axle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Differential {
    /// Even split, always.
    Open,
    /// Locked: slower wheel can take the full axle torque.
    Locked,
    /// Limited slip: biases torque to the slower wheel once the speed
    /// difference passes `engage_ratio` (fraction), up to
    /// `engage_torque` (N m) of transfer.
    LimitedSlip {
        engage_torque: f32,
        engage_ratio: f32,
    },
}

impl Default for Differential {
    fn default() -> Self {
        Self::LimitedSlip {
            engage_torque: 400.0,
            engage_ratio: 0.05,
        }
    }
}

impl Differential {
    /// Split `axle_torque` (N m) into (left, right) by wheel speeds.
    pub fn split(&self, axle_torque: f32, left_speed: f32, right_speed: f32) -> (f32, f32) {
        match *self {
            Self::Open => (axle_torque * 0.5, axle_torque * 0.5),
            Self::Locked => {
                // Locked: total stays available; report even unless one
                // side is clearly slower (it can then take it all).
                let avg = (left_speed + right_speed) * 0.5;
                if (left_speed - avg).abs() < 1e-3 && (right_speed - avg).abs() < 1e-3 {
                    (axle_torque * 0.5, axle_torque * 0.5)
                } else if left_speed < right_speed {
                    (axle_torque, 0.0)
                } else {
                    (0.0, axle_torque)
                }
            }
            Self::LimitedSlip {
                engage_torque,
                engage_ratio,
            } => {
                let mean = (left_speed.abs() + right_speed.abs()) * 0.5;
                let diff = (left_speed - right_speed).abs();
                let slip = if mean > 1.0 { diff / mean } else { 0.0 };
                if slip <= engage_ratio.max(0.0) || axle_torque == 0.0 {
                    return (axle_torque * 0.5, axle_torque * 0.5);
                }
                let transfer =
                    engage_torque.max(0.0).min(axle_torque.abs() * 0.5) * axle_torque.signum();
                // Slower wheel gains.
                if left_speed <= right_speed {
                    (axle_torque * 0.5 + transfer, axle_torque * 0.5 - transfer)
                } else {
                    (axle_torque * 0.5 - transfer, axle_torque * 0.5 + transfer)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gearbox_ratios_and_shifts() {
        let gb = Gearbox::default();
        let mut g = GearState::new();
        assert!(g.ratio(&gb) > 0.0);
        assert!(g.shift_up(&gb));
        assert!(g.is_shifting());
        assert_eq!(g.ratio(&gb), 0.0);
        // Mid-length step: shift finishes but cooldown still blocks.
        g.update(&gb, 3000.0, 1.0, 10.0, 0.5);
        assert!(!g.is_shifting());
        assert_eq!(g.gear, 2);
        assert!(!g.shift_up(&gb));
        // Full settle, then shifting works again.
        g.update(&gb, 3000.0, 1.0, 10.0, 1.0);
        assert!(g.shift_up(&gb));
    }

    #[test]
    fn gearbox_auto_shifts() {
        let gb = Gearbox::default();
        let mut g = GearState::new();
        g.update(&gb, 7000.0, 1.0, 20.0, 0.01);
        assert_eq!(g.gear, 2);
    }

    #[test]
    fn gearbox_reverse_and_neutral() {
        let gb = Gearbox::default();
        let mut g = GearState::neutral();
        assert_eq!(g.ratio(&gb), 0.0);
        g.gear = -1;
        assert!(g.ratio(&gb) < 0.0);
    }

    #[test]
    fn clutch_disengages_on_shift() {
        let mut c = Clutch::default();
        c.update(0.0, true, 3000.0, 1.0);
        assert_eq!(c.engagement, 0.0);
        c.update(0.0, false, 3000.0, 1.0);
        assert_eq!(c.engagement, 1.0);
    }

    #[test]
    fn clutch_bite_slips_at_low_rpm() {
        let mut c = Clutch {
            bite_rpm: 1500.0,
            ..Clutch::default()
        };
        c.engagement = 1.0;
        c.update(0.0, false, 800.0, 1.0);
        assert!(c.engagement < 0.5, "eng = {}", c.engagement);
        c.update(0.0, false, 2000.0, 1.0);
        assert_eq!(c.engagement, 1.0);
    }

    #[test]
    fn center_diff_slews_in_window() {
        let d = CenterDiff {
            base_front_split: 0.4,
            variable_range: 0.2,
            response: 1.0,
        };
        assert_eq!(d.update(0.4, 1.0, 10.0), 0.6);
        assert_eq!(d.update(0.4, 0.0, 10.0), 0.2);
        assert!((d.update(0.4, 1.0, 0.1) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn diff_splits() {
        let (l, r) = Differential::Open.split(100.0, 10.0, 20.0);
        assert_eq!((l, r), (50.0, 50.0));
        let (l, r) = Differential::Locked.split(100.0, 5.0, 20.0);
        assert_eq!((l, r), (100.0, 0.0));
        let lsd = Differential::LimitedSlip {
            engage_torque: 400.0,
            engage_ratio: 0.05,
        };
        let (l, r) = lsd.split(100.0, 10.0, 10.0);
        assert_eq!((l, r), (50.0, 50.0));
        let (l, r) = lsd.split(100.0, 10.0, 30.0);
        assert!(l > r);
    }
}
