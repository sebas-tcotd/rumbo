//! Utilidades matemáticas. Toda función trascendente pasa por `libm`
//! para que el resultado sea idéntico en nativo y en WASM.

pub use rapier3d::math::glamx::{Mat3, Quat, Vec3};

pub const DEG: f32 = core::f32::consts::PI / 180.0;

#[inline]
pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}
#[inline]
pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}
#[inline]
pub fn tan(x: f32) -> f32 {
    libm::tanf(x)
}
#[inline]
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}
#[inline]
pub fn asin(x: f32) -> f32 {
    libm::asinf(x.clamp(-1.0, 1.0))
}

#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

#[inline]
pub fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Normaliza sin depender de rutinas internas: sqrt es exacta en IEEE-754.
#[inline]
pub fn norm(v: Vec3) -> Vec3 {
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if l > 1e-9 {
        v / l
    } else {
        Vec3::ZERO
    }
}

/// Cuaternión a partir de una base ortonormal (derecha, arriba, adelante).
/// Convención del motor: X derecha, Y arriba, -Z adelante.
pub fn basis_to_quat(right: Vec3, up: Vec3, forward: Vec3) -> Quat {
    Quat::from_mat3(&Mat3::from_cols(right, up, -forward)).normalize()
}

pub fn is_finite3(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}
