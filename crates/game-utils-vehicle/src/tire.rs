//! Tires: Magic-Formula-lite friction with load sensitivity and a
//! friction ellipse coupling longitudinal/lateral slip.

use serde::{Deserialize, Serialize};

/// Per-surface grip response. Split axes let loose surfaces slide
/// sideways while still putting power down.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SurfaceGrip {
    /// Scales the longitudinal friction peak.
    pub longitudinal_scale: f32,
    /// Scales the lateral friction peak.
    pub lateral_scale: f32,
    /// Scales rolling resistance.
    pub drag_scale: f32,
    /// Scales tire stiffness (loose surfaces feel vague).
    pub stiffness_scale: f32,
}

impl Default for SurfaceGrip {
    fn default() -> Self {
        Self {
            longitudinal_scale: 1.0,
            lateral_scale: 1.0,
            drag_scale: 1.0,
            stiffness_scale: 1.0,
        }
    }
}

/// Tire compound and geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TireConfig {
    /// Wheel radius (m). Used for spin/slip conversion.
    pub radius: f32,
    /// Tire width (m). Scales the contact patch / peak force.
    pub width: f32,
    /// Longitudinal friction peak (u).
    pub mu_long: f32,
    /// Lateral friction peak (u).
    pub mu_lat: f32,
    /// Magic-Formula shape factor (higher = sharper peak, snappier).
    pub stiffness: f32,
    /// How much peak grip fades with load: `u_eff = u / (1 + sensitivity * load_kN)`.
    pub load_sensitivity: f32,
    /// Base rolling resistance coefficient.
    pub rolling_resist: f32,
    /// Per-surface response, indexed by surface id; out-of-range ids
    /// use [`SurfaceGrip::default`].
    pub surfaces: Vec<SurfaceGrip>,
}

impl Default for TireConfig {
    fn default() -> Self {
        Self {
            radius: 0.33,
            width: 0.245,
            mu_long: 1.5,
            mu_lat: 1.5,
            stiffness: 10.0,
            load_sensitivity: 0.05,
            rolling_resist: 0.015,
            surfaces: vec![SurfaceGrip::default()],
        }
    }
}

impl TireConfig {
    pub fn surface(&self, id: u32) -> SurfaceGrip {
        self.surfaces.get(id as usize).copied().unwrap_or_default()
    }

    /// Effective friction peaks under `load_n` newtons on `surface`.
    pub fn peaks(&self, load_n: f32, surface: u32) -> (f32, f32) {
        let grip = self.surface(surface);
        let load_kn = (load_n.max(0.0) / 1000.0).max(0.0);
        let fade = 1.0 / (1.0 + self.load_sensitivity.max(0.0) * load_kn);
        let width_scale = (self.width / 0.245).clamp(0.5, 1.6);
        (
            self.mu_long.max(0.0) * grip.longitudinal_scale.max(0.0) * fade * width_scale,
            self.mu_lat.max(0.0) * grip.lateral_scale.max(0.0) * fade * width_scale,
        )
    }

    /// Magic-Formula-lite: peak-normalized force in [-1, 1] for a slip
    /// input (ratio or tan(angle)). Rises steeply, peaks, then falls
    /// off past the peak like a real tire.
    pub fn curve(&self, slip: f32) -> f32 {
        self.curve_on(slip, 1.0)
    }

    /// [`TireConfig::curve`] with an explicit stiffness scale (surface).
    pub fn curve_on(&self, slip: f32, stiffness_scale: f32) -> f32 {
        let c = (self.stiffness * stiffness_scale).max(1.0);
        ((c * slip).atan() * 1.35).sin()
    }

    /// Combined tire force (N) in contact frame: +x lateral, +z... no -
    /// returns `(longitudinal, lateral)` for `slip_ratio`,
    /// `slip_angle_tan`, normal `load_n`, and `surface`.
    pub fn force(
        &self,
        slip_ratio: f32,
        slip_angle_tan: f32,
        load_n: f32,
        surface: u32,
    ) -> (f32, f32) {
        let load = load_n.max(0.0);
        if load <= 0.0 {
            return (0.0, 0.0);
        }
        let grip = self.surface(surface);
        let stiff = grip.stiffness_scale.max(0.05);
        let (mu_x, mu_y) = self.peaks(load, surface);
        let fx = self.curve_on(slip_ratio, stiff) * mu_x * load;
        let fy = -self.curve_on(slip_angle_tan, stiff) * mu_y * load;
        // Friction ellipse: cap the combined vector at the peak.
        let peak = (mu_x * mu_x + mu_y * mu_y).sqrt() * load;
        let mag = (fx * fx + fy * fy).sqrt();
        if mag > peak && mag > 0.0 {
            let s = peak / mag;
            (fx * s, fy * s)
        } else {
            (fx, fy)
        }
    }

    /// Rolling resistance force (N) opposing motion.
    pub fn rolling_force(&self, load_n: f32, surface: u32) -> f32 {
        load_n.max(0.0) * self.rolling_resist.max(0.0) * self.surface(surface).drag_scale.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tire_peaks_fade_with_load() {
        let t = TireConfig::default();
        let (light_x, _) = t.peaks(1000.0, 0);
        let (heavy_x, _) = t.peaks(9000.0, 0);
        assert!(light_x > heavy_x);
        assert!(heavy_x > 0.0);
    }

    #[test]
    fn tire_force_saturates() {
        let t = TireConfig::default();
        let (fx_small, _) = t.force(0.02, 0.0, 4000.0, 0);
        let (fx_big, _) = t.force(2.0, 0.0, 4000.0, 0);
        assert!(fx_small > 0.0 && fx_big > 0.0);
        // Saturation: 100x slip must not give 100x force.
        assert!(fx_big < fx_small * 10.0);
        // Combined slip is capped by the ellipse.
        let (cx, cy) = t.force(2.0, 2.0, 4000.0, 0);
        let (mu_x, mu_y) = t.peaks(4000.0, 0);
        let peak = (mu_x * mu_x + mu_y * mu_y).sqrt() * 4000.0;
        assert!((cx * cx + cy * cy).sqrt() <= peak * 1.001);
    }

    #[test]
    fn tire_surface_scales() {
        let mut t = TireConfig::default();
        t.surfaces.push(SurfaceGrip {
            longitudinal_scale: 0.5,
            lateral_scale: 0.5,
            drag_scale: 3.0,
            stiffness_scale: 1.0,
        });
        let (road, _) = t.force(0.1, 0.0, 4000.0, 0);
        let (loose, _) = t.force(0.1, 0.0, 4000.0, 1);
        assert!((road - loose * 2.0).abs() < road * 0.15);
        // Lateral and longitudinal scale independently.
        t.surfaces[1].longitudinal_scale = 1.0;
        let (long_ok, _) = t.force(0.1, 0.0, 4000.0, 1);
        assert!((long_ok - road).abs() < road * 0.15);
        // Softer surfaces feel vaguer: less force at the same slip.
        t.surfaces[1].stiffness_scale = 0.4;
        let (vague, _) = t.force(0.1, 0.0, 4000.0, 1);
        assert!(vague < long_ok);
        assert!(t.rolling_force(4000.0, 1) > t.rolling_force(4000.0, 0));
        // Unknown surface falls back to default.
        assert_eq!(t.surface(99), SurfaceGrip::default());
    }
}
