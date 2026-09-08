//! Vehicle dynamics with no engine coupling.
//!
//! - [`VehicleState`]: kinematic bicycle (prototyping, AI, netcode).
//! - [`arcade`]: scalar speed + yaw rate (karts, top-down racers).
//! - [`car`]: full 3D car - gearbox, suspension, tires, aero,
//!   stability, TCS/ABS, nitrous - over any [`ground::GroundProbe`].
//! - [`flight`], [`marine`]: planes and boats on the shared
//!   [`body::BodyState`] integrator.
//! - [`ai`], [`race`], [`track`], [`replay`], [`draft`]: racing logic.
//!
//! Pure logic everywhere; `repame-sim` adds components/systems, `rapier`
//! adds a rapier3d ground probe.

pub mod ai;
pub mod arcade;
pub mod body;
pub mod boost;
pub mod car;
pub mod draft;
pub mod drift;
pub mod drivetrain;
pub mod engine;
pub mod flight;
pub mod ground;
pub mod input;
pub mod marine;
pub mod race;
pub mod replay;
pub mod state;
pub mod steering;
pub mod suspension;
pub mod tire;
pub mod track;
pub mod tuning;
pub mod wheel;

pub mod repame_integration;

pub use ai::{AiConfig, AiOutput, Path, pursue};
pub use arcade::{ArcadeConfig, ArcadeState, SurfaceMod};
pub use body::{BodyState, RigidConfig};
pub use boost::BoostPool;
pub use car::{
    AeroConfig, AeroDevice, AxleConfig, CarConfig, CarState, GearShift, InductionConfig,
    NitrousConfig, StabilityConfig,
};
pub use draft::{DraftConfig, DraftState};
pub use drift::{ChargeLevel, DriftConfig, DriftState};
pub use drivetrain::{CenterDiff, Clutch, Differential, GearState, Gearbox};
pub use engine::{EngineConfig, EngineOutput, TorqueCurve};
pub use flight::{PlaneConfig, PlaneControls, PlaneState};
#[cfg(feature = "rapier")]
pub use ground::rapier_backend::RapierGround;
pub use ground::{FlatGround, GroundHit, GroundProbe, NoGround};
pub use input::VehicleInput;
pub use marine::{BoatConfig, BoatControls, BoatState};
pub use race::{Checkpoint, RaceConfig, RaceEvent, RaceState};
pub use replay::{Sample, Trace, lerp_angle};
pub use state::{VehicleConfig, VehicleState};
pub use steering::SteeringConfig;
pub use suspension::{SuspensionConfig, SuspensionState, anti_roll_force};
pub use tire::{SurfaceGrip, TireConfig};
pub use track::{TrackConfig, TrackEvent, TrackState};
pub use tuning::{StatMod, StatMods};
pub use wheel::{AbsConfig, TcsConfig, WheelConfig, WheelState, axle_anti_roll, corner_masses};
