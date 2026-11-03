//! Perfil del vehículo: todos los parámetros de manejo, con unidades.
//!
//! El struct, sus valores por defecto y el esquema que consume el panel de ajuste
//! salen de una sola definición (la macro de abajo), para que no se desincronicen.

use crate::hash::Fnv64;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ParamMeta {
    pub key: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub help: &'static str,
    pub default: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

macro_rules! profile_params {
    ($( $group:literal { $( $field:ident : $label:literal, $unit:literal, $default:expr, $min:expr, $max:expr, $step:expr, $help:literal; )* } )*) => {
        /// Parámetros de manejo. Unidades del SI salvo que la unidad diga otra cosa.
        #[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct VehicleProfile {
            $( $( pub $field: f32, )* )*
        }

        impl Default for VehicleProfile {
            fn default() -> Self {
                Self { $( $( $field: $default, )* )* }
            }
        }

        pub const PARAMS: &[ParamMeta] = &[
            $( $( ParamMeta {
                key: stringify!($field), group: $group, label: $label, unit: $unit, help: $help,
                default: $default, min: $min, max: $max, step: $step,
            }, )* )*
        ];

        impl VehicleProfile {
            /// Valores en el orden del esquema (orden canónico para el hash).
            pub fn values(&self) -> Vec<f32> {
                vec![ $( $( self.$field, )* )* ]
            }

            /// Recorta cada valor a su rango válido.
            pub fn clamped(mut self) -> Self {
                let mut i = 0;
                $( $(
                    self.$field = self.$field.clamp(PARAMS[i].min, PARAMS[i].max);
                    i += 1;
                )* )*
                let _ = i;
                self
            }
        }
    };
}

profile_params! {
    "Chasis" {
        mass_kg: "Masa", "kg", 1250.0, 700.0, 2500.0, 10.0, "Masa total del auto.";
        com_height_m: "Altura del centro de masa", "m", -0.15, -0.40, 0.20, 0.01, "Relativa al centro del chasis. Más bajo = menos balanceo.";
        com_forward_m: "Centro de masa hacia adelante", "m", 0.0, -0.50, 0.50, 0.01, "Positivo = más peso adelante (subviraje).";
        yaw_inertia_scale: "Escala de inercia de giro", "×", 1.0, 0.5, 2.0, 0.05, "Mayor = el auto cambia de dirección con más pereza.";
    }
    "Suspensión" {
        suspension_rest_m: "Recorrido de suspensión", "m", 0.30, 0.15, 0.50, 0.01, "Longitud libre del resorte.";
        spring_n_per_m: "Rigidez de resorte", "N/m", 38000.0, 10000.0, 90000.0, 500.0, "Por rueda.";
        damper_n_s_per_m: "Amortiguación", "N·s/m", 3100.0, 500.0, 9000.0, 50.0, "Por rueda.";
        anti_roll_nm_per_rad: "Estabilizador", "N·m/rad", 9000.0, 0.0, 30000.0, 250.0, "Mantiene el chasis alineado con el suelo.";
        roll_pitch_damping: "Amortiguación de balanceo", "N·m·s/rad", 1500.0, 0.0, 8000.0, 50.0, "Frena las oscilaciones de balanceo y cabeceo.";
    }
    "Motor y frenos" {
        drive_force_n: "Fuerza de tracción", "N", 9500.0, 3000.0, 20000.0, 100.0, "Fuerza máxima desde detenido.";
        top_speed_kmh: "Velocidad máxima", "km/h", 230.0, 100.0, 350.0, 5.0, "La tracción cae a cero en esta velocidad.";
        front_drive_ratio: "Tracción delantera", "0–1", 0.0, 0.0, 1.0, 0.05, "0 = trasera, 1 = delantera.";
        brake_force_n: "Fuerza de frenado", "N", 16000.0, 5000.0, 30000.0, 250.0, "Total de las cuatro ruedas.";
        brake_front_bias: "Reparto de frenada delantero", "0–1", 0.62, 0.30, 0.85, 0.01, "Fracción de la frenada en el eje delantero.";
        drag_coef: "Arrastre aerodinámico", "N/(m/s)²", 0.42, 0.0, 1.2, 0.01, "";
        downforce_coef: "Carga aerodinámica", "N/(m/s)²", 1.1, 0.0, 5.0, 0.05, "Más agarre a alta velocidad.";
    }
    "Neumáticos" {
        front_grip: "Agarre lateral delantero", "μ", 1.65, 0.6, 3.0, 0.05, "";
        rear_grip: "Agarre lateral trasero", "μ", 1.60, 0.6, 3.0, 0.05, "";
        long_grip: "Agarre longitudinal", "μ", 1.50, 0.6, 3.0, 0.05, "Limita tracción y frenada.";
        slip_peak_deg: "Ángulo de deslizamiento pico", "°", 7.0, 2.0, 20.0, 0.5, "Ángulo en que el neumático da su máximo agarre.";
        slide_grip_ratio: "Agarre al deslizar", "0–1", 0.82, 0.30, 1.0, 0.01, "Fracción del pico que se conserva al pasarlo. Alto = arcade.";
        combined_slip_coupling: "Acople tracción-giro", "0–1", 0.35, 0.0, 1.0, 0.05, "Cuánto agarre lateral se pierde al acelerar o frenar.";
        force_height_ratio: "Altura de aplicación de fuerzas", "0–1", 0.70, 0.0, 1.0, 0.05, "1 = fuerzas a la altura del centro de masa (sin vuelcos).";
    }
    "Dirección" {
        steer_lock_low_deg: "Giro máximo a baja velocidad", "°", 34.0, 10.0, 50.0, 0.5, "";
        steer_lock_high_deg: "Giro máximo a alta velocidad", "°", 6.5, 2.0, 20.0, 0.5, "";
        steer_lock_speed_kmh: "Velocidad del giro mínimo", "km/h", 170.0, 60.0, 300.0, 5.0, "Velocidad a la que se alcanza el giro máximo de alta velocidad.";
        steer_rate: "Velocidad del volante", "1/s", 3.2, 0.5, 12.0, 0.1, "Fracción del recorrido por segundo al girar. Clave con teclado.";
        steer_return_rate: "Retorno al centro", "1/s", 5.5, 0.5, 15.0, 0.1, "Fracción del recorrido por segundo al soltar.";
    }
    "Asistencias" {
        yaw_damping: "Estabilidad de giro", "N·m·s/rad", 3500.0, 0.0, 15000.0, 100.0, "Evita trompos que el jugador no provocó.";
        counter_steer_assist: "Contravolante asistido", "×", 0.55, 0.0, 1.5, 0.05, "Alinea las ruedas con la trayectoria al deslizar.";
        max_assist_torque_nm: "Par máximo de asistencia", "N·m", 9000.0, 1000.0, 30000.0, 250.0, "";
    }
    "Derrape" {
        drift_entry_speed_kmh: "Velocidad mínima de derrape", "km/h", 55.0, 20.0, 150.0, 1.0, "";
        drift_target_angle_deg: "Ángulo objetivo de derrape", "°", 24.0, 5.0, 50.0, 0.5, "Con volante a fondo hacia la curva.";
        drift_rear_grip_scale: "Agarre trasero en derrape", "0–1", 0.58, 0.20, 1.0, 0.01, "";
        drift_angle_kp: "Respuesta del ángulo", "1/s", 4.0, 0.5, 15.0, 0.1, "Qué tan rápido el ángulo de derrape va hacia el objetivo.";
        drift_angle_kd: "Firmeza del control de giro", "N·m·s/rad", 18000.0, 0.0, 60000.0, 250.0, "Par por cada rad/s de error de giro durante el derrape.";
        drift_speed_assist_n: "Empuje en derrape", "N", 2600.0, 0.0, 8000.0, 50.0, "Mantiene la velocidad al derrapar con acelerador.";
        drift_exit_angle_deg: "Ángulo de salida", "°", 6.0, 1.0, 20.0, 0.5, "Por debajo de este ángulo el derrape termina.";
        handbrake_grip_scale: "Agarre trasero con freno de mano", "0–1", 0.35, 0.05, 1.0, 0.01, "";
        drift_brake_tap: "Derrapar también con toque de freno", "sí/no", 0.0, 0.0, 1.0, 1.0, "Apagado: solo el freno de mano inicia el derrape y el freno normal frena en curva.";
    }
    "Colisiones y aire" {
        wall_friction: "Fricción contra muros", "μ", 0.06, 0.0, 1.0, 0.01, "Bajo = rozar un muro no frena en seco.";
        wall_restitution: "Rebote contra muros", "0–1", 0.12, 0.0, 1.0, 0.01, "";
        air_control_nm: "Control en el aire", "N·m", 1800.0, 0.0, 8000.0, 50.0, "";
    }
}

/// Archivo de perfil tal como vive en `assets/profiles/*.json`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProfileFile {
    pub id: String,
    pub name: String,
    pub params: VehicleProfile,
}

impl Default for ProfileFile {
    fn default() -> Self {
        Self { id: "default".into(), name: "Por defecto".into(), params: VehicleProfile::default() }
    }
}

impl VehicleProfile {
    /// Hash canónico: bits de cada valor, en el orden del esquema, más su clave.
    pub fn hash(&self) -> u64 {
        let mut h = Fnv64::new();
        h.str("vehicle-profile/1");
        for (meta, v) in PARAMS.iter().zip(self.values()) {
            h.str(meta.key).f32(v);
        }
        h.finish()
    }
}

pub fn schema_json() -> String {
    serde_json::to_string(PARAMS).expect("schema serializa")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_within_ranges() {
        for (m, v) in PARAMS.iter().zip(VehicleProfile::default().values()) {
            assert!(v >= m.min && v <= m.max, "{} fuera de rango", m.key);
            assert_eq!(v, m.default);
        }
    }

    #[test]
    fn json_roundtrip_and_unknown_keys() {
        let p = VehicleProfile::default();
        let s = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<VehicleProfile>(&s).unwrap(), p);
        assert!(serde_json::from_str::<VehicleProfile>(r#"{"masa":1}"#).is_err());
    }

    #[test]
    fn hash_changes_with_values() {
        let a = VehicleProfile::default();
        let b = VehicleProfile { rear_grip: 1.61, ..a };
        assert_ne!(a.hash(), b.hash());
    }
}
