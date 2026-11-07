use tiff::decoder::{Decoder, DecodingResult};
use tiff::tags::Tag;
use track_compiler_core::{dem::Dem, hash::Fnv64};

const TAG_PIXEL_SCALE: u16 = 33550;
const TAG_TIEPOINT: u16 = 33922;
const TAG_GEOKEYS: u16 = 34735;
const KEY_RASTER_TYPE: u16 = 1025;
const RASTER_PIXEL_IS_POINT: u16 = 2;

/// Loads a georeferenced raster once.
/// # Errors
/// Returns file access and decoding errors.
pub fn from_path(path: &str) -> Result<Dem, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("no se pudo leer {path}: {e}"))?;
    from_geotiff(&bytes)
}

/// Decodes a single-band GeoTIFF and records the identity of its original bytes.
/// # Errors
/// Returns an error for unsupported or malformed rasters.
pub fn from_geotiff(bytes: &[u8]) -> Result<Dem, String> {
    let err = |e: tiff::TiffError| format!("GeoTIFF inválido: {e}");
    let mut d = Decoder::new(std::io::Cursor::new(bytes)).map_err(err)?;
    let (w, h) = d.dimensions().map_err(err)?;
    let tie = d
        .get_tag_f64_vec(Tag::Unknown(TAG_TIEPOINT))
        .map_err(|_| "al GeoTIFF le falta ModelTiepoint".to_string())?;
    let scale = d
        .get_tag_f64_vec(Tag::Unknown(TAG_PIXEL_SCALE))
        .map_err(|_| "al GeoTIFF le falta ModelPixelScale".to_string())?;
    if tie.len() < 6 || scale.len() < 2 || tie[0] != 0.0 || tie[1] != 0.0 {
        return Err("solo se admiten GeoTIFF con un tiepoint en el píxel (0, 0)".into());
    }
    // El tiepoint marca la esquina del píxel (PixelIsArea, lo habitual) o su centro (PixelIsPoint).
    let is_point = d
        .get_tag_u16_vec(Tag::Unknown(TAG_GEOKEYS))
        .ok()
        .and_then(|keys| {
            keys.as_chunks::<4>()
                .0
                .iter()
                .skip(1)
                .find(|k| k[0] == KEY_RASTER_TYPE)
                .map(|k| k[3])
        })
        == Some(RASTER_PIXEL_IS_POINT);
    let half = if is_point { 0.0 } else { 0.5 };
    let data = match d.read_image().map_err(err)? {
        DecodingResult::F32(v) => v,
        DecodingResult::I16(v) => v.into_iter().map(|x| x as f32).collect(),
        _ => return Err("el GeoTIFF debe ser de una banda en f32 o i16".into()),
    };
    if (w as usize).checked_mul(h as usize) != Some(data.len()) {
        return Err("el GeoTIFF tiene más de una banda".into());
    }
    Dem::new(
        [w as usize, h as usize],
        [tie[3] + half * scale[0], tie[4] - half * scale[1]],
        [scale[0], scale[1]],
        data,
        Fnv64::default().bytes(bytes).hex(),
    )
}
