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
//! One test, three steps, each running the ungated image
//! (`FwImage::NO_IN_ENDPOINT_GATE`, the firmware's
//! `fixture-no-in-endpoint-gate`) beside the shipped, gated one through the
//! same conversation:
//!
//! 1. **no lag**: neither loses a byte, which is the finding that the model's
//!    default has no path to this loss. The block measures how soon after a
//!    drain each image touches the endpoint: the ungated one's next `ep1`
//!    write, the gated one's next `ep1_conf` read;
//! 2. **the timing condition**: the ungated write comes sooner than the gated
//!    check;
//! 3. **a lag between the two**: the ungated image tears packed frames, by
//!    less than a packet each; the gated image delivers every frame and never
//!    waits out a chunk timeout.
//!
//! The lag is chosen from step 1's measurements, not written down, so a
//! firmware change that moves either path moves the lag with it. Step 2 is
//! the one that must keep holding.
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
use lpc_wire::json::to_serial_line;
use lpc_wire::message::client::{ClientMessage, ClientRequest};
use lpc_wire::{PACK_FORMAT_VERSION, WireEncoding};

/// The conversation, by emulated millisecond: the opt-in, the free lag set
/// (which also restarts the block's wake measurements, so boot is not in
/// them), then a Hello every [`EVERY_MS`]. A packed Hello is three packets,
/// so every reply gives esp-hal's loop two wakes to write straight into.
const OPT_IN_AT_MS: u64 = 1_500;
const LAG_AT_MS: u64 = 1_900;
const FIRST_AT_MS: u64 = 2_000;
const EVERY_MS: u64 = 20;
const REQUESTS: u64 = 40;
const OPT_IN_ID: u64 = 1;
const FIRST_ID: u64 = 10;

#[test]
#[ignore = "needs two built fw-esp32c6 ELFs and a release emulator; `just test-emu-c6` runs it"]
fn a_free_lag_between_esp_hals_write_and_the_gates_check_tears_only_the_ungated_image() {
    let (Some(ungated), Some(gated)) = (
        image(&FwImage::NO_IN_ENDPOINT_GATE),
        image(&FwImage::SHIPPED),
    ) else {
        return;
    };

    // 1. No lag: neither image loses a byte (the model's default has no
    //    path to this loss), and each says how soon it touches the
    //    endpoint after a drain.
    let (before, after) = both(&ungated, &gated, 0);
    for (name, run) in [("ungated", &before), ("gated", &after)] {
        let scan = scan(&run.delivered);
        eprintln!(
            "no lag, {name}: {} | {}",
            run.summary(&scan),
            run.wake_line()
        );
        assert_eq!(scan.torn, 0, "{name}: {}", run.summary(&scan));
        assert!(
            run.tried.is_empty(),
            "{name}: {} B refused",
            run.tried.len()
        );
        assert_eq!(
            scan.replies,
            REQUESTS as usize,
            "{name}: {}",
            run.summary(&scan)
        );
    }

    // 2. The timing condition: esp-hal writes a frame's next packet sooner
    //    after the drain than the gate reads `serial_in_ep_data_free`.
    let write = before
        .span("ep1 write")
        .expect("the ungated image wrote after a drain");
    let check = after
        .span("ep1_conf read")
        .expect("the gated image checked the buffer after a drain");
    // (The ungated image reads `ep1_conf` too, later: esp-hal's RX drain
    // tests `serial_out_ep_data_avail` in the same register. Not a free
    // check, so its `ep1_conf` span is not the comparison.)
    eprintln!(
        "esp-hal writes {write} ns after a drain at the soonest; the gate checks at {check} ns"
    );
    assert!(
        write < check,
        "esp-hal's write ({write} ns) is not sooner than the gate's check ({check} ns)"
    );

    // 3. A lag between the two: the ungated image tears packed frames by a
    //    few bytes; the gated one delivers every frame and never waits one
    //    out.
    let lag = (write + check) / 2;
    let (before, after) = both(&ungated, &gated, lag);

    let scan_before = scan(&before.delivered);
    eprintln!(
        "free lag {lag} ns, ungated: {} | {}",
        before.summary(&scan_before),
        before.wake_line()
    );
    assert!(
        scan_before.torn > 0,
        "no packed frame was torn: {}",
        before.summary(&scan_before)
    );
    // Bytes lost per torn Hello: a few, not a packet (silicon: ~5). Counted
    // from the torn frames themselves: with learned tables a torn frame
    // that taught the board a name also costs the frames after it (dropped
    // as out of step, `scan.desynced`), and those lost no bytes.
    let damaged = scan_before.torn;
    let per_loss = before.tried.len() / damaged.max(1);
    eprintln!("{damaged} Hellos damaged, {per_loss} bytes lost from each");
    assert!(
        (1..64).contains(&per_loss),
        "{per_loss} refused bytes per damaged Hello — not a partial packet: {}",
        before.summary(&scan_before)
    );

    let scan_after = scan(&after.delivered);
    eprintln!(
        "free lag {lag} ns, gated: {} | {}",
        after.summary(&scan_after),
        after.wake_line()
    );
    assert_eq!(scan_after.torn, 0, "{}", after.summary(&scan_after));
    assert_eq!(scan_after.desynced, 0, "{}", after.summary(&scan_after));
    assert!(after.tried.is_empty(), "{} B refused", after.tried.len());
    assert_eq!(
        scan_after.replies,
        REQUESTS as usize,
        "{}",
        after.summary(&scan_after)
    );
    assert!(
        !String::from_utf8_lossy(&after.delivered).contains("timed out"),
        "the gate waited out a chunk timeout"
    );
}

/// The ungated and the gated image, side by side, at one lag.
fn both(ungated: &std::path::Path, gated: &std::path::Path, lag_ns: u64) -> (Run, Run) {
    std::thread::scope(|s| {
        let a = s.spawn(|| converse(ungated, lag_ns));
        let b = s.spawn(|| converse(gated, lag_ns));
        (a.join().unwrap(), b.join().unwrap())
    })
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
    /// The emulator's wake line (`usb-sj: after N drains: …`).
    fn wake_line(&self) -> &str {
        self.stderr
            .lines()
            .find(|l| l.starts_with("usb-sj: after "))
            .unwrap_or("no wake line")
    }

    /// The soonest `next <what>` after a drain, in ns, from the wake line.
    fn span(&self, what: &str) -> Option<u64> {
        let line = self.wake_line();
        let rest = &line[line.find(&format!("next {what} "))? + what.len() + 6..];
        rest.split("..").next()?.parse().ok()
    }

    fn summary(&self, scan: &Scan) -> String {
        format!(
            "{} B delivered, {} B refused, {} packed frames whole, {} torn ({} bad COBS, {} \
             undecodable), {} dropped out of step, {} of {REQUESTS} Hellos answered",
            self.delivered.len(),
            self.tried.len(),
            scan.whole,
            scan.torn,
            scan.bad_cobs,
            scan.undecodable,
            scan.desynced,
            scan.replies
        )
    }
}

/// The conversation on `elf`, the model's free lag set to `lag_ns` at
/// [`LAG_AT_MS`].
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
    script += &format!("{LAG_AT_MS}  free-lag {lag_ns}\n");
    script += &send(
        OPT_IN_AT_MS,
        OPT_IN_ID,
        ClientRequest::SetEncoding {
            encoding: WireEncoding::Packed,
            format: PACK_FORMAT_VERSION,
        },
    );
    for n in 0..REQUESTS {
        script += &send(
            FIRST_AT_MS + n * EVERY_MS,
            FIRST_ID + n,
            ClientRequest::Hello,
        );
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
    /// Frames whose body was not valid COBS.
    bad_cobs: usize,
    /// Frames that were valid COBS and did not decode.
    undecodable: usize,
    /// `bad_cobs + undecodable`.
    torn: usize,
    /// Whole frames dropped because the reader's learned table was out of
    /// step after an earlier tear (this conversation never re-asks).
    desynced: usize,
    /// Distinct Hello replies to this conversation's requests, in a packed
    /// frame that decoded.
    replies: usize,
}

fn scan(bytes: &[u8]) -> Scan {
    let mut whole = 0;
    let mut bad_cobs = 0;
    let mut undecodable = 0;
    let mut desynced = 0;
    let mut ids = std::collections::BTreeSet::new();
    // One reader for the whole capture: it holds the link's learned table.
    lpc_wire::WireStream::new().push(bytes, |chunk| match chunk {
        lpc_wire::WireChunk::Line(_) => {}
        lpc_wire::WireChunk::Frame(frame) if frame.is_packed() => {
            whole += 1;
            let message: lpc_wire::WireServerMessage =
                lpc_wire::json::from_str(&frame.json).expect("a decoded frame parses");
            if (FIRST_ID..FIRST_ID + REQUESTS).contains(&message.id) {
                ids.insert(message.id);
            }
        }
        lpc_wire::WireChunk::Frame(_) => {}
        lpc_wire::WireChunk::Error(error) if error.contains("not valid COBS") => bad_cobs += 1,
        lpc_wire::WireChunk::Error(error) if error.contains("did not decode") => undecodable += 1,
        lpc_wire::WireChunk::Error(error) => panic!("a frame was dropped: {error}"),
        lpc_wire::WireChunk::Desync(_) => desynced += 1,
    });
    Scan {
        whole,
        bad_cobs,
        undecodable,
        torn: bad_cobs + undecodable,
        desynced,
        replies: ids.len(),
    }
}
