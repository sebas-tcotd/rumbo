//! Native peer for the cross-language contract test. Never linked into the browser binary.
//!
//! What is real: the state layout and bit values come from `sim_core` (`STATE_FIELDS`, `write_state`), the
//! input word from `InputFrame::pack`, the envelope and hash from `track_format`, and the `rumbo-replay/2`
//! header from `ReplayHeader::wire_json` of a real session (story 0.10; TS reads it through `replay_summary`).
//! What is prospective: `protocol`, `load`, `fatal`, `link` and `codec` are hand-written Rust fixtures of
//! contracts that have no Rust types yet (link and challenge codec arrive in 6.7). They pin names and shapes
//! only; real encoders and readers must replace them when those formats are implemented.
use serde_json::{json, Value};
use sim_core::{InputFrame, ProfileFile, Simulation, TrackDef, STATE_LEN};
use std::io::{self, Read};

fn protocol() -> Value {
    json!({
        "commands":{
            "start":[],"pause":[],"resume":[],"dispose":[],"reset":[],"restart":[],"exportReplay":[],
            "load":["profile","race","track","zone"],"input":["frame"],"loadGhost":["replay"],
            "verifyReplay":["bytes"],"newSession":["profileJson"],"autopilot":["on"],
            "hashContent":["bytes","requestId"],"returnBuffer":["buffer","slot"],
            "devTeleport":["s"],"devSeekReplay":["bytes","tick"]
        },
        "responses":{
            "ready":["barriers","centerline","checkpoints","groundY","hashes","layout","meshes","physicsVersion",
                     "schema","tickHz","track","trackLength","vehicle"],
            "commandError":["code","operation"],"contentHash":["hash","requestId"],"ghostReady":["trackLen"],
            "snapshot":["at","autopilot","buffer","layoutVersion","paused","simHz","slot","stepMs"],
            "session":["hashes"],"lap":["best","ghost","ticks"],"replay":["bytes","ticks"],"verify":["report"],
            "verifyError":["code","message"],"fatal":["code","message"],"unsupported":["code","operation"]
        }
    })
}

fn fixture() -> Result<Value, Box<dyn std::error::Error>> {
    let track = TrackDef::from_json(include_str!("../../../assets/tracks/test-loop.json"))?;
    let mut sim = Simulation::new(&track, &ProfileFile::default(), 7)?;
    let frame = InputFrame { steer: -31, throttle: 200, brake: 3, flags: 1 };
    sim.step(frame);
    let mut values = vec![0.0_f32; STATE_LEN];
    sim.write_state(&mut values);
    let envelope = track_format::codec::encode(&frame)?;
    let fields: Vec<_> = sim_core::simulation::STATE_FIELDS
        .iter()
        .enumerate()
        .map(|(i, name)| json!({"name": name, "type": "f32", "offset": i * size_of::<f32>()}))
        .collect();
    Ok(json!({
        "layout":{"version":1, "stride":STATE_LEN * size_of::<f32>(), "fields":fields},
        "bits": values.into_iter().map(f32::to_bits).collect::<Vec<_>>(),
        "protocol":protocol(), "input": frame.pack(), "envelope": envelope, "hash":track_format::hash::content_hash(&envelope).hex(),
        "edgeErrorCodes": sim_wasm::RumboError::CODES,
        "load":{"type":"load","zone":null,"race":null,"track":"track","profile":"profile","protocolVersion":3,"generation":2},
        "fatal":{"type":"fatal","code":"SIM_FAILED","message":"trap","protocolVersion":3,"generation":2},
        // AR28 dev commands (story 0.6): shapes only; behaviour lives in sim_core::replay::resimulate_until
        // and Simulation::teleport_to_s, reached through Lab::from_replay and Lab::teleport. `bytes` is a
        // Uint8Array in TS; the fixture carries its values.
        "devTeleport":{"type":"devTeleport","s":120.5,"protocolVersion":3,"generation":2},
        "devSeekReplay":{"type":"devSeekReplay","bytes":[1],"tick":sim_core::replay::MAX_SEEK_TICKS,"protocolVersion":3,"generation":2},
        "header": sim.replay().header.wire_json(),
        "link":"#v1.i.YQ", "codec":{"encodeChallenge":["race","replay","routeMeta"],"decodeChallenge":["bytes"],"hashContent":["bytes"],"routeMeta":{"name":"Lima","durationEstimate":60}}
    }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expected = fixture()?;
    if std::env::args().nth(1).as_deref() == Some("check") {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        let supplied: Value = serde_json::from_str(&input)?;
        if supplied != expected {
            return Err("TS fixture differs from native contract".into());
        }
    } else {
        println!("{}", expected);
    }
    Ok(())
}
