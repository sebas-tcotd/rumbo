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

impl InputFrame {
    pub fn pack(self) -> u32 {
        (self.steer as u8 as u32)
            | (self.throttle as u32) << 8
            | (self.brake as u32) << 16
            | (self.flags as u32) << 24
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_roundtrip() {
        let f = InputFrame { steer: -127, throttle: 200, brake: 3, flags: FLAG_HANDBRAKE };
        assert_eq!(InputFrame::unpack(f.pack()), f);
    }
}
