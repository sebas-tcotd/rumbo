//! Piloto automático: sigue el eje de la pista con persecución pura.
//!
//! Sirve para la prueba de manejo automática (detecta golpes o pistas rotas)
//! y para generar replays de referencia. Produce entradas normales: lo que se graba
//! en el replay son esas entradas, no el algoritmo.

use crate::input::InputFrame;
use crate::mathx::{atan2, norm, Vec3};
use crate::simulation::Simulation;

#[derive(Clone, Copy, Debug)]
pub struct Autopilot {
    /// Fracción del agarre teórico que usa para elegir la velocidad de curva.
    pub aggression: f32,
}

impl Default for Autopilot {
    fn default() -> Self {
        Self { aggression: 0.80 }
    }
}

impl Autopilot {
    pub fn input(&self, sim: &Simulation) -> InputFrame {
        let st = sim.view();
        let track = sim.track();
        let speed = st.vel.length();
        let rot = st.rot;
        let fwd = rot * -Vec3::Z;
        let right = rot * Vec3::X;

        // Punto objetivo por delante sobre el eje.
        let look = 7.0 + speed * 0.55;
        // En una esquina cerrada, apuntar lejos corta camino por dentro (persecución pura):
        // se acorta la mirada para que la cuerda no se aparte del eje más de `cut` metros.
        let k_near = track.max_curvature_ahead(st.sample_index, look).max(1e-4);
        let cut = 0.8 + 0.06 * speed;
        let chord = 2.0 * (2.0 * cut / k_near - cut * cut).max(0.0).sqrt();
        let look = look.min(chord.max(5.0));
        let ahead = (look / track.spacing).ceil() as isize;
        let target = track.sample(st.sample_index as isize + ahead).pos;
        let d = norm(target - st.pos);
        let ang = atan2(d.dot(right), d.dot(fwd));
        let steer = (ang * 2.6).clamp(-1.0, 1.0);

        // Velocidad objetivo según la curvatura de los próximos metros.
        let p = sim.profile();
        let horizon = 30.0 + speed * 1.6;
        let k = track.max_curvature_ahead(st.sample_index, horizon).max(1e-4);
        let v_curve = (self.aggression * p.front_grip * 9.81 / k).sqrt();
        let v_max = p.top_speed_kmh / 3.6 * 0.97;
        let v_target = v_curve.min(v_max);
        // Sprint: después de la llegada, frena hasta detenerse antes del muro final.
        let v_target = if !track.closed && st.s > track.finish_s + 2.0 { 0.0 } else { v_target };
        let (throttle, brake) = if !track.closed && v_target == 0.0 {
            (0.0, if speed > 0.5 { 1.0 } else { 0.0 })
        } else if speed < v_target - 1.0 {
            (1.0, 0.0)
        } else if speed > v_target + 2.0 {
            (0.0, ((speed - v_target) / 10.0).clamp(0.15, 1.0))
        } else {
            (0.35, 0.0)
        };

        InputFrame {
            steer: (steer * 127.0) as i8,
            throttle: (throttle * 255.0) as u8,
            brake: (brake * 255.0) as u8,
            flags: 0,
        }
    }
}
