use bevy_app::{App, FixedUpdate, Plugin, Update};
use bevy_ecs::prelude::{Component, Query, Res, Resource};
use bevy_math::{Quat, Vec3};
use bevy_time::Time;
use bevy_transform::components::Transform;

use crate::arcade::{ArcadeConfig, ArcadeState, SurfaceMod};
use crate::car::{CarConfig, CarState, GearShift};
use crate::flight::{PlaneConfig, PlaneControls, PlaneState};
use crate::ground::GroundProbe;
use crate::marine::{BoatConfig, BoatControls, BoatState};
use crate::{VehicleConfig, VehicleInput, VehicleState};

/// Simulation-model vehicle. Stepped in `FixedUpdate`; see the sync fns.
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
pub fn step_vehicles(time: Res<Time>, mut query: Query<&mut Vehicle>) {
    let dt = time.delta_secs();
    for mut v in &mut query {
        let input = v.input;
        let config = v.config.clone();
        v.state.step(&config, &input, dt);
    }
}

pub fn step_arcade_vehicles(time: Res<Time>, mut query: Query<&mut ArcadeVehicle>) {
    let dt = time.delta_secs();
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
pub fn sync_vehicle_transforms_xz(mut query: Query<(&Vehicle, &mut Transform)>) {
    for (v, mut t) in &mut query {
        t.translation.x = v.state.pos.x;
        t.translation.z = v.state.pos.y;
        t.rotation = Quat::from_rotation_y(-v.state.heading_rad);
    }
}

pub fn sync_arcade_transforms_xz(mut query: Query<(&ArcadeVehicle, &mut Transform)>) {
    for (v, mut t) in &mut query {
        t.translation.x = v.state.pos.x;
        t.translation.z = v.state.pos.y;
        t.rotation = Quat::from_rotation_y(-v.state.heading_rad);
    }
}

/// Sync for top-down games: pos -> xy, heading about +Z.
pub fn sync_vehicle_transforms_xy(mut query: Query<(&Vehicle, &mut Transform)>) {
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
/// with your probe type: `step_full_cars::<MyProbe>`. Rapier users
/// rebuild the probe per step (see `rapier_backend`).
pub fn step_full_cars<P: GroundProbe + Resource>(
    time: Res<Time>,
    probe: Res<P>,
    mut query: Query<&mut FullCar>,
) {
    let dt = time.delta_secs();
    for mut v in &mut query {
        let input = v.input;
        let shift = v.shift;
        let config = v.config.clone();
        v.shift = GearShift::None; // edge-triggered: consume manual shifts
        v.state.step(&config, &input, shift, &*probe, dt);
    }
}

pub fn step_planes(time: Res<Time>, mut query: Query<&mut PlaneBody>) {
    let dt = time.delta_secs();
    for mut v in &mut query {
        let controls = v.controls;
        let config = v.config;
        v.state.step(&config, &controls, dt);
    }
}

pub fn step_boats(time: Res<Time>, mut query: Query<&mut BoatBody>) {
    let dt = time.delta_secs();
    for mut v in &mut query {
        let controls = v.controls;
        let config = v.config.clone();
        v.state.step(&config, &controls, dt);
    }
}

fn sync_body(transform: &mut Transform, pos: glam::Vec3, orient: glam::Quat) {
    transform.translation = Vec3::new(pos.x, pos.y, pos.z);
    transform.rotation = Quat::from_xyzw(orient.x, orient.y, orient.z, orient.w);
}

/// Sync position + orientation for 3D bodies.
pub fn sync_car_transforms(mut query: Query<(&FullCar, &mut Transform)>) {
    for (v, mut t) in &mut query {
        sync_body(&mut t, v.state.body.pos, v.state.body.orient);
    }
}

pub fn sync_plane_transforms(mut query: Query<(&PlaneBody, &mut Transform)>) {
    for (v, mut t) in &mut query {
        sync_body(&mut t, v.state.body.pos, v.state.body.orient);
    }
}

pub fn sync_boat_transforms(mut query: Query<(&BoatBody, &mut Transform)>) {
    for (v, mut t) in &mut query {
        sync_body(&mut t, v.state.body.pos, v.state.body.orient);
    }
}

pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (step_vehicles, step_arcade_vehicles));
        app.add_systems(
            Update,
            (sync_vehicle_transforms_xz, sync_arcade_transforms_xz),
        );
    }
}

/// Steps planes/boats in `FixedUpdate` with syncs in `Update`.
/// Full cars need `step_full_cars::<P>` registered separately.
pub struct FullVehiclePlugin;

impl Plugin for FullVehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (step_planes, step_boats));
        app.add_systems(
            Update,
            (
                sync_car_transforms,
                sync_plane_transforms,
                sync_boat_transforms,
            ),
        );
    }
}
