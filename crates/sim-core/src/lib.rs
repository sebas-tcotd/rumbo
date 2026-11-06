//! sim-core: simulación determinista del laboratorio de manejo (S0).
//!
//! Reglas de determinismo (ver documento de arquitectura):
//! - Paso fijo (`TICK_HZ`). El llamador decide cuándo avanzar; aquí no hay reloj.
//! - Trigonometría solo vía `libm` (nunca funciones de la plataforma).
//! - Rapier con `enhanced-determinism`, que además fuerza matemática escalar en glam.
//! - Sin colecciones con orden aleatorio en el estado simulado.
//! - El hash del estado se calcula sobre una representación canónica (bits de f32 en orden fijo).

pub mod autopilot;
pub mod hash;
pub mod input;
pub mod mathx;
pub mod profile;
pub mod race;
pub mod replay;
pub mod simulation;
pub mod track;
pub mod vehicle;

pub use input::InputFrame;
pub use profile::{ProfileFile, VehicleProfile};
pub use replay::{ReplayManifest, VerifyReport};
pub use simulation::{Simulation, StateView, STATE_LEN};
pub use track::{CompiledTrack, TrackDef};

pub use replay::verify_replay;
pub use track::compile_track;
pub use track_format::version::{COMPILER_VERSION, PHYSICS_VERSION, TICK_HZ};
