//! M6 P3's gates: the host control channel, on the shipped image minus
//! flash (`esp32c6,server,radio,memory_fs`), strict `t1`.
//!
//! - **G3-1** the s7 shape, scripted: attach + open from cycle zero, the
//!   cable pulled at 6 s, back in at 9 s, the port re-opened at 9.5 s. The
//!   delivered log carries the hello and the first heartbeat before 6 s,
//!   **nothing** between the detach and the re-open, and a full 64-byte
//!   commit drained plus the 10 s heartbeat after it. P1b's stamps are all
//!   still their "never" sentinel: through this whole unplug the firmware
//!   noticed nothing, because its latch resets optimistically on a cable
//!   loss and it had nothing to write in the 500 ms the port was closed.
//! - **G3-1b** the same unplug with the port held closed to 13 s — long
//!   enough for the firmware to have something to say while nobody is
//!   reading. That is where the transition is actually visible: a 64-byte
//!   packet committed and **held**, bytes dropped past it, delivery within
//!   the drain latency of the `open`, and then the firmware's own
//!   `[io_task] host draining again` line — the recovery half of the pair
//!   P1 exists to measure, reproduced with no rig, because a control
//!   channel can open a port at a chosen moment. Its twin, "host not
//!   draining", is absent from the delivered log, exactly as the monitor's
//!   own gating predicts (M6 discovery §4) — and P1b's stamp **pair** is
//!   there to say so on the firmware's own clock (DD38: the pair is the
//!   claim, never the count).
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

    /// M6 P1b's vehicle, read straight out of the guest's memory: the
    /// firmware's own account of its link, on its own clock.
    /// `(host_not_draining_ms, host_draining_again_ms, not_draining_count)`,
    /// each stamp `None` when it is still the "never" sentinel.
    ///
    /// Per DD38 the **pair** is the transition claim, not the count: with no
    /// host at all the monitor latches once during boot and never recovers,
    /// so `not_draining_count == 1` alone says nothing.
    fn link_stamps(&mut self) -> (Option<u32>, Option<u32>, u32) {
        let read = |m: &mut Esp32C6Machine, name: &str| {
            m.peek_symbol(&format!("fw_esp32_common::serial::link_counters::{name}"))
                .unwrap_or_else(|| panic!("the image carries {name}"))
                .1
        };
        let never = |v: u32| (v != u32::MAX).then_some(v);
        (
            never(read(&mut self.m, "HOST_NOT_DRAINING_MS")),
            never(read(&mut self.m, "HOST_DRAINING_AGAIN_MS")),
            read(&mut self.m, "NOT_DRAINING_COUNT"),
        )
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
    // Before the unplug: the boot, the hello, the first heartbeat.
    let hello = delivered
        .find("\nM!{\"id\":0,\"msg\":{\"hello\":{\"proto\":20,")
        .expect("the unsolicited hello reached the host");
    let first_beat = delivered
        .find("\"uptime_ms\":5000")
        .expect("the 5 s heartbeat reached the host");
    assert!(
        delivered.starts_with("[INIT] Initializing board...\n"),
        "the first line a host attached from cycle zero sees"
    );
    assert!(hello < first_beat, "the hello precedes the heartbeat");

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

    // After the re-open, a full packet drains and the 10 s heartbeat lands.
    let after: Vec<(f64, usize)> = r
        .deliveries()
        .into_iter()
        .filter(|(ms, _)| *ms > 9_500.0)
        .collect();
    assert!(
        after.iter().any(|(_, n)| *n == 64),
        "no full 64 B commit drained after the re-open: {after:?}"
    );
    assert!(
        delivered.contains("\"uptime_ms\":10000"),
        "the 10 s heartbeat never reached the re-attached host"
    );

    // Liveness: the guest kept running through the whole detached window.
    assert!(r.m.idle_skips() > 1_000, "idle skips {}", r.m.idle_skips());
    assert!(r.m.uart0().is_empty(), "the link is USB, not UART0");

    // P1b's vehicle, and the finding it makes exact: through this whole
    // unplug the firmware **noticed nothing**. The monitor resets its latch
    // optimistically on a cable loss, so when SOF returns at 9 s it already
    // believes it is connected — and it had nothing to write in the 500 ms
    // before the port re-opened. No write, no timeout, no latch, no
    // recovery, no stamp of either kind.
    assert_eq!(
        r.link_stamps(),
        (None, None, 0),
        "the firmware recorded a link transition the register trace says never happened"
    );
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

    // The firmware's next thing to say after the re-attach is the 10 s
    // heartbeat. It is committed to an endpoint nobody drains and HELD.
    let held = r
        .commits()
        .into_iter()
        .find(|(ms, n)| (9_000.0..13_000.0).contains(ms) && *n == 64)
        .expect("no 64 B commit while the port was closed");
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
    // Bytes written past the committed packet are dropped, and the model
    // says so once rather than pretending they went.
    assert!(
        r.lines
            .iter()
            .any(|(ms, line)| (9_000.0..13_000.0).contains(ms)
                && line.contains("(host attached-idle): byte")
                && line.contains("dropped")),
        "no dropped byte recorded while the port was closed:\n{}",
        r.window(10_000.0, 11_000.0)
    );

    // The open delivers the held packet within the drain latency, and the
    // rest of the backlog behind it.
    let first = r
        .deliveries()
        .into_iter()
        .find(|(ms, _)| *ms > 13_000.0)
        .expect("nothing was delivered after the port opened");
    assert_eq!(first.1, 64, "the held packet leaves first");
    let latency_ms = usb_drain_latency_ms();
    assert!(
        (first.0 - 13_000.0 - latency_ms).abs() < 1.0,
        "the held packet left at {} ms, not {} ms after the open",
        first.0,
        latency_ms
    );

    // And the firmware's own account of it. `UsbConnectionMonitor` logs two
    // strings and both go through the queue that `is_connected()` gates, so
    // on a USB link they behave differently (M6 discovery §4): the
    // "not draining" line is queued and then dropped by the very latch it
    // reports, while "draining again" is emitted *after* the latch flips
    // back and so reaches the host that just opened the port. That
    // asymmetry is the reason P1 exists, and this is it, reproduced with no
    // rig at all — because a control channel can open a port at a chosen
    // moment.
    let delivered = r.m.usb_sj().text();
    let again = delivered
        .find("[io_task] host draining again; resuming protocol writes")
        .expect("the recovery line never reached the re-opened port");
    assert!(
        !delivered.contains("host not draining"),
        "the self-erasing line reached a host, which contradicts the monitor's own gating"
    );
    let beat = delivered
        .find("\"uptime_ms\":15000")
        .expect("no heartbeat after the recovery");
    assert!(again < beat, "the recovery line precedes the heartbeat");

    // What the held packet itself was: the first 64 bytes of the 10 s
    // heartbeat frame. Everything the firmware wrote behind it went into a
    // committed endpoint and was lost, so that heartbeat never arrives
    // whole — the drops above are those bytes.
    assert!(
        !delivered.contains("\"uptime_ms\":10000"),
        "the heartbeat written into a closed port arrived whole, which would mean the \
         committed endpoint took bytes it had no room for"
    );

    // And P1b's vehicle agrees, on the firmware's own clock. Per DD38 the
    // PAIR is the claim — with no host at all the monitor latches once
    // during boot and never recovers, so a count alone proves nothing. Here
    // both stamps exist, in order, and each lands where the script put it.
    let (not_draining, again, count) = r.link_stamps();
    let not_draining = not_draining.expect("the firmware never latched `host not draining`");
    let again = again.expect("the firmware never recorded a recovery");
    assert_eq!(count, 1, "exactly one silence in this run");
    assert!(
        (9_000..13_000).contains(&not_draining),
        "the latch is stamped at {not_draining} ms, outside the window the port was closed"
    );
    // The recovery is **the first probe that finds the endpoint free**, and
    // that is the whole assertion: at or after the open, never before it,
    // and within one probe interval of it.
    //
    // Not "strictly after the open", which is what this first said and what
    // CI caught. After the latch the firmware's only traffic is one `\n`
    // every `PROBE_INTERVAL` (io_task, 2 s), and while the port is closed
    // even that byte is dropped into the still-committed endpoint. So which
    // millisecond the recovery lands on is decided by where that 2 s grid
    // falls relative to the open, and the grid is anchored at io_task's
    // start — a few milliseconds of boot that differ between two builds of
    // the firmware. This machine's grid sits at 12,899 / 14,899 ms, so the
    // open at 13,000 just misses one probe and waits 1,899 ms for the next;
    // CI's sits within the same millisecond as the open, so it recovers at
    // 13,000. Both are the same behaviour seen from either side of a
    // one-millisecond boundary, and pinning either number would be pinning
    // the build, not the firmware.
    const PROBE_INTERVAL_MS: u32 = 2_000;
    assert!(
        again >= 13_000,
        "the recovery is stamped at {again} ms, before the port opened at 13,000 ms"
    );
    assert!(
        again > not_draining,
        "the recovery at {again} ms precedes the latch at {not_draining} ms"
    );
    assert!(
        again <= 13_000 + PROBE_INTERVAL_MS,
        "the recovery is stamped at {again} ms, more than one {PROBE_INTERVAL_MS} ms probe \
         interval after the open — the probe is the recovery path (M6 discovery §4), so a \
         longer gap means something else woke the link"
    );
    // The latency itself is the firmware's cadence, not the model's, and is
    // reported rather than gated (PD9/D13, DD33).
    eprintln!(
        "G3-1b link stamps: not_draining {not_draining} ms, draining_again {again} ms \
         ({} ms after the open at 13,000 ms — the first probe of io_task's \
         {PROBE_INTERVAL_MS} ms grid to find the endpoint free), count {count}",
        again - 13_000
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
