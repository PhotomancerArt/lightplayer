//! Relay protocol 2's bytes, as hex, so a change to them shows up in
//! review: the hello's firmware tail, `Project` (`0x0a`), `Picture`
//! (`0x0b`) and `PictureRate` (`0x0c`), and the project tags' derivation.
//! These bytes are what protocol 2 cores speak: once one is released, a
//! change here is a protocol 3, never a golden edit.
//!
//! Protocol 1's bytes live in `relay_frame_golden.rs`, which is never
//! edited; this file only reads one of its hellos, copied as a literal.

use hmac::{Hmac, Mac};
use lpc_relay::{
    LanAddress, PictureRate, RELAY_PROTO_1, RelayFrame, RelayHello, RelayPicture, RelayProject,
    frame_protocol, project_content_tag, project_tag_key, project_uid_tag,
};
use sha2::Sha256;

#[test]
fn every_protocol_2_frame_kind_encodes_to_its_golden_bytes() {
    let cases: Vec<(RelayFrame, String)> = vec![
        (
            RelayFrame::Hello(
                RelayHello::new(
                    [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30],
                    "Lamp",
                    39,
                    Some(LanAddress {
                        ip: [192, 168, 4, 20],
                        port: 80,
                    }),
                    vec![[0x22; 16], [0x33; 16]],
                )
                .with_firmware("2026.10.09-1"),
            ),
            format!(
                "01 0200 10bda3b08e30 27000000 01c0a804145000 044c616d70 02{}{} 0c 323032362e31302e30392d31",
                "22".repeat(16),
                "33".repeat(16)
            ),
        ),
        (
            RelayFrame::Hello(RelayHello::new([1; 6], "", 1, None, vec![]).with_firmware("")),
            "01 0200 010101010101 01000000 00 00 00 00".into(),
        ),
        (RelayFrame::Project(None), "0a 00".into()),
        (
            RelayFrame::Project(Some(RelayProject {
                name: "Rocaille".into(),
                uid_tag: Some(tag16("6ff9f3a914cbbab7b4f4392be259d74f")),
                content_tag: Some(tag16("6ddafa9fb17ba36b634e65ca8c3e90d1")),
            })),
            "0a 01 08 526f6361696c6c65 01 6ff9f3a914cbbab7b4f4392be259d74f 01 6ddafa9fb17ba36b634e65ca8c3e90d1"
                .into(),
        ),
        (
            RelayFrame::Project(Some(RelayProject {
                name: "Rocaille".into(),
                uid_tag: None,
                content_tag: None,
            })),
            "0a 01 08 526f6361696c6c65 00 00".into(),
        ),
        (
            RelayFrame::Picture(RelayPicture {
                outputs: vec![],
                colors: vec![],
            }),
            "0b 00 0000".into(),
        ),
        (
            RelayFrame::Picture(RelayPicture {
                outputs: vec![3],
                colors: vec![0xff, 0, 0, 0, 0xff, 0, 0, 0, 0xff],
            }),
            "0b 01 03000000 0300 ff0000 00ff00 0000ff".into(),
        ),
        (
            RelayFrame::Picture(RelayPicture {
                outputs: vec![5, 3],
                colors: vec![
                    0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0,
                ],
            }),
            "0b 02 05000000 03000000 0400 102030 405060 708090 a0b0c0".into(),
        ),
        (
            RelayFrame::PictureRate(PictureRate {
                idle_s: 60,
                watched_ms: 500,
                watched_for_s: 0,
            }),
            "0c 3c00 f401 0000".into(),
        ),
        (
            RelayFrame::PictureRate(PictureRate {
                idle_s: 60,
                watched_ms: 500,
                watched_for_s: 15,
            }),
            "0c 3c00 f401 0f00".into(),
        ),
    ];

    for (frame, golden) in cases {
        let golden = golden.replace(' ', "");
        assert_eq!(hex(&frame.encode()), golden, "{frame}");
        assert_eq!(
            RelayFrame::decode(&unhex(&golden)),
            Ok(frame.clone()),
            "{frame}"
        );
    }
}

/// The tags' vectors, pinned. Each expected value was computed with an
/// HMAC that is not the code under test (RustCrypto's `hmac`, checked
/// again below, and Python's `hmac` when the vectors were first written).
#[test]
fn the_project_tags_derive_to_their_golden_bytes() {
    let k = [0x42; 32];
    let uid = "prj7m3qk2x9z4w8v6t5r1n0p2a4c";
    let package_hash = [0x11; 32];

    let tag_key_golden = "066b272f5f4586f6386c24a5f82424f1a011a96b840e68e3225fc037a9d3bd3e";
    let uid_tag_golden = "6ff9f3a914cbbab7b4f4392be259d74f";
    let content_tag_golden = "6ddafa9fb17ba36b634e65ca8c3e90d1";

    // The oracle agrees with the pinned hex…
    let oracle_key = oracle_hmac(&k, &[b"lp-relay project/1"]);
    assert_eq!(hex(&oracle_key), tag_key_golden);
    assert_eq!(
        hex(&oracle_hmac(&oracle_key, &[b"uid\0", uid.as_bytes()])[..16]),
        uid_tag_golden
    );
    assert_eq!(
        hex(&oracle_hmac(&oracle_key, &[b"content\0", &package_hash])[..16]),
        content_tag_golden
    );

    // …and so does the code under test.
    let tag_key = project_tag_key(&k);
    assert_eq!(hex(&tag_key), tag_key_golden);
    assert_eq!(hex(&project_uid_tag(&tag_key, uid)), uid_tag_golden);
    assert_eq!(
        hex(&project_content_tag(&tag_key, &package_hash)),
        content_tag_golden
    );
}

/// Protocol 1's hellos, copied as literals from `relay_frame_golden.rs`
/// (never imported, never touched): a protocol 2 decoder reads them as
/// protocol 1 hellos with no firmware.
#[test]
fn protocol_1_golden_hellos_decode_as_protocol_1_with_no_firmware() {
    for golden in [
        format!(
            "01 0100 10bda3b08e30 27000000 01c0a804145000 044c616d70 02{}{}",
            "22".repeat(16),
            "33".repeat(16)
        ),
        "01 0100 010101010101 01000000 00 00 00".to_string(),
    ] {
        let bytes = unhex(&golden.replace(' ', ""));
        let Ok(RelayFrame::Hello(hello)) = RelayFrame::decode(&bytes) else {
            panic!("not a hello: {golden}");
        };
        assert_eq!(hello.relay_proto, RELAY_PROTO_1);
        assert_eq!(hello.relay_proto, 1);
        assert_eq!(hello.firmware, None);
        assert_eq!(RelayFrame::Hello(hello).encode(), bytes, "re-encodes as is");
    }
}

#[test]
fn every_tag_has_its_golden_protocol() {
    for tag in 0x01..=0x09u8 {
        assert_eq!(frame_protocol(&[tag]), Some(1), "{tag:#04x}");
    }
    for tag in 0x0a..=0x0cu8 {
        assert_eq!(frame_protocol(&[tag]), Some(2), "{tag:#04x}");
    }
    for tag in [0x00, 0x0d, 0x7f, 0xff] {
        assert_eq!(frame_protocol(&[tag]), None, "{tag:#04x}");
    }
    assert_eq!(frame_protocol(&[]), None);
}

fn oracle_hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    for part in parts {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}

fn tag16(text: &str) -> [u8; 16] {
    unhex(text).try_into().unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
