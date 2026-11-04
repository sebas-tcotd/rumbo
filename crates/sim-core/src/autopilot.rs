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

        // Velocidad objetivo: en cada punto por delante, la velocidad de curva que permite el
        // agarre, más lo que se alcanza a frenar hasta llegar ahí. Así frena tarde y fuerte,
        // justo lo necesario, como un piloto, en vez de ir soltando velocidad desde lejos.
        let p = sim.profile();
        let g = 9.81;
        let decel = (p.brake_force_n / p.mass_kg).min(p.front_grip * g) * self.aggression;
        let v_max = p.top_speed_kmh / 3.6 * 0.97;
        let horizon = 20.0 + speed * speed / (2.0 * decel) * 1.3;
        let steps = (horizon / track.spacing).ceil() as isize;
        let mut v_target = v_max;
        for k in 0..=steps {
            let smp = track.sample(st.sample_index as isize + k);
            let curv = smp.curvature.abs().max(smp.curvature_tight).max(1e-4);
            let v_curve = (self.aggression * p.front_grip * g / curv).sqrt();
            let dist = (k as f32 * track.spacing - 3.0).max(0.0);
            v_target = v_target.min((v_curve * v_curve + 2.0 * decel * dist).sqrt());
            if !track.closed && st.s + k as f32 * track.spacing > track.finish_s + 2.0 {
                break;
            }
        }
        // Sprint: después de la llegada, frena hasta detenerse antes del muro final.
        let v_target = if !track.closed && st.s > track.finish_s + 2.0 { 0.0 } else { v_target };
        let (throttle, brake) = if !track.closed && v_target == 0.0 {
            (0.0, if speed > 0.5 { 1.0 } else { 0.0 })
        } else if speed > v_target + 0.5 {
            (0.0, ((speed - v_target) / 3.0).clamp(0.3, 1.0))
        } else if speed < v_target - 1.0 {
            (1.0, 0.0)
        } else {
            (0.5, 0.0)
        };

        InputFrame {
            steer: (steer * 127.0) as i8,
            throttle: (throttle * 255.0) as u8,
            brake: (brake * 255.0) as u8,
            flags: 0,
        }
    }
}
