//! The emulated C6 loses bytes inside a packed frame the way the real one did
//! — **under a hypothesis** — and the IN-endpoint gate stops it.
//!
//! On a desk XIAO ESP32-C6, before PR #795's gate, ~4 of ~1,400 packed frames
//! arrived a few bytes short; with the gate, 0 of 1,327
//! (`docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md`).
//! The link model (`lp_emu_esp_common::ip::usb_sj`) never lost a byte with or
//! without the gate: after boot the C6 has one writer on the IN endpoint, and
//! the model raises `serial_in_empty` and returns `serial_in_ep_data_free` at
//! the same cycle, so esp-hal's write future always wakes onto a free buffer.
//!
//! The condition the model was missing is **a gap between those two**: the
//! drain's `serial_in_empty` edge arriving before the buffer is writable
//! again (the model's *free lag*, `--usb-in-free-lag <ns>`). esp-hal's
//! `write_async` writes a frame's next 64-byte packet the moment its future
//! wakes, with no free check, so its first few bytes land inside the lag and
//! are refused — a loss of a few bytes, not a packet, with nothing logged.
//! The gate reads `serial_in_ep_data_free` before every packet, later on its
//! own path, and does not write until the buffer is free.
//!
//! ⚠️ The lag is a **hypothesis**, off by default. No document gives silicon
//! such a gap, and nobody has measured one. What it has going for it: it is
//! the one single-writer path to the symptom's *shape* (a short frame, a few
//! bytes, silent), and ESP-IDF's own driver does not trust the edge either —
//! its ISR re-checks the FIFO is writable after `SERIAL_IN_EMPTY` and ignores
//! the interrupt if not. The model's docs record it that way.
//!
//! The pair, same lag, same conversation:
//!
//! 1. the image **without** the gate (`FwImage::NO_IN_ENDPOINT_GATE`, the
//!    firmware's `fixture-no-in-endpoint-gate`) tears packed frames, and
//!    each loss is shorter than a packet;
//! 2. the **shipped** image, gate in, delivers every frame whole, and its
//!    waits stay short of the firmware's 250 ms chunk timeout;
//! 3. with the lag off, the ungated image loses nothing — the model's
//!    default still has no path to this loss, which is the finding.
//!
//! It lives in `lp-cli` for the reason `emu_usb_json_pack.rs` does: the
//! requests are framed and the frames decoded by `lpc-wire` and
//! `lp-json-pack`, which nothing under `lp-emu/` may depend on.
//!
//! `#[ignore]`d and run by `just test-emu-c6`: it needs two built
//! `fw-esp32c6` ELFs (`LP_EMU_BUILD_FW=1`) and builds the emulator in
//! release.

use std::process::Command;

use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lp_json_pack::{DropReason, FRAME_KIND_PACK, FrameScanner, ScanEvent, VecFrameBuffer};
use lpc_wire::json::to_serial_line;
use lpc_wire::message::client::{ClientMessage, ClientRequest};
use lpc_wire::{WIRE_DICTIONARY_FINGERPRINT, WireEncoding};

/// The free lag the pair runs at, in emulated nanoseconds. `LP_EMU_FREE_LAG_NS`
/// overrides it, for sweeping.
const FREE_LAG_NS: u64 = 1_000;

/// The conversation, by emulated millisecond: the opt-in, then a Hello every
/// [`EVERY_MS`]. A packed Hello is three packets, so every reply gives
/// esp-hal's loop two wakes to write straight into.
const OPT_IN_AT_MS: u64 = 1_500;
const FIRST_AT_MS: u64 = 2_000;
const EVERY_MS: u64 = 20;
const REQUESTS: u64 = 40;
const OPT_IN_ID: u64 = 1;
const FIRST_ID: u64 = 10;

#[test]
#[ignore = "needs two built fw-esp32c6 ELFs and a release emulator; `just test-emu-c6` runs it"]
fn without_the_gate_a_free_lag_tears_packed_frames_by_a_few_bytes() {
    let Some(elf) = image(&FwImage::NO_IN_ENDPOINT_GATE) else {
        return;
    };
    let run = converse(&elf, free_lag_ns());
    let scan = scan(&run.delivered);
    eprintln!("free lag, no gate: {}", run.summary(&scan));
    assert!(
        scan.torn > 0,
        "no packed frame was torn: {}",
        run.summary(&scan)
    );
    assert!(!run.tried.is_empty(), "no byte was refused");
    // A few bytes per loss, not a packet: the silicon symptom's shape.
    let per_loss = run.tried.len() / scan.torn;
    assert!(
        per_loss < 64,
        "{per_loss} refused bytes per torn frame — a whole packet or more: {}",
        run.summary(&scan)
    );
    assert!(
        run.stderr.contains("inside the free lag"),
        "the refusals were not the lag's:\n{}",
        run.stderr
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and a release emulator; `just test-emu-c6` runs it"]
fn with_the_gate_the_same_free_lag_loses_nothing() {
    let Some(elf) = image(&FwImage::SHIPPED) else {
        return;
    };
    let run = converse(&elf, free_lag_ns());
    let scan = scan(&run.delivered);
    eprintln!("free lag, gated: {}", run.summary(&scan));
    assert_eq!(scan.torn, 0, "{}", run.summary(&scan));
    assert!(
        run.tried.is_empty(),
        "{} bytes refused",
        run.tried.len()
    );
    assert_eq!(
        scan.replies,
        REQUESTS as usize,
        "every Hello answered, packed: {}",
        run.summary(&scan)
    );
    assert!(
        !String::from_utf8_lossy(&run.delivered).contains("timed out"),
        "the gate waited out a chunk timeout"
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and a release emulator; `just test-emu-c6` runs it"]
fn without_the_gate_and_without_the_lag_the_model_loses_nothing() {
    let Some(elf) = image(&FwImage::NO_IN_ENDPOINT_GATE) else {
        return;
    };
    let run = converse(&elf, 0);
    let scan = scan(&run.delivered);
    eprintln!("no lag, no gate: {}", run.summary(&scan));
    assert_eq!(scan.torn, 0, "{}", run.summary(&scan));
    assert!(run.tried.is_empty(), "{} bytes refused", run.tried.len());
    assert_eq!(scan.replies, REQUESTS as usize, "{}", run.summary(&scan));
}

fn free_lag_ns() -> u64 {
    std::env::var("LP_EMU_FREE_LAG_NS")
        .ok()
        .map(|v| v.parse().expect("LP_EMU_FREE_LAG_NS: nanoseconds"))
        .unwrap_or(FREE_LAG_NS)
}

fn image(image: &FwImage) -> Option<std::path::PathBuf> {
    match fw_esp32c6_image(image) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_usb_free_lag: skipped — {reason}");
            None
        }
    }
}

/// What one run left behind.
struct Run {
    /// The `usb-sj` stream: what the host received.
    delivered: Vec<u8>,
    /// The observation stream: bytes the guest wrote and the block refused.
    tried: Vec<u8>,
    stderr: String,
}

impl Run {
    fn summary(&self, scan: &Scan) -> String {
        format!(
            "{} B delivered, {} B refused, {} packed frames whole, {} torn, {} of {REQUESTS} \
             Hellos answered",
            self.delivered.len(),
            self.tried.len(),
            scan.whole,
            scan.torn,
            scan.replies
        )
    }
}

/// The conversation on `elf` with the model's free lag at `lag_ns`.
fn converse(elf: &std::path::Path, lag_ns: u64) -> Run {
    let dir = tempfile::tempdir().expect("a temp dir");
    let script_path = dir.path().join("free-lag.usb-script");
    let delivered = dir.path().join("delivered.bin");
    let tried = dir.path().join("tried.bin");

    // Each request framed by the single framer, written as the hex bytes an
    // emulator script carries (see `emu_usb_hello.rs` for why hex).
    let send = |ms: u64, id: u64, msg: ClientRequest| {
        let line = to_serial_line(&ClientMessage { id, msg }).expect("framing a request");
        let hex: Vec<String> = line.bytes().map(|b| format!("{b:02x}")).collect();
        format!("{ms}  {}\n", hex.join(" "))
    };
    let mut script = String::from("0  attach\n0  open\n");
    script += &send(
        OPT_IN_AT_MS,
        OPT_IN_ID,
        ClientRequest::SetEncoding {
            encoding: WireEncoding::Packed,
            dictionary: WIRE_DICTIONARY_FINGERPRINT,
        },
    );
    for n in 0..REQUESTS {
        script += &send(FIRST_AT_MS + n * EVERY_MS, FIRST_ID + n, ClientRequest::Hello);
    }
    std::fs::write(&script_path, &script).expect("writing the script");

    let end_ms = FIRST_AT_MS + REQUESTS * EVERY_MS + 500;
    let output = Command::new("cargo")
        .args(["run", "-q", "-p", "lp-emu-esp32c6", "--release", "--"])
        .args(["--elf", elf.to_str().expect("a utf-8 path")])
        .args(["--usb-host", "attached"])
        .args(["--usb-script", script_path.to_str().expect("a utf-8 path")])
        .args(["--usb-sj", &format!("file:{}", delivered.display())])
        .args(["--usb-sj-tried", &format!("file:{}", tried.display())])
        .args(["--usb-in-free-lag", &lag_ns.to_string()])
        .args(["--timeout", &format!("{end_ms}ms"), "--wall-timeout", "300"])
        .arg("--strict-bus")
        .output()
        .expect("running lp-emu-esp32c6");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stderr.contains("emulated timeout reached, no fault"),
        "the emulator did not run to its emulated timeout ({:?})\n{stderr}",
        output.status.code()
    );
    Run {
        delivered: std::fs::read(&delivered).expect("reading the capture"),
        tried: std::fs::read(&tried).unwrap_or_default(),
        stderr,
    }
}

/// The packed frames on the link, as a host reader counts them.
struct Scan {
    whole: usize,
    torn: usize,
    /// Distinct Hello replies to this conversation's requests, in a packed
    /// frame that decoded.
    replies: usize,
}

fn scan(bytes: &[u8]) -> Scan {
    let mut whole = 0;
    let mut torn = 0;
    let mut ids = std::collections::BTreeSet::new();
    let mut scanner = FrameScanner::new(VecFrameBuffer::new(64 * 1024));
    scanner.push(bytes, |event| match event {
        ScanEvent::Text(_) => {}
        ScanEvent::Frame { kind, payload } => {
            assert_eq!(kind, FRAME_KIND_PACK);
            match lpc_wire::decode_packed_to_json(payload) {
                Ok(json) => {
                    whole += 1;
                    let message: lpc_wire::WireServerMessage =
                        lpc_wire::json::from_str(&json).expect("a decoded frame parses");
                    if (FIRST_ID..FIRST_ID + REQUESTS).contains(&message.id) {
                        ids.insert(message.id);
                    }
                }
                Err(_) => torn += 1,
            }
        }
        ScanEvent::Dropped(DropReason::BadCobs) => torn += 1,
        ScanEvent::Dropped(reason) => panic!("a frame was dropped: {reason:?}"),
    });
    Scan {
        whole,
        torn,
        replies: ids.len(),
    }
}
