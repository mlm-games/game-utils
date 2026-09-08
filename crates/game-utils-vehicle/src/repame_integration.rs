//! Repame (`repame-sim`) integration: components + fixed-step systems.
//!
//! Replaces the old `bevy` feature. Core dynamics stay engine-free
//! (`glam` only); this module adds `repame_sim::bevy_ecs` components,
//! per-step systems driven by [`SimTime`], and a renderer-agnostic
//! [`VehicleTransform`] snapshot (translation + rotation) that games
//! copy into their viewport frame.

use bevy_ecs::prelude::{Component, Query, Res, Resource};
use bevy_ecs::schedule::IntoScheduleConfigs;
use glam::{Quat, Vec3};
use repame_sim::{Sim, SimTime};

use crate::arcade::{ArcadeConfig, ArcadeState, SurfaceMod};
use crate::car::{CarConfig, CarState, GearShift};
use crate::flight::{PlaneConfig, PlaneControls, PlaneState};
use crate::ground::GroundProbe;
use crate::marine::{BoatConfig, BoatControls, BoatState};
use crate::{VehicleConfig, VehicleInput, VehicleState};

/// Renderer-agnostic transform snapshot. Games copy this into their
/// viewport frame (`repame-sprite` snapshot, 3D viewport, netcode, ...).
#[derive(Component, Debug, Clone, Copy)]
pub struct VehicleTransform {
    pub translation: Vec3,
    pub rotation: Quat,
}

impl Default for VehicleTransform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }
}

/// Simulation-model vehicle. Stepped once per fixed step; see the sync fns.
#[derive(Component, Debug, Clone)]
pub struct Vehicle {
    pub config: VehicleConfig,
    pub state: VehicleState,
    pub input: VehicleInput,
}

impl Default for Vehicle {
    fn default() -> Self {
        Self {
            config: VehicleConfig::default(),
            state: VehicleState::default(),
            input: VehicleInput::neutral(),
        }
    }
}

/// Arcade-model vehicle with a surface response slot.
#[derive(Component, Debug, Clone)]
pub struct ArcadeVehicle {
    pub config: ArcadeConfig,
    pub state: ArcadeState,
    pub input: VehicleInput,
    pub surface: SurfaceMod,
    /// Per-frame boost feed (idle defaults).
    pub boost_speed_mult: f32,
    pub boost_accel: f32,
}

impl Default for ArcadeVehicle {
    fn default() -> Self {
        Self {
            config: ArcadeConfig::default(),
            state: ArcadeState::new(),
            input: VehicleInput::neutral(),
            surface: SurfaceMod::road(),
            boost_speed_mult: 1.0,
            boost_accel: 0.0,
        }
    }
}

/// Fixed-step simulation (frame-rate independent).
pub fn step_vehicles(time: Res<SimTime>, mut query: Query<&mut Vehicle>) {
    let dt = time.delta_secs;
    for mut v in &mut query {
        let input = v.input;
        let config = v.config.clone();
        v.state.step(&config, &input, dt);
    }
}

pub fn step_arcade_vehicles(time: Res<SimTime>, mut query: Query<&mut ArcadeVehicle>) {
    let dt = time.delta_secs;
    for mut v in &mut query {
        let input = v.input;
        let surface = v.surface;
        let config = v.config.clone();
        let (boost_mult, boost_accel) = (v.boost_speed_mult, v.boost_accel);
        v.state
            .step(&config, &input, &surface, boost_mult, boost_accel, dt);
    }
}

/// Sync for ground-plane games: pos.x -> x, pos.y -> z, yaw about +Y.
pub fn sync_vehicle_transforms_xz(mut query: Query<(&Vehicle, &mut VehicleTransform)>) {
    for (v, mut t) in &mut query {
        t.translation.x = v.state.pos.x;
        t.translation.z = v.state.pos.y;
        t.rotation = Quat::from_rotation_y(-v.state.heading_rad);
    }
}

pub fn sync_arcade_transforms_xz(mut query: Query<(&ArcadeVehicle, &mut VehicleTransform)>) {
    for (v, mut t) in &mut query {
        t.translation.x = v.state.pos.x;
        t.translation.z = v.state.pos.y;
        t.rotation = Quat::from_rotation_y(-v.state.heading_rad);
    }
}

/// Sync for top-down games: pos -> xy, heading about +Z.
pub fn sync_vehicle_transforms_xy(mut query: Query<(&Vehicle, &mut VehicleTransform)>) {
    for (v, mut t) in &mut query {
        t.translation.x = v.state.pos.x;
        t.translation.y = v.state.pos.y;
        t.rotation = Quat::from_rotation_z(v.state.heading_rad);
    }
}

/// Full-model car. Stepped with an explicit ground probe.
#[derive(Component, Debug, Clone)]
pub struct FullCar {
    pub config: CarConfig,
    pub state: CarState,
    pub input: VehicleInput,
    pub shift: GearShift,
}

impl Default for FullCar {
    fn default() -> Self {
        Self {
            config: CarConfig::default(),
            state: CarState::new(),
            input: VehicleInput::neutral(),
            shift: GearShift::None,
        }
    }
}

/// Plane: config + state + controls in one component.
#[derive(Component, Debug, Clone)]
pub struct PlaneBody {
    pub config: PlaneConfig,
    pub state: PlaneState,
    pub controls: PlaneControls,
}

impl Default for PlaneBody {
    fn default() -> Self {
        Self {
            config: PlaneConfig::default(),
            state: PlaneState::new(),
            controls: PlaneControls::default(),
        }
    }
}

/// Boat: config + state + controls in one component.
#[derive(Component, Debug, Clone)]
pub struct BoatBody {
    pub config: BoatConfig,
    pub state: BoatState,
    pub controls: BoatControls,
}

impl Default for BoatBody {
    fn default() -> Self {
        Self {
            config: BoatConfig::default(),
            state: BoatState::new(),
            controls: BoatControls::default(),
        }
    }
}

/// Full-car simulation against a [`GroundProbe`] resource. Register
/// with your probe type via `sim.add_system(step_full_cars::<MyProbe>)`.
/// Rapier users rebuild the probe per step (see `rapier_backend`).
pub fn step_full_cars<P: GroundProbe + Resource>(
    time: Res<SimTime>,
    probe: Res<P>,
    mut query: Query<&mut FullCar>,
) {
    let dt = time.delta_secs;
    for mut v in &mut query {
        let input = v.input;
        let shift = v.shift;
        let config = v.config.clone();
        v.shift = GearShift::None; // edge-triggered: consume manual shifts
        v.state.step(&config, &input, shift, &*probe, dt);
    }
}

pub fn step_planes(time: Res<SimTime>, mut query: Query<&mut PlaneBody>) {
    let dt = time.delta_secs;
    for mut v in &mut query {
        let controls = v.controls;
        let config = v.config;
        v.state.step(&config, &controls, dt);
    }
}

pub fn step_boats(time: Res<SimTime>, mut query: Query<&mut BoatBody>) {
    let dt = time.delta_secs;
    for mut v in &mut query {
        let controls = v.controls;
        let config = v.config.clone();
        v.state.step(&config, &controls, dt);
    }
}

fn sync_body(transform: &mut VehicleTransform, pos: Vec3, orient: Quat) {
    transform.translation = pos;
    transform.rotation = orient;
}

/// Sync position + orientation for 3D bodies.
pub fn sync_car_transforms(mut query: Query<(&FullCar, &mut VehicleTransform)>) {
    for (v, mut t) in &mut query {
        sync_body(&mut t, v.state.body.pos, v.state.body.orient);
    }
}

pub fn sync_plane_transforms(mut query: Query<(&PlaneBody, &mut VehicleTransform)>) {
    for (v, mut t) in &mut query {
        sync_body(&mut t, v.state.body.pos, v.state.body.orient);
    }
}

pub fn sync_boat_transforms(mut query: Query<(&BoatBody, &mut VehicleTransform)>) {
    for (v, mut t) in &mut query {
        sync_body(&mut t, v.state.body.pos, v.state.body.orient);
    }
}

/// Register arcade/kinematic vehicle stepping + ground-plane syncs.
/// Chained so every step runs before every sync, every fixed step.
/// Full cars need `step_full_cars::<P>` registered separately.
pub fn register_vehicle_systems(sim: &mut Sim) {
    sim.add_chained_systems(
        (
            step_vehicles,
            step_arcade_vehicles,
            sync_vehicle_transforms_xz,
            sync_arcade_transforms_xz,
        )
            .chain(),
    );
}

/// Register planes/boats stepping + 3D body syncs. Chained so every
/// step runs before every sync, every fixed step.
pub fn register_full_vehicle_systems(sim: &mut Sim) {
    sim.add_chained_systems(
        (
            step_planes,
            step_boats,
            sync_car_transforms,
            sync_plane_transforms,
            sync_boat_transforms,
        )
            .chain(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_steps_on_sim_time() {
        let mut sim = Sim::with_default_step();
        sim.world
            .spawn((Vehicle::default(), VehicleTransform::default()));
        sim.add_system(step_vehicles);
        sim.tick();
        let time = sim.world.resource::<SimTime>();
        assert!(time.delta_secs > 0.0);
    }

    #[test]
    fn xz_sync_maps_heading_to_yaw() {
        let mut world = bevy_ecs::prelude::World::new();
        let e = world
            .spawn((Vehicle::default(), VehicleTransform::default()))
            .id();
        {
            let mut v = world.get_mut::<Vehicle>(e).unwrap();
            v.state.pos = glam::Vec2::new(10.0, 20.0);
            v.state.heading_rad = 0.0;
        }
        let mut query = world.query::<(&Vehicle, &mut VehicleTransform)>();
        for (v, mut t) in query.iter_mut(&mut world) {
            t.translation.x = v.state.pos.x;
            t.translation.z = v.state.pos.y;
            t.rotation = Quat::from_rotation_y(-v.state.heading_rad);
        }
        let t = world.get::<VehicleTransform>(e).unwrap();
        assert_eq!((t.translation.x, t.translation.z), (10.0, 20.0));
    }
}
