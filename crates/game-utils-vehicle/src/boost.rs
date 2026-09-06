use serde::{Deserialize, Serialize};

/// Timed speed burst. Overlapping boosts take the max, never stack.
/// Tick in fixed step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoostPool {
    pub speed_multiplier: f32,
    pub accel_bonus: f32,
    timer: f32,
}

impl Default for BoostPool {
    fn default() -> Self {
        Self::idle()
    }
}

impl BoostPool {
    pub fn idle() -> Self {
        Self {
            speed_multiplier: 1.0,
            accel_bonus: 0.0,
            timer: 0.0,
        }
    }

    pub fn is_boosting(&self) -> bool {
        self.timer > 0.0
    }

    pub fn remaining(&self) -> f32 {
        self.timer.max(0.0)
    }

    /// Add (or refresh) a boost. Stronger effects win per channel;
    /// duration extends to the longest.
    pub fn add_boost(&mut self, speed_multiplier: f32, accel_bonus: f32, duration_secs: f32) {
        if speed_multiplier > self.speed_multiplier {
            self.speed_multiplier = speed_multiplier;
        }
        if accel_bonus > self.accel_bonus {
            self.accel_bonus = accel_bonus;
        }
        self.timer = self.timer.max(duration_secs.max(0.0));
    }

    pub fn tick(&mut self, dt: f32) {
        if self.timer > 0.0 {
            self.timer -= dt.max(0.0);
            if self.timer <= 0.0 {
                *self = Self::idle();
            }
        }
    }

    pub fn clear(&mut self) {
        *self = Self::idle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_max_wins_and_expires() {
        let mut b = BoostPool::idle();
        assert!(!b.is_boosting());
        b.add_boost(1.5, 10.0, 2.0);
        b.add_boost(1.2, 20.0, 1.0);
        assert_eq!(b.speed_multiplier, 1.5);
        assert_eq!(b.accel_bonus, 20.0);
        assert_eq!(b.remaining(), 2.0);
        b.tick(2.5);
        assert!(!b.is_boosting());
        assert_eq!(b.speed_multiplier, 1.0);
    }
}
