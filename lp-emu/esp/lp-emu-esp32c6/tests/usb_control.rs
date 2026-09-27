//! M6 P3's gates: the host control channel, on the shipped image minus
//! flash (`esp32c6,server,radio,memory_fs`), strict `t1`.
//!
//! - **G3-1** the s7 shape, scripted: attach + open from cycle zero, the
//!   cable pulled at 6 s, back in at 9 s, the port re-opened at 9.5 s. The
//!   delivered log carries the boot before 6 s, **nothing** between the
//!   detach and the re-open, and the link task's traffic again after it;
//!   while the cable was out the link task discarded what it had to send
//!   (no SOF) rather than writing it.
//! - **G3-1b** the same unplug with the port held closed to 13 s — long
//!   enough for the firmware to have something to say while nobody is
//!   reading: a packet committed and **held**, nothing written past it (the
//!   IN-endpoint gate waits; before 2026-09-24 bytes were dropped past it),
//!   delivery within the drain latency of the `open`, held packet first.
//!
//! ⚠️ Since wire proto 30 (plan `lp-link-usb-cutover`) the image speaks
//! lp-link past its boot text, so what crosses the port after the boot is
//! link frames, and the hello and heartbeats go out only to a host that
//! brought the link up — which only a product crate may be (the MIT fence).
//! These gates therefore stop at the port: the cable and the port move when
//! the script says, bytes cross when a host reads and never otherwise. What
//! the old G3-1/G3-1b also read off the wire — the hello (and the
//! `hello.proto` figure), the 5 s and 10 s heartbeats, the io_task's
//! "draining again" line and the not-draining latch's stamps — went with the
//! `M!` wire and the latch. The link-level twin, a host on the link through
//! the same cable pull, is `lp-cli/tests/emu_usb_link_gates.rs`.
//! - **G3-2** determinism: G3-1 twice, byte-identical delivered logs and
//!   identical cycle counts. The scripted form only — the socket form is
//!   host time, and says so.
//! - **G3-4** the two dances over the channel: the host tooling's own
//!   `dtr`/`rts` sequences end the run as `Outcome::Reset` naming
//!   `USB_DEVICE chip_rst (serial)`, `strap = app` and `strap = download`.
//!   `chip_rst` bit 2's suppression is `machine::tests` (no firmware
//!   needed) and `periph::usb_sj`'s model tests.
//!
//! G3-3 (the socket form end to end) is `tests/usb_socket.rs`: it spawns
//! the binary and speaks TCP, which is a different kind of test.
//!
//! `#[ignore]`d for the usual reason (`test_support` will not start a
//! cross-target firmware build inside a workspace test run); `just
//! test-emu-c6` runs them.

use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::control::parse_usb_script;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};
use sha2::{Digest, Sha256};

const MS: u64 = 1_000 * memmap::CYCLES_PER_US;

/// The brief's s7 shape. `attach` before `open`, because a cable is not a
/// port open — the coupling rule the control channel exists to express.
const S7: &str = "\
# the s7 shape: plugged in and read from boot, unplugged mid-op, plugged back in
0      attach
0      open
6000   detach
9000   attach
9500   open
";

/// The same unplug, with the port held closed long enough that the
/// firmware tries to write while nobody is reading.
const S7_WIDE: &str = "\
0      attach
0      open
6000   detach
9000   attach
13000  open
";

struct Run {
    m: Esp32C6Machine,
    /// Every trace line, with its millisecond.
    lines: Vec<(f64, String)>,
    outcome: Outcome,
}

impl Run {
    /// The trace's lines between two milliseconds, as one string.
    fn window(&self, from: f64, to: f64) -> String {
        self.lines
            .iter()
            .filter(|(ms, _)| *ms >= from && *ms <= to)
            .map(|(ms, line)| format!("{ms:9.1} ms  {line}\n"))
            .collect()
    }

    /// `(millisecond, bytes)` for every IN packet a host actually took.
    fn deliveries(&self) -> Vec<(f64, usize)> {
        self.events("USB_DEVICE IN packet of ", " bytes delivered")
    }

    /// The link task's edge counter `name`
    /// (`fw_esp32_common::usb_link::usb_link_counters`), read straight out
    /// of the guest's memory.
    fn link_counter(&mut self, name: &str) -> u32 {
        let sym = format!("fw_esp32_common::usb_link::usb_link_counters::{name}");
        self.m
            .peek_symbol(&sym)
            .unwrap_or_else(|| panic!("the image carries {sym}"))
            .1
    }

    /// `(millisecond, bytes)` for every `wr_done` that committed a packet.
    fn commits(&self) -> Vec<(f64, usize)> {
        self.events("USB_DEVICE wr_done: ", " bytes committed")
    }

    fn events(&self, before: &str, after: &str) -> Vec<(f64, usize)> {
        self.lines
            .iter()
            .filter_map(|(ms, line)| {
                let rest = line.split_once(before)?.1;
                let n = rest.split_once(after)?.0;
                Some((*ms, n.parse().ok()?))
            })
            .collect()
    }
}

/// Run the shipped-minus-flash image with `script` driving the host, with
/// `USB_DEVICE` traced. `None` when there is no firmware to run.
fn run(script: &str, micros: u64) -> Option<Run> {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("usb_control", &reason);
            return None;
        }
    };
    let parsed = parse_usb_script(script).expect("the gate's own script parses");
    let buf = SharedBuffer::new();
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .time_grade(TimeGrade::T1)
        .usb_script(parsed.commands)
        .usb_sj_source(Box::new(parsed.bytes))
        .trace(Box::new(buf.clone()), vec!["USB_DEVICE".to_string()])
        .build()
        .expect("the shipped-minus-flash image builds a machine");
    let outcome = m.run_until(&StopCondition::after_micros(micros));
    let lines = buf
        .contents()
        .lines()
        .filter_map(|line| {
            let cyc = line.strip_prefix("cyc=")?.split_whitespace().next()?;
            let cyc: u64 = cyc.parse().ok()?;
            Some((
                cyc as f64 / memmap::CYCLES_PER_US as f64 / 1_000.0,
                line.to_string(),
            ))
        })
        .collect();
    Some(Run { m, lines, outcome })
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn g3_1_the_cable_comes_out_at_six_seconds_and_the_link_comes_back_at_nine() {
    let Some(mut r) = run(S7, 12_000_000) else {
        return;
    };
    assert!(
        matches!(r.outcome, Outcome::Deadline { .. }),
        "{:?}",
        r.outcome
    );
    assert_eq!(
        r.m.scripted_commands_left(),
        0,
        "every line of the script came due"
    );
    assert_eq!(r.m.control_lines(), 5, "and every one of them applied");

    let delivered = r.m.usb_sj().text();
    // Before the unplug: the boot, to the server loop's boot marker.
    assert!(
        delivered.starts_with("[INIT] Initializing board...\n"),
        "the first line a host attached from cycle zero sees"
    );
    assert!(
        delivered.contains("starting server loop... proto="),
        "the boot marker reached the host"
    );

    // The transitions land on the cycles the script asked for, to the cycle.
    for (ms, needle) in [
        (0.0, "host attached: bus reset"),
        (0.0, "port opened: the host drains"),
        (6_000.0, "host detached: SOF stops"),
        (9_000.0, "host attached: bus reset"),
        (9_500.0, "port opened: the host drains"),
    ] {
        assert!(
            r.lines
                .iter()
                .any(|(at, line)| (*at - ms).abs() < 0.01 && line.contains(needle)),
            "`{needle}` at {ms} ms is missing from the trace"
        );
    }

    // Nothing at all crosses the link while the cable is out, and nothing
    // crosses it between the re-attach and the re-open either.
    let quiet: Vec<(f64, usize)> = r
        .deliveries()
        .into_iter()
        .filter(|(ms, _)| (6_000.0..9_500.0).contains(ms))
        .collect();
    assert!(
        quiet.is_empty(),
        "the host was not there, yet {quiet:?} reached it:\n{}",
        r.window(6_000.0, 9_500.0)
    );

    // After the re-open, the link task's traffic reaches the host again.
    let after: Vec<(f64, usize)> = r
        .deliveries()
        .into_iter()
        .filter(|(ms, _)| *ms > 9_500.0)
        .collect();
    assert!(
        !after.is_empty(),
        "nothing drained after the re-open:\n{}",
        r.window(9_500.0, 12_000.0)
    );

    // Liveness: the guest kept running through the whole detached window.
    assert!(r.m.idle_skips() > 1_000, "idle skips {}", r.m.idle_skips());
    assert!(r.m.uart0().is_empty(), "the link is USB, not UART0");

    // The link task's own account: with the cable out there is no SOF, and
    // it discards what it would have sent rather than writing into an
    // endpoint nobody will drain. The server's unsolicited hello and
    // heartbeats had no link to ride and were dropped and counted; nothing
    // was dropped for a full send budget.
    let discarded = r.link_counter("FRAMES_DISCARDED_NO_HOST");
    assert!(
        discarded > 0,
        "the link task wrote with no host instead of discarding"
    );
    assert!(r.link_counter("REPLIES_DROPPED_NO_LINK") >= 1);
    assert_eq!(r.link_counter("REPLIES_DROPPED_FULL"), 0);
    eprintln!("G3-1: {discarded} frame(s) discarded while the cable was out");
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn g3_1b_a_port_held_closed_after_the_replug_holds_a_packet_until_it_opens() {
    let Some(mut r) = run(S7_WIDE, 16_000_000) else {
        return;
    };
    assert!(
        matches!(r.outcome, Outcome::Deadline { .. }),
        "{:?}",
        r.outcome
    );

    // After the re-attach the link task has frames to send (its handshake,
    // on its own timer). The first is committed to an endpoint nobody
    // drains and HELD.
    let held = r
        .commits()
        .into_iter()
        .find(|(ms, _)| (9_000.0..13_000.0).contains(ms))
        .expect("no commit while the port was closed");
    assert!(
        r.lines.iter().any(|(ms, line)| (*ms - held.0).abs() < 0.01
            && line.contains("the host is attached but not draining (port closed)")),
        "the commit at {} ms is not recorded as held:\n{}",
        held.0,
        r.window(held.0 - 1.0, held.0 + 1.0)
    );
    // Nothing is delivered while it is held.
    assert!(
        r.deliveries()
            .iter()
            .all(|(ms, _)| !(6_000.0..13_000.0).contains(ms)),
        "something reached a host that was not reading:\n{}",
        r.window(6_000.0, 13_000.0)
    );
    // Nothing is written past the committed packet. ⚠️ Re-pinned 2026-09-24
    // for the io_task's IN-endpoint gate (`fw_esp32_common::serial::
    // in_endpoint`, PR #805, ported to the C6 for
    // docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md):
    // before it, esp-hal 1.1.1's `write_async` kept writing the rest of the
    // heartbeat and the `\n` probes into the held packet and the model
    // dropped them (`(host attached-idle): byte … dropped`) — this used to
    // assert that line was present. With the gate the io_task waits for a
    // free buffer and its chunk timeout fires instead, so there is no such
    // line. What a host sees after the open is the M6 silicon capture's
    // shape (`lp-emu/transcripts/esp32c6/usb-negative-control/`), checked
    // below; the capture itself records nothing from while the port was
    // closed, so nothing here contradicts it — but it was taken on a
    // pre-gate image, and a desk re-capture on a gated image is owed.
    assert!(
        !r.lines
            .iter()
            .any(|(ms, line)| (9_000.0..13_000.0).contains(ms)
                && line.contains("(host attached-idle): byte")
                && line.contains("dropped")),
        "a byte was written into the held packet past the IN-endpoint gate:\n{}",
        r.window(10_000.0, 11_000.0)
    );

    // The open delivers the held packet within the drain latency, and the
    // rest of the backlog behind it.
    let first = r
        .deliveries()
        .into_iter()
        .find(|(ms, _)| *ms > 13_000.0)
        .expect("nothing was delivered after the port opened");
    assert_eq!(first.1, held.1, "the held packet leaves first");
    let latency_ms = usb_drain_latency_ms();
    assert!(
        (first.0 - 13_000.0 - latency_ms).abs() < 1.0,
        "the held packet left at {} ms, not {} ms after the open",
        first.0,
        latency_ms
    );

    // And the firmware's own account: its frame writes waited on the gate
    // while the packet was held, and its write bound abandoned them.
    let timeouts = r.link_counter("WRITE_TIMEOUTS");
    assert!(
        timeouts >= 1,
        "no write timed out while the port was closed"
    );
    eprintln!(
        "G3-1b: held {} B at {:.1} ms, delivered at {:.1} ms; {timeouts} write timeouts",
        held.1, held.0, first.0
    );
}

/// The model's own `IN_DRAIN_LATENCY_US`, in milliseconds — read from the
/// constant rather than written down twice.
fn usb_drain_latency_ms() -> f64 {
    lp_emu_esp32c6::periph::usb_sj::IN_DRAIN_LATENCY_US as f64 / 1_000.0
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn g3_2_the_scripted_form_is_the_same_run_twice() {
    let Some(a) = run(S7, 12_000_000) else {
        return;
    };
    let Some(b) = run(S7, 12_000_000) else {
        return;
    };
    let digest = |m: &Esp32C6Machine| {
        let mut h = Sha256::new();
        h.update(m.usb_sj().bytes());
        format!("{:x}", h.finalize())
    };
    assert_eq!(
        digest(&a.m),
        digest(&b.m),
        "two runs of one script delivered different bytes"
    );
    assert_eq!(a.m.usb_sj().len(), b.m.usb_sj().len());
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(a.m.idle_skips(), b.m.idle_skips());
    eprintln!(
        "G3-2: {} B delivered, sha256 {}, {} cycles, {} idle skips — both runs",
        a.m.usb_sj().len(),
        digest(&a.m),
        a.m.cycles(),
        a.m.idle_skips()
    );
}

/// The host tooling's own hard-reset dance
/// (`lpa-client/src/transport_serial/hardware.rs:228`,
/// `SerialResetStyle::UsbSerialJtag`): `D0; sleep 100; R1; D0; R1; sleep
/// 100; discard input; R0`. DTR never goes high, so the RTS falling edge at
/// the end is a plain reset.
const RESET_DANCE: &str = "\
200   attach
200   open
300   dtr 0
400   rts 1
400   dtr 0
400   rts 1
500   rts 0
";

/// The download dance (`browser_serial.rs:150`, `byte_stream.rs:129`):
/// `R0 D0 W100 D1 R0 W100 R1 D0 R1 W100 R0 D0`. DTR goes high before the
/// last RTS falling edge, which is what makes it download mode.
const DOWNLOAD_DANCE: &str = "\
200   attach
200   open
300   rts 0
300   dtr 0
400   dtr 1
400   rts 0
500   rts 1
500   dtr 0
500   rts 1
600   rts 0
600   dtr 0
";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn g3_4_the_hosts_own_dances_over_the_channel_reach_the_two_straps() {
    use lp_emu_esp_common::Strap;

    for (script, strap, at_ms) in [
        (RESET_DANCE, Strap::App, 500u64),
        (DOWNLOAD_DANCE, Strap::Download, 600),
    ] {
        let Some(r) = run(script, 2_000_000) else {
            return;
        };
        let Outcome::Reset {
            cycle,
            source,
            strap: got,
        } = r.outcome
        else {
            panic!("the dance did not reset the chip: {:?}", r.outcome);
        };
        assert_eq!(source, "USB_DEVICE chip_rst (serial)");
        assert_eq!(got, strap);
        assert_eq!(
            cycle,
            at_ms * MS,
            "the reset is stamped with the cycle the last RTS edge was drained at"
        );
        assert_eq!(
            r.outcome.exit_code(),
            2,
            "a reset the emulator cannot perform"
        );
        // `chip_rst` bit 0 records that the serial channel asked.
        assert!(
            r.lines.iter().any(|(_, line)| line.contains(&format!(
                "chip reset from the serial channel, strap = {strap}"
            ))),
            "the trace does not name the strap"
        );
    }
}
