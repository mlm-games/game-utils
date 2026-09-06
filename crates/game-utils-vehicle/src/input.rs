use serde::{Deserialize, Serialize};

/// Normalized driver input. All fields are unitless and clamped to
/// their documented ranges by [`VehicleInput::clamp`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehicleInput {
    /// Longitudinal demand: +1 full throttle, -1 full reverse, 0 coast.
    pub throttle: f32,
    /// Service brake [0, 1].
    pub brake: f32,
    /// Steering demand [-1, 1] (negative = left).
    pub steer: f32,
    /// Clutch disengagement [0, 1]. Only manual gearboxes read it.
    pub clutch: f32,
    /// Handbrake [0, 1]. Rear-biased decel for tight rotation.
    pub handbrake: f32,
    /// Boost demand [0, 1]. Ignored without a boost source.
    pub boost: f32,
}

impl VehicleInput {
    pub fn neutral() -> Self {
        Self {
            throttle: 0.0,
            brake: 0.0,
            steer: 0.0,
            clutch: 0.0,
            handbrake: 0.0,
            boost: 0.0,
        }
    }

    pub fn clamp(&mut self) {
        self.throttle = self.throttle.clamp(-1.0, 1.0);
        self.brake = self.brake.clamp(0.0, 1.0);
        self.steer = self.steer.clamp(-1.0, 1.0);
        self.clutch = self.clutch.clamp(0.0, 1.0);
        self.handbrake = self.handbrake.clamp(0.0, 1.0);
        self.boost = self.boost.clamp(0.0, 1.0);
    }

    pub fn clamped(mut self) -> Self {
        self.clamp();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_clamp() {
        let i = VehicleInput {
            throttle: 2.0,
            brake: -1.0,
            steer: 5.0,
            clutch: 0.5,
            handbrake: 9.0,
            boost: -3.0,
        }
        .clamped();
        assert_eq!(i.throttle, 1.0);
        assert_eq!(i.brake, 0.0);
        assert_eq!(i.steer, 1.0);
        assert_eq!(i.handbrake, 1.0);
        assert_eq!(i.boost, 0.0);
    }

    #[test]
    fn reverse_throttle_survives_clamp() {
        let i = VehicleInput {
            throttle: -0.6,
            ..VehicleInput::neutral()
        }
        .clamped();
        assert_eq!(i.throttle, -0.6);
    }
}
