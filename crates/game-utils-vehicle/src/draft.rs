//! Slipstream: followers in the leader's wake get drag cuts, plus
//! optional tow charge. Pairwise over positions and headings.

use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DraftConfig {
    /// Wake length behind the leader (m).
    pub length: f32,
    /// Wake half-width (m).
    pub half_width: f32,
    /// Minimum follower speed to engage (m/s).
    pub min_speed: f32,
    /// Drag multiplier while drafting (0.25 = quarter drag).
    pub drag_scale: f32,
    /// Charge gained per second in the wake (0 = no boost game).
    pub charge_rate: f32,
    /// Charge lost per second outside the wake.
    pub decay_rate: f32,
}

impl Default for DraftConfig {
    fn default() -> Self {
        Self {
            length: 12.0,
            half_width: 1.6,
            min_speed: 8.0,
            drag_scale: 0.35,
            charge_rate: 0.25,
            decay_rate: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct DraftState {
    /// 0..1 tow charge (feeds boost systems when the game wants).
    pub charge: f32,
    pub in_wake: bool,
}

impl DraftState {
    /// Update against one leader. `fwd`/`leader_fwd` are unit forward
    /// vectors. Returns the drag multiplier to apply (1.0 outside).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        cfg: &DraftConfig,
        pos: Vec3,
        fwd: Vec3,
        speed: f32,
        leader_pos: Vec3,
        leader_fwd: Vec3,
        dt: f32,
    ) -> f32 {
        let to_follower = pos - leader_pos;
        // Behind the leader, within the wake box, roughly aligned.
        let back = to_follower.dot(-leader_fwd);
        let side = (to_follower + leader_fwd * back).length();
        let aligned = leader_fwd.dot(fwd) > 0.3;
        self.in_wake = back > 0.0
            && back < cfg.length.max(0.0)
            && side < cfg.half_width.max(0.0)
            && speed >= cfg.min_speed.max(0.0)
            && aligned;
        if self.in_wake {
            self.charge = (self.charge + cfg.charge_rate.max(0.0) * dt.max(0.0)).min(1.0);
            cfg.drag_scale.clamp(0.0, 1.0)
        } else {
            self.charge = (self.charge - cfg.decay_rate.max(0.0) * dt.max(0.0)).max(0.0);
            1.0
        }
    }

    /// Spend the charge (e.g. on a pass attempt); returns what was held.
    pub fn release(&mut self) -> f32 {
        std::mem::replace(&mut self.charge, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_engages_behind_leader() {
        let cfg = DraftConfig::default();
        let mut d = DraftState::default();
        let leader_pos = Vec3::ZERO;
        let leader_fwd = Vec3::new(0.0, 0.0, -1.0);
        // 5 m behind, centered, fast, aligned.
        let m = d.update(
            &cfg,
            Vec3::new(0.0, 0.0, 5.0),
            leader_fwd,
            20.0,
            leader_pos,
            leader_fwd,
            0.1,
        );
        assert!(d.in_wake);
        assert!(m < 1.0);
        assert!(d.charge > 0.0);
        // Beside the leader: no wake.
        let m = d.update(
            &cfg,
            Vec3::new(10.0, 0.0, 5.0),
            leader_fwd,
            20.0,
            leader_pos,
            leader_fwd,
            1.0,
        );
        assert!(!d.in_wake);
        assert_eq!(m, 1.0);
        // Ahead of the leader: no wake.
        let m = d.update(
            &cfg,
            Vec3::new(0.0, 0.0, -5.0),
            leader_fwd,
            20.0,
            leader_pos,
            leader_fwd,
            0.1,
        );
        assert!(!d.in_wake);
        assert_eq!(m, 1.0);
    }

    #[test]
    fn draft_charge_and_release() {
        let cfg = DraftConfig::default();
        let mut d = DraftState::default();
        let fwd = Vec3::new(0.0, 0.0, -1.0);
        for _ in 0..100 {
            d.update(
                &cfg,
                Vec3::new(0.0, 0.0, 5.0),
                fwd,
                20.0,
                Vec3::ZERO,
                fwd,
                0.1,
            );
        }
        assert_eq!(d.charge, 1.0);
        assert_eq!(d.release(), 1.0);
        assert_eq!(d.charge, 0.0);
    }
}
