//! **The update protocol v1 golden transcript.** `v1_golden.hex` is one
//! fixed exchange, every message type both ways, one message per line as
//! `<dir> <type> <hex of the bytes after the type byte>`.
//!
//! A mismatch is a protocol break: **never re-capture it.** A new protocol
//! version gets a new golden beside this one; v1's stays, because fielded
//! cores speak v1 for as long as they exist. `lpa-update`'s
//! `v1_board_fixture.rs` drives the host side against the board lines here.

use lpc_access::LoginOffer;
use lpc_update::{
    BoardLoginStep, BoardManifest, BoardMessage, BoardState, ChunkEncoding, ChunkRef,
    HostLoginStep, HostMessage, Mismatch, Offer, PieceKind, ReadBackRequest, Refusal, Request,
    build_id_field,
};

/// Who sent a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    HostToBoard,
    BoardToHost,
}

/// The exchange the golden records, as values, in order.
fn exchange() -> Vec<(Dir, Vec<u8>)> {
    use Dir::{BoardToHost as B, HostToBoard as H};
    let manifest = BoardManifest {
        proto: 1,
        target: "esp32c6-4mb".into(),
        chip: "esp32c6".into(),
        version: "2026.10.05-3".into(),
        build_id: "2026.10.05-3+abc123456789".into(),
        wire_proto: 36,
        core_sha256: "11".repeat(32),
        core_len: 1_160_000,
        engine_sha256: "22".repeat(32),
        engine_len: Some(1_830_000),
        layout: 1,
        loader: 1,
        region_len: 3_375_104,
        state: BoardState::Running,
        refused_build: None,
        transfer: None,
    };
    let manifest_json = manifest.to_json();
    let offer = Offer {
        proto: 1,
        flags: 0,
        chip: 1,
        layout: 1,
        min_loader: 1,
        core_len: 1_160_000,
        engine_len: 1_830_000,
        core_sha256: [0x33; 32],
        engine_sha256: [0x44; 32],
        build_id: build_id_field(b"2026.10.06-1+def456789012").unwrap(),
    };
    // A real raw-deflate stream: one stored block holding "abc".
    let z = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
    vec![
        (H, HostMessage::Query { proto: 1 }.encode()),
        (B, BoardMessage::Manifest(&manifest_json).encode()),
        (H, HostMessage::Offer(offer).encode()),
        (
            B,
            BoardMessage::Request(Request {
                kind: PieceKind::Core,
                off: 0,
                len: 4096,
                flags: 1,
            })
            .encode(),
        ),
        (
            B,
            BoardMessage::Request(Request {
                kind: PieceKind::Core,
                off: 4096,
                len: 4096,
                flags: 0,
            })
            .encode(),
        ),
        (
            H,
            HostMessage::Chunk(ChunkRef {
                encoding: ChunkEncoding::Encoding1,
                kind: PieceKind::Core,
                off: 0,
                payload: &z,
            })
            .encode(),
        ),
        (
            H,
            HostMessage::Chunk(ChunkRef {
                encoding: ChunkEncoding::Raw,
                kind: PieceKind::Core,
                off: 4096,
                payload: &[0, 1, 2, 3, 4, 5, 6, 7],
            })
            .encode(),
        ),
        (
            B,
            Refusal::FailedBuild {
                build_hash: 0xdead_beef,
            }
            .encode(),
        ),
        (B, Refusal::Access.encode()),
        (
            B,
            Refusal::DoesNotFit {
                need: 3_000_000,
                room: 2_000_000,
            }
            .encode(),
        ),
        (
            B,
            Refusal::Incompatible {
                what: Mismatch::Loader,
                have: 0,
                need: 1,
            }
            .encode(),
        ),
        (
            B,
            Refusal::Incompatible {
                what: Mismatch::Flags,
                have: 0,
                need: 0x80,
            }
            .encode(),
        ),
        (
            B,
            Refusal::Busy {
                done: 40_960,
                total: 1_160_000,
            }
            .encode(),
        ),
        (B, Refusal::HashMismatch.encode()),
        (B, Refusal::Untrusted.encode()),
        (B, Refusal::UnknownMessage { ty: b'X' }.encode()),
        (H, HostLoginStep::Begin.encode()),
        (
            B,
            BoardLoginStep::Challenge {
                nonce: [0x5a; 32],
                offers: vec![
                    LoginOffer {
                        salt: [0x01; 16],
                        iterations: 1000,
                    },
                    LoginOffer {
                        salt: [0x02; 16],
                        iterations: 20_000,
                    },
                ],
            }
            .encode(),
        ),
        (
            H,
            HostLoginStep::Answer {
                macs: vec![[0xa1; 32], [0xb2; 32]],
            }
            .encode(),
        ),
        (
            B,
            BoardLoginStep::Verdict {
                tier_code: 2,
                retry_after_ms: 0,
            }
            .encode(),
        ),
        (
            B,
            BoardLoginStep::Verdict {
                tier_code: 0,
                retry_after_ms: 4000,
            }
            .encode(),
        ),
        (
            H,
            ReadBackRequest {
                kind: PieceKind::Engine,
                off: 0,
                len: 4096,
            }
            .encode(),
        ),
        (
            B,
            BoardMessage::Data(ChunkRef {
                encoding: ChunkEncoding::Raw,
                kind: PieceKind::Engine,
                off: 0,
                payload: b"LPEH\x01\x00\x58\x00",
            })
            .encode(),
        ),
    ]
}

#[test]
fn the_codec_reproduces_the_v1_golden_byte_for_byte() {
    let got: Vec<String> = exchange().iter().map(|(d, m)| line(*d, m)).collect();
    let want = golden_lines();
    if got != want {
        panic!(
            "protocol v1 moved — a protocol break, never a golden to re-capture. \
             This run produced:\n{}",
            got.join("\n")
        );
    }
}

#[test]
fn every_golden_line_decodes_to_the_fields_it_was_made_from() {
    let values = exchange();
    let lines = golden_lines();
    assert_eq!(values.len(), lines.len());
    for ((dir, bytes), text) in values.iter().zip(&lines) {
        let (golden_dir, golden_bytes) = parse_line(text);
        assert_eq!(golden_dir, *dir, "{text}");
        assert_eq!(&golden_bytes, bytes, "{text}");
        match dir {
            Dir::HostToBoard => {
                let msg = HostMessage::decode(&golden_bytes).expect(text);
                assert!(!matches!(msg, HostMessage::Unknown { .. }), "{text}");
                assert_eq!(msg.encode(), golden_bytes, "re-encodes: {text}");
            }
            Dir::BoardToHost => {
                let msg = BoardMessage::decode(&golden_bytes).expect(text);
                assert!(!matches!(msg, BoardMessage::Unknown { .. }), "{text}");
                assert_eq!(msg.encode(), golden_bytes, "re-encodes: {text}");
                if let BoardMessage::Manifest(json) = msg {
                    let m = BoardManifest::from_json(json).expect("the golden M is a manifest");
                    assert_eq!(m.target, "esp32c6-4mb");
                    assert_eq!(m.state, BoardState::Running);
                }
            }
        }
    }
}

#[test]
fn the_golden_covers_every_message_and_every_refusal_reason() {
    let lines = golden_lines();
    let types: Vec<(Dir, u8)> = lines
        .iter()
        .map(|l| {
            let (d, b) = parse_line(l);
            (d, b[0])
        })
        .collect();
    for (dir, ty) in [
        (Dir::HostToBoard, b'Q'),
        (Dir::BoardToHost, b'M'),
        (Dir::HostToBoard, b'O'),
        (Dir::BoardToHost, b'R'),
        (Dir::HostToBoard, b'D'),
        (Dir::HostToBoard, b'Z'),
        (Dir::HostToBoard, b'G'),
        (Dir::BoardToHost, b'D'),
        (Dir::BoardToHost, b'N'),
        (Dir::HostToBoard, b'L'),
        (Dir::BoardToHost, b'L'),
    ] {
        assert!(types.contains(&(dir, ty)), "no {} {dir:?}", ty as char);
    }
    let reasons: Vec<u8> = lines
        .iter()
        .map(|l| parse_line(l).1)
        .filter(|b| b[0] == b'N')
        .map(|b| b[1])
        .collect();
    for r in b"FASVBHTU" {
        assert!(reasons.contains(r), "no N/{}", *r as char);
    }
}

fn line(dir: Dir, bytes: &[u8]) -> String {
    let d = match dir {
        Dir::HostToBoard => "H>B",
        Dir::BoardToHost => "B>H",
    };
    let hex: String = bytes[1..].iter().map(|b| format!("{b:02x}")).collect();
    format!("{d} {} {hex}", bytes[0] as char)
}

fn parse_line(text: &str) -> (Dir, Vec<u8>) {
    let mut parts = text.splitn(3, ' ');
    let dir = match parts.next() {
        Some("H>B") => Dir::HostToBoard,
        Some("B>H") => Dir::BoardToHost,
        other => panic!("bad direction {other:?} in {text}"),
    };
    let ty = parts.next().expect("a type").as_bytes();
    assert_eq!(ty.len(), 1, "{text}");
    let hex = parts.next().unwrap_or("");
    let mut bytes = vec![ty[0]];
    for i in (0..hex.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&hex[i..i + 2], 16).expect(text));
    }
    (dir, bytes)
}

fn golden_lines() -> Vec<String> {
    include_str!("v1_golden.hex")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}
