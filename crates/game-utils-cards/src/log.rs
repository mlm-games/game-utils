use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Bounded run-event log. Over-capacity pushes evict the oldest.
/// Unbounded by default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLog<E> {
    events: VecDeque<E>,
    capacity: Option<usize>,
    dropped: u64,
}

impl<E> Default for EventLog<E> {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            capacity: None,
            dropped: 0,
        }
    }
}

impl<E> EventLog<E> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bounded log keeping at most `capacity` recent events.
    pub fn bounded(capacity: usize) -> Self {
        Self {
            events: VecDeque::new(),
            capacity: Some(capacity.max(1)),
            dropped: 0,
        }
    }

    pub fn push(&mut self, event: E) {
        if let Some(cap) = self.capacity {
            while self.events.len() >= cap {
                self.events.pop_front();
                self.dropped += 1;
            }
        }
        self.events.push_back(event);
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Events evicted by the capacity cap so far.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, E> {
        self.events.iter()
    }

    pub fn last(&self) -> Option<&E> {
        self.events.back()
    }

    /// Drain matching events, preserving order of the rest.
    pub fn drain_where(&mut self, mut pred: impl FnMut(&E) -> bool) -> Vec<E> {
        let mut removed = Vec::new();
        let mut kept = VecDeque::with_capacity(self.events.len());
        for event in std::mem::take(&mut self.events) {
            if pred(&event) {
                removed.push(event);
            } else {
                kept.push_back(event);
            }
        }
        self.events = kept;
        removed
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl<'a, E> IntoIterator for &'a EventLog<E> {
    type Item = &'a E;
    type IntoIter = std::collections::vec_deque::Iter<'a, E>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_push_iter() {
        let mut log = EventLog::new();
        log.push("a");
        log.push("b");
        assert_eq!(log.len(), 2);
        assert_eq!(log.last(), Some(&"b"));
        let all: Vec<&&str> = (&log).into_iter().collect();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn log_bounded_evicts() {
        let mut log = EventLog::bounded(2);
        log.push(1);
        log.push(2);
        log.push(3);
        assert_eq!(log.len(), 2);
        assert_eq!(log.dropped(), 1);
        let all: Vec<i32> = log.iter().copied().collect();
        assert_eq!(all, vec![2, 3]);
        let removed = log.drain_where(|e| *e == 2);
        assert_eq!(removed, vec![2]);
        assert_eq!(log.len(), 1);
    }
}
