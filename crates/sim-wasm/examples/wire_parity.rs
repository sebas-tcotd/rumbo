//! Native peer for the cross-language contract test. Never linked into the browser binary.
//!
//! What is real: the state layout and bit values come from `sim_core` (`STATE_FIELDS`, `write_state`), the
//! input word from `InputFrame::pack`, and the envelope and hash from `track_format`. What is prospective:
//! `protocol`, `load`, `fatal`, `header`, `link` and `codec` are hand-written Rust fixtures of contracts that
//! have no Rust types yet (replay v2, link, challenge codec arrive in 0.10 and 6.7). They pin names and
//! shapes only; real encoders and readers must replace them when those formats are implemented.
use serde_json::{Value, json};
use sim_core::{InputFrame, ProfileFile, STATE_LEN, Simulation, TrackDef};
use std::io::{self, Read};

fn protocol() -> Value {
    json!({
        "commands":{
            "start":[],"pause":[],"resume":[],"dispose":[],"reset":[],"restart":[],"exportReplay":[],
            "load":["profile","race","track","zone"],"input":["frame"],"loadGhost":["replay"],
            "verifyReplay":["json"],"newSession":["profileJson"],"autopilot":["on"],
            "hashContent":["bytes","requestId"],"returnBuffer":["buffer","slot"]
        },
        "responses":{
            "ready":["barriers","centerline","checkpoints","groundY","hashes","layout","meshes","physicsVersion",
                     "schema","tickHz","track","trackLength","vehicle"],
            "commandError":["code","operation"],"contentHash":["hash","requestId"],"ghostReady":["trackLen"],
            "snapshot":["at","autopilot","buffer","layoutVersion","paused","simHz","slot","stepMs"],
            "session":["hashes"],"lap":["best","ghost","ticks"],"replay":["json","ticks"],"verify":["report"],
            "verifyError":["code","message"],"fatal":["code","message"],"unsupported":["code","operation"]
        }
    })
}

fn fixture() -> Result<Value, Box<dyn std::error::Error>> {
    let track = TrackDef::from_json(include_str!("../../../assets/tracks/test-loop.json"))?;
    let mut sim = Simulation::new(&track, &ProfileFile::default(), 7)?;
    let frame = InputFrame {
        steer: -31,
        throttle: 200,
        brake: 3,
        flags: 1,
    };
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
        "load":{"type":"load","zone":null,"race":null,"track":"track","profile":"profile","protocolVersion":1,"generation":2},
        "fatal":{"type":"fatal","code":"SIM_FAILED","message":"trap","protocolVersion":1,"generation":2},
        "header":{"format":"rumbo-replay/2","physicsVersion":sim_core::PHYSICS_VERSION,"trackHash":"t","raceHash":"r","profileRef":{"id":"p","hash":"h"},"seed":u64::MAX.to_string()},
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
