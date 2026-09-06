use glam::Vec2;
use serde::{Deserialize, Serialize};

/// 2D layout slot for one held item: position relative to the hand
/// anchor (usually its center), rotation in degrees, uniform scale.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Transform2d {
    pub position: Vec2,
    pub rotation_deg: f32,
    pub scale: f32,
}

impl Transform2d {
    pub fn identity() -> Self {
        Self {
            position: Vec2::ZERO,
            rotation_deg: 0.0,
            scale: 1.0,
        }
    }
}

/// Maps a held-item count to per-item slots. Angles in degrees,
/// units in pixels. Implement for custom layouts.
pub trait HandLayout {
    fn arrange(&self, count: usize) -> Vec<Transform2d>;
}

/// Evenly spaced row centered on the anchor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinearLayout {
    pub spacing: f32,
}

impl LinearLayout {
    pub fn new(spacing: f32) -> Self {
        Self { spacing }
    }
}

impl Default for LinearLayout {
    fn default() -> Self {
        Self { spacing: 96.0 }
    }
}

impl HandLayout for LinearLayout {
    fn arrange(&self, count: usize) -> Vec<Transform2d> {
        (0..count)
            .map(|i| Transform2d {
                position: Vec2::new((i as f32 - (count as f32 - 1.0) / 2.0) * self.spacing, 0.0),
                rotation_deg: 0.0,
                scale: 1.0,
            })
            .collect()
    }
}

/// Arc fan on a circle below the anchor. `radius` sets curvature;
/// `spread_deg` caps the arc, `step_deg` spaces small hands.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FanLayout {
    pub radius: f32,
    pub spread_deg: f32,
    pub step_deg: f32,
}

impl FanLayout {
    pub fn new(radius: f32, spread_deg: f32, step_deg: f32) -> Self {
        Self {
            radius,
            spread_deg,
            step_deg,
        }
    }
}

impl Default for FanLayout {
    fn default() -> Self {
        Self {
            radius: 1000.0,
            spread_deg: 20.0,
            step_deg: 4.0,
        }
    }
}

impl HandLayout for FanLayout {
    fn arrange(&self, count: usize) -> Vec<Transform2d> {
        if count == 0 {
            return Vec::new();
        }
        let capacity = if self.step_deg > 0.0 {
            (self.spread_deg / self.step_deg).floor() as usize + 1
        } else {
            count
        };
        let step = if count <= capacity.max(1) {
            self.step_deg
        } else {
            self.spread_deg / (count as f32 - 1.0).max(1.0)
        };
        let total = step * (count as f32 - 1.0);
        let start = -total / 2.0;
        (0..count)
            .map(|i| {
                let angle = start + i as f32 * step;
                let rad = angle.to_radians();
                Transform2d {
                    position: Vec2::new(
                        rad.sin() * self.radius,
                        rad.cos() * self.radius - self.radius,
                    ),
                    rotation_deg: angle,
                    scale: 1.0,
                }
            })
            .collect()
    }
}

/// Overlapping cascade. Grows to `max_extent`, then compresses.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OverlapLayout {
    pub step: Vec2,
    pub max_extent: f32,
}

impl OverlapLayout {
    pub fn new(step: Vec2, max_extent: f32) -> Self {
        Self { step, max_extent }
    }
}

impl Default for OverlapLayout {
    fn default() -> Self {
        Self {
            step: Vec2::new(0.0, -28.0),
            max_extent: 320.0,
        }
    }
}

impl HandLayout for OverlapLayout {
    fn arrange(&self, count: usize) -> Vec<Transform2d> {
        if count == 0 {
            return Vec::new();
        }
        let natural = self.step * (count as f32 - 1.0);
        let natural_len = natural.length();
        let scale = if natural_len > self.max_extent && natural_len > 0.0 {
            self.max_extent / natural_len
        } else {
            1.0
        };
        let step = self.step * scale;
        let origin = step * ((count as f32 - 1.0) / -2.0);
        (0..count)
            .map(|i| Transform2d {
                position: origin + step * i as f32,
                rotation_deg: 0.0,
                scale: 1.0,
            })
            .collect()
    }
}

/// Bounded holder. Overflow is caller policy (bounce, reroute, signal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hand<T, L = FanLayout> {
    pub max_size: usize,
    pub layout: L,
    items: Vec<T>,
}

impl<T, L: Default> Default for Hand<T, L> {
    fn default() -> Self {
        Self {
            max_size: 10,
            layout: L::default(),
            items: Vec::new(),
        }
    }
}

impl<T, L> Hand<T, L> {
    pub fn new(max_size: usize, layout: L) -> Self {
        Self {
            max_size,
            layout,
            items: Vec::new(),
        }
    }

    pub fn with_items(max_size: usize, layout: L, items: Vec<T>) -> Self {
        Self {
            max_size,
            layout,
            items,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.items.len() >= self.max_size
    }

    pub fn can_add(&self) -> bool {
        self.items.len() < self.max_size
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    pub fn get(&self, idx: usize) -> Option<&T> {
        self.items.get(idx)
    }

    pub fn try_push(&mut self, item: T) -> Result<(), T> {
        if self.can_add() {
            self.items.push(item);
            Ok(())
        } else {
            Err(item)
        }
    }

    pub fn insert_at(&mut self, idx: usize, item: T) -> Result<(), T> {
        if !self.can_add() {
            return Err(item);
        }
        if idx >= self.items.len() {
            self.items.push(item);
        } else {
            self.items.insert(idx, item);
        }
        Ok(())
    }

    pub fn remove(&mut self, idx: usize) -> Option<T> {
        if idx < self.items.len() {
            Some(self.items.remove(idx))
        } else {
            None
        }
    }

    pub fn drain(&mut self) -> Vec<T> {
        std::mem::take(&mut self.items)
    }

    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size;
    }
}

impl<T, L: HandLayout> Hand<T, L> {
    pub fn arrange(&self) -> Vec<Transform2d> {
        self.layout.arrange(self.items.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_max_size() {
        let mut h = Hand::new(2, LinearLayout::new(10.0));
        assert!(h.try_push(1).is_ok());
        assert!(h.try_push(2).is_ok());
        assert!(h.try_push(3).is_err());
        assert!(h.is_full());
        assert!(h.insert_at(0, 9).is_err());
    }

    #[test]
    fn hand_linear_arrange() {
        let h = Hand::with_items(10, LinearLayout::new(100.0), vec![1, 2, 3]);
        let t = h.arrange();
        assert_eq!(t.len(), 3);
        assert!((t[1].position.x).abs() < 0.01);
        assert_eq!(t[1].rotation_deg, 0.0);
    }

    #[test]
    fn hand_fan_arrange() {
        let h = Hand::with_items(10, FanLayout::default(), vec![1, 2, 3, 4, 5]);
        let t = h.arrange();
        assert_eq!(t.len(), 5);
        assert!(t[2].rotation_deg.abs() < t[0].rotation_deg.abs());
    }

    #[test]
    fn hand_overlap_compresses() {
        let layout = OverlapLayout::new(Vec2::new(0.0, -40.0), 100.0);
        let small = layout.arrange(2);
        assert!((small[1].position.y - small[0].position.y - -40.0).abs() < 0.01);
        let big = layout.arrange(10);
        let extent = (big[9].position.y - big[0].position.y).abs();
        assert!(extent <= 100.0 + 0.01);
    }

    #[test]
    fn transform_roundtrip() {
        let t = Transform2d::identity();
        let s = ron::ser::to_string(&t).unwrap();
        let de: Transform2d = ron::from_str(&s).unwrap();
        assert_eq!(de, t);
    }
}
