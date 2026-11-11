//! Proyección WGS84 → marco local ENU (este, norte, arriba) en metros.
//!
//! Se pasa por ECEF y se rota al plano tangente en el origen de la región.
//! Para una zona de pocos kilómetros la distorsión es despreciable.
//! Toda la trigonometría usa `libm` para que el resultado sea igual en cualquier plataforma.

use serde::{Deserialize, Serialize};

const A: f64 = 6_378_137.0; // semieje mayor WGS84
const F: f64 = 1.0 / 298.257_223_563; // achatamiento WGS84
const E2: f64 = F * (2.0 - F);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

impl GeoPoint {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// Acepta "lat,lon" y rechaza valores fuera de rango.
    /// Ojo: el rango no basta para detectar un orden invertido (la longitud de Lima,
    /// −77, cabe en el rango de latitud). Esa detección se hace al ajustar el punto a
    /// la red vial, donde se conoce la región (ver `routing::snap`).
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut it = s.split(',').map(|p| p.trim().parse::<f64>());
        match (it.next(), it.next(), it.next()) {
            (Some(Ok(lat)), Some(Ok(lon)), None) => {
                if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                    let hint = if (-90.0..=90.0).contains(&lon) { " (¿está invertido el orden?)" } else { "" };
                    return Err(format!("\"{s}\" no es lat,lon válido{hint}"));
                }
                Ok(Self { lat, lon })
            }
            _ => Err(format!("\"{s}\" no tiene el formato lat,lon")),
        }
    }
}

fn rad(d: f64) -> f64 {
    d * core::f64::consts::PI / 180.0
}
fn deg(r: f64) -> f64 {
    r * 180.0 / core::f64::consts::PI
}

fn ecef(p: GeoPoint, h: f64) -> [f64; 3] {
    let (phi, lam) = (rad(p.lat), rad(p.lon));
    let (sp, cp) = (libm::sin(phi), libm::cos(phi));
    let (sl, cl) = (libm::sin(lam), libm::cos(lam));
    let n = A / libm::sqrt(1.0 - E2 * sp * sp);
    [(n + h) * cp * cl, (n + h) * cp * sl, (n * (1.0 - E2) + h) * sp]
}

/// Marco local de una región: plano tangente en `origin`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocalFrame {
    pub origin: GeoPoint,
}

impl LocalFrame {
    pub fn new(origin: GeoPoint) -> Self {
        Self { origin }
    }

    fn basis(&self) -> ([f64; 3], [f64; 3], [f64; 3]) {
        let (phi, lam) = (rad(self.origin.lat), rad(self.origin.lon));
        let (sp, cp) = (libm::sin(phi), libm::cos(phi));
        let (sl, cl) = (libm::sin(lam), libm::cos(lam));
        let east = [-sl, cl, 0.0];
        let north = [-sp * cl, -sp * sl, cp];
        let up = [cp * cl, cp * sl, sp];
        (east, north, up)
    }

    /// Geográfico → ENU en metros (altura elipsoidal 0).
    pub fn to_enu(&self, p: GeoPoint) -> [f64; 3] {
        let o = ecef(self.origin, 0.0);
        let q = ecef(p, 0.0);
        let d = [q[0] - o[0], q[1] - o[1], q[2] - o[2]];
        let (e, n, u) = self.basis();
        let dot = |a: [f64; 3]| a[0] * d[0] + a[1] * d[1] + a[2] * d[2];
        [dot(e), dot(n), dot(u)]
    }

    /// Solo el plano: [este, norte].
    pub fn to_en(&self, p: GeoPoint) -> [f64; 2] {
        let v = self.to_enu(p);
        [v[0], v[1]]
    }

    /// ENU → geográfico (iteración de Bowring sobre ECEF). Se usa para verificar y exportar.
    pub fn to_geo(&self, enu: [f64; 3]) -> GeoPoint {
        let o = ecef(self.origin, 0.0);
        let (e, n, u) = self.basis();
        let x = o[0] + e[0] * enu[0] + n[0] * enu[1] + u[0] * enu[2];
        let y = o[1] + e[1] * enu[0] + n[1] * enu[1] + u[1] * enu[2];
        let z = o[2] + e[2] * enu[0] + n[2] * enu[1] + u[2] * enu[2];
        let lon = libm::atan2(y, x);
        let pxy = libm::sqrt(x * x + y * y);
        let mut lat = libm::atan2(z, pxy * (1.0 - E2));
        for _ in 0..6 {
            let s = libm::sin(lat);
            let nn = A / libm::sqrt(1.0 - E2 * s * s);
            let h = pxy / libm::cos(lat) - nn;
            lat = libm::atan2(z, pxy * (1.0 - E2 * nn / (nn + h)));
        }
        GeoPoint { lat: deg(lat), lon: deg(lon) }
    }

    /// Plano → geográfico, compensando la caída por curvatura para volver a altura 0.
    pub fn en_to_geo(&self, en: [f64; 2]) -> GeoPoint {
        let first = self.to_geo([en[0], en[1], 0.0]);
        let u = self.to_enu(first)[2];
        self.to_geo([en[0], en[1], u])
    }
}

/// Radios de curvatura del elipsoide en la latitud `lat`: (meridiano M, primer vertical N).
/// Metros por radián hacia el norte = M; hacia el este = N·cos(lat).
pub fn curvature_radii(lat: f64) -> (f64, f64) {
    let s = libm::sin(rad(lat));
    let w = 1.0 - E2 * s * s;
    (A * (1.0 - E2) / (w * libm::sqrt(w)), A / libm::sqrt(w))
}

/// Distancia aproximada sobre una esfera media (haversine). Cerca del ecuador sobrestima
/// el arco norte-sur en ~0,5 % frente al elipsoide; sirve solo como verificación gruesa.
pub fn haversine_m(a: GeoPoint, b: GeoPoint) -> f64 {
    let r = 6_371_008.8;
    let (p1, p2) = (rad(a.lat), rad(b.lat));
    let dp = p2 - p1;
    let dl = rad(b.lon - a.lon);
    let (sp, sl) = (libm::sin(dp / 2.0), libm::sin(dl / 2.0));
    let h = sp * sp + libm::cos(p1) * libm::cos(p2) * (sl * sl);
    2.0 * r * libm::asin(libm::sqrt(h))
}
