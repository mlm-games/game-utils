//! Ground queries for suspension raycasts. Models ask a
//! [`GroundProbe`] for ground under each wheel; plug in [`FlatGround`],
//! a stub, or the `rapier`-feature pipeline probe.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// One ground contact along a probe ray.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GroundHit {
    /// Distance from the ray origin to the surface (m).
    pub distance: f32,
    pub point: Vec3,
    pub normal: Vec3,
    /// Surface id for grip tables (0 = default).
    pub surface: u32,
}

/// Abstraction over "what is the ground here?".
pub trait GroundProbe {
    /// Cast from `origin` along unit `dir` up to `max_dist` meters.
    fn probe(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<GroundHit>;
}

/// Infinite flat plane. Exact and deterministic; default for tests.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FlatGround {
    pub height: f32,
    pub normal: Vec3,
    pub surface: u32,
}

impl Default for FlatGround {
    fn default() -> Self {
        Self {
            height: 0.0,
            normal: Vec3::Y,
            surface: 0,
        }
    }
}

impl FlatGround {
    pub fn new(height: f32) -> Self {
        Self {
            height,
            ..Self::default()
        }
    }
}

impl GroundProbe for FlatGround {
    fn probe(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<GroundHit> {
        let denom = dir.dot(self.normal);
        if denom.abs() < 1e-6 {
            return None;
        }
        let t = (self.height - origin.dot(self.normal)) / denom;
        if t < 0.0 || t > max_dist.max(0.0) {
            return None;
        }
        Some(GroundHit {
            distance: t,
            point: origin + dir * t,
            normal: self.normal,
            surface: self.surface,
        })
    }
}

/// Always misses. Useful for airborne checks and benches.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoGround;

impl GroundProbe for NoGround {
    fn probe(&self, _origin: Vec3, _dir: Vec3, _max_dist: f32) -> Option<GroundHit> {
        None
    }
}

#[cfg(feature = "rapier")]
pub mod rapier_backend {
    //! [`GroundProbe`] over a rapier3d scene. Borrows world storage
    //! per step; surface ids come from collider `user_data` low bits.

    use glam::Vec3;
    use rapier3d::parry::query::QueryDispatcher;
    use rapier3d::prelude::*;

    use super::{GroundHit, GroundProbe};

    /// Scene-query view over live rapier storage (no ownership taken).
    pub struct RapierGround<'a> {
        broad_phase: &'a BroadPhaseBvh,
        dispatcher: &'a dyn QueryDispatcher,
        bodies: &'a RigidBodySet,
        colliders: &'a ColliderSet,
        filter: QueryFilter<'a>,
    }

    impl<'a> RapierGround<'a> {
        pub fn new(
            broad_phase: &'a BroadPhaseBvh,
            dispatcher: &'a dyn QueryDispatcher,
            bodies: &'a RigidBodySet,
            colliders: &'a ColliderSet,
            filter: QueryFilter<'a>,
        ) -> Self {
            Self {
                broad_phase,
                dispatcher,
                bodies,
                colliders,
                filter,
            }
        }
    }

    impl GroundProbe for RapierGround<'_> {
        fn probe(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<GroundHit> {
            if max_dist <= 0.0 || dir.length_squared() < 1e-8 {
                return None;
            }
            let pipeline = self.broad_phase.as_query_pipeline(
                self.dispatcher,
                self.bodies,
                self.colliders,
                self.filter,
            );
            let ray = Ray::new(
                Vector::new(origin.x, origin.y, origin.z),
                Vector::new(dir.x, dir.y, dir.z),
            );
            let (handle, hit) = pipeline.cast_ray_and_get_normal(&ray, max_dist, true)?;
            let point = ray.point_at(hit.time_of_impact);
            let surface = pipeline
                .colliders
                .get(handle)
                .map(|co| co.user_data as u32)
                .unwrap_or(0);
            Some(GroundHit {
                distance: hit.time_of_impact,
                point: Vec3::new(point.x, point.y, point.z),
                normal: Vec3::new(hit.normal.x, hit.normal.y, hit.normal.z).normalize_or_zero(),
                surface,
            })
        }
    }
}

#[cfg(all(test, feature = "rapier"))]
mod rapier_tests {
    use super::rapier_backend::RapierGround;
    use super::*;
    use rapier3d::math::Vector as RVector;
    use rapier3d::prelude::*;

    #[test]
    fn rapier_ground_probe_hits() {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let mut islands = IslandManager::new();
        let mut broad = BroadPhaseBvh::new();
        let mut narrow = NarrowPhase::new();
        let mut collision = CollisionPipeline::new();
        let hooks = ();
        let events = ();
        let ground =
            bodies.insert(RigidBodyBuilder::fixed().translation(RVector::new(0.0, -0.1, 0.0)));
        let mut co = ColliderBuilder::cuboid(10.0, 0.1, 10.0).build();
        co.user_data = 7;
        colliders.insert_with_parent(co, ground, &mut bodies);
        collision.step(
            0.01,
            &mut islands,
            &mut broad,
            &mut narrow,
            &mut bodies,
            &mut colliders,
            &hooks,
            &events,
        );
        let probe = RapierGround::new(
            &broad,
            narrow.query_dispatcher(),
            &bodies,
            &colliders,
            QueryFilter::default(),
        );
        let hit = probe
            .probe(Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Y, 10.0)
            .expect("should hit the ground cuboid");
        assert!((hit.distance - 5.0).abs() < 1e-4);
        assert!((hit.normal - Vec3::Y).length() < 1e-4);
        assert_eq!(hit.surface, 7);
        assert!(
            probe
                .probe(Vec3::new(0.0, 5.0, 0.0), Vec3::Y, 10.0)
                .is_none()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_ground_hits() {
        let g = FlatGround::new(0.0);
        let hit = g.probe(Vec3::new(0.0, 1.0, 0.0), Vec3::NEG_Y, 2.0).unwrap();
        assert!((hit.distance - 1.0).abs() < 1e-6);
        assert_eq!(hit.normal, Vec3::Y);
        assert!(
            g.probe(Vec3::new(0.0, 1.0, 0.0), Vec3::NEG_Y, 0.5)
                .is_none()
        );
        assert!(g.probe(Vec3::new(0.0, 1.0, 0.0), Vec3::X, 5.0).is_none());
        assert!(NoGround.probe(Vec3::ZERO, Vec3::NEG_Y, 5.0).is_none());
    }
}
