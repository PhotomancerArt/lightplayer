//! One encoded example of every device-leg frame, as hex, so a change to
//! the encoding shows up in review. These bytes are what fielded boards
//! speak: a change here is a `RELAY_PROTO_VERSION` bump (and, while boards
//! in the field speak the old one, the hub keeps it), never a golden edit.

use lpc_relay::{LanAddress, RefuseReason, RelayFrame, RelayHello, RouteCloseReason};

#[test]
fn every_frame_kind_encodes_to_its_golden_bytes() {
    let cases: Vec<(RelayFrame, String)> = vec![
        (
            RelayFrame::Hello(RelayHello::new(
                [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30],
                "Lamp",
                39,
                Some(LanAddress {
                    ip: [192, 168, 4, 20],
                    port: 80,
                }),
                vec![[0x22; 16], [0x33; 16]],
            )),
            format!(
                "01 0100 10bda3b08e30 27000000 01c0a804145000 044c616d70 02{}{}",
                "22".repeat(16),
                "33".repeat(16)
            ),
        ),
        (
            RelayFrame::Hello(RelayHello::new([1; 6], "", 1, None, vec![])),
            "01 0100 010101010101 01000000 00 00 00".into(),
        ),
        (
            RelayFrame::Challenge { nonce: [0x5a; 32] },
            format!("02 {}", "5a".repeat(32)),
        ),
        (
            RelayFrame::Proof {
                proofs: vec![[1; 32], [2; 32]],
            },
            format!("03 02 {}{}", "01".repeat(32), "02".repeat(32)),
        ),
        (
            RelayFrame::Registered {
                accounts_ok: 0b11,
                ping_s: 25,
            },
            "04 03 1900".into(),
        ),
        (
            RelayFrame::Refused {
                reason: RefuseReason::UnknownAccount,
                retry_after_s: 30,
            },
            "05 01 1e00".into(),
        ),
        (RelayFrame::Open { route: 7 }, "06 0700".into()),
        (
            RelayFrame::Frame {
                route: 0x0102,
                bytes: vec![0xa5, 0xff],
            },
            "07 0201 a5ff".into(),
        ),
        (
            RelayFrame::Close {
                route: 7,
                reason: RouteCloseReason::Busy,
            },
            "08 0700 01".into(),
        ),
        (
            RelayFrame::LanChanged {
                lan: Some(LanAddress {
                    ip: [10, 0, 0, 9],
                    port: 8080,
                }),
            },
            "09 01 0a000009 901f".into(),
        ),
        (RelayFrame::LanChanged { lan: None }, "09 00".into()),
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

#[test]
fn every_refusal_reason_has_its_golden_code() {
    let codes = [
        (RefuseReason::UnknownAccount, 1),
        (RefuseReason::VersionTooOld, 2),
        (RefuseReason::VersionTooNew, 3),
        (RefuseReason::TooManyBoards, 4),
        (RefuseReason::Malformed, 5),
        (RefuseReason::Busy, 6),
    ];
    for (reason, code) in codes {
        assert_eq!(reason.code(), code, "{reason}");
    }
    for (reason, code) in [
        (RouteCloseReason::Normal, 0),
        (RouteCloseReason::Busy, 1),
        (RouteCloseReason::Gone, 2),
    ] {
        assert_eq!(reason.code(), code, "{reason}");
    }
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
