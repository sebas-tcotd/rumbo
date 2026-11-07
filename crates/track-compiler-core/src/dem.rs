//! Modelo de elevación (Copernicus GLO-30 en GeoTIFF).
//!
//! Ojo: GLO-30 es un modelo de *superficie* (DSM): mide lo que hay encima del terreno,
//! incluidos edificios y árboles, y suaviza los acantilados en 3 o 4 píxeles de 30 m.
//! Por eso la altura nunca se usa cruda: `elevation` la filtra a lo largo del eje.

use crate::geo::GeoPoint;
#[derive(Clone, Debug)]
pub struct Dem {
    pub width: usize,
    pub height: usize,
    /// Longitud y latitud del centro del píxel (0, 0).
    lon0: f64,
    lat0: f64,
    dlon: f64,
    dlat: f64,
    data: Vec<f32>,
    /// Hash del archivo, para registrar de dónde salió una pista.
    pub hash: String,
}

impl Dem {
    /// Constructs a raster from pixel-center coordinates and positive angular spacing.
    /// # Errors
    /// Returns an error for invalid dimensions, spacing, or origin.
    pub fn new(
        [width, height]: [usize; 2],
        [lon0, lat0]: [f64; 2],
        [dlon, dlat]: [f64; 2],
        data: Vec<f32>,
        hash: String,
    ) -> Result<Self, String> {
        if width == 0
            || height == 0
            || width.checked_mul(height) != Some(data.len())
            || !lon0.is_finite()
            || !lat0.is_finite()
            || !dlon.is_finite()
            || !dlat.is_finite()
            || dlon <= 0.0
            || dlat <= 0.0
        {
            return Err("invalid elevation raster".into());
        }
        Ok(Self {
            width,
            height,
            lon0,
            lat0,
            dlon,
            dlat,
            data,
            hash,
        })
    }
    fn at(&self, c: usize, r: usize) -> f64 {
        self.data[r * self.width + c] as f64
    }

    /// Altura (m) interpolada bilinealmente entre los cuatro píxeles vecinos.
    /// `None` fuera de la cobertura del archivo.
    pub fn height(&self, p: GeoPoint) -> Option<f64> {
        let fc = (p.lon - self.lon0) / self.dlon;
        let fr = (self.lat0 - p.lat) / self.dlat;
        if !(0.0..=(self.width - 1) as f64).contains(&fc)
            || !(0.0..=(self.height - 1) as f64).contains(&fr)
        {
            return None;
        }
        let (c0, r0) = (fc.floor() as usize, fr.floor() as usize);
        let (c1, r1) = ((c0 + 1).min(self.width - 1), (r0 + 1).min(self.height - 1));
        let (tx, ty) = (fc - c0 as f64, fr - r0 as f64);
        let top = self.at(c0, r0) * (1.0 - tx) + self.at(c1, r0) * tx;
        let bottom = self.at(c0, r1) * (1.0 - tx) + self.at(c1, r1) * tx;
        Some(top * (1.0 - ty) + bottom * ty)
    }
}

impl crate::ports::ElevationSource for Dem {
    fn height(&self, point: GeoPoint) -> Option<f64> {
        self.height(point)
    }
    fn identity(&self) -> &str {
        &self.hash
    }
}
