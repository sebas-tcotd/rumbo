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

/// Versión de la simulación. Cambia cuando cambia cualquier cosa que altere el resultado
/// de un replay (física, asistencias, orden de pasos). Los replays guardan esta versión.
pub const PHYSICS_VERSION: &str = "0.4.0";

/// Versión del mini compilador de pistas de S0.
pub const COMPILER_VERSION: &str = "0.2.0";

/// Frecuencia de simulación por defecto (decisión a validar en S0).
pub const TICK_HZ: u32 = 120;
