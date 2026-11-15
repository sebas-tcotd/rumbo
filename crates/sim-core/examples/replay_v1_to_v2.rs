//! Story 0.10 (D7, D17): deterministic migration of the golden replays from `race-lab-replay/1` to
//! `rumbo-replay/2`, with the tolerance table the PR attaches.
//!
//! For every `crates/sim-core/tests/golden-v1/<track>.<case>.json`:
//! 1. Reproduces the v1 run tick by tick with the old per-tick input semantics
//!    (`Simulation::step_legacy_v1`) and requires its control hashes and final hash, so the reference
//!    trajectory is exactly the recorded one.
//! 2. Builds the v2 run. Open-loop drivers (`scripted`, `adversarial`: inputs are a function of time)
//!    are **converted**: `sim_core::replay::migrate_v1` keeps the axes of the first tick of each pair and
//!    the union of the event flags of both ticks. The closed-loop driver (`autopilot`: inputs are a
//!    function of the state) is **re-driven** at 60 Hz from the same seed, track and profile, because
//!    replaying a controller's per-tick output open loop at half the rate leaves it without feedback.
//! 3. Compares both runs: laps (count and time), mean and maximum lateral offset difference at the same
//!    tick, the first tick where they differ by more than 0.5 m, and the chaos floor: the mean lateral
//!    difference that one extra full-brake tick (an input edge moved by one tick, which is what the
//!    conversion does to edges on odd ticks) produces by itself in the v1 run.
//! 4. Writes `crates/sim-core/tests/golden/<track>.<case>.bin` only when every case is within tolerance,
//!    or with `--accept` after the investigation is recorded in the PR.
//!
//! The Markdown table goes to stdout. Usage:
//! `cargo run --release --locked -p sim-core --example replay_v1_to_v2 [-- --accept | --check]`
//! `--check` writes nothing and fails if a committed `.bin` differs from the migration output.
#![allow(clippy::disallowed_methods, reason = "migration tool outside the core: it reads and writes fixtures on disk")]
use sim_core::autopilot::Autopilot;
use sim_core::replay::migrate_v1;
use sim_core::{InputFrame, ReplayManifest, ReplayV2, Simulation, TrackDef};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Tolerances fixed in the PR (AC 5).
const MAX_LAP_DELTA_PCT: f64 = 1.0;
const MAX_MEAN_LATERAL_M: f64 = 0.5;
/// Odd tick of the single-tick perturbation that measures the chaos floor.
const PERTURB_TICK: u64 = 121;

struct Run {
    laps: Vec<u64>,
    lateral: Vec<f32>,
}

struct Row {
    name: String,
    mode: &'static str,
    ticks: u64,
    v1: Run,
    v2: Run,
    floor: f64,
    bytes_v1: usize,
    bytes_v2: usize,
}

fn lateral(sim: &Simulation) -> f32 {
    let view = sim.view();
    let sample = &sim.track().samples[view.sample_index];
    (view.pos - sample.pos).dot(sample.right)
}

fn v1_inputs(m: &ReplayManifest) -> impl Iterator<Item = InputFrame> + '_ {
    m.inputs.iter().flat_map(|r| std::iter::repeat_n(InputFrame::unpack(r[1]), r[0] as usize))
}

/// v1 run with per-tick inputs. With `perturb`, one tick brakes fully (an edge moved by one tick).
fn run_v1(m: &ReplayManifest, track: &TrackDef, perturb: bool) -> Result<Run, String> {
    let mut sim = Simulation::new(track, &m.profile, m.seed)?;
    let mut checks = m.checks.iter().peekable();
    let mut lateral_v = Vec::new();
    for mut input in v1_inputs(m) {
        if perturb && sim.tick() == PERTURB_TICK {
            input.brake = 255;
            input.throttle = 0;
        }
        sim.step_legacy_v1(input);
        lateral_v.push(lateral(&sim));
        if let Some(c) = checks.next_if(|c| c.tick == sim.tick()) {
            if !perturb && c.hash != sim.state_hash_hex() {
                return Err(format!("v1 no se reproduce en el tick {}", c.tick));
            }
        }
    }
    if !perturb && (sim.state_hash_hex() != m.final_state_hash || sim.tick() != m.ticks) {
        return Err("v1 no reproduce su hash final".into());
    }
    Ok(Run { laps: sim.timer().completed.clone(), lateral: lateral_v })
}

/// v2 run, measured tick by tick: re-driven autopilot or converted frames. Returns the replay too.
fn run_v2(m: &ReplayManifest, track: &TrackDef, autopilot: bool) -> Result<(Run, ReplayV2), String> {
    let converted = if autopilot { None } else { Some(migrate_v1(m, track)?) };
    let frames: Vec<InputFrame> = converted.iter().flat_map(ReplayV2::tick_frames).collect();
    let mut sim = Simulation::new(track, &m.profile, m.seed)?;
    let ap = Autopilot::default();
    let mut lateral_v = Vec::new();
    for t in 0..m.ticks {
        let input = if autopilot { ap.input(&sim) } else { frames[t as usize] };
        sim.step(input);
        lateral_v.push(lateral(&sim));
    }
    let replay = sim.replay();
    if converted.as_ref().is_some_and(|c| *c != replay) {
        return Err("la medición v2 no coincide con la migración".into());
    }
    Ok((Run { laps: sim.timer().completed.clone(), lateral: lateral_v }, replay))
}

fn mean_max(a: &Run, b: &Run) -> (f64, f64) {
    let d: Vec<f64> = a.lateral.iter().zip(&b.lateral).map(|(x, y)| f64::from((x - y).abs())).collect();
    (d.iter().sum::<f64>() / d.len().max(1) as f64, d.iter().copied().fold(0.0, f64::max))
}

/// First tick (1-based) where the lateral offsets differ by more than the tolerance.
fn first_divergence(a: &Run, b: &Run) -> Option<usize> {
    a.lateral.iter().zip(&b.lateral).position(|(x, y)| f64::from((x - y).abs()) > MAX_MEAN_LATERAL_M).map(|i| i + 1)
}

fn lap_deltas(r: &Row) -> Vec<f64> {
    r.v1.laps.iter().zip(&r.v2.laps).map(|(a, b)| (*b as f64 - *a as f64) / *a as f64 * 100.0).collect()
}

fn laps_ok(r: &Row) -> bool {
    r.v1.laps.len() == r.v2.laps.len() && lap_deltas(r).iter().all(|d| d.abs() <= MAX_LAP_DELTA_PCT)
}

fn within(r: &Row) -> bool {
    laps_ok(r) && mean_max(&r.v1, &r.v2).0 <= MAX_MEAN_LATERAL_M
}

fn fmt_laps(laps: &[u64]) -> String {
    if laps.is_empty() {
        return "—".into();
    }
    laps.iter().map(|t| format!("{:.3} s", *t as f64 / 120.0)).collect::<Vec<_>>().join(", ")
}

fn print_table(rows: &[Row]) {
    println!(
        "| Dorado | Modo v2 | Ticks | Vueltas v1 | Vueltas v2 | Δ vuelta | Desvío lateral medio | Máximo | Tick > 0,5 m | Piso caótico | JSON v1 | `.bin` v2 | Vueltas | Lateral |"
    );
    println!("|---|---|---:|---|---|---|---:|---:|---:|---:|---:|---:|---|---|");
    for r in rows {
        let (mean, max) = mean_max(&r.v1, &r.v2);
        let deltas = lap_deltas(r);
        let delta = if deltas.is_empty() {
            "—".into()
        } else {
            deltas.iter().map(|d| format!("{d:+.2} %")).collect::<Vec<_>>().join(", ")
        };
        println!(
            "| `{}` | {} | {} | {} | {} | {} | {:.3} m | {:.3} m | {} | {:.3} m | {} B | {} B | {} | {} |",
            r.name,
            r.mode,
            r.ticks,
            fmt_laps(&r.v1.laps),
            fmt_laps(&r.v2.laps),
            delta,
            mean,
            max,
            first_divergence(&r.v1, &r.v2).map_or("—".into(), |t| t.to_string()),
            r.floor,
            r.bytes_v1,
            r.bytes_v2,
            if laps_ok(r) { "dentro" } else { "FUERA" },
            if mean <= MAX_MEAN_LATERAL_M { "dentro" } else { "FUERA" },
        );
    }
    println!();
    println!(
        "Tolerancias: mismas vueltas cerradas con |Δ vuelta| ≤ {MAX_LAP_DELTA_PCT} % y desvío lateral medio ≤ {MAX_MEAN_LATERAL_M} m. \
         Piso caótico: desvío lateral medio de v1 contra v1 con un solo tick de freno a fondo en el tick {PERTURB_TICK}."
    );
}

fn row(path: &Path, root: &Path) -> Result<(Row, Vec<u8>), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let m = ReplayManifest::from_json(&json)?;
    let track_path = root.join(format!("assets/tracks/{}.json", m.track_id));
    let track = TrackDef::from_json(&std::fs::read_to_string(track_path).map_err(|e| e.to_string())?)?;
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    let autopilot = name.ends_with(".autopilot");
    let (v2, replay) = run_v2(&m, &track, autopilot)?;
    let bytes = replay.encode().map_err(|e| e.to_string())?;
    let v1 = run_v1(&m, &track, false)?;
    let floor = mean_max(&v1, &run_v1(&m, &track, true)?).0;
    let mode = if autopilot { "autopiloto a 60 Hz" } else { "conversión" };
    let row = Row { name, mode, ticks: m.ticks, v1, v2, floor, bytes_v1: json.len(), bytes_v2: bytes.len() };
    Ok((row, bytes))
}

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (check, accept) = (args.iter().any(|a| a == "--check"), args.iter().any(|a| a == "--accept"));
    match run(&root, check, accept) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("replay_v1_to_v2: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path, check: bool, accept: bool) -> Result<(), String> {
    let v1_dir = root.join("crates/sim-core/tests/golden-v1");
    let out_dir = root.join("crates/sim-core/tests/golden");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&v1_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    let mut rows = Vec::new();
    let mut outputs = Vec::new();
    for path in &paths {
        let (r, bytes) = row(path, root).map_err(|e| format!("{}: {e}", path.display()))?;
        outputs.push((r.name.clone(), bytes));
        rows.push(r);
    }
    print_table(&rows);
    if check {
        for (name, bytes) in &outputs {
            let committed = std::fs::read(out_dir.join(format!("{name}.bin"))).map_err(|e| format!("{name}: {e}"))?;
            if &committed != bytes {
                return Err(format!("{name}.bin no coincide con la migración"));
            }
        }
        return Ok(());
    }
    let out_of_tolerance: Vec<_> = rows.iter().filter(|r| !within(r)).map(|r| r.name.clone()).collect();
    if !out_of_tolerance.is_empty() && !accept {
        return Err(format!("fuera de tolerancia (no se escribió nada): {out_of_tolerance:?}"));
    }
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    for (name, bytes) in &outputs {
        std::fs::write(out_dir.join(format!("{name}.bin")), bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}
