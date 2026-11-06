use track_format::rng::{Pcg32, Subsystem};
#[test]
fn official_reference() {
    let mut r = Pcg32::seeded(42, 54);
    assert_eq!(
        (0..6).map(|_| r.next_u32()).collect::<Vec<_>>(),
        [0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e]
    );
}
#[test]
fn stream_consumption_is_independent() {
    let mut bots = Pcg32::for_subsystem(7, Subsystem::Bots);
    let mut start = Pcg32::for_subsystem(7, Subsystem::Start);
    let mut rules = Pcg32::for_subsystem(7, Subsystem::Rules);
    let mut expected = Pcg32::for_subsystem(7, Subsystem::Start);
    for _ in 0..100 {
        bots.next_u32();
        rules.next_u32();
    }
    assert_eq!(
        (0..20).map(|_| start.next_u32()).collect::<Vec<_>>(),
        (0..20).map(|_| expected.next_u32()).collect::<Vec<_>>()
    );
    assert_eq!(Subsystem::Bots as u64, 1);
    assert_eq!(Subsystem::Start as u64, 2);
    assert_eq!(Subsystem::Rules as u64, 3);
}

#[test]
fn subsystem_vectors_are_frozen() {
    let mut r = Pcg32::for_subsystem(7, Subsystem::Bots);
    assert_eq!(
        (0..4).map(|_| r.next_u32()).collect::<Vec<_>>(),
        [0x840d99ca, 0x12c757de, 0x7481b420, 0xfa13115f]
    );
    let mut r = Pcg32::for_subsystem(7, Subsystem::Start);
    assert_eq!(
        (0..4).map(|_| r.next_u32()).collect::<Vec<_>>(),
        [0x97aef5d4, 0x2ffeb853, 0x99f5940e, 0x94502745]
    );
    let mut r = Pcg32::for_subsystem(7, Subsystem::Rules);
    assert_eq!(
        (0..4).map(|_| r.next_u32()).collect::<Vec<_>>(),
        [0xdb7b2723, 0xbde0071c, 0x62a7a91c, 0x8bc19702]
    );
}
