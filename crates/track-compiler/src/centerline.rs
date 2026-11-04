//! Eje de la pista a partir de un recorrido resuelto: suavizado, re-muestreado y con
//! los atributos de la calle de origen (ancho, puente, túnel) en cada muestra.
//!
//! Las vías de OSM son polilíneas con vértices a veces muy angulosos; un auto no puede
//! seguir un quiebre de 30° en un punto. Se suaviza con un núcleo gaussiano a lo largo del
//! recorrido, que redondea los quiebres sin desplazar el trazado más de unos metros.

use crate::graph::RoadGraph;
use crate::routing::ResolvedRoute;

#[derive(Clone, Debug)]
pub struct CenterSample {
    /// Posición en el plano local (este, norte), en metros.
    pub en: [f64; 2],
    /// Distancia a lo largo del eje, en metros.
    pub s: f64,
    /// Índice de la arista del grafo de la que sale la muestra.
    pub edge: usize,
    /// Ancho de la calzada para la pista, ya acotado y suavizado.
    pub width: f64,
    pub bridge: bool,
    pub tunnel: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct CenterlineOptions {
    pub spacing_m: f64,
    /// Desviación del núcleo gaussiano (m). Más grande = curvas más redondas.
    pub smooth_sigma_m: f64,
    pub min_width_m: f64,
    pub max_width_m: f64,
    /// Ventana para suavizar los cambios de ancho entre calles (m).
    pub width_window_m: f64,
}

impl Default for CenterlineOptions {
    fn default() -> Self {
        Self { spacing_m: 2.0, smooth_sigma_m: 4.0, min_width_m: 7.0, max_width_m: 16.0, width_window_m: 30.0 }
    }
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    libm::hypot(b[0] - a[0], b[1] - a[1])
}

/// Re-muestrea una polilínea a paso constante (el último tramo puede quedar más corto).
pub fn resample(pts: &[[f64; 2]], step: f64) -> Vec<[f64; 2]> {
    let mut out = vec![pts[0]];
    let mut carry = 0.0;
    for w in pts.windows(2) {
        let seg = dist(w[0], w[1]);
        if seg < 1e-9 {
            continue;
        }
        let mut t = step - carry;
        while t <= seg {
            let k = t / seg;
            out.push([w[0][0] + (w[1][0] - w[0][0]) * k, w[0][1] + (w[1][1] - w[0][1]) * k]);
            t += step;
        }
        carry = seg - (t - step);
    }
    let last = *pts.last().unwrap();
    if dist(*out.last().unwrap(), last) > step * 0.25 {
        out.push(last);
    }
    out
}

/// Suavizado gaussiano a paso constante. Los extremos se mantienen fijos y la ventana se
/// recorta cerca de ellos, para no acortar el recorrido.
fn gaussian(pts: &[[f64; 2]], step: f64, sigma: f64, closed: bool) -> Vec<[f64; 2]> {
    let n = pts.len();
    let r = ((3.0 * sigma) / step).ceil() as isize;
    let weights: Vec<f64> = (-r..=r).map(|k| libm::exp(-0.5 * ((k as f64 * step) / sigma).powi(2))).collect();
    (0..n)
        .map(|i| {
            if !closed && (i == 0 || i == n - 1) {
                return pts[i];
            }
            let (mut x, mut y, mut wsum) = (0.0, 0.0, 0.0);
            let reach = if closed { r } else { r.min(i as isize).min((n - 1 - i) as isize) };
            for k in -reach..=reach {
                let j = if closed { (i as isize + k).rem_euclid(n as isize) } else { i as isize + k } as usize;
                let w = weights[(k + r) as usize];
                x += pts[j][0] * w;
                y += pts[j][1] * w;
                wsum += w;
            }
            [x / wsum, y / wsum]
        })
        .collect()
}

/// Proyección de `p` sobre la polilínea `en`: distancia al cuadrado.
fn dist2_to(en: &[[f64; 2]], p: [f64; 2]) -> f64 {
    let mut best = f64::INFINITY;
    for w in en.windows(2) {
        let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        let l2 = dx * dx + dy * dy;
        let t = if l2 < 1e-12 { 0.0 } else { (((p[0] - w[0][0]) * dx + (p[1] - w[0][1]) * dy) / l2).clamp(0.0, 1.0) };
        let (qx, qy) = (w[0][0] + dx * t - p[0], w[0][1] + dy * t - p[1]);
        best = best.min(qx * qx + qy * qy);
    }
    best
}

pub fn build(g: &RoadGraph, r: &ResolvedRoute, opts: &CenterlineOptions) -> Result<Vec<CenterSample>, String> {
    if r.en.len() < 2 || r.length_m < 50.0 {
        return Err(format!("el recorrido mide {:.0} m: hace falta al menos 50 m", r.length_m));
    }
    // Aristas recorridas, en orden, para heredar sus atributos.
    let index: std::collections::BTreeMap<&str, usize> = g.edges.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let route_edges: Vec<usize> =
        r.legs.iter().flat_map(|l| l.steps.iter()).filter_map(|s| index.get(&s.0[1..]).copied()).collect();

    // 1. Paso de 1 m, suavizado y nuevo paso final.
    let fine = resample(&r.en, 1.0);
    let smooth = gaussian(&fine, 1.0, opts.smooth_sigma_m, r.closed);
    let pts = resample(&smooth, opts.spacing_m);

    // 2. Atributos por muestra: la arista del recorrido más cercana. La búsqueda avanza por
    //    el recorrido (ventana de aristas) para no saltar a una calle que pasa cerca más tarde.
    let mut samples = Vec::with_capacity(pts.len());
    let mut s = 0.0;
    let mut cursor = 0usize;
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            s += dist(pts[i - 1], *p);
        }
        let lo = cursor.saturating_sub(1);
        let hi = (cursor + 4).min(route_edges.len());
        let (mut best, mut best_k) = (f64::INFINITY, cursor);
        for (k, &ei) in route_edges.iter().enumerate().take(hi).skip(lo) {
            let d = dist2_to(&g.edges[ei].en, *p);
            if d < best {
                best = d;
                best_k = k;
            }
        }
        cursor = best_k.max(cursor);
        let e = &g.edges[route_edges[best_k]];
        samples.push(CenterSample {
            en: *p,
            s,
            edge: route_edges[best_k],
            width: e.width_m.clamp(opts.min_width_m, opts.max_width_m),
            bridge: e.bridge,
            tunnel: e.tunnel,
        });
    }

    // 3. Ancho suavizado: sin escalones al pasar de una calle a otra.
    let half = ((opts.width_window_m * 0.5) / opts.spacing_m).round() as usize;
    let raw: Vec<f64> = samples.iter().map(|c| c.width).collect();
    for (i, c) in samples.iter_mut().enumerate() {
        let (a, b) = (i.saturating_sub(half), (i + half).min(raw.len() - 1));
        c.width = raw[a..=b].iter().sum::<f64>() / (b - a + 1) as f64;
    }
    Ok(samples)
}

/// Ángulo máximo entre segmentos consecutivos del eje, en grados por metro.
/// Sirve como diagnóstico: un valor alto delata un quiebre que el suavizado no absorbió.
pub fn max_turn_rate_deg_per_m(samples: &[CenterSample]) -> f64 {
    let mut worst: f64 = 0.0;
    for w in samples.windows(3) {
        let a = [w[1].en[0] - w[0].en[0], w[1].en[1] - w[0].en[1]];
        let b = [w[2].en[0] - w[1].en[0], w[2].en[1] - w[1].en[1]];
        let ang = crate::routing::turn_angle_deg(a, b);
        let len = (dist(w[0].en, w[1].en) + dist(w[1].en, w[2].en)) * 0.5;
        if len > 1e-6 {
            worst = worst.max(ang / len);
        }
    }
    worst
}
