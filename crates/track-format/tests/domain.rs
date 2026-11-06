use track_format::{
    codec,
    domain::{
        input::InputFrame,
        profile::ProfileFile,
        replay::ReplayManifest,
        track::{NetworkDef, TrackDef},
    },
};
#[test]
fn json_defaults_and_binary_network_options() {
    let base = r#"{"id":"x","name":"x","closed":true,"points":[]}"#;
    let absent = TrackDef::from_json(base).unwrap();
    assert!(absent.network.is_none());
    let null =
        TrackDef::from_json(&base.replace("\"points\":[]", "\"points\":[],\"network\":null"))
            .unwrap();
    assert!(null.network.is_none());
    for mut t in [absent, null] {
        for net in [
            None,
            Some(NetworkDef {
                vertices: vec![
                    [0., 0., 0.],
                    [1., 0., 0.],
                    [0., 0., 1.],
                    [0., 8., 0.],
                    [1., 8., 0.],
                    [0., 8., 1.],
                ],
                triangles: vec![[0, 1, 2], [3, 4, 5]],
                surfaces: vec![0, 0],
                walls: vec![],
                barriers: vec![],
            }),
        ] {
            t.network = net;
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(TrackDef::from_json(&json).unwrap(), t);
            let back: TrackDef = codec::decode(&codec::encode(&t).unwrap()).unwrap();
            assert_eq!(back, t);
            if let Some(n) = back.network {
                assert_eq!(n.triangles[0], [0, 1, 2]);
                assert_eq!(n.triangles[1], [3, 4, 5]);
                assert_eq!(n.vertices[0][0], n.vertices[3][0]);
                assert_eq!(n.vertices[0][2], n.vertices[3][2]);
                assert_eq!(n.vertices[3][1] - n.vertices[0][1], 8.);
            }
        }
    }
}
#[test]
fn input_profile_and_full_replay_roundtrip() {
    let i = InputFrame {
        steer: -127,
        throttle: 200,
        brake: 3,
        flags: 7,
    };
    assert_eq!(
        codec::decode::<InputFrame>(&codec::encode(&i).unwrap()).unwrap(),
        i
    );
    let p = ProfileFile::default();
    assert_eq!(
        codec::decode::<ProfileFile>(&codec::encode(&p).unwrap()).unwrap(),
        p
    );
    let m = ReplayManifest::from_json(include_str!(
        "../../sim-core/tests/golden/test-loop.scripted.json"
    ))
    .unwrap();
    assert_eq!(
        codec::decode::<ReplayManifest>(&codec::encode(&m).unwrap()).unwrap(),
        m
    );
}

#[test]
fn input_wire_fixture_and_legacy_versions() {
    use track_format::version::*;
    assert_eq!(
        (PHYSICS_VERSION, COMPILER_VERSION, TICK_HZ, REPLAY_FORMAT),
        ("0.4.0", "0.2.0", 120, "race-lab-replay/1")
    );
    let input = InputFrame {
        steer: 0,
        throttle: 200,
        brake: 3,
        flags: 7,
    };
    assert_eq!(codec::encode(&input).unwrap(), b"RTF\0\x01\0\0\xc8\x03\x07");
}

#[test]
fn real_source_tracks_roundtrip_without_runtime() {
    for json in [
        include_str!("../../../assets/tracks/test-loop.json"),
        include_str!("../../../assets/tracks/costa-verde-sprint.json"),
        include_str!("../../../assets/tracks/san-borja-las-artes.json"),
    ] {
        let t = TrackDef::from_json(json).unwrap();
        let bytes = codec::encode(&t).unwrap();
        assert_eq!(codec::decode::<TrackDef>(&bytes).unwrap(), t);
        assert!(codec::decode::<TrackDef>(&bytes[..bytes.len() - 1]).is_err());
    }
}
