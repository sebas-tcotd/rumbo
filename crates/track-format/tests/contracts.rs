use track_format::{
    codec,
    hash::{content_hash, Fnv64},
};
#[test]
fn legacy_string_protocols() {
    assert_eq!(
        Fnv64::new().str_u32_len("legacy/1").hex(),
        "4b0525974c61ba5e"
    );
    assert_eq!(
        Fnv64::new().str_u64_len("legacy/1").hex(),
        "c46940799d17a32e"
    );
}
#[test]
fn content_and_envelope() {
    assert_eq!(content_hash(b"").hex(), "af1349b9f5f9a1a6a0404dea36dcc949");
    let bytes = codec::encode(&42u32).unwrap();
    assert_eq!(bytes, b"RTF\0\x01\0*");
    assert_eq!(codec::decode::<u32>(&bytes).unwrap(), 42);
    assert!(codec::decode::<u32>(&bytes[..6]).is_err());
    let mut invalid = bytes.clone();
    invalid[4] = 2;
    assert!(codec::decode::<u32>(&invalid).is_err());
    invalid = bytes.clone();
    invalid[0] = 0;
    assert!(codec::decode::<u32>(&invalid).is_err());
    invalid = bytes;
    invalid.push(0);
    assert!(codec::decode::<u32>(&invalid).is_err());
}

#[test]
fn numeric_bits_and_order() {
    let mut h = Fnv64::new();
    h.u32(0x12345678)
        .u64(0x0102030405060708)
        .i64(-123456789)
        .f32(f32::from_bits(0x80000000))
        .f64(f64::from_bits(0x7ff8000000000042));
    assert_eq!(h.hex(), "31493008a9baf753");
    assert_eq!(Fnv64::new().bytes(b"hello").hex(), "a430d84680aabd0b");
    assert_ne!(
        Fnv64::new().f32(0.0).finish(),
        Fnv64::new().f32(-0.0).finish()
    );
}

#[test]
fn framing_errors_are_explicit() {
    for n in 0..6 {
        assert_eq!(
            codec::decode::<u32>(&b"RTF\0\x01\0*"[..n]),
            Err(codec::Error::TruncatedHeader)
        );
    }
    assert_eq!(
        codec::decode::<u32>(b"RTF\0\x02\0"),
        Err(codec::Error::UnsupportedVersion(2))
    );
    assert_eq!(
        codec::decode::<bool>(b"RTF\0\x01\0\x02"),
        Err(codec::Error::Payload(postcard::Error::DeserializeBadBool))
    );
    assert!(matches!(
        codec::decode::<u32>(b"RTF\0\x01\0\x80"),
        Err(codec::Error::Payload(_))
    ));
    assert_eq!(
        content_hash(b"abc").hex(),
        "6437b3ac38465133ffb63b75273a8db5"
    );
}
