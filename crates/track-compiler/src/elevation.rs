//! Perfil de elevación a lo largo del eje.
//!
//! El modelo de elevación sirve para la forma general (bajadas, lomas), pero sus datos
//! crudos no son una calzada: incluyen edificios (es un modelo de superficie) y difuminan
//! los acantilados en 100 m o más. Muestrear punto a punto haría que el Circuito de Playas,
//! pegado a la base del acantilado, subiera 20 o 30 m a media pared. Por eso:
//!
//! 1. Se mira a lo ancho de la calzada, no solo en el eje: se toma un percentil bajo de
//!    las alturas entre los bordes. Una calle al pie de un acantilado tiene el terreno más
//!    alto del lado de la pared; su altura real está del lado bajo.
//! 2. Puentes y túneles no se muestrean: su altura se interpola entre los extremos.
//! 3. Mediana móvil larga: descarta picos cortos (edificios, árboles).
//! 4. Límite de pendiente: ninguna calzada sube más de `max_grade`.
//! 5. Suavizado gaussiano: sin quiebres verticales.
//! 6. Anclas opcionales: alturas conocidas por quien crea la pista corrigen el perfil.
//!
//! Hay datos que ningún filtro arregla: el modelo se tomó por radar entre 2011 y 2015, y
//! una vía construida después sobre rellenos aparece como terreno alto o como mar. Por eso
//! se avisa, localizado, donde el terreno varía mucho a lo ancho de la vía (pared al lado)
//! o donde el perfil se aparta mucho de los datos: allí la altura es una estimación.

use crate::centerline::CenterSample;
use crate::dem::Dem;
use crate::diagnostics::{Diagnostic, Severity};
use crate::geo::LocalFrame;

/// Altura conocida en un punto (p. ej. "la vía está a 8 m aquí"), en metros.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub at: crate::geo::GeoPoint,
    pub height_m: f64,
}

#[derive(Clone, Debug)]
pub struct ElevationOptions {
    /// Margen más allá del borde de la calzada que se mira a cada lado (m).
    pub lateral_margin_m: f64,
    /// Percentil de las alturas a lo ancho que se toma como altura de la vía (0-100).
    pub lateral_percentile: f64,
    /// Variación a lo ancho (dentro de ±`wall_probe_m`) a partir de la cual se avisa…
    pub wall_spread_m: f64,
    pub wall_probe_m: f64,
    /// …si se mantiene al menos esta distancia.
    pub wall_length_m: f64,
    pub anchors: Vec<Anchor>,
    /// Alcance de cada ancla (m): su corrección se desvanece a esta distancia. Entre dos anclas
    /// más cercanas que el doble del alcance, la vía queda en línea recta de una a otra.
    pub anchor_reach_m: f64,
    pub median_window_m: f64,
    /// Pendiente máxima (0,14 = 14 %). Las bajadas de la Costa Verde rondan el 10-12 %.
    pub max_grade: f64,
    pub smooth_sigma_m: f64,
    /// Desviación entre crudo y filtrado a partir de la cual se avisa (m)…
    pub warn_deviation_m: f64,
    /// …si se mantiene al menos esta distancia (m).
    pub warn_length_m: f64,
}

impl Default for ElevationOptions {
    fn default() -> Self {
        Self {
            lateral_margin_m: 6.0,
            lateral_percentile: 20.0,
            wall_spread_m: 15.0,
            wall_probe_m: 30.0,
            wall_length_m: 100.0,
            anchors: Vec::new(),
            anchor_reach_m: 400.0,
            median_window_m: 120.0,
            max_grade: 0.14,
            smooth_sigma_m: 12.0,
            warn_deviation_m: 6.0,
            warn_length_m: 30.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Profile {
    /// Altura del modelo en el eje de cada muestra (`None` en puentes, túneles o sin cobertura).
    pub center: Vec<Option<f64>>,
    /// Altura tomada a lo ancho (percentil bajo): la entrada del filtro.
    pub raw: Vec<Option<f64>>,
    /// Variación de altura a lo ancho de la vía (m): alta junto a una pared o acantilado.
    pub spread: Vec<f64>,
    pub filtered: Vec<f64>,
    pub diagnostics: Vec<Diagnostic>,
    pub climb_m: f64,
    pub max_grade: f64,
}

/// Rellena huecos por interpolación lineal; los extremos copian el valor más cercano.
fn fill(v: &[Option<f64>]) -> Option<Vec<f64>> {
    let known: Vec<usize> = (0..v.len()).filter(|&i| v[i].is_some()).collect();
    if known.is_empty() {
        return None;
    }
    let mut out = vec![0.0; v.len()];
    for (i, o) in out.iter_mut().enumerate() {
        *o = match v[i] {
            Some(h) => h,
            None => {
                let next = known.partition_point(|&k| k < i);
                match (next.checked_sub(1).map(|k| known[k]), known.get(next).copied()) {
                    (Some(a), Some(b)) => {
                        let t = (i - a) as f64 / (b - a) as f64;
                        v[a].unwrap() * (1.0 - t) + v[b].unwrap() * t
                    }
                    (Some(a), None) => v[a].unwrap(),
                    (None, Some(b)) => v[b].unwrap(),
                    (None, None) => unreachable!(),
                }
            }
        };
    }
    Some(out)
}

fn median(window: &[f64]) -> f64 {
    let mut w = window.to_vec();
    w.sort_by(|a, b| a.total_cmp(b));
    let n = w.len();
    if n % 2 == 1 { w[n / 2] } else { (w[n / 2 - 1] + w[n / 2]) * 0.5 }
}

/// Limita la pendiente: pasada hacia adelante y hacia atrás, y el promedio de ambas
/// (el promedio de dos perfiles con pendiente acotada también la respeta).
fn limit_grade(h: &[f64], ds: f64, grade: f64) -> Vec<f64> {
    let n = h.len();
    let step = grade * ds;
    let mut f = h.to_vec();
    for i in 1..n {
        f[i] = f[i].clamp(f[i - 1] - step, f[i - 1] + step);
    }
    let mut b = h.to_vec();
    for i in (0..n - 1).rev() {
        b[i] = b[i].clamp(b[i + 1] - step, b[i + 1] + step);
    }
    (0..n).map(|i| (f[i] + b[i]) * 0.5).collect()
}

fn smooth(h: &[f64], ds: f64, sigma: f64) -> Vec<f64> {
    let n = h.len();
    let r = ((3.0 * sigma) / ds).ceil() as isize;
    (0..n as isize)
        .map(|i| {
            let reach = r.min(i).min(n as isize - 1 - i);
            let (mut acc, mut wsum) = (0.0, 0.0);
            for k in -reach..=reach {
                let w = libm::exp(-0.5 * ((k as f64 * ds) / sigma).powi(2));
                acc += h[(i + k) as usize] * w;
                wsum += w;
            }
            acc / wsum
        })
        .collect()
}

/// Filtro completo sobre alturas crudas (útil también para probarlo sin un modelo real).
pub fn filter(raw: &[Option<f64>], ds: f64, opts: &ElevationOptions) -> Option<Vec<f64>> {
    let filled = fill(raw)?;
    let half = ((opts.median_window_m * 0.5) / ds).round() as usize;
    let n = filled.len();
    let med: Vec<f64> = (0..n).map(|i| median(&filled[i.saturating_sub(half)..=(i + half).min(n - 1)])).collect();
    let graded = limit_grade(&med, ds, opts.max_grade);
    // El suavizado puede reintroducir algo de pendiente en los cambios bruscos; una última
    // pasada de límite la deja dentro del máximo.
    Some(limit_grade(&smooth(&graded, ds, opts.smooth_sigma_m), ds, opts.max_grade))
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    let k = ((p / 100.0) * (v.len() - 1) as f64).round() as usize;
    v[k.min(v.len() - 1)]
}

/// Muestra `n` alturas a lo ancho, entre `-reach` y `reach` metros del eje.
fn across(dem: &Dem, frame: &LocalFrame, en: [f64; 2], right: [f64; 2], reach: f64, step: f64) -> Vec<f64> {
    let n = (reach / step).round() as i32;
    (-n..=n)
        .filter_map(|k| {
            let d = k as f64 * step;
            dem.height(frame.en_to_geo([en[0] + right[0] * d, en[1] + right[1] * d]))
        })
        .collect()
}

pub fn profile(samples: &[CenterSample], dem: &Dem, frame: &LocalFrame, opts: &ElevationOptions) -> Result<Profile, String> {
    let mut diags = Vec::new();
    let mut outside = 0usize;
    let n = samples.len();
    let mut center = Vec::with_capacity(n);
    let mut raw = Vec::with_capacity(n);
    let mut spread = Vec::with_capacity(n);
    for (i, c) in samples.iter().enumerate() {
        let (a, b) = (samples[i.saturating_sub(1)].en, samples[(i + 1).min(n - 1)].en);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = libm::hypot(dx, dy).max(1e-9);
        let right = [dy / l, -dx / l];
        let probe = across(dem, frame, c.en, right, opts.wall_probe_m, 3.0);
        spread.push(probe.iter().copied().fold(f64::NEG_INFINITY, f64::max) - probe.iter().copied().fold(f64::INFINITY, f64::min));
        if c.bridge || c.tunnel {
            center.push(None);
            raw.push(None);
            continue;
        }
        let h = dem.height(frame.en_to_geo(c.en));
        if h.is_none() {
            outside += 1;
        }
        center.push(h);
        let mut road = across(dem, frame, c.en, right, c.width * 0.5 + opts.lateral_margin_m, 2.0);
        raw.push(if road.is_empty() { h } else { Some(percentile(&mut road, opts.lateral_percentile)) });
    }
    if outside > 0 {
        diags.push(Diagnostic::new(
            Severity::Warning,
            "elevation-no-coverage",
            format!("{outside} muestras caen fuera del modelo de elevación; su altura se interpoló"),
        ));
    }
    let structures = samples.iter().filter(|c| c.bridge || c.tunnel).count();
    if structures > 0 {
        diags.push(Diagnostic::new(
            Severity::Info,
            "elevation-structures",
            format!("{structures} muestras sobre puentes o en túneles: altura interpolada entre sus extremos"),
        ));
    }
    let ds = if samples.len() > 1 { samples[1].s - samples[0].s } else { 1.0 };
    let mut filtered = filter(&raw, ds, opts).ok_or("ninguna muestra del recorrido tiene elevación")?;

    // Muestras cuya altura fijó un ancla: ahí el aviso de "altura estimada" no aplica.
    let mut anchored = vec![false; n];
    // Anclas: corrección interpolada a lo largo de la pista entre alturas conocidas.
    if !opts.anchors.is_empty() {
        // (muestra, corrección, altura) de cada ancla.
        let mut fixes: Vec<(usize, f64, f64)> = opts
            .anchors
            .iter()
            .map(|a| {
                let q = frame.to_en(a.at);
                let k = (0..n)
                    .min_by(|&x, &y| {
                        let dx = libm::hypot(samples[x].en[0] - q[0], samples[x].en[1] - q[1]);
                        let dy = libm::hypot(samples[y].en[0] - q[0], samples[y].en[1] - q[1]);
                        dx.total_cmp(&dy)
                    })
                    .unwrap();
                (k, a.height_m - filtered[k], a.height_m)
            })
            .collect();
        fixes.sort_by_key(|f| f.0);
        // La corrección es local: cada ancla arregla su entorno y no mueve el resto de la pista.
        // Dos anclas cercanas fijan el tramo entre ellas: el error del modelo no tiene forma de
        // recta, así que restarle una corrección lineal dejaría hoyos y lomas.
        let reach = (opts.anchor_reach_m / ds).max(1.0);
        let fade = |fix: (usize, f64, f64), i: usize| fix.1 * (1.0 - (fix.0.abs_diff(i) as f64 / reach)).max(0.0);
        let mut corr = vec![0.0; n];
        for (i, c) in corr.iter_mut().enumerate() {
            let next = fixes.partition_point(|f| f.0 < i);
            *c = match (next.checked_sub(1).map(|k| fixes[k]), fixes.get(next).copied()) {
                (Some(a), Some(b)) if b.0 > a.0 && (b.0 - a.0) as f64 <= 2.0 * reach => {
                    a.2 + (b.2 - a.2) * (i - a.0) as f64 / (b.0 - a.0) as f64 - filtered[i]
                }
                (a, b) => a.map_or(0.0, |a| fade(a, i)) + b.map_or(0.0, |b| fade(b, i)),
            };
        }
        // Suavizada como el perfil, para que el cambio de pendiente en cada ancla no sea un quiebre.
        for (i, c) in smooth(&corr, ds, opts.smooth_sigma_m).into_iter().enumerate() {
            filtered[i] += c;
            anchored[i] = corr[i] != 0.0;
        }
        filtered = limit_grade(&filtered, ds, opts.max_grade);
        diags.push(Diagnostic::new(
            Severity::Info,
            "elevation-anchored",
            format!("{} anclas de elevación aplicadas", fixes.len()),
        ));
    }

    // Pared o acantilado al costado: el modelo mezcla alturas a lo ancho de la vía.
    let wall_run = (opts.wall_length_m / ds).ceil() as usize;
    let mut i = 0;
    while i < n {
        if spread[i] <= opts.wall_spread_m {
            i += 1;
            continue;
        }
        let start = i;
        let mut worst: f64 = 0.0;
        while i < n && spread[i] > opts.wall_spread_m {
            worst = worst.max(spread[i]);
            i += 1;
        }
        if i - start >= wall_run {
            diags.push(
                Diagnostic::new(
                    Severity::Warning,
                    "elevation-wall",
                    format!(
                        "entre los metros {:.0} y {:.0} el terreno varía hasta {:.0} m a lo ancho de la vía \
                         (pared o acantilado al costado): la altura allí es poco confiable; \
                         si la conoces, fíjala con un ancla",
                        samples[start].s,
                        samples[i - 1].s,
                        worst
                    ),
                )
                .at(frame.en_to_geo(samples[(start + i) / 2].en)),
            );
        }
    }

    // Tramos donde el filtrado se aparta mucho de los datos: altura estimada, avisar dónde.
    let min_run = (opts.warn_length_m / ds).ceil() as usize;
    let mut i = 0;
    while i < samples.len() {
        let dev = |k: usize| if anchored[k] { 0.0 } else { raw[k].map(|h| (h - filtered[k]).abs()).unwrap_or(0.0) };
        if dev(i) <= opts.warn_deviation_m {
            i += 1;
            continue;
        }
        let start = i;
        let mut worst: f64 = 0.0;
        while i < samples.len() && dev(i) > opts.warn_deviation_m {
            worst = worst.max(dev(i));
            i += 1;
        }
        if i - start >= min_run {
            let mid = (start + i) / 2;
            diags.push(
                Diagnostic::new(
                    Severity::Warning,
                    "elevation-ambiguous",
                    format!(
                        "entre los metros {:.0} y {:.0} el modelo se aparta hasta {:.0} m del perfil de la calzada \
                         (acantilado, edificios o árboles): la altura allí es una estimación",
                        samples[start].s,
                        samples[i - 1].s,
                        worst
                    ),
                )
                .at(frame.en_to_geo(samples[mid].en)),
            );
        }
    }

    let climb_m = filtered.windows(2).map(|w| (w[1] - w[0]).max(0.0)).sum();
    let max_grade = filtered.windows(2).map(|w| ((w[1] - w[0]) / ds).abs()).fold(0.0, f64::max);
    Ok(Profile { center, raw, spread, filtered, diagnostics: diags, climb_m, max_grade })
}
