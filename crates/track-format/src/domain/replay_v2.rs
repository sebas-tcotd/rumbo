//! `rumbo-replay/2` (D17, AR10): binary, versioned replay with 60 Hz input.
//!
//! Bytes: the `RTF\0` envelope (`codec`) around a postcard `ReplayWire`:
//!
//! ```text
//! ReplayWire { header: ReplayHeader, ticks: u64, streams: Vec<Vec<u8>>, checks: Vec<ControlHash>, final_hash: u64 }
//! ```
//!
//! - `header` identifies what was raced: physics version, track, race, the player's vehicle and the seed.
//!   Its hashes are 16-byte content identities, so the layout does not change when the real `raceHash`
//!   (1.1, 2.5) and `vehicleRef` (1.3) replace the E0 sentinels (`legacy_race`, `VehicleRef::legacy`).
//!   `players` reserves room for N players; the MVP accepts exactly 1 (bots are never recorded).
//! - `ticks` is the number of simulation ticks (120 Hz). There are `ceil(ticks / 2)` input frames:
//!   frame `k` drives ticks `2k` and `2k + 1` (D7); a replay may end after the first tick of a pair.
//! - `streams` holds one encoded input stream per player, as a sequence of records:
//!   `repeat` (LEB128 varint, ≥ 1 frames) · `mask` (u8: bit 0 steer, 1 throttle, 2 brake, 3 flags) ·
//!   for each set bit, the zigzag LEB128 delta of that field against the previous frame (the first
//!   record against the all-zero frame). The encoding is canonical: adjacent records always differ
//!   (`mask != 0` except in the first record), varints are minimal and no other mask bit is set, so
//!   equal inputs always produce equal bytes.
//! - `checks` are FNV state hashes every `CHECK_EVERY_TICKS` (10 s); `final_hash` is the state hash
//!   after the last tick.
use super::input::InputFrame;
use crate::codec;
use crate::hash::{content_hash, ContentHash};
use crate::version::{REPLAY_FORMAT_V2, TICKS_PER_FRAME};
use serde::{Deserialize, Serialize};

/// Largest replay the reader accepts: two hours at 120 Hz (P5: a crafted file cannot hang a worker).
pub const MAX_REPLAY_TICKS: u64 = 2 * 60 * 60 * 120;
/// Players accepted by this version of the reader (the header reserves room for more).
pub const MVP_PLAYERS: u8 = 1;

const MASK_STEER: u8 = 1 << 0;
const MASK_THROTTLE: u8 = 1 << 1;
const MASK_BRAKE: u8 = 1 << 2;
const MASK_FLAGS: u8 = 1 << 3;
const MASK_ALL: u8 = MASK_STEER | MASK_THROTTLE | MASK_BRAKE | MASK_FLAGS;

/// A vehicle profile reference: base model id plus the content hash of the effective profile (ADR-009).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleRef {
    pub id: String,
    pub hash: ContentHash,
}

impl VehicleRef {
    /// E0 sentinel `LEGACY_VEHICLE`: no hashed vehicle profile exists yet (story 1.3 replaces it).
    /// The verifier receives the profile from the caller and cannot check it against this reference.
    pub fn legacy() -> Self {
        Self { id: LEGACY_VEHICLE_ID.into(), hash: content_hash(b"rumbo/legacy-vehicle/1") }
    }

    /// Whether this is the `LEGACY_VEHICLE` sentinel.
    pub fn is_legacy(&self) -> bool {
        *self == Self::legacy()
    }
}

/// Id of the `LEGACY_VEHICLE` sentinel.
pub const LEGACY_VEHICLE_ID: &str = "legacy-vehicle";

/// E0 sentinel `LEGACY_RACE`: no `Race` exists yet (stories 1.1 and 2.5 replace it).
/// Derived from a fixed label, never zeros, so an uninitialized hash does not pass for it.
pub fn legacy_race() -> ContentHash {
    content_hash(b"rumbo/legacy-race/1")
}

/// E0 track identity: the BLAKE3-128 of the label `rumbo/legacy-track/1` followed by the legacy 64-bit
/// FNV track hash (little endian). Derivable from the compiled track, so the reader verifies it. Story
/// 2.5 replaces it with the `Track` content identity in its own `physics:` commit.
pub fn legacy_track_hash(fnv: u64) -> ContentHash {
    let mut bytes = b"rumbo/legacy-track/1".to_vec();
    bytes.extend_from_slice(&fnv.to_le_bytes());
    content_hash(&bytes)
}

/// What was raced. Field order is the wire order; never reorder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayHeader {
    pub format: String,
    pub physics_version: String,
    pub track_hash: ContentHash,
    pub race_hash: ContentHash,
    /// The local player's vehicle (ADR-009, ADR-010); bots take theirs from the `Race`.
    pub vehicle_ref: VehicleRef,
    pub seed: u64,
    /// Recorded players; one input stream each. The MVP writes and accepts 1.
    pub players: u8,
}

impl ReplayHeader {
    /// JSON shape read by TypeScript (`isReplayHeader` in `@rumbo/contracts/wire.ts`): camelCase,
    /// hashes in lowercase hex and the seed as a decimal string (u64 does not fit a JS number).
    pub fn wire_json(&self) -> serde_json::Value {
        serde_json::json!({
            "format": self.format,
            "physicsVersion": self.physics_version,
            "trackHash": self.track_hash.hex(),
            "raceHash": self.race_hash.hex(),
            "vehicleRef": { "id": self.vehicle_ref.id, "hash": self.vehicle_ref.hash.hex() },
            "seed": self.seed.to_string(),
            "players": self.players,
        })
    }
}

/// A run of identical input frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputRun {
    /// Number of 60 Hz frames (≥ 1).
    pub frames: u64,
    pub frame: InputFrame,
}

/// FNV state hash after `tick`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlHash {
    pub tick: u64,
    pub hash: u64,
}

/// A decoded `rumbo-replay/2` for the single MVP player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayV2 {
    pub header: ReplayHeader,
    /// Simulation ticks at 120 Hz.
    pub ticks: u64,
    /// The player's 60 Hz frames as canonical runs (adjacent runs differ).
    pub inputs: Vec<InputRun>,
    pub checks: Vec<ControlHash>,
    pub final_hash: u64,
}

#[derive(Serialize, Deserialize)]
struct ReplayWire {
    header: ReplayHeader,
    ticks: u64,
    streams: Vec<Vec<u8>>,
    checks: Vec<ControlHash>,
    final_hash: u64,
}

/// Why a replay cannot be written or read.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplayError {
    Codec(codec::Error),
    UnknownFormat(String),
    UnsupportedPlayers(u8),
    StreamCount { players: u8, streams: usize },
    TooManyTicks(u64),
    Inputs(&'static str),
    FrameCount { expected: u64, actual: u64 },
    Checks(&'static str),
}

impl core::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Codec(e) => write!(f, "{e}"),
            Self::UnknownFormat(s) => write!(f, "formato desconocido: {s}"),
            Self::UnsupportedPlayers(n) => write!(f, "jugadores no soportados: {n}"),
            Self::StreamCount { players, streams } => write!(f, "{streams} flujos de entrada para {players} jugadores"),
            Self::TooManyTicks(t) => write!(f, "replay demasiado largo: {t} ticks (máximo {MAX_REPLAY_TICKS})"),
            Self::Inputs(why) => write!(f, "entradas inválidas: {why}"),
            Self::FrameCount { expected, actual } => write!(f, "{actual} frames de entrada para {expected} esperados"),
            Self::Checks(why) => write!(f, "controles inválidos: {why}"),
        }
    }
}

impl std::error::Error for ReplayError {}

impl ReplayV2 {
    /// Number of 60 Hz frames needed to cover `ticks`.
    pub fn frames_for(ticks: u64) -> u64 {
        ticks.div_ceil(TICKS_PER_FRAME)
    }

    /// The frame of each simulation tick, in order (`ticks` items): both ticks of a pair get the pair's
    /// frame. The simulation itself drops the event flags on the second tick (D7).
    pub fn tick_frames(&self) -> impl Iterator<Item = InputFrame> + '_ {
        self.inputs
            .iter()
            .flat_map(|run| core::iter::repeat_n(run.frame, saturating_ticks(run.frames)))
            .take(usize::try_from(self.ticks).unwrap_or(usize::MAX))
    }

    /// Serialize to `rumbo-replay/2` bytes.
    /// # Errors
    /// Rejects a replay that violates the format (see `validate`) or that postcard cannot encode.
    pub fn encode(&self) -> Result<Vec<u8>, ReplayError> {
        self.validate()?;
        let wire = ReplayWire {
            header: self.header.clone(),
            ticks: self.ticks,
            streams: vec![encode_inputs(&self.inputs)],
            checks: self.checks.clone(),
            final_hash: self.final_hash,
        };
        codec::encode(&wire).map_err(ReplayError::Codec)
    }

    /// Parse `rumbo-replay/2` bytes.
    /// # Errors
    /// Rejects a bad envelope, an unknown format, unsupported player counts, oversized or inconsistent
    /// inputs, non-canonical input streams and out-of-order control hashes.
    pub fn decode(bytes: &[u8]) -> Result<Self, ReplayError> {
        let wire: ReplayWire = codec::decode(bytes).map_err(ReplayError::Codec)?;
        check_header(&wire.header)?;
        if wire.streams.len() != usize::from(wire.header.players) {
            return Err(ReplayError::StreamCount { players: wire.header.players, streams: wire.streams.len() });
        }
        if wire.ticks > MAX_REPLAY_TICKS {
            return Err(ReplayError::TooManyTicks(wire.ticks));
        }
        let inputs = decode_inputs(&wire.streams[0], Self::frames_for(wire.ticks))?;
        let replay =
            Self { header: wire.header, ticks: wire.ticks, inputs, checks: wire.checks, final_hash: wire.final_hash };
        replay.validate()?;
        Ok(replay)
    }

    /// Format invariants shared by the writer and the reader.
    /// # Errors
    /// See `decode`.
    pub fn validate(&self) -> Result<(), ReplayError> {
        check_header(&self.header)?;
        if self.ticks > MAX_REPLAY_TICKS {
            return Err(ReplayError::TooManyTicks(self.ticks));
        }
        if self.inputs.iter().any(|r| r.frames == 0) {
            return Err(ReplayError::Inputs("tanda vacía"));
        }
        let actual = self.inputs.iter().try_fold(0u64, |n, r| n.checked_add(r.frames));
        let expected = Self::frames_for(self.ticks);
        if actual != Some(expected) {
            return Err(ReplayError::FrameCount { expected, actual: actual.unwrap_or(u64::MAX) });
        }
        check_controls(&self.checks, self.ticks)
    }
}

fn saturating_ticks(frames: u64) -> usize {
    usize::try_from(frames.saturating_mul(TICKS_PER_FRAME)).unwrap_or(usize::MAX)
}

fn check_header(h: &ReplayHeader) -> Result<(), ReplayError> {
    if h.format != REPLAY_FORMAT_V2 {
        return Err(ReplayError::UnknownFormat(h.format.clone()));
    }
    if h.players != MVP_PLAYERS {
        return Err(ReplayError::UnsupportedPlayers(h.players));
    }
    Ok(())
}

fn check_controls(checks: &[ControlHash], ticks: u64) -> Result<(), ReplayError> {
    let mut last = 0;
    for c in checks {
        if c.tick <= last || c.tick > ticks {
            return Err(ReplayError::Checks("fuera de orden o de rango"));
        }
        last = c.tick;
    }
    Ok(())
}

fn fields(f: InputFrame) -> [i16; 4] {
    [i16::from(f.steer), i16::from(f.throttle), i16::from(f.brake), i16::from(f.flags)]
}

/// Encode runs canonically: empty runs are skipped and adjacent equal frames merged.
pub fn encode_inputs(runs: &[InputRun]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut merged: Vec<InputRun> = Vec::with_capacity(runs.len());
    for run in runs.iter().filter(|r| r.frames > 0) {
        match merged.last_mut() {
            Some(last) if last.frame == run.frame => last.frames = last.frames.saturating_add(run.frames),
            _ => merged.push(*run),
        }
    }
    let mut prev = [0i16; 4];
    for run in &merged {
        let cur = fields(run.frame);
        write_varint(&mut out, run.frames);
        let mask = (0..4).filter(|&i| cur[i] != prev[i]).fold(0u8, |m, i| m | (1 << i));
        out.push(mask);
        for i in (0..4).filter(|&i| mask & (1 << i) != 0) {
            write_varint(&mut out, u64::from(zigzag(cur[i] - prev[i])));
        }
        prev = cur;
    }
    out
}

/// Decode a canonical input stream holding at most `max_frames` frames.
/// # Errors
/// Truncated or overlong varints, empty records, unknown or empty masks, out-of-range values and more
/// frames than `max_frames`.
pub fn decode_inputs(bytes: &[u8], max_frames: u64) -> Result<Vec<InputRun>, ReplayError> {
    let mut pos = 0;
    let mut prev = [0i16; 4];
    let mut total = 0u64;
    let mut runs = Vec::new();
    while pos < bytes.len() {
        let frames = read_varint(bytes, &mut pos)?;
        total = total
            .checked_add(frames)
            .filter(|&t| frames > 0 && t <= max_frames)
            .ok_or(ReplayError::Inputs("tanda vacía o demasiadas tandas"))?;
        let mask = *bytes.get(pos).ok_or(ReplayError::Inputs("registro truncado"))?;
        pos += 1;
        if mask & !MASK_ALL != 0 || (mask == 0 && !runs.is_empty()) {
            return Err(ReplayError::Inputs("máscara no canónica"));
        }
        for i in (0..4).filter(|&i| mask & (1 << i) != 0) {
            let delta = unzigzag(
                u16::try_from(read_varint(bytes, &mut pos)?)
                    .map_err(|_| ReplayError::Inputs("delta fuera de rango"))?,
            );
            prev[i] = prev[i].checked_add(delta).ok_or(ReplayError::Inputs("delta fuera de rango"))?;
        }
        runs.push(InputRun { frames, frame: frame_from(prev)? });
    }
    Ok(runs)
}

fn frame_from(v: [i16; 4]) -> Result<InputFrame, ReplayError> {
    let range = ReplayError::Inputs("valor fuera de rango");
    Ok(InputFrame {
        steer: i8::try_from(v[0]).map_err(|_| range)?,
        throttle: u8::try_from(v[1]).map_err(|_| ReplayError::Inputs("valor fuera de rango"))?,
        brake: u8::try_from(v[2]).map_err(|_| ReplayError::Inputs("valor fuera de rango"))?,
        flags: u8::try_from(v[3]).map_err(|_| ReplayError::Inputs("valor fuera de rango"))?,
    })
}

fn zigzag(v: i16) -> u16 {
    ((v << 1) ^ (v >> 15)) as u16
}

fn unzigzag(v: u16) -> i16 {
    ((v >> 1) as i16) ^ -((v & 1) as i16)
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// Minimal LEB128 u64: at most 10 bytes and no redundant trailing zero group.
fn read_varint(bytes: &[u8], pos: &mut usize) -> Result<u64, ReplayError> {
    let mut v = 0u64;
    for i in 0..10 {
        let b = *bytes.get(*pos).ok_or(ReplayError::Inputs("varint truncado"))?;
        *pos += 1;
        let low = u64::from(b & 0x7f);
        if i == 9 && b > 1 {
            return Err(ReplayError::Inputs("varint fuera de rango"));
        }
        v |= low << (7 * i);
        if b & 0x80 == 0 {
            if b == 0 && i > 0 {
                return Err(ReplayError::Inputs("varint no mínimo"));
            }
            return Ok(v);
        }
    }
    Err(ReplayError::Inputs("varint demasiado largo"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(frames: u64, steer: i8, throttle: u8, brake: u8, flags: u8) -> InputRun {
        InputRun { frames, frame: InputFrame { steer, throttle, brake, flags } }
    }

    #[test]
    fn zigzag_roundtrip_extremes() {
        for v in [-510i16, -255, -254, -1, 0, 1, 254, 255, 510] {
            assert_eq!(unzigzag(zigzag(v)), v);
        }
    }

    #[test]
    fn inputs_roundtrip_including_extreme_deltas() {
        let runs = vec![run(3, -127, 0, 0, 0), run(1, 127, 255, 255, 7), run(9, -127, 0, 0, 0), run(2, 0, 1, 0, 2)];
        let bytes = encode_inputs(&runs);
        assert_eq!(decode_inputs(&bytes, u64::MAX).unwrap(), runs);
    }

    #[test]
    fn empty_and_zero_first_frame() {
        assert!(encode_inputs(&[]).is_empty());
        assert_eq!(decode_inputs(&[], 0).unwrap(), vec![]);
        let zero = vec![run(5, 0, 0, 0, 0)];
        let bytes = encode_inputs(&zero);
        assert_eq!(bytes, [5, 0]);
        assert_eq!(decode_inputs(&bytes, 5).unwrap(), zero);
    }

    #[test]
    fn runs_beyond_u32_and_canonical_merge() {
        let big = u64::from(u32::MAX) + 7;
        let runs = [run(big, 1 + 3, 0, 0, 0), run(2, 4, 0, 0, 0), run(0, 9, 9, 9, 9)];
        let decoded = decode_inputs(&encode_inputs(&runs), u64::MAX).unwrap();
        assert_eq!(decoded, vec![run(big + 2, 4, 0, 0, 0)]);
        assert!(decode_inputs(&encode_inputs(&runs), big).is_err());
    }

    #[test]
    fn rejects_non_canonical_streams() {
        let bad: &[&[u8]] = &[
            &[0, 1, 2],                // zero-frame run
            &[1],                      // truncated record
            &[1, 0x10],                // unknown mask bit
            &[1, 1, 2, 1, 0],          // empty mask after the first record
            &[1, 1, 0x80],             // truncated varint
            &[0x81, 0x00, 1, 2],       // non-minimal varint
            &[1, 1, 0xff, 0xff, 0x07], // delta beyond u16
            &[1, 1, 0x80, 0x02],       // steer 128 out of range
        ];
        for b in bad {
            assert!(decode_inputs(b, u64::MAX).is_err(), "{b:?}");
        }
    }

    fn replay(ticks: u64, inputs: Vec<InputRun>) -> ReplayV2 {
        ReplayV2 {
            header: ReplayHeader {
                format: REPLAY_FORMAT_V2.into(),
                physics_version: "x".into(),
                track_hash: legacy_track_hash(1),
                race_hash: legacy_race(),
                vehicle_ref: VehicleRef::legacy(),
                seed: u64::MAX,
                players: 1,
            },
            ticks,
            inputs,
            checks: vec![ControlHash { tick: 2, hash: u64::MAX }],
            final_hash: 42,
        }
    }

    #[test]
    fn replay_roundtrip_odd_ticks_and_tick_frames() {
        let r = replay(5, vec![run(2, 10, 0, 0, 2), run(1, -10, 0, 0, 0)]);
        let back = ReplayV2::decode(&r.encode().unwrap()).unwrap();
        assert_eq!(back, r);
        let steer: Vec<i8> = back.tick_frames().map(|f| f.steer).collect();
        assert_eq!(steer, [10, 10, 10, 10, -10]);
    }

    #[test]
    fn replay_rejections() {
        assert!(matches!(replay(5, vec![run(2, 0, 0, 0, 0)]).encode(), Err(ReplayError::FrameCount { .. })));
        let mut r = replay(2, vec![run(1, 0, 0, 0, 0)]);
        r.checks = vec![ControlHash { tick: 3, hash: 0 }];
        assert!(r.encode().is_err());
        let mut r = replay(2, vec![run(1, 0, 0, 0, 0)]);
        r.header.players = 2;
        assert_eq!(r.encode(), Err(ReplayError::UnsupportedPlayers(2)));
        let mut r = replay(2, vec![run(1, 0, 0, 0, 0)]);
        r.header.format = "race-lab-replay/1".into();
        assert!(matches!(r.encode(), Err(ReplayError::UnknownFormat(_))));
        let r = replay(MAX_REPLAY_TICKS + 2, vec![run(MAX_REPLAY_TICKS / 2 + 1, 0, 0, 0, 0)]);
        assert_eq!(r.encode(), Err(ReplayError::TooManyTicks(MAX_REPLAY_TICKS + 2)));
        let mut bytes = replay(2, vec![run(1, 0, 0, 0, 0)]).encode().unwrap();
        bytes.push(0);
        assert!(matches!(ReplayV2::decode(&bytes), Err(ReplayError::Codec(_))));
    }

    #[test]
    fn sentinels_are_stable_and_not_zero() {
        assert_ne!(legacy_race(), ContentHash([0; 16]));
        assert!(VehicleRef::legacy().is_legacy());
        assert!(!VehicleRef { id: LEGACY_VEHICLE_ID.into(), hash: ContentHash([0; 16]) }.is_legacy());
        assert_ne!(legacy_track_hash(1), legacy_track_hash(2));
        assert_eq!(ContentHash::from_hex(&legacy_race().hex()), Some(legacy_race()));
        assert_eq!(ContentHash::from_hex("AF"), None);
    }
}
