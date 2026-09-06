//! Coil-over suspension: spring + bump/rebound damping + anti-roll.
/// Compression 0 = droop, 1 = bottomed. Damping ratios are fractions
/// of critical so one knob retunes across masses.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SuspensionConfig {
    /// Free length from mount to full droop (m).
    pub rest_length: f32,
    /// Usable stroke (m). Bottoming out past this clamps hard.
    pub travel: f32,
    /// Spring rate (N/m).
    pub spring_rate: f32,
    /// Bump damping as a fraction of critical.
    pub bump_ratio: f32,
    /// Rebound damping as a fraction of critical.
    pub rebound_ratio: f32,
    /// Preload as a fraction of travel already compressed at rest.
    pub preload: f32,
}

impl Default for SuspensionConfig {
    fn default() -> Self {
        Self {
            rest_length: 0.45,
            travel: 0.25,
            spring_rate: 35000.0,
            bump_ratio: 0.35,
            rebound_ratio: 0.7,
            preload: 0.1,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct SuspensionState {
    /// 0 = full droop, 1 = fully compressed.
    pub compression: f32,
    pub compression_vel: f32,
    pub grounded: bool,
}

impl SuspensionState {
    /// Advance from a probe hit (or `None` airborne). Returns spring
    /// force (N) for this corner's sprung mass.
    pub fn update(
        &mut self,
        cfg: &SuspensionConfig,
        hit_dist: Option<f32>,
        ray_len: f32,
        sprung_mass: f32,
        dt: f32,
    ) -> f32 {
        let travel = cfg.travel.max(0.01);
        let prev = self.compression;
        match hit_dist {
            Some(d) => {
                // Compression grows as the ground gets closer.
                let c = ((ray_len - d) / travel).clamp(0.0, 1.0);
                self.compression = c;
                self.grounded = c > 0.001;
            }
            None => {
                self.compression = 0.0;
                self.grounded = false;
            }
        }
        if dt > 0.0 {
            // Blow-off: dampers cannot react infinitely fast (and spawn
            // contact must not cannonball the body on first touch).
            self.compression_vel = ((self.compression - prev) / dt).clamp(-4.0, 4.0);
        }
        if !self.grounded {
            return 0.0;
        }
        let mass = sprung_mass.max(1.0);
        let critical = 2.0 * (cfg.spring_rate.max(0.0) * mass).sqrt();
        let ratio = if self.compression_vel >= 0.0 {
            cfg.bump_ratio
        } else {
            cfg.rebound_ratio
        }
        .max(0.0);
        let preload_force = cfg.spring_rate.max(0.0) * travel * cfg.preload.clamp(0.0, 1.0);
        let spring = cfg.spring_rate.max(0.0) * travel * self.compression + preload_force;
        let damper = critical * ratio * self.compression_vel * travel;
        // Damper can only remove energy: never push past the spring.
        (spring + damper).max(0.0)
    }
}

/// Anti-roll bar: transfer force resisting axle compression difference.
pub fn anti_roll_force(stiffness: f32, left_comp: f32, right_comp: f32) -> f32 {
    stiffness.max(0.0) * (left_comp - right_comp) * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspension_supports_weight() {
        // 1-DOF drop must settle at static equilibrium.
        let cfg = SuspensionConfig::default();
        let mut s = SuspensionState::default();
        let ray_len = cfg.rest_length + cfg.travel;
        let mass = 300.0;
        let mut y = ray_len; // mass height above ground
        let mut vy = 0.0;
        let dt = 1.0 / 600.0;
        let mut force = 0.0;
        for _ in 0..6000 {
            // Probe from the mass straight down: hit distance = y.
            force = s.update(&cfg, Some(y), ray_len, mass, dt);
            vy += (force / mass - 9.81) * dt;
            y += vy * dt;
            if y < 0.0 {
                y = 0.0;
                vy = 0.0;
            }
        }
        let weight = mass * 9.81;
        assert!((force - weight).abs() < weight * 0.05, "force = {force}");
        assert!(vy.abs() < 0.15, "vy = {vy}");
        assert!(s.grounded);
        assert!((0.0..=1.0).contains(&s.compression));
    }

    #[test]
    fn suspension_airborne_is_free() {
        let cfg = SuspensionConfig::default();
        let mut s = SuspensionState {
            compression: 0.8,
            compression_vel: 0.0,
            grounded: true,
        };
        assert_eq!(s.update(&cfg, None, 1.0, 300.0, 0.016), 0.0);
        assert!(!s.grounded);
        assert_eq!(s.compression, 0.0);
    }

    #[test]
    fn anti_roll_direction() {
        assert!(anti_roll_force(1000.0, 0.8, 0.2) > 0.0);
        assert_eq!(anti_roll_force(1000.0, 0.5, 0.5), 0.0);
        assert_eq!(anti_roll_force(0.0, 0.8, 0.2), 0.0);
    }
}
