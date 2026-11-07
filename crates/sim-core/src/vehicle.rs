//! Vehículo arcade sobre un cuerpo rígido de Rapier.
//!
//! Capas:
//! 1. Física base: suspensión por raycast y neumáticos simplificados (curva con meseta).
//! 2. Asistencias deterministas: rampa de dirección, giro según velocidad, contravolante,
//!    estabilidad de giro, derrape por toque de freno, estabilizador y control en el aire.
//!
//! Convención del motor: X derecha, Y arriba, -Z adelante. Dirección positiva = derecha.

use crate::hash::Fnv64;
use crate::input::InputFrame;
use crate::mathx::{atan2, clamp01, cos, lerp, norm, sign, sin, tan, Quat, Vec3, DEG};
use crate::profile::VehicleProfile;
use crate::track::{CompiledTrack, Surface};
use rapier3d::prelude::*;

pub const HALF_EXTENTS: Vec3 = Vec3::new(0.9, 0.35, 2.1);
pub const WHEEL_RADIUS: f32 = 0.33;
const FRONT_Z: f32 = -1.35;
const REAR_Z: f32 = 1.30;
const TRACK_HALF: f32 = 0.78;
const HARDPOINT_Y: f32 = -0.05;
pub const WHEELBASE: f32 = REAR_Z - FRONT_Z;
pub const HARDPOINTS: [Vec3; 4] = [
    Vec3::new(-TRACK_HALF, HARDPOINT_Y, FRONT_Z),
    Vec3::new(TRACK_HALF, HARDPOINT_Y, FRONT_Z),
    Vec3::new(-TRACK_HALF, HARDPOINT_Y, REAR_Z),
    Vec3::new(TRACK_HALF, HARDPOINT_Y, REAR_Z),
];
const G: f32 = 9.81;

#[derive(Clone, Copy, Debug, Default)]
pub struct Telemetry {
    pub speed_kmh: f32,
    pub slip_deg: f32,
    pub steer_deg: f32,
    pub yaw_rate: f32,
    pub lat_g: f32,
    pub throttle: f32,
    pub brake: f32,
    pub surfaces: [u8; 4],
}

pub struct Vehicle {
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
    /// Entrada de dirección ya pasada por la rampa, en -1..1.
    pub steer: f32,
    /// Ángulo aplicado a las ruedas delanteras (rad), incluye contravolante.
    pub steer_angle: f32,
    pub drift: bool,
    drift_dir: f32,
    drift_time: f32,
    prev_beta: f32,
    prev_brake: f32,
    brake_time: f32,
    prev_handbrake: bool,
    handbrake_time: f32,
    prev_v_heading: f32,
    pub compression: [f32; 4],
    /// Falso justo después de recolocar el auto: el primer tick no usa velocidad de
    /// compresión (la compresión "salta" de 0 a la de reposo y el amortiguador lo lanzaría).
    compression_valid: bool,
    pub contact: [bool; 4],
    pub touching_wall: bool,
    pub telemetry: Telemetry,
}

struct WheelHit {
    contact: bool,
    point: Vec3,
    normal: Vec3,
    load: f32,
    grip: f32,
    surface: Surface,
    hardpoint: Vec3,
    spring_force: f32,
}

impl Vehicle {
    pub fn spawn(world: &mut PhysicsWorld, profile: &VehicleProfile, pos: Vec3, rot: Quat) -> Self {
        let m = profile.mass_kg;
        let (w, h, l) = (HALF_EXTENTS.x * 2.0, 1.2f32, HALF_EXTENTS.z * 2.0);
        let inertia = Vec3::new(
            m / 12.0 * (h * h + l * l),
            m / 12.0 * (w * w + l * l) * profile.yaw_inertia_scale,
            m / 12.0 * (w * w + h * h),
        );
        let com = Vec3::new(0.0, profile.com_height_m, -profile.com_forward_m);
        let rb = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(pos, rot))
            .additional_mass_properties(MassProperties::new(com, m, inertia))
            .ccd_enabled(true)
            .can_sleep(false)
            .angular_damping(0.05)
            .build();
        // Aristas redondeadas: el auto se desliza por el muro en vez de engancharse.
        let r = 0.12;
        let col = ColliderBuilder::round_cuboid(HALF_EXTENTS.x - r, HALF_EXTENTS.y - r, HALF_EXTENTS.z - r, r)
            .density(0.0)
            .friction(profile.wall_friction)
            .friction_combine_rule(CoefficientCombineRule::Min)
            .restitution(profile.wall_restitution)
            .restitution_combine_rule(CoefficientCombineRule::Min)
            .build();
        let (body, collider) = world.insert(rb, col);
        Self {
            body,
            collider,
            steer: 0.0,
            steer_angle: 0.0,
            drift: false,
            drift_dir: 0.0,
            drift_time: 0.0,
            prev_beta: 0.0,
            prev_brake: 0.0,
            brake_time: 10.0,
            prev_handbrake: false,
            handbrake_time: 10.0,
            prev_v_heading: 0.0,
            compression: [0.0; 4],
            compression_valid: false,
            contact: [false; 4],
            touching_wall: false,
            telemetry: Telemetry::default(),
        }
    }

    /// Recoloca el auto en reposo (reset o respawn).
    pub fn place(&mut self, world: &mut PhysicsWorld, pos: Vec3, rot: Quat) {
        let rb = &mut world.bodies[self.body];
        rb.set_position(Pose::from_parts(pos, rot), true);
        rb.set_linvel(Vec3::ZERO, true);
        rb.set_angvel(Vec3::ZERO, true);
        rb.reset_forces(true);
        rb.reset_torques(true);
        self.steer = 0.0;
        self.steer_angle = 0.0;
        self.drift = false;
        self.drift_time = 0.0;
        self.prev_beta = 0.0;
        self.compression = [0.0; 4];
        self.compression_valid = false;
    }

    /// Calcula y aplica todas las fuerzas del tick. Rapier integra en `world.step()`.
    pub fn apply(
        &mut self,
        world: &mut PhysicsWorld,
        drivable: ColliderHandle,
        track: &CompiledTrack,
        p: &VehicleProfile,
        input: InputFrame,
        dt: f32,
    ) {
        // ── Muro indulgente ──
        // Mientras el chasis toca un muro, se recorta la velocidad vertical hacia arriba
        // y se amortiguan cabeceo y balanceo: rozar un muro desvía, no lanza el auto.
        let touching_wall = world.narrow_phase.contact_pairs_with(self.collider).any(|pair| {
            let other = if pair.collider1 == self.collider { pair.collider2 } else { pair.collider1 };
            pair.has_any_active_contact()
                && world.colliders.get(other).is_some_and(|c| c.user_data == crate::simulation::USER_WALL)
        });
        self.touching_wall = touching_wall;
        if touching_wall {
            let rb = &mut world.bodies[self.body];
            let up = *rb.rotation() * Vec3::Y;
            let mut v = rb.linvel();
            if v.y > 0.5 {
                v.y = 0.5;
            }
            let w = rb.angvel();
            let w_yaw = up * w.dot(up);
            rb.set_linvel(v, true);
            rb.set_angvel(w_yaw + (w - w_yaw) * 0.6, true);
        }

        let rb = &world.bodies[self.body];
        let pos = rb.translation();
        let rot = *rb.rotation();
        let com = rb.center_of_mass();
        let v = rb.linvel();
        let w = rb.angvel();
        let mass = rb.mass();
        let right = rot * Vec3::X;
        let up = rot * Vec3::Y;
        let fwd = rot * -Vec3::Z;

        let v_fwd = v.dot(fwd);
        let v_right = v.dot(right);
        let speed = v.length();
        let speed_kmh = speed * 3.6;
        let throttle = input.throttle_f();
        let brake = input.brake_f();
        let handbrake = input.handbrake();

        // ── Dirección: rampa (clave con teclado) y giro máximo según velocidad ──
        let target = input.steer_f();
        let returning = target.abs() < self.steer.abs() - 1e-4 || target * self.steer < 0.0;
        let rate = if returning { p.steer_return_rate } else { p.steer_rate };
        self.steer += (target - self.steer).clamp(-rate * dt, rate * dt);
        let lock = lerp(p.steer_lock_low_deg, p.steer_lock_high_deg, clamp01(speed_kmh / p.steer_lock_speed_kmh)) * DEG;
        let beta = if v_fwd > 2.0 { atan2(v_right, v_fwd) } else { 0.0 };
        // Rumbo de la trayectoria (ángulo antihorario sobre Y desde -Z) y su tasa de cambio.
        let v_heading = atan2(-v.x, -v.z);
        let r_path = if speed > 2.0 {
            let mut d = v_heading - self.prev_v_heading;
            if d > core::f32::consts::PI {
                d -= 2.0 * core::f32::consts::PI;
            } else if d < -core::f32::consts::PI {
                d += 2.0 * core::f32::consts::PI;
            }
            d / dt
        } else {
            0.0
        };
        let cs = p.counter_steer_assist * beta * clamp01((speed - 5.0) / 10.0);
        let max_angle = p.steer_lock_low_deg * DEG;
        self.steer_angle = (self.steer * lock + cs).clamp(-max_angle, max_angle);

        // ── Máquina de estados del derrape ──
        // Como en los NFS clásicos: freno + dirección es una frenada en curva normal;
        // freno de mano + dirección inicia el derrape. La entrada por toque de freno
        // queda como opción del perfil (`drift_brake_tap`).
        // El freno de mano requiere intención actual; el freno normal conserva
        // su ventana opcional de entrada.
        if brake > 0.3 && self.prev_brake <= 0.3 {
            self.brake_time = 0.0;
        } else {
            self.brake_time = (self.brake_time + dt).min(10.0);
        }
        if handbrake && !self.prev_handbrake {
            self.handbrake_time = 0.0;
        } else {
            self.handbrake_time = (self.handbrake_time + dt).min(10.0);
        }
        let brake_tap = p.drift_brake_tap >= 0.5 && self.brake_time < 0.35;
        let steer_threshold = if handbrake { 0.4 } else { 0.55 };
        if !self.drift
            && speed_kmh > p.drift_entry_speed_kmh
            && self.steer.abs() > steer_threshold
            && v_fwd > 0.0
            && (brake_tap || handbrake)
        {
            self.drift = true;
            self.drift_dir = sign(self.steer);
            self.drift_time = 0.0;
        }
        if self.drift {
            self.drift_time += dt;
            let settled = self.drift_time > 0.3 && beta.abs() < p.drift_exit_angle_deg * DEG;
            let counter_steering = self.steer * self.drift_dir < -0.2;
            if settled
                || (counter_steering && !handbrake && self.drift_time > 0.3)
                || speed_kmh < p.drift_entry_speed_kmh * 0.6
                || v_fwd <= 0.0
                || (!handbrake && !brake_tap && self.drift_time > 0.5)
            {
                self.drift = false;
            }
        }

        // ── Suspensión por raycast ──
        // Las ruedas solo apoyan en superficies conducibles: un rayo que toca un muro, o una
        // normal casi vertical, no es suelo (si no, el estabilizador volcaría el auto).
        let not_wall = |_: ColliderHandle, c: &Collider| c.user_data != crate::simulation::USER_WALL;
        let filter = QueryFilter::default().exclude_rigid_body(self.body).predicate(&not_wall);
        let max_len = p.suspension_rest_m + WHEEL_RADIUS;
        let mut hits: Vec<WheelHit> = Vec::with_capacity(4);
        for (i, hp_local) in HARDPOINTS.iter().enumerate() {
            let hp = pos + rot * *hp_local;
            let ray = Ray::new(hp, -up);
            let hit = world.cast_ray_and_get_normal(&ray, max_len, true, filter);
            let hit = hit.filter(|(_, inter)| inter.normal.dot(Vec3::Y) > 0.5);
            match hit {
                Some((handle, inter)) => {
                    let t = inter.time_of_impact;
                    let comp = (max_len - t).clamp(0.0, p.suspension_rest_m);
                    let comp_vel = if self.compression_valid { (comp - self.compression[i]) / dt } else { 0.0 };
                    let mut f = p.spring_n_per_m * comp + p.damper_n_s_per_m * comp_vel;
                    // Tope de suspensión: evita que el chasis toque el piso en compresiones fuertes.
                    let bump = comp - p.suspension_rest_m * 0.9;
                    if bump > 0.0 {
                        f += p.spring_n_per_m * 6.0 * bump;
                    }
                    let f = f.max(0.0);
                    let surface = if handle == drivable {
                        match inter.feature {
                            FeatureId::Face(tri) => track.drivable.triangle_surface(tri as usize),
                            _ => Surface::Asphalt,
                        }
                    } else {
                        Surface::Wall
                    };
                    self.compression[i] = comp;
                    self.contact[i] = true;
                    hits.push(WheelHit {
                        contact: true,
                        point: hp + (-up) * t,
                        normal: norm(inter.normal),
                        load: f,
                        grip: surface.grip(),
                        surface,
                        hardpoint: hp,
                        spring_force: f,
                    });
                }
                None => {
                    self.compression[i] = 0.0;
                    self.contact[i] = false;
                    hits.push(WheelHit {
                        contact: false,
                        point: hp,
                        normal: up,
                        load: 0.0,
                        grip: 0.0,
                        surface: Surface::Asphalt,
                        hardpoint: hp,
                        spring_force: 0.0,
                    });
                }
            }
        }

        let grounded_front = hits[..2].iter().filter(|h| h.contact).count() as f32;
        let grounded_rear = hits[2..].iter().filter(|h| h.contact).count() as f32;
        let grounded = grounded_front + grounded_rear;

        // ── Tracción, frenada y marcha atrás ──
        let reversing = brake > 0.1 && throttle < 0.1 && v_fwd < 1.0;
        let top = p.top_speed_kmh / 3.6;
        let drive_total = if reversing {
            if v_fwd > -7.0 { -0.45 * p.drive_force_n * brake } else { 0.0 }
        } else {
            let k = (v_fwd.max(0.0) / top).min(1.0);
            p.drive_force_n * throttle * (1.0 - k * k)
        };
        let brake_total = if reversing { 0.0 } else { p.brake_force_n * brake };

        // Precalcula velocidades de contacto antes de pedir el cuerpo como mutable.
        let contact_vel: Vec<Vec3> = hits.iter().map(|h| rb.velocity_at_point(h.point)).collect();

        let rb = &mut world.bodies[self.body];
        rb.reset_forces(true);
        rb.reset_torques(true);

        for (i, h) in hits.iter().enumerate() {
            if !h.contact {
                continue;
            }
            let front = i < 2;
            // Resorte, aplicado en el punto de anclaje.
            rb.add_force_at_point(up * h.spring_force, h.hardpoint, true);

            let delta = if front { self.steer_angle } else { 0.0 };
            let fw = fwd * cos(delta) + right * sin(delta);
            let n = h.normal;
            let fc = norm(fw - n * fw.dot(n));
            let rc = norm(fc.cross(n));
            let vp = contact_vel[i];
            let vl = vp.dot(fc);
            let vt = vp.dot(rc);

            // Longitudinal.
            let (axle_drive, axle_count) = if front {
                (p.front_drive_ratio, grounded_front)
            } else {
                (1.0 - p.front_drive_ratio, grounded_rear)
            };
            let drive = drive_total * axle_drive / axle_count.max(1.0);
            let bias = if front { p.brake_front_bias } else { 1.0 - p.brake_front_bias };
            let roll_sign = (vl / 0.5).clamp(-1.0, 1.0);
            let mut fx = drive - brake_total * bias / axle_count.max(1.0) * roll_sign - vl * 12.0;
            if !front && handbrake {
                fx -= p.brake_force_n * 0.8 * roll_sign;
            }

            // Agarre disponible.
            let mut lat_grip = if front { p.front_grip } else { p.rear_grip };
            if !front {
                // En derrape con freno de mano sostenido manda el menor de los dos: no se suman.
                let scale = match (self.drift, handbrake) {
                    (true, true) => p.drift_rear_grip_scale.min(p.handbrake_grip_scale),
                    (true, false) => p.drift_rear_grip_scale,
                    (false, true) => p.handbrake_grip_scale,
                    (false, false) => 1.0,
                };
                lat_grip *= scale;
            }
            let fx_max = p.long_grip * h.load * h.grip;
            fx = fx.clamp(-fx_max, fx_max);
            let ratio = if fx_max > 1e-3 { fx / fx_max } else { 0.0 };
            let fy_peak = lat_grip * h.load * h.grip * (1.0 - p.combined_slip_coupling * ratio * ratio).max(0.0).sqrt();

            // Lateral: curva lineal hasta el pico y meseta arcade al deslizar.
            let alpha = atan2(vt, vl.abs() + 0.5);
            let a = alpha.abs() / (p.slip_peak_deg * DEG * h.surface.slip_peak_multiplier());
            let g = if a <= 1.0 {
                a
            } else if a <= 3.0 {
                1.0 - (1.0 - p.slide_grip_ratio) * (a - 1.0) * 0.5
            } else {
                p.slide_grip_ratio
            };
            let fy_hi = -sign(vt) * fy_peak * g;
            let fy_lo = (-vt * mass * 0.25 * 15.0).clamp(-fy_peak, fy_peak);
            let fy = lerp(fy_lo, fy_hi, clamp01(speed / 4.0));

            // Punto de aplicación elevado hacia el centro de masa: menos vuelco, más arcade.
            let lift = (com - h.point).dot(up) * p.force_height_ratio;
            let app = h.point + up * lift;
            rb.add_force_at_point(fc * fx + rc * fy, app, true);
        }

        // ── Aerodinámica ──
        rb.add_force(-v * speed * p.drag_coef, true);
        rb.add_force(-up * (p.downforce_coef * speed * speed), true);

        // ── Asistencias de giro ──
        let r = w.dot(up);
        let mut yaw_torque = 0.0;
        if grounded >= 2.0 && speed > 2.0 {
            if self.drift {
                // Control de velocidad de giro: β = ψ_cuerpo − ψ_trayectoria, así que
                // dβ/dt = r − r_trayectoria. Se pide r = r_trayectoria + k·(β_obj − β),
                // y el ángulo converge sin oscilar.
                let k = self.steer * self.drift_dir;
                let mag = (p.drift_target_angle_deg * (0.55 + 0.45 * k)).max(p.drift_target_angle_deg * 0.1);
                let recovering = !handbrake && self.steer * self.drift_dir < -0.2;
                let beta_t = if recovering { 0.0 } else { -self.drift_dir * mag * DEG };
                let recovery_gain = if recovering { 2.5 } else { 1.0 };
                let r_des = r_path + (beta_t - beta) * p.drift_angle_kp * recovery_gain;
                yaw_torque = p.drift_angle_kd * (r_des - r);
                let vh = norm(v - up * v.dot(up));
                rb.add_force(vh * (p.drift_speed_assist_n * throttle), true);
            } else {
                let mut r_des = -v_fwd * tan(self.steer_angle) / WHEELBASE;
                let r_lim = p.front_grip * G / speed.max(1.0);
                r_des = r_des.clamp(-r_lim, r_lim);
                yaw_torque = -p.yaw_damping * (r - r_des);
            }
        }
        let torque_cap = if self.drift { p.max_assist_torque_nm * 2.5 } else { p.max_assist_torque_nm };
        let yaw_torque = yaw_torque.clamp(-torque_cap, torque_cap);
        let mut torque = up * yaw_torque;

        if grounded >= 1.0 {
            // Estabilizador: alinea el chasis con la normal media del suelo.
            let mut n_avg = Vec3::ZERO;
            for h in hits.iter().filter(|h| h.contact) {
                n_avg += h.normal;
            }
            let n_avg = norm(n_avg);
            torque += up.cross(n_avg) * p.anti_roll_nm_per_rad;
            let w_rp = w - up * w.dot(up);
            torque -= w_rp * p.roll_pitch_damping;
        } else {
            // Control en el aire.
            torque += up * (-self.steer * p.air_control_nm);
            torque += right * (-(throttle - brake) * p.air_control_nm * 0.5);
            torque -= w * 200.0;
        }
        rb.add_torque(torque, true);

        self.compression_valid = true;
        self.prev_beta = beta;
        self.prev_brake = brake;
        self.prev_handbrake = handbrake;
        self.prev_v_heading = v_heading;
        self.telemetry = Telemetry {
            speed_kmh: v_fwd.signum() * speed_kmh,
            slip_deg: beta / DEG,
            steer_deg: self.steer_angle / DEG,
            yaw_rate: r,
            lat_g: r * v_fwd / G,
            throttle,
            brake,
            surfaces: [hits[0].surface as u8, hits[1].surface as u8, hits[2].surface as u8, hits[3].surface as u8],
        };
    }

    pub fn hash_into(&self, h: &mut Fnv64) {
        h.f32(self.steer).f32(self.steer_angle).u32(self.drift as u32).f32(self.drift_dir).f32(self.drift_time);
        h.f32(self.prev_beta).f32(self.prev_brake).f32(self.brake_time).u32(self.prev_handbrake as u32).f32(self.handbrake_time).f32(self.prev_v_heading).u32(self.touching_wall as u32);
        for c in self.compression {
            h.f32(c);
        }
        h.u32(self.compression_valid as u32);
    }
}
