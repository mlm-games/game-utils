//! Engine-agnostic loading progress: track asset/catalog loads as plain data.

/// Fractional load tracker (`done` of `total`).
#[derive(Debug, Clone, Default)]
pub struct LoadingProgress {
    pub done: u32,
    pub total: u32,
}

impl LoadingProgress {
    pub fn new(total: u32) -> Self {
        Self { done: 0, total }
    }

    pub fn advance(&mut self) {
        self.done = self.done.saturating_add(1);
    }

    pub fn set(&mut self, done: u32, total: u32) {
        self.done = done.min(total);
        self.total = total;
    }

    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 1.0;
        }
        (self.done.min(self.total) as f32) / (self.total as f32)
    }

    pub fn is_ready(&self) -> bool {
        self.done >= self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_total_is_ready() {
        assert!(LoadingProgress::new(0).is_ready());
        assert_eq!(LoadingProgress::new(0).fraction(), 1.0);
    }

    #[test]
    fn advance_saturates_count_not_ready() {
        let mut p = LoadingProgress::new(2);
        assert!(!p.is_ready());
        p.advance();
        assert!((p.fraction() - 0.5).abs() < 1e-6);
        p.advance();
        assert!(p.is_ready());
        // `advance` saturates the counter; `fraction`/`set` still clamp.
        p.advance();
        assert_eq!(p.done, 3);
        assert_eq!(p.fraction(), 1.0);
    }

    #[test]
    fn set_clamps_done() {
        let mut p = LoadingProgress::new(0);
        p.set(9, 3);
        assert_eq!((p.done, p.total), (3, 3));
        assert!(p.is_ready());
    }
}
