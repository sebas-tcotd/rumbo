//! S3 · Altura de la red como un campo 2D suavizado.
//!
//! En S2 la altura se filtraba a lo largo de un solo eje. Con cruces, dos calles que se
//! encuentran tienen que dar exactamente la misma altura en el mismo punto, así que la
//! altura pasa a ser una función de la posición: `HeightField::at(este, norte)`.
//!
//! Se arma sobre una grilla: en cada celda, un percentil bajo del modelo dentro de un radio
//! (quita edificios y árboles, que en el modelo de superficie suben la altura), y luego un
//! suavizado gaussiano. Sirve para terreno sin acantilados, como San Borja; la Costa Verde
//! sigue usando el perfil a lo largo del eje de S2.

use crate::ports::ElevationSource;
use crate::geo::LocalFrame;

#[derive(Clone, Copy, Debug)]
pub struct TerrainOptions {
    /// Paso de la grilla (m).
    pub step_m: f64,
    /// Radio en el que se toma el percentil bajo (m).
    pub radius_m: f64,
    /// Percentil (0-100). Bajo, para quedarse con el nivel de la calle.
    pub percentile: f64,
    /// Suavizado gaussiano final (m).
    pub sigma_m: f64,
}

impl Default for TerrainOptions {
    fn default() -> Self {
        Self { step_m: 4.0, radius_m: 24.0, percentile: 20.0, sigma_m: 16.0 }
    }
}

#[derive(Clone, Debug)]
pub struct HeightField {
    e0: f64,
    n0: f64,
    step: f64,
    w: usize,
    h: usize,
    data: Vec<f64>,
}

impl HeightField {
    /// Campo que cubre la caja `[min, max]` (este, norte) con margen.
    pub fn build(dem: &dyn ElevationSource, frame: &LocalFrame, min: [f64; 2], max: [f64; 2], opts: &TerrainOptions) -> Result<Self, String> {
        let margin = opts.radius_m + 3.0 * opts.sigma_m + opts.step_m;
        let (e0, n0) = (min[0] - margin, min[1] - margin);
        let w = ((max[0] - min[0] + 2.0 * margin) / opts.step_m).ceil() as usize + 1;
        let h = ((max[1] - min[1] + 2.0 * margin) / opts.step_m).ceil() as usize + 1;
        let mut raw = Vec::with_capacity(w * h);
        for j in 0..h {
            for i in 0..w {
                let p = [e0 + i as f64 * opts.step_m, n0 + j as f64 * opts.step_m];
                raw.push(dem.height(frame.en_to_geo(p)));
            }
        }
        let missing = raw.iter().filter(|v| v.is_none()).count();
        if missing * 2 > raw.len() {
            return Err("el modelo de elevación no cubre la zona de la pista".into());
        }

        // Percentil bajo dentro de un disco.
        let r = (opts.radius_m / opts.step_m).round() as isize;
        let offsets: Vec<(isize, isize)> =
            (-r..=r).flat_map(|dj| (-r..=r).map(move |di| (di, dj))).filter(|(di, dj)| di * di + dj * dj <= r * r).collect();
        let mut low = vec![0.0; w * h];
        let mut buf = Vec::with_capacity(offsets.len());
        for j in 0..h as isize {
            for i in 0..w as isize {
                buf.clear();
                for (di, dj) in &offsets {
                    let (x, y) = (i + di, j + dj);
                    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                        if let Some(v) = raw[y as usize * w + x as usize] {
                            buf.push(v);
                        }
                    }
                }
                low[j as usize * w + i as usize] = if buf.is_empty() {
                    f64::NAN
                } else {
                    buf.sort_by(|a, b| a.total_cmp(b));
                    buf[((opts.percentile / 100.0) * (buf.len() - 1) as f64).round() as usize]
                };
            }
        }
        // Celdas sin dato (fuera del modelo): el promedio de las que sí tienen.
        let known: Vec<f64> = low.iter().copied().filter(|v| v.is_finite()).collect();
        let mean = known.iter().sum::<f64>() / known.len().max(1) as f64;
        for v in low.iter_mut().filter(|v| !v.is_finite()) {
            *v = mean;
        }

        // Gaussiano separable, con los bordes recortados.
        let k = ((3.0 * opts.sigma_m) / opts.step_m).ceil() as isize;
        let weights: Vec<f64> = (-k..=k).map(|d| libm::exp(-0.5 * (d as f64 * opts.step_m / opts.sigma_m).powi(2))).collect();
        let pass = |src: &[f64], horizontal: bool| -> Vec<f64> {
            let mut out = vec![0.0; w * h];
            for j in 0..h as isize {
                for i in 0..w as isize {
                    let (mut acc, mut ws) = (0.0, 0.0);
                    for d in -k..=k {
                        let (x, y) = if horizontal { (i + d, j) } else { (i, j + d) };
                        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                            let wt = weights[(d + k) as usize];
                            acc += src[y as usize * w + x as usize] * wt;
                            ws += wt;
                        }
                    }
                    out[j as usize * w + i as usize] = acc / ws;
                }
            }
            out
        };
        let data = pass(&pass(&low, true), false);
        Ok(Self { e0, n0, step: opts.step_m, w, h, data })
    }

    /// Altura (m) en un punto del plano local, interpolada bilinealmente.
    pub fn at(&self, p: [f64; 2]) -> f64 {
        let fx = ((p[0] - self.e0) / self.step).clamp(0.0, (self.w - 1) as f64);
        let fy = ((p[1] - self.n0) / self.step).clamp(0.0, (self.h - 1) as f64);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let v = |x: usize, y: usize| self.data[y * self.w + x];
        let top = v(x0, y0) * (1.0 - tx) + v(x1, y0) * tx;
        let bottom = v(x0, y1) * (1.0 - tx) + v(x1, y1) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}
