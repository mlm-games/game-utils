use serde::{Deserialize, Serialize};

/// Normalized torque curve: rpm fraction [0, 1] -> factor [0, 1].
/// Linear interpolation, clamped ends. Empty curves evaluate to 0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorqueCurve {
    /// Sorted `(rpm_fraction, factor)` control points.
    pub points: Vec<(f32, f32)>,
}

impl Default for TorqueCurve {
    fn default() -> Self {
        Self::triangular(0.6)
    }
}

impl TorqueCurve {
    /// Placeholder peak-at-`peak_ratio` curve. Prefer measured tables.
    pub fn triangular(peak_ratio: f32) -> Self {
        let peak = peak_ratio.clamp(0.05, 0.95);
        Self {
            points: vec![(0.0, 0.3), (peak, 1.0), (1.0, 0.4)],
        }
    }

    pub fn flat() -> Self {
        Self {
            points: vec![(0.0, 1.0), (1.0, 1.0)],
        }
    }

    pub fn eval_norm(&self, norm: f32) -> f32 {
        if self.points.is_empty() {
            return 0.0;
        }
        let x = norm.clamp(0.0, 1.0);
        if x <= self.points[0].0 {
            return self.points[0].1.max(0.0);
        }
        for w in self.points.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            if x <= x1 && x1 > x0 {
                let t = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
                return (y0 + (y1 - y0) * t).max(0.0);
            }
        }
        self.points.last().map(|p| p.1.max(0.0)).unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Peak shaft torque in newton-meters.
    pub max_torque: f32,
    pub max_rpm: f32,
    pub idle_rpm: f32,
    pub curve: TorqueCurve,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_torque: 400.0,
            max_rpm: 6000.0,
            idle_rpm: 800.0,
            curve: TorqueCurve::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EngineOutput {
    pub torque: f32,
    pub rpm: f32,
}

impl EngineConfig {
    /// Shaft torque for `throttle` [0, 1] at `rpm` (clamped to range).
    pub fn eval(&self, throttle: f32, rpm: f32) -> EngineOutput {
        let t = throttle.clamp(0.0, 1.0);
        let rpm = rpm.clamp(self.idle_rpm, self.max_rpm);
        let span = (self.max_rpm - self.idle_rpm).max(1.0);
        let norm = (rpm - self.idle_rpm) / span;
        let torque = self.max_torque.max(0.0) * self.curve.eval_norm(norm) * t;
        EngineOutput { torque, rpm }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_curve() {
        let e = EngineConfig::default();
        let at_idle = e.eval(1.0, e.idle_rpm);
        let at_peak = e.eval(1.0, e.idle_rpm + (e.max_rpm - e.idle_rpm) * 0.6);
        let at_max = e.eval(1.0, e.max_rpm);
        assert!(at_peak.torque > at_idle.torque);
        assert!(at_peak.torque > at_max.torque);
        assert_eq!(e.eval(0.0, 3000.0).torque, 0.0);
    }

    #[test]
    fn engine_custom_curve() {
        let e = EngineConfig {
            curve: TorqueCurve {
                points: vec![(0.0, 0.5), (0.5, 0.5), (1.0, 0.5)],
            },
            ..EngineConfig::default()
        };
        let mid = e.eval(1.0, 3400.0);
        assert!((mid.torque - e.max_torque * 0.5).abs() < 1.0);
        let empty = TorqueCurve { points: vec![] };
        assert_eq!(empty.eval_norm(0.5), 0.0);
        assert_eq!(TorqueCurve::flat().eval_norm(0.3), 1.0);
    }
}
