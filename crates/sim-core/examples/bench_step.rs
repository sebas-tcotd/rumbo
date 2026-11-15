//! Story 0.9 (D9, NFR3): native release benchmark of `Simulation::step`, reported by CI on every push/PR.
//!
//! Fixed scenario: track `test-loop`, the default profile (the replay carries `LEGACY_VEHICLE`), and the
//! seed and recorded 60 Hz frames of the golden `test-loop.autopilot` replay (7200 ticks, `rumbo-replay/2`). File loading, input expansion and `Simulation::new` stay
//! outside the timed window, as do the first `WARMUP_TICKS`. The remaining ticks are timed in batches of
//! `SAMPLE_TICKS`, `RUNS` times from a fresh simulation. A run is valid only if it ends on the replay's
//! final state hash, so the measure is of the real simulation and not of a short-circuit.
//!
//! D9 budgets ≤ 2 ms per tick for four cars in a race. The current API simulates one vehicle: this is the
//! one-car E0 baseline, recorded as a runner measurement. It does NOT certify the four-car budget; the
//! shared race scenario is measured when it exists (E3, ADR-010).
//!
//! Usage: `cargo run --release --locked -p sim-core --example bench_step [-- --json out.json]`
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "benchmark harness outside the core: it reads fixtures from disk and times with the wall clock"
)]
use sim_core::{InputFrame, ProfileFile, ReplayV2, Simulation, TrackDef};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

const TRACK: &str = "test-loop";
const REPLAY: &str = "test-loop.autopilot";
const WARMUP_TICKS: usize = 1200;
const SAMPLE_TICKS: usize = 600;
const RUNS: usize = 5;
const CARS: usize = 1;

struct Stats {
    samples: usize,
    min: f64,
    median: f64,
    p95: f64,
    max: f64,
    mean: f64,
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn stats(mut us_per_tick: Vec<f64>) -> Result<Stats, String> {
    if us_per_tick.is_empty() || us_per_tick.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err(format!("medición inválida: {us_per_tick:?}"));
    }
    us_per_tick.sort_by(f64::total_cmp);
    let n = us_per_tick.len();
    Ok(Stats {
        samples: n,
        min: us_per_tick[0],
        median: percentile(&us_per_tick, 0.5),
        p95: percentile(&us_per_tick, 0.95),
        max: us_per_tick[n - 1],
        mean: us_per_tick.iter().sum::<f64>() / n as f64,
    })
}

fn run() -> Result<(serde_json::Value, String), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |p: PathBuf| std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()));
    let track = TrackDef::from_json(&read(root.join(format!("assets/tracks/{TRACK}.json")))?)?;
    let golden = root.join(format!("crates/sim-core/tests/golden/{REPLAY}.bin"));
    let bytes = std::fs::read(&golden).map_err(|e| format!("{}: {e}", golden.display()))?;
    let manifest = ReplayV2::decode(&bytes).map_err(|e| e.to_string())?;
    let profile = ProfileFile::default();
    let inputs: Vec<InputFrame> = manifest.tick_frames().collect();
    if inputs.len() as u64 != manifest.ticks || inputs.len() <= WARMUP_TICKS {
        return Err(format!("escenario inválido: {} entradas para {} ticks", inputs.len(), manifest.ticks));
    }
    let timed = &inputs[WARMUP_TICKS..];
    let mut samples = Vec::new();
    for _ in 0..RUNS {
        let mut sim = Simulation::new(&track, &profile, manifest.header.seed)?;
        for input in &inputs[..WARMUP_TICKS] {
            sim.step(*input);
        }
        for batch in timed.chunks(SAMPLE_TICKS) {
            let t0 = Instant::now();
            for input in batch {
                sim.step(*input);
            }
            samples.push(t0.elapsed().as_secs_f64() * 1e6 / batch.len() as f64);
        }
        if sim.tick() != manifest.ticks || sim.state_hash() != manifest.final_hash {
            return Err(format!(
                "la corrida no reprodujo el replay: tick {} hash {} (esperado {} {:016x})",
                sim.tick(),
                sim.state_hash_hex(),
                manifest.ticks,
                manifest.final_hash
            ));
        }
    }
    let s = stats(samples)?;
    let json = serde_json::json!({
        "scenario": { "track": TRACK, "replay": REPLAY, "seed": manifest.header.seed, "profile": profile.id,
            "ticks": manifest.ticks, "warmupTicks": WARMUP_TICKS, "sampleTicks": SAMPLE_TICKS, "runs": RUNS },
        "cars": CARS,
        "samples": s.samples,
        "usPerTick": { "min": s.min, "median": s.median, "p95": s.p95, "max": s.max, "mean": s.mean },
        "budget": { "d9MsPerTick": 2.0, "d9Cars": 4, "certifies": false },
    });
    let md = format!(
        "## Benchmark nativo de `Simulation::step` (release)\n\n\
         Escenario: pista `{TRACK}`, entradas, perfil `{}` y semilla {} del replay `{REPLAY}` ({} ticks); \
         {WARMUP_TICKS} ticks de calentamiento fuera de la ventana; {RUNS} corridas × muestras de {SAMPLE_TICKS} ticks.\n\n\
         | Autos | Muestras | Mínimo | Mediana | p95 | Máximo | Media |\n| --- | --- | --- | --- | --- | --- | --- |\n\
         | {CARS} | {} | {:.1} µs | {:.1} µs | {:.1} µs | {:.1} µs | {:.1} µs |\n\n\
         Dato del runner, línea base de **un auto** en E0. D9 fija ≤ 2 ms por tick para **cuatro** autos en carrera: \
         esta medición no certifica ese presupuesto; el escenario compartido se mide cuando exista (E3).",
        profile.id, manifest.header.seed, manifest.ticks, s.samples, s.min, s.median, s.p95, s.max, s.mean
    );
    Ok((json, md))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let json_out = args.iter().position(|a| a == "--json").and_then(|i| args.get(i + 1));
    match run() {
        Ok((json, md)) => {
            println!("{md}");
            if let Some(path) = json_out {
                if let Err(e) = std::fs::write(path, json.to_string()) {
                    eprintln!("{path}: {e}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("benchmark inválido: {e}");
            ExitCode::FAILURE
        }
    }
}
