//! Modelo de elevación (Copernicus GLO-30 en GeoTIFF).
//!
//! Ojo: GLO-30 es un modelo de *superficie* (DSM): mide lo que hay encima del terreno,
//! incluidos edificios y árboles, y suaviza los acantilados en 3 o 4 píxeles de 30 m.
//! Por eso la altura nunca se usa cruda: `elevation` la filtra a lo largo del eje.

use crate::geo::GeoPoint;
use crate::hash::Fnv64;
use tiff::decoder::{Decoder, DecodingResult};
use tiff::tags::Tag;

const TAG_PIXEL_SCALE: u16 = 33550;
const TAG_TIEPOINT: u16 = 33922;
const TAG_GEOKEYS: u16 = 34735;
const KEY_RASTER_TYPE: u16 = 1025;
const RASTER_PIXEL_IS_POINT: u16 = 2;

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
    pub fn from_path(path: &str) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("no se pudo leer {path}: {e}"))?;
        Self::from_geotiff(&bytes)
    }

    pub fn from_geotiff(bytes: &[u8]) -> Result<Self, String> {
        let err = |e: tiff::TiffError| format!("GeoTIFF inválido: {e}");
        let mut d = Decoder::new(std::io::Cursor::new(bytes)).map_err(err)?;
        let (w, h) = d.dimensions().map_err(err)?;
        let tie = d.get_tag_f64_vec(Tag::Unknown(TAG_TIEPOINT)).map_err(|_| "al GeoTIFF le falta ModelTiepoint".to_string())?;
        let scale = d.get_tag_f64_vec(Tag::Unknown(TAG_PIXEL_SCALE)).map_err(|_| "al GeoTIFF le falta ModelPixelScale".to_string())?;
        if tie.len() < 6 || scale.len() < 2 || tie[0] != 0.0 || tie[1] != 0.0 {
            return Err("solo se admiten GeoTIFF con un tiepoint en el píxel (0, 0)".into());
        }
        // El tiepoint marca la esquina del píxel (PixelIsArea, lo habitual) o su centro (PixelIsPoint).
        let is_point = d
            .get_tag_u16_vec(Tag::Unknown(TAG_GEOKEYS))
            .ok()
            .and_then(|keys| keys.chunks_exact(4).skip(1).find(|k| k[0] == KEY_RASTER_TYPE).map(|k| k[3]))
            == Some(RASTER_PIXEL_IS_POINT);
        let half = if is_point { 0.0 } else { 0.5 };
        let data = match d.read_image().map_err(err)? {
            DecodingResult::F32(v) => v,
            DecodingResult::I16(v) => v.into_iter().map(|x| x as f32).collect(),
            _ => return Err("el GeoTIFF debe ser de una banda en f32 o i16".into()),
        };
        if data.len() != (w as usize) * (h as usize) {
            return Err("el GeoTIFF tiene más de una banda".into());
        }
        Ok(Self {
            width: w as usize,
            height: h as usize,
            lon0: tie[3] + half * scale[0],
            lat0: tie[4] - half * scale[1],
            dlon: scale[0],
            dlat: scale[1],
            data,
            hash: Fnv64::default().bytes(bytes).hex(),
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
        if !(0.0..=(self.width - 1) as f64).contains(&fc) || !(0.0..=(self.height - 1) as f64).contains(&fr) {
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
