//! Simulación completa de una sesión: mundo físico, pista, auto, cronómetro y grabación.
//!
//! Una sesión empieza con el auto detenido detrás de la meta y un perfil congelado.
//! Cambiar el perfil significa empezar una sesión nueva (así cada replay es reproducible).

use crate::hash::{to_hex, Fnv64};
use crate::input::InputFrame;
use crate::mathx::{basis_to_quat, is_finite3, Quat, Vec3};
use crate::profile::{ProfileFile, VehicleProfile};
use crate::race::{LapEvent, LapTimer};
use crate::replay::{Recorder, ReplayManifest, StateCheck, CHECK_EVERY, REPLAY_FORMAT};
use crate::track::{CompiledTrack, TrackDef};
use crate::vehicle::{Vehicle, WHEEL_RADIUS};
use crate::TICK_HZ;
use rapier3d::prelude::*;

/// Nombres de los campos del estado exportado, en orden. El front los lee de aquí
/// (vía `state_layout`) para no duplicar índices a mano.
pub const STATE_FIELDS: &[&str] = &[
    "tick", "px", "py", "pz", "qx", "qy", "qz", "qw", "vx", "vy", "vz",
    "speedKmh", "slipDeg", "steerDeg", "steerInput", "drift",
    "contactFL", "contactFR", "contactRL", "contactRR",
    "compFL", "compFR", "compRL", "compRR",
    "surfFL", "surfFR", "surfRL", "surfRR",
    "lap", "lapTicks", "lastLapTicks", "bestLapTicks", "nextCheckpoint",
    "progressS", "trackLength", "yawRate", "latG", "throttle", "brake", "invalid", "sampleIndex",
];
pub const STATE_LEN: usize = STATE_FIELDS.len();

const SPAWN_BEHIND: isize = 8;
/// Sprint: distancia del inicio de la pista a la grilla (la salida está más adelante).
const SPAWN_OPEN_M: f32 = 8.0;
/// Marca de los colisionadores de muro (los rayos de rueda los ignoran).
pub const USER_WALL: u128 = 1;
const SPAWN_HEIGHT: f32 = 0.62;

pub struct Simulation {
    world: PhysicsWorld,
    track: CompiledTrack,
    drivable: ColliderHandle,
    profile_file: ProfileFile,
    profile: VehicleProfile,
    vehicle: Vehicle,
    timer: LapTimer,
    tick: u64,
    near_idx: usize,
    /// Última muestra donde el auto estuvo sobre la pista; **R** vuelve ahí y no al punto
    /// más cercano, que podría estar más adelante en el circuito (atajo).
    last_good_idx: usize,
    s: f32,
    invalid: bool,
    seed: u64,
    recorder: Recorder,
    last_event: LapEvent,
}

/// Vista de solo lectura para quien consume el estado (autopiloto, pruebas).
pub struct StateView {
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub sample_index: usize,
    pub s: f32,
}

impl Simulation {
    pub fn new(track_def: &TrackDef, profile_file: &ProfileFile, seed: u64) -> Result<Self, String> {
        let track = crate::track::compile_track(track_def)?;
        let profile = profile_file.params.clamped();
        let mut world = PhysicsWorld::new();
        world.integration_parameters.dt = 1.0 / TICK_HZ as f32;

        let ground_body = world.insert_body(RigidBodyBuilder::fixed());
        let drivable_col = ColliderBuilder::trimesh_with_flags(
            track.drivable.vertices(),
            track.drivable.triangles(),
            TriMeshFlags::FIX_INTERNAL_EDGES,
        )
        .map_err(|e| format!("malla conducible inválida: {e:?}"))?
        .friction(1.0)
        .restitution(1.0)
        .build();
        let drivable = world.insert_collider(drivable_col, Some(ground_body));
        for b in &track.wall_boxes {
            let col = ColliderBuilder::cuboid(b.half_extents.x, b.half_extents.y, b.half_extents.z)
                .position(Pose::from_parts(b.center, b.rotation))
                .friction(1.0)
                .restitution(1.0)
                .user_data(USER_WALL)
                .build();
            world.insert_collider(col, Some(ground_body));
        }
        let floor = ColliderBuilder::cuboid(5000.0, 0.5, 5000.0)
            .translation(Vec3::new(0.0, track.ground_y - 0.5, 0.0))
            .friction(1.0)
            .restitution(1.0)
            .build();
        world.insert_collider(floor, Some(ground_body));

        let spawn_idx = Self::spawn_index(&track);
        let (pos, rot) = Self::pose_at(&track, spawn_idx);
        let vehicle = Vehicle::spawn(&mut world, &profile, pos, rot);
        // Construye la estructura de consultas antes del primer raycast.
        world.detect_collisions(&(), &());

        let s = track.project_s(pos, spawn_idx);
        Ok(Self {
            world,
            track,
            drivable,
            profile_file: ProfileFile { params: profile, ..profile_file.clone() },
            profile,
            vehicle,
            timer: LapTimer::default(),
            tick: 0,
            near_idx: spawn_idx,
            last_good_idx: spawn_idx,
            s,
            invalid: false,
            seed,
            recorder: Recorder::default(),
            last_event: LapEvent::None,
        })
    }

    /// Circuito: unos metros detrás de la meta. Sprint: al principio, antes de la salida.
    fn spawn_index(track: &CompiledTrack) -> usize {
        if track.closed {
            (track.samples.len() as isize - SPAWN_BEHIND).rem_euclid(track.samples.len() as isize) as usize
        } else {
            ((SPAWN_OPEN_M / track.spacing).round() as usize).min(track.samples.len() - 1)
        }
    }

    fn pose_at(track: &CompiledTrack, idx: usize) -> (Vec3, Quat) {
        let smp = track.samples[idx];
        let up = smp.up;
        let fwd = smp.tangent;
        let right = fwd.cross(up).normalize();
        let pos = smp.pos + up * (SPAWN_HEIGHT + WHEEL_RADIUS * 0.0);
        (pos, basis_to_quat(right, up, fwd))
    }

    /// Avanza exactamente un tick con la entrada dada.
    pub fn step(&mut self, input: InputFrame) {
        self.recorder.push(input);
        if input.restart() {
            self.restart();
        } else if input.reset() {
            self.reset_to_track();
        }
        let dt = self.world.integration_parameters.dt;
        self.vehicle.apply(&mut self.world, self.drivable, &self.track, &self.profile, input, dt);
        self.world.step();

        if !self.body_finite() {
            self.invalid = true;
            self.reset_to_track();
        }

        let pos = self.world.bodies[self.vehicle.body].translation();
        self.near_idx = self.track.nearest_near(pos, self.near_idx, 12);
        // En una red (S3) el auto puede irse por una calle lateral y volver a la pista por
        // otro lado: si quedó lejos de la ventana, se busca en toda la pista.
        let mut jumped = false;
        if self.track.network {
            let smp = &self.track.samples[self.near_idx];
            if (smp.pos - pos).length() > smp.width * 0.5 + 6.0 {
                self.near_idx = self.track.nearest_global(pos);
                jumped = true;
            }
        }
        // Sobre la pista: a no más de 3 m del borde de la calzada, siguiendo el avance normal.
        let smp = &self.track.samples[self.near_idx];
        if !jumped && (smp.pos - pos).length() <= smp.width * 0.5 + 3.0 {
            self.last_good_idx = self.near_idx;
        }
        let new_s = self.track.project_s(pos, self.near_idx);
        self.tick += 1;
        self.last_event = self.timer.update(&self.track, self.s, new_s, self.tick);
        self.s = new_s;

        if self.tick.is_multiple_of(CHECK_EVERY) {
            let hash = self.state_hash_hex();
            self.recorder.checks.push(StateCheck { tick: self.tick, hash });
        }
    }

    fn body_finite(&self) -> bool {
        let rb = &self.world.bodies[self.vehicle.body];
        let q = *rb.rotation();
        is_finite3(rb.translation())
            && is_finite3(rb.linvel())
            && is_finite3(rb.angvel())
            && q.x.is_finite()
            && q.y.is_finite()
            && q.z.is_finite()
            && q.w.is_finite()
    }

    /// Vuelve a la grilla de salida en reposo y descarta el intento en curso.
    pub fn restart(&mut self) {
        let idx = Self::spawn_index(&self.track);
        let (p, r) = Self::pose_at(&self.track, idx);
        self.vehicle.place(&mut self.world, p, r);
        self.near_idx = idx;
        self.last_good_idx = idx;
        self.s = self.track.project_s(p, idx);
        self.timer.restart();
    }

    /// Recoloca el auto en el centro de la pista, mirando hacia adelante, en el último punto
    /// donde estuvo sobre ella: salirse nunca acerca a la meta.
    pub fn reset_to_track(&mut self) {
        let idx = self.last_good_idx;
        let (p, r) = Self::pose_at(&self.track, idx);
        self.vehicle.place(&mut self.world, p, r);
        self.near_idx = idx;
        self.s = self.track.project_s(p, idx);
    }

    /// Dev tool (AR28): places the car at rest on the centerline at lap distance `s` (wrapped on
    /// circuits, clamped on sprints) and discards the current attempt, like `restart`.
    ///
    /// It is not an input, so the replay does not record it: a session that teleports no longer
    /// reproduces and the host must treat it as altered. The normal tick is unchanged.
    pub fn teleport_to_s(&mut self, s: f32) {
        let n = self.track.samples.len();
        let raw = if s.is_finite() { (s / self.track.spacing).round() as i64 } else { 0 };
        let idx = if self.track.closed {
            raw.rem_euclid(n as i64) as usize
        } else {
            raw.clamp(0, n as i64 - 1) as usize
        };
        let (p, r) = Self::pose_at(&self.track, idx);
        self.vehicle.place(&mut self.world, p, r);
        self.near_idx = idx;
        self.last_good_idx = idx;
        self.s = self.track.project_s(p, idx);
        self.timer.restart();
    }

    // ── Lectura ──

    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn track(&self) -> &CompiledTrack {
        &self.track
    }
    pub fn profile(&self) -> &VehicleProfile {
        &self.profile
    }
    pub fn timer(&self) -> &LapTimer {
        &self.timer
    }
    pub fn last_event(&self) -> LapEvent {
        self.last_event
    }
    pub fn invalid(&self) -> bool {
        self.invalid
    }
    pub fn vehicle(&self) -> &Vehicle {
        &self.vehicle
    }

    pub fn view(&self) -> StateView {
        let rb = &self.world.bodies[self.vehicle.body];
        StateView {
            pos: rb.translation(),
            rot: *rb.rotation(),
            vel: rb.linvel(),
            sample_index: self.near_idx,
            s: self.s,
        }
    }

    pub fn track_hash_hex(&self) -> String {
        self.track.track_hash_hex()
    }

    pub fn profile_hash_hex(&self) -> String {
        to_hex(self.profile.hash())
    }

    /// Hash canónico del estado: bits de f32 en orden fijo, nunca objetos serializados.
    pub fn state_hash(&self) -> u64 {
        let rb = &self.world.bodies[self.vehicle.body];
        let mut h = Fnv64::new();
        h.str_u32_len("sim-state/1").u64(self.tick);
        let t = rb.translation();
        let q = *rb.rotation();
        let v = rb.linvel();
        let w = rb.angvel();
        for x in [t.x, t.y, t.z, q.x, q.y, q.z, q.w, v.x, v.y, v.z, w.x, w.y, w.z] {
            h.f32(x);
        }
        self.vehicle.hash_into(&mut h);
        self.timer.hash_into(&mut h);
        h.u32(self.near_idx as u32).f32(self.s).u32(self.invalid as u32);
        h.finish()
    }

    pub fn state_hash_hex(&self) -> String {
        to_hex(self.state_hash())
    }

    /// Escribe el estado exportado en `out` siguiendo `STATE_FIELDS`.
    pub fn write_state(&self, out: &mut [f32]) {
        let rb = &self.world.bodies[self.vehicle.body];
        let t = rb.translation();
        let q = *rb.rotation();
        let v = rb.linvel();
        let veh = &self.vehicle;
        let tel = &veh.telemetry;
        let opt = |x: Option<u64>| x.map_or(-1.0, |v| v as f32);
        let vals: [f32; STATE_LEN] = [
            self.tick as f32,
            t.x, t.y, t.z, q.x, q.y, q.z, q.w, v.x, v.y, v.z,
            tel.speed_kmh, tel.slip_deg, tel.steer_deg, veh.steer, veh.drift as u8 as f32,
            veh.contact[0] as u8 as f32, veh.contact[1] as u8 as f32,
            veh.contact[2] as u8 as f32, veh.contact[3] as u8 as f32,
            veh.compression[0], veh.compression[1], veh.compression[2], veh.compression[3],
            tel.surfaces[0] as f32, tel.surfaces[1] as f32, tel.surfaces[2] as f32, tel.surfaces[3] as f32,
            self.timer.lap as f32,
            self.timer.current_ticks(self.tick) as f32,
            opt(self.timer.last_lap_ticks),
            opt(self.timer.best_lap_ticks),
            self.timer.next_cp as f32,
            self.s, self.track.length, tel.yaw_rate, tel.lat_g, tel.throttle, tel.brake,
            self.invalid as u8 as f32,
            self.near_idx as f32,
        ];
        let n = out.len().min(STATE_LEN);
        out[..n].copy_from_slice(&vals[..n]);
    }

    /// Replay de toda la sesión hasta el tick actual.
    pub fn replay(&self) -> ReplayManifest {
        ReplayManifest {
            format: REPLAY_FORMAT.into(),
            physics_version: crate::PHYSICS_VERSION.into(),
            compiler_version: crate::COMPILER_VERSION.into(),
            tick_hz: TICK_HZ,
            track_id: self.track.id.clone(),
            track_hash: self.track_hash_hex(),
            profile: self.profile_file.clone(),
            profile_hash: self.profile_hash_hex(),
            seed: self.seed,
            ticks: self.tick,
            inputs: self.recorder.runs.clone(),
            checks: self.recorder.checks.clone(),
            final_state_hash: self.state_hash_hex(),
            laps_ticks: self.timer.completed.clone(),
            invalid: self.invalid,
        }
    }
}
