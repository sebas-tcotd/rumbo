//! Entradas cuantizadas por tick. Un replay es una secuencia de estos valores.

use serde::{Deserialize, Serialize};

pub const FLAG_HANDBRAKE: u8 = 1 << 0;
/// Recoloca el auto sobre la pista (evento de un solo tick).
pub const FLAG_RESET: u8 = 1 << 1;
/// Vuelve a la grilla de salida y descarta el intento en curso (evento de un solo tick).
pub const FLAG_RESTART: u8 = 1 << 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputFrame {
    /// -127 (izquierda) .. 127 (derecha). El teclado envía -127, 0 o 127.
    pub steer: i8,
    /// 0..255
    pub throttle: u8,
    /// 0..255. También es marcha atrás cuando el auto está detenido.
    pub brake: u8,
    pub flags: u8,
}

/// One-shot event flags (D7, P1): they act only on the first tick of each 60 Hz frame.
/// Held controls such as `FLAG_HANDBRAKE` act on both ticks.
pub const EVENT_FLAGS: u8 = FLAG_RESET | FLAG_RESTART;

/// Canonical steering deadband (D7): a quantized `|steer|` below this value becomes 0.
/// About 3 % of the travel; the keyboard (±127) never falls inside it. It is part of the
/// physics contract: changing it needs a `physics:` commit.
pub const STEER_DEADBAND: i8 = 4;

impl InputFrame {
    /// Canonical quantization of analog axes (D7): steering in [-1, 1], throttle and brake in [0, 1].
    /// Values are clamped and rounded; NaN becomes 0. The steering deadband is applied after rounding.
    pub fn from_axes(steer: f32, throttle: f32, brake: f32, flags: u8) -> Self {
        let steer = quantize(steer, -1.0, 127.0) as i8;
        Self {
            steer: if steer.abs() < STEER_DEADBAND { 0 } else { steer },
            throttle: quantize(throttle, 0.0, 255.0) as u8,
            brake: quantize(brake, 0.0, 255.0) as u8,
            flags,
        }
    }

    /// The same frame as applied on the second tick of its pair: event flags removed (D7, P1).
    #[must_use]
    pub fn held(self) -> Self {
        Self { flags: self.flags & !EVENT_FLAGS, ..self }
    }

    pub fn pack(self) -> u32 {
        (self.steer as u8 as u32) | (self.throttle as u32) << 8 | (self.brake as u32) << 16 | (self.flags as u32) << 24
    }

    pub fn unpack(v: u32) -> Self {
        Self {
            steer: (v & 0xff) as u8 as i8,
            throttle: ((v >> 8) & 0xff) as u8,
            brake: ((v >> 16) & 0xff) as u8,
            flags: ((v >> 24) & 0xff) as u8,
        }
    }

    pub fn steer_f(self) -> f32 {
        (self.steer as f32 / 127.0).clamp(-1.0, 1.0)
    }

    pub fn throttle_f(self) -> f32 {
        self.throttle as f32 / 255.0
    }

    pub fn brake_f(self) -> f32 {
        self.brake as f32 / 255.0
    }

    pub fn handbrake(self) -> bool {
        self.flags & FLAG_HANDBRAKE != 0
    }

    pub fn reset(self) -> bool {
        self.flags & FLAG_RESET != 0
    }

    pub fn restart(self) -> bool {
        self.flags & FLAG_RESTART != 0
    }
}

/// Clamp `v` to `[lo, 1]`, scale by `scale` and round to an integer value (NaN → 0).
/// `as` casts from the result saturate, so the caller's integer conversion is exact.
fn quantize(v: f32, lo: f32, scale: f32) -> f32 {
    if v.is_nan() {
        return 0.0;
    }
    (v.clamp(lo, 1.0) * scale).round()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_axes_quantizes_with_deadband() {
        let f = InputFrame::from_axes(1.0, 0.5, 2.0, FLAG_HANDBRAKE);
        assert_eq!(f, InputFrame { steer: 127, throttle: 128, brake: 255, flags: FLAG_HANDBRAKE });
        assert_eq!(InputFrame::from_axes(-1.5, -1.0, f32::NAN, 0), InputFrame { steer: -127, ..Default::default() });
        // 3/127 rounds to 3, inside the deadband; 4/127 survives.
        assert_eq!(InputFrame::from_axes(3.0 / 127.0, 0.0, 0.0, 0).steer, 0);
        assert_eq!(InputFrame::from_axes(-3.0 / 127.0, 0.0, 0.0, 0).steer, 0);
        assert_eq!(InputFrame::from_axes(4.0 / 127.0, 0.0, 0.0, 0).steer, 4);
        assert_eq!(InputFrame::from_axes(f32::NAN, 0.0, 0.0, 0).steer, 0);
    }

    #[test]
    fn held_keeps_axes_and_handbrake_but_drops_events() {
        let f = InputFrame { steer: -5, throttle: 9, brake: 1, flags: FLAG_HANDBRAKE | FLAG_RESET | FLAG_RESTART };
        assert_eq!(f.held(), InputFrame { flags: FLAG_HANDBRAKE, ..f });
    }

    #[test]
    fn pack_roundtrip() {
        let f = InputFrame { steer: -127, throttle: 200, brake: 3, flags: FLAG_HANDBRAKE };
        assert_eq!(InputFrame::unpack(f.pack()), f);
    }
}
