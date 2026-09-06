//! Attack telegraphs: windup -> fire -> recover, cancellable.
//!
//! States drive the visible anticipation (flash, sound, lunge pose);
//! `tick` reports the exact frame the hit lands. Stuns cancel via
//! [`Telegraph::cancel`].

use serde::{Deserialize, Serialize};

/// Telegraph phase.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Phase {
    #[default]
    Idle,
    Winding,
    Recovering,
}

/// One telegraphed attack.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Telegraph {
    pub windup: f32,
    pub recover: f32,
    phase: Phase,
    t: f32,
}

impl Telegraph {
    pub fn new(windup: f32, recover: f32) -> Self {
        Self {
            windup: windup.max(0.0),
            recover: recover.max(0.0),
            phase: Phase::Idle,
            t: 0.0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Fraction through the current phase (0..1, 1 when idle).
    pub fn progress(&self) -> f32 {
        let total = match self.phase {
            Phase::Winding => self.windup,
            Phase::Recovering => self.recover,
            Phase::Idle => return 1.0,
        };
        if total <= 0.0 {
            1.0
        } else {
            (self.t / total).clamp(0.0, 1.0)
        }
    }

    /// Begin the windup. False when already busy.
    pub fn start(&mut self) -> bool {
        if self.phase != Phase::Idle {
            return false;
        }
        self.phase = Phase::Winding;
        self.t = 0.0;
        true
    }

    /// Abort (stun, flinch, death). True when something was cancelled.
    pub fn cancel(&mut self) -> bool {
        if self.phase == Phase::Idle {
            return false;
        }
        self.phase = Phase::Idle;
        self.t = 0.0;
        true
    }

    /// Advance. Returns true on the step the hit lands (windup elapsed).
    pub fn tick(&mut self, dt: f32) -> bool {
        self.t += dt.max(0.0);
        match self.phase {
            Phase::Idle => false,
            Phase::Winding if self.t >= self.windup => {
                self.phase = Phase::Recovering;
                self.t = 0.0;
                true
            }
            Phase::Recovering if self.t >= self.recover => {
                self.phase = Phase::Idle;
                self.t = 0.0;
                false
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_then_recovers() {
        let mut t = Telegraph::new(0.5, 0.3);
        assert!(t.start());
        assert!(!t.start());
        assert!(!t.tick(0.4));
        assert_eq!(t.phase(), Phase::Winding);
        assert!(t.tick(0.2));
        assert_eq!(t.phase(), Phase::Recovering);
        assert!(!t.tick(0.3));
        assert_eq!(t.phase(), Phase::Idle);
    }

    #[test]
    fn cancel_aborts() {
        let mut t = Telegraph::new(1.0, 0.5);
        assert!(!t.cancel());
        t.start();
        assert!(t.cancel());
        assert_eq!((t.phase(), t.progress()), (Phase::Idle, 1.0));
    }
}
