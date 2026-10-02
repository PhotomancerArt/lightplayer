//! `walks/five-wire.script` and the five pads. **M4 P4's second wave** —
//! five wires over four RMT slots on the desk board's own pins, the fifth
//! time-sharing a slot by per-transmission pad muxing (`wire_pusher.rs`),
//! which only the product path exercises: `projects/test/five-wire`
//! (ruling R7) uploaded and rendered — used to run here, on the `frame-dump`
//! image, over the committed `M!` walk.
//!
//! ⚠️ Since wire proto 32 (plan `classic-uart-on-lp-link`) the shipped image
//! speaks lp-link on UART0 and no longer reads `M!` lines, the `[OUT] frame=…
//! crc=` summary lines are log records on the link's log channel, and
//! nothing under `lp-emu/` may host a link (the MIT fence). The walk
//! therefore moved, with every assertion — the routing off the pin log (four
//! pooled signals, one on two pads with a park between), every frame whole,
//! determinism across two runs and two quanta, five distinct lit wires each
//! FNV-1a-equal to a summary line the guest printed, and no frame dropped
//! between the driver and the pad — to `lp-cli/tests/emu_v3_link_gates.rs`'s
//! `five_wires_share_four_slots_and_each_matches_the_guests_own_checksum`,
//! with the report-group reader (`reached`) and the pin-log reader
//! (`routes`) and their tests. The console repair (`deinterleave`) and the
//! UART-byte "line still in flight" reader did not move: on the link every
//! record arrives whole, so a summary line cannot be cut by the deadline.
//!
//! What stays here is what needs no firmware: the committed script's shape
//! (the pre-lp-link record of the walk), the five pads and `RMT_SIG_0`'s
//! out_sel, and the FNV-1a.

use std::path::{Path, PathBuf};

use lp_emu_esp_common::pins::{RouteSource, SignalId};
use lp_emu_esp32v3::control::parse_byte_script;
use lp_emu_esp32v3::periph::rmt;

/// The project's five ports, in the order `output.json` declares them:
/// IO18 / IO16 / IO14 / IO2 are the fused DATA terminals, IO13 the spare
/// (`lp-core/lpc-hardware/boards/domraem/dom-z-102.json`).
const PADS: [u8; 5] = [18, 16, 14, 2, 13];

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("walks")
        .join("five-wire.script")
}

/// FNV-1a, 32-bit — the firmware's own `frame_checksum`, restated inside the
/// `lp-emu/` fence rather than imported across it.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[test]
fn the_five_wire_walk_is_the_twelve_requests_the_client_sends() {
    let text = std::fs::read_to_string(script_path()).expect("committed");
    let afters = text.lines().filter(|l| l.starts_with("after ")).count();
    assert_eq!(afters, 12, "one `after` per request");
    let first = text
        .lines()
        .find(|l| l.starts_with("after "))
        .expect("a first step");
    assert!(
        first.contains("[RECOVERY] boot complete (first frame served)"),
        "{first}"
    );
    // The project is the board's own five labels, authored — never a scratch
    // copy, because `projects/test/five-wire` already names the pins this
    // board has.
    for label in ["IO18", "IO16", "IO14", "IO2", "IO13"] {
        assert!(
            text.contains(&format!("ws281x:local:{label}")),
            "the walk never uploads an output on {label}"
        );
    }
    assert!(parse_byte_script(&text).is_ok());
}

/// The published FNV-1a 32-bit vectors, so this file's copy cannot disagree
/// with the guest's `frame_checksum` while looking healthy.
#[test]
fn fnv1a_matches_the_published_vectors() {
    assert_eq!(fnv1a(b""), 0x811c_9dc5);
    assert_eq!(fnv1a(b"a"), 0xe40c_292c);
    assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
}

/// The five pads are the board's, and `RouteSource`'s signal form is what a
/// routed pad carries — a transcription check that needs no firmware.
#[test]
fn the_five_pads_are_the_boards_own() {
    assert_eq!(PADS, [18, 16, 14, 2, 13]);
    assert_eq!(rmt::RMT_SIG_0, 87, "RMT_SIG_0's out_sel on the classic");
    let route = RouteSource::Signal(SignalId(rmt::RMT_SIG_0), false);
    assert_eq!(route, RouteSource::Signal(SignalId(87), false));
}
