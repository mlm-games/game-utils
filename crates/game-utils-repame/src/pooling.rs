//! Sim-side entity pooling over `repame-sim`.
//!
//! Port of `game-utils-bevy` pooling without `bevy_camera::visibility`:
//! pooled entities carry [`PoolHidden`] instead of `Visibility::Hidden`,
//! and snapshot producers skip hidden entities. No plugin needed —
//! `EntityPool<M>` is a plain resource with associated helpers.
//!
//! Reset contract: [`ObjectPool::acquire`] resets `M` to `M::default()`
//! on every acquire (fresh or reused), then runs the caller's `spawn`
//! closure for extra bundles, so reused entities never keep stale state
//! (e.g. leftover velocity). Put per-spawn initialization in the closure.

use std::collections::VecDeque;
use std::marker::PhantomData;

use bevy_ecs::prelude::*;

/// Default [`EntityPool`] capacity used by [`EntityPool::default`].
pub const DEFAULT_MAX_SIZE: usize = 64;

/// Marker for pooled-but-inactive entities. Snapshot producers must skip
/// entities carrying this component.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct PoolHidden;

#[derive(Resource)]
pub struct EntityPool<M: Component + Default> {
    available: VecDeque<Entity>,
    active: Vec<Entity>,
    max_size: usize,
    _marker: PhantomData<M>,
}

impl<M: Component + Default> EntityPool<M> {
    pub fn new(max_size: usize) -> Self {
        Self {
            available: VecDeque::new(),
            active: Vec::new(),
            max_size,
            _marker: PhantomData,
        }
    }
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn available_count(&self) -> usize {
        self.available.len()
    }

    pub fn total_count(&self) -> usize {
        self.active.len() + self.available.len()
    }
}

impl<M: Component + Default> Default for EntityPool<M> {
    /// Pool with [`DEFAULT_MAX_SIZE`] capacity.
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SIZE)
    }
}

pub struct ObjectPool;

impl ObjectPool {
    pub fn prewarm<M: Component + Default>(
        pool: &mut EntityPool<M>,
        commands: &mut Commands,
        count: usize,
        mut spawn: impl FnMut(&mut EntityCommands),
    ) {
        let room = pool
            .max_size
            .saturating_sub(pool.active.len() + pool.available.len());
        for _ in 0..count.min(room) {
            let mut ec = commands.spawn((PoolHidden, M::default()));
            spawn(&mut ec);
            pool.available.push_back(ec.id());
        }
    }

    pub fn acquire<M: Component + Default>(
        pool: &mut EntityPool<M>,
        commands: &mut Commands,
        mut spawn: impl FnMut(&mut EntityCommands),
    ) -> Option<Entity> {
        while let Some(e) = pool.available.pop_front() {
            // Skip entities that were despawned while pooled.
            let Ok(mut ec) = commands.get_entity(e) else {
                continue;
            };
            pool.active.push(e);
            // Reset contract: reused entities never keep stale `M` state;
            // the spawn closure then runs for extra per-spawn bundles.
            ec.insert(M::default());
            ec.remove::<PoolHidden>();
            spawn(&mut ec);
            return Some(e);
        }
        if pool.total_count() >= pool.max_size {
            return None;
        }
        let mut ec = commands.spawn(M::default());
        spawn(&mut ec);
        let e = ec.id();
        pool.active.push(e);
        Some(e)
    }

    /// Release back to the pool. Returns `false` (and does nothing) when
    /// `entity` is not an active pooled entity — e.g. never acquired,
    /// already released, or despawned without a [`Self::scrub`].
    pub fn try_release<M: Component + Default>(
        pool: &mut EntityPool<M>,
        entity: Entity,
        commands: &mut Commands,
    ) -> bool {
        if let Some(i) = pool.active.iter().position(|&e| e == entity) {
            pool.active.swap_remove(i);
            if let Ok(mut ec) = commands.get_entity(entity) {
                ec.insert(PoolHidden);
                pool.available.push_back(entity);
                return true;
            }
            return false;
        }
        false
    }

    pub fn release<M: Component + Default>(
        pool: &mut EntityPool<M>,
        entity: Entity,
        commands: &mut Commands,
    ) {
        let _ = Self::try_release(pool, entity, commands);
    }

    /// Drop dead entities from both lists (call once per frame if desired).
    pub fn scrub<M: Component + Default>(
        pool: &mut EntityPool<M>,
        exists: impl Fn(Entity) -> bool,
    ) {
        pool.available.retain(|&e| exists(e));
        pool.active.retain(|&e| exists(e));
    }

    /// [`Self::scrub`] against a live [`World`]: drops entities that no
    /// longer exist (despawned while pooled or active). Call at a frame
    /// boundary after the command queue has flushed, so just-spawned
    /// entities are visible to the existence check.
    pub fn scrub_world<M: Component + Default>(pool: &mut EntityPool<M>, world: &World) {
        Self::scrub(pool, |e| world.get_entity(e).is_ok());
    }
}

/// Ready-made exclusive system: drops dead pooled entities for `M` (see
/// [`ObjectPool::scrub_world`]). Run at a frame boundary after commands
/// flush: `world.run_system_once(scrub_dead::<Bullet>)`.
pub fn scrub_dead<M: Component + Default>(world: &mut World) {
    let Some(pool) = world.get_resource::<EntityPool<M>>() else {
        return;
    };
    let available: Vec<Entity> = pool.available.iter().copied().collect();
    let active: Vec<Entity> = pool.active.clone();
    let available: VecDeque<Entity> = available
        .into_iter()
        .filter(|&e| world.get_entity(e).is_ok())
        .collect();
    let active: Vec<Entity> = active
        .into_iter()
        .filter(|&e| world.get_entity(e).is_ok())
        .collect();
    if let Some(mut pool) = world.get_resource_mut::<EntityPool<M>>() {
        pool.available = available;
        pool.active = active;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    #[derive(Component, Default)]
    struct Bullet;

    #[test]
    fn acquire_release_cycle() {
        let mut world = World::new();
        world.insert_resource(EntityPool::<Bullet>::new(8));
        let _ = world.run_system_once(
            |mut pool: ResMut<EntityPool<Bullet>>, mut commands: Commands| {
                ObjectPool::prewarm(&mut pool, &mut commands, 4, |_| {});
            },
        );
        let _ = world.run_system_once(
            |mut pool: ResMut<EntityPool<Bullet>>, mut commands: Commands| {
                let e = ObjectPool::acquire(&mut pool, &mut commands, |_| {}).unwrap();
                assert_eq!(pool.active_count(), 1);
                ObjectPool::release::<Bullet>(&mut pool, e, &mut commands);
                assert_eq!(pool.active_count(), 0);
                assert_eq!(pool.available_count(), 4);
            },
        );
    }

    #[test]
    fn pool_caps_at_max_size() {
        let mut world = World::new();
        world.insert_resource(EntityPool::<Bullet>::new(2));
        let _ = world.run_system_once(
            |mut pool: ResMut<EntityPool<Bullet>>, mut commands: Commands| {
                for _ in 0..2 {
                    assert!(ObjectPool::acquire(&mut pool, &mut commands, |_| {}).is_some());
                }
                assert!(ObjectPool::acquire(&mut pool, &mut commands, |_| {}).is_none());
            },
        );
    }

    #[derive(Component, Default, PartialEq, Debug)]
    struct Shell {
        speed: f32,
    }

    #[test]
    fn reuse_runs_spawn_closure_after_reset() {
        let mut world = World::new();
        world.insert_resource(EntityPool::<Shell>::new(4));
        // Acquire, dirty the pooled component, release, re-acquire: the
        // reused entity must come back at `M::default()`.
        let _ = world.run_system_once(
            |mut pool: ResMut<EntityPool<Shell>>, mut commands: Commands| {
                let e = ObjectPool::acquire(&mut pool, &mut commands, |_| {}).unwrap();
                commands.entity(e).insert(Shell { speed: 99.0 });
                ObjectPool::release::<Shell>(&mut pool, e, &mut commands);
            },
        );
        world.flush();
        let _ = world.run_system_once(
            |mut pool: ResMut<EntityPool<Shell>>, mut commands: Commands| {
                let e = ObjectPool::acquire(&mut pool, &mut commands, |_| {}).unwrap();
                assert_eq!(pool.active_count(), 1);
                ObjectPool::release::<Shell>(&mut pool, e, &mut commands);
            },
        );
        world.flush();
        // Re-acquire with a per-spawn closure: reset-then-customize order
        // means the closure sees the default and overwrites it (neither
        // the stale 99.0 nor the bare default survives).
        let _ = world.run_system_once(
            |mut pool: ResMut<EntityPool<Shell>>, mut commands: Commands| {
                let _e = ObjectPool::acquire(
                    &mut pool,
                    &mut commands,
                    |ec| {
                        ec.insert(Shell { speed: 7.0 });
                    },
                )
                .unwrap();
                assert_eq!(pool.active_count(), 1);
            },
        );
        world.flush();
        let mut query = world.query::<&Shell>();
        let speeds: Vec<f32> = query.iter(&world).map(|s| s.speed).collect();
        assert_eq!(speeds, vec![7.0]);
    }

    #[test]
    fn reuse_without_closure_comes_back_default() {
        let mut world = World::new();
        world.insert_resource(EntityPool::<Shell>::new(4));
        let _ = world.run_system_once(
            |mut pool: ResMut<EntityPool<Shell>>, mut commands: Commands| {
                let e = ObjectPool::acquire(&mut pool, &mut commands, |_| {}).unwrap();
                commands.entity(e).insert(Shell { speed: 99.0 });
                ObjectPool::release::<Shell>(&mut pool, e, &mut commands);
            },
        );
        world.flush();
        let _ = world.run_system_once(
            |mut pool: ResMut<EntityPool<Shell>>, mut commands: Commands| {
                ObjectPool::acquire(&mut pool, &mut commands, |_| {}).unwrap();
            },
        );
        world.flush();
        // No customizing closure: the reused entity is exactly default.
        let mut query = world.query::<&Shell>();
        let speeds: Vec<f32> = query.iter(&world).map(|s| s.speed).collect();
        assert_eq!(speeds, vec![0.0]);
    }

    #[test]
    fn try_release_reports_unknown_entities() {
        let mut world = World::new();
        world.insert_resource(EntityPool::<Bullet>::new(4));
        let _ = world.run_system_once(
            |mut pool: ResMut<EntityPool<Bullet>>, mut commands: Commands| {
                let stray = commands.spawn_empty().id();
                assert!(!ObjectPool::try_release::<Bullet>(&mut pool, stray, &mut commands));
                let e = ObjectPool::acquire(&mut pool, &mut commands, |_| {}).unwrap();
                assert!(ObjectPool::try_release::<Bullet>(&mut pool, e, &mut commands));
                // Double release is a no-op that reports false.
                assert!(!ObjectPool::try_release::<Bullet>(&mut pool, e, &mut commands));
            },
        );
    }

    #[test]
    fn pool_default_has_capacity() {
        let pool = EntityPool::<Bullet>::default();
        assert_eq!(pool.available_count(), 0);
        assert_eq!(pool.active_count(), 0);
        assert!(pool.max_size >= 1);
    }

    #[test]
    fn scrub_dead_purges_despawned() {
        let mut world = World::new();
        world.insert_resource(EntityPool::<Bullet>::new(4));
        let (active, pooled) = world
            .run_system_once(
                |mut pool: ResMut<EntityPool<Bullet>>, mut commands: Commands| {
                    let active =
                        ObjectPool::acquire(&mut pool, &mut commands, |_| {}).unwrap();
                    let pooled =
                        ObjectPool::acquire(&mut pool, &mut commands, |_| {}).unwrap();
                    ObjectPool::release::<Bullet>(&mut pool, pooled, &mut commands);
                    (active, pooled)
                },
            )
            .expect("acquire systems run");
        world.flush();
        world.despawn(active);
        world.despawn(pooled);
        world.run_system_once(scrub_dead::<Bullet>)
            .expect("scrub system runs");
        let pool = world.resource::<EntityPool<Bullet>>();
        assert_eq!(pool.active_count(), 0);
        assert_eq!(pool.available_count(), 0);
    }
}
