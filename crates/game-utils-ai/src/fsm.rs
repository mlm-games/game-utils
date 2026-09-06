//! Named-state machine: enter/exit/update with timed transitions.
//!
//! Behavior stays game-side; the machine owns state, timers, and the
//! change journal. `from` of "*" matches any state (death, stun).

use serde::{Deserialize, Serialize};

/// One allowed hop. `after` fires it automatically in `tick`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub after: Option<f32>,
}

impl Transition {
    pub fn on_event(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            after: None,
        }
    }

    pub fn timed(from: impl Into<String>, to: impl Into<String>, after: f32) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            after: Some(after.max(0.0)),
        }
    }

    fn matches(&self, current: &str) -> bool {
        self.from == "*" || self.from == current
    }
}

/// State changes, drained by game code.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FsmEvent {
    Entered { state: String },
    Exited { state: String },
}

/// Minimal FSM with an interrupt stack. Unknown targets are refused,
/// never panicking.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Fsm {
    current: String,
    elapsed: f32,
    transitions: Vec<Transition>,
    /// Suspended states under interrupts (`push`/`pop`).
    stack: Vec<(String, f32)>,
    #[serde(skip, default = "_events")]
    events: Vec<FsmEvent>,
}

fn _events() -> Vec<FsmEvent> {
    Vec::new()
}

impl Fsm {
    pub fn new(initial: impl Into<String>) -> Self {
        let initial = initial.into();
        Self {
            current: initial.clone(),
            elapsed: 0.0,
            transitions: Vec::new(),
            stack: Vec::new(),
            events: vec![FsmEvent::Entered { state: initial }],
        }
    }

    pub fn add(&mut self, t: Transition) {
        self.transitions.push(t);
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    pub fn drain_events(&mut self) -> Vec<FsmEvent> {
        core::mem::take(&mut self.events)
    }

    fn enter(&mut self, to: &str) {
        let from = core::mem::replace(&mut self.current, to.to_owned());
        self.elapsed = 0.0;
        self.events.push(FsmEvent::Exited { state: from });
        self.events.push(FsmEvent::Entered { state: to.into() });
    }

    /// Follow an event transition. False when no `from -> to` edge exists.
    pub fn request(&mut self, to: &str) -> bool {
        if to == self.current {
            return true;
        }
        let ok = self
            .transitions
            .iter()
            .any(|t| t.after.is_none() && t.matches(&self.current) && t.to == to);
        if ok {
            self.enter(to);
        }
        ok
    }

    /// Jump regardless of edges.
    pub fn force(&mut self, to: &str) {
        if to != self.current {
            self.enter(to);
        }
    }

    /// Interrupt: suspend the current state (with its timer) and enter
    /// `to`. Edges are still enforced; use `force_push` to skip them.
    pub fn push(&mut self, to: &str) -> bool {
        if to == self.current {
            return true;
        }
        let ok = self
            .transitions
            .iter()
            .any(|t| t.after.is_none() && t.matches(&self.current) && t.to == to);
        if ok {
            self.stack.push((self.current.clone(), self.elapsed));
            self.enter(to);
        }
        ok
    }

    /// Interrupt without edge checks.
    pub fn force_push(&mut self, to: &str) {
        if to != self.current {
            self.stack.push((self.current.clone(), self.elapsed));
            self.enter(to);
        }
    }

    /// Resume the suspended state, restoring its timer. False when empty.
    pub fn pop(&mut self) -> bool {
        match self.stack.pop() {
            Some((state, elapsed)) => {
                let from = core::mem::replace(&mut self.current, state);
                self.elapsed = elapsed;
                self.events.push(FsmEvent::Exited { state: from });
                self.events.push(FsmEvent::Entered {
                    state: self.current.clone(),
                });
                true
            }
            None => false,
        }
    }

    pub fn stack_depth(&self) -> usize {
        self.stack.len()
    }

    /// Advance; fires the first elapsed timed edge (event edges ignored).
    pub fn tick(&mut self, dt: f32) {
        self.elapsed += dt.max(0.0);
        if let Some(to) = self
            .transitions
            .iter()
            .find(|t| t.matches(&self.current) && t.after.is_some_and(|a| self.elapsed >= a))
            .map(|t| t.to.clone())
        {
            self.enter(&to);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fsm() -> Fsm {
        let mut f = Fsm::new("idle");
        f.add(Transition::on_event("idle", "chase"));
        f.add(Transition::on_event("chase", "idle"));
        f.add(Transition::on_event("*", "dead"));
        f.add(Transition::timed("chase", "idle", 5.0));
        f
    }

    #[test]
    fn request_follows_edges() {
        let mut f = fsm();
        assert!(f.request("chase"));
        assert_eq!(f.current(), "chase");
        assert!(!f.request("dead2"));
        assert!(f.request("dead"));
    }

    #[test]
    fn timeout_returns() {
        let mut f = fsm();
        f.request("chase");
        f.tick(4.0);
        assert_eq!(f.current(), "chase");
        f.tick(2.0);
        assert_eq!(f.current(), "idle");
    }

    #[test]
    fn any_state_matches() {
        let mut f = fsm();
        f.request("chase");
        assert!(f.request("dead"));
        assert_eq!(f.drain_events().len(), 5);
    }

    #[test]
    fn force_ignores_edges() {
        let mut f = Fsm::new("a");
        f.force("zzz");
        assert_eq!(f.current(), "zzz");
    }

    #[test]
    fn push_pop_restores_timer() {
        let mut f = fsm();
        f.request("chase");
        f.tick(3.0);
        assert!(f.push("dead"));
        assert_eq!((f.current(), f.stack_depth()), ("dead", 1));
        assert!(f.pop());
        assert_eq!(f.current(), "chase");
        // Timer restored (3.0 of 5.0 elapsed): 1s more stays, 3s fires.
        f.tick(1.0);
        assert_eq!(f.current(), "chase");
        f.tick(3.0);
        assert_eq!(f.current(), "idle");
        assert!(!f.pop());
    }
}
