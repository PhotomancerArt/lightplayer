//! M6 P2's machine-level gates on the shipped image minus flash
//! (`esp32c6,server,radio,memory_fs`), strict `t1`, 5.5 s:
//!
//! - **G2-1** attached-draining from boot: every `[INIT]` line up to the
//!   boot marker reaches the **delivered** `usb-sj` log in order;
//!   `TIMED_OUT` never latches; the link task arms
//!   `int_ena.serial_out_recv_pkt`; no `SPIN` on `USB_DEVICE`.
//! - **G2-3** attached-idle from boot (`Attached { draining: false }`):
//!   nothing is delivered and nothing is written past the held boot line;
//!   `TIMED_OUT == 1`; the link task's frame writes wait on the IN-endpoint
//!   gate and time out, counted by the firmware; liveness from `idle_skips`
//!   and the RWDT feeds.
//!
//! ⚠️ Since wire proto 30 (plan `lp-link-usb-cutover`) the image speaks
//! lp-link past its boot text: the hello and the heartbeat go out only once
//! a host has brought the link up, and only a product crate may be that host
//! (the MIT fence). So G2-1 stops at the boot marker here, and the rest of
//! what it used to prove — the hello, the first heartbeat and the stack line
//! reach a host in order, and the `hello.proto` / `heartbeat.total_bytes`
//! figures — is `lp-cli/tests/emu_usb_link_gates.rs`, the same image with the
//! product's own link host. G2-3's old signature was the not-draining latch,
//! which went with the latch; what it asserts now is the link task's.
//! - **G2-4** determinism: G2-1 twice → identical delivered logs and cycle
//!   counts.
//!
//! G2-2 (host absent) is `tests/host_absent.rs`. `#[ignore]`d for the usual
//! reason (`test_support`); `just test-emu-c6` runs them.

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::RegGrade;
use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, TimeGrade, UsbHost,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};
use sha2::{Digest, Sha256};

const GATE_US: u64 = 5_500_000;
const MS: u64 = 1_000 * memmap::CYCLES_PER_US;
const TIMED_OUT: &str = "esp_println::serial_jtag_printer::TIMED_OUT";
const SERIAL_OUT_RECV_PKT: u32 = 1 << 2;
const SERIAL_IN_EMPTY: u32 = 1 << 3;

/// The delivered log's markers, in order: the raw boot text a host reads
/// before the link task owns the port, ending at the boot marker the server
/// loop prints as it starts.
const DELIVERED_IN_ORDER: &[&str] = &[
    "[INIT] Initializing board...\n",
    "[INIT] Spawning USB link task...",
    "[INIT] USB link task spawned",
    "[INIT] LpServer created",
    "[INIT] fw-esp32 initialized, starting server loop... proto=",
];

struct Run {
    m: Esp32C6Machine,
    lines: Vec<String>,
    /// `TIMED_OUT` at 3 s and at the end.
    timed_out: (u32, u32),
}

fn run(host: UsbHost) -> Option<Run> {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("usb_attached", &reason);
            return None;
        }
    };
    let buf = SharedBuffer::new();
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .time_grade(TimeGrade::T1)
        .usb_host(host)
        .trace(
            Box::new(buf.clone()),
            vec!["USB_DEVICE".to_string(), "LP_WDT".to_string()],
        )
        .build()
        .expect("the shipped-minus-flash image builds a machine");
    let at_3s = m.run_until(&StopCondition::after_micros(3_000_000));
    assert!(matches!(at_3s, Outcome::Deadline { .. }), "{at_3s:?}");
    let timed_out_3s = m.peek_symbol(TIMED_OUT).expect("TIMED_OUT").1;
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US));
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert_eq!(
        m.bus.unmapped_reads() + m.bus.unmapped_writes(),
        0,
        "unmapped"
    );
    let timed_out_end = m.peek_symbol(TIMED_OUT).expect("TIMED_OUT").1;
    Some(Run {
        m,
        lines: buf.lines(),
        timed_out: (timed_out_3s, timed_out_end),
    })
}

fn cycle_of(line: &str) -> Cycles {
    line.split_whitespace().next().unwrap()["cyc=".len()..]
        .parse()
        .unwrap()
}

fn value_of(line: &str) -> u32 {
    u32::from_str_radix(line.rsplit("= 0x").next().unwrap(), 16).unwrap()
}

/// The machine is idle in `wfi` with the RWDT fed to the end — the liveness
/// evidence every gate shares (G6-3's).
fn assert_alive(r: &Run) {
    assert!(r.m.idle_skips() > 1_000, "{} idle skips", r.m.idle_skips());
    let feeds: Vec<&String> = r
        .lines
        .iter()
        .filter(|l| l.contains("W4 LP_WDT+0x014 wdtfeed = 0x80000000"))
        .collect();
    assert!(feeds.len() > 1_000, "{} feeds", feeds.len());
    let last = cycle_of(feeds.last().unwrap());
    assert!(
        last > (GATE_US - 100_000) * memmap::CYCLES_PER_US,
        "last feed at {last}"
    );
    assert!(!r.lines.iter().any(|l| l.contains("EXPIRED")));
    assert!(r.m.uart0().is_empty(), "the shipped console is USB-SJ");
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn g2_1_attached_and_draining_from_boot_delivers_the_boot_to_the_server_loop() {
    let Some(r) = run(UsbHost::Attached { draining: true }) else {
        return;
    };
    let text = r.m.usb_sj().text();

    // Everything reached the host, in order; nothing was merely tried.
    let mut from = 0;
    for marker in DELIVERED_IN_ORDER {
        let at = text[from..]
            .find(marker)
            .unwrap_or_else(|| panic!("{marker:?} not after byte {from} in:\n{text}"));
        from += at + marker.len();
    }
    let init_lines: Vec<&str> = text.lines().filter(|l| l.starts_with("[INIT] ")).collect();
    assert!(init_lines.len() >= 5, "{init_lines:?}");
    assert!(
        r.m.usb_sj_tried().is_empty(),
        "tried: {:?}",
        r.m.usb_sj_tried().text()
    );

    // esp-println never timed out: the host drained every packet.
    assert_eq!(r.timed_out, (0, 0), "TIMED_OUT at 3 s and at the end");
    let usb_spins: Vec<&String> = r
        .lines
        .iter()
        .filter(|l| l.contains(" SPIN ") && l.contains("USB_DEVICE"))
        .collect();
    assert!(usb_spins.is_empty(), "{usb_spins:?}");

    // The link task armed RX (its read future).
    let armed = r
        .lines
        .iter()
        .filter(|l| l.contains("W4 USB_DEVICE+0x010 int_ena"))
        .any(|l| value_of(l) & SERIAL_OUT_RECV_PKT != 0);
    assert!(armed, "int_ena.serial_out_recv_pkt was never written set");
    let delivered_notes = r
        .lines
        .iter()
        .filter(|l| l.contains("IN packet of") && l.contains("delivered to the host"))
        .count();
    assert!(delivered_notes >= 20, "{delivered_notes} deliveries");
    assert_alive(&r);

    // For the PR body: the first 20 delivered lines and the log's digest.
    println!("G2-1 delivered log, first 20 lines:");
    for l in text.lines().take(20) {
        println!("  | {l}");
    }
    println!(
        "G2-1 delivered {} bytes, sha256 {}, {} cycles, {} idle skips",
        text.len(),
        sha256(text.as_bytes()),
        r.m.cycles(),
        r.m.idle_skips()
    );
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn g2_3_attached_idle_from_boot_holds_the_boot_line_and_times_the_link_writes_out() {
    let Some(mut r) = run(UsbHost::Attached { draining: false }) else {
        return;
    };
    // The port is closed from power-on. esp-println's first packet (the
    // first `[INIT]` line) commits and is HELD: nobody takes it, so
    // esp-println waits once and latches `TIMED_OUT`, and everything the
    // link task has to send waits behind the IN-endpoint gate
    // (`fw_esp32_common::serial::in_endpoint`, PR #805) for a buffer that
    // never frees. Until proto 30 the signature here was the io_task's
    // not-draining latch (write attempts 250 ms apart, then a probe every
    // 2 s, and the latch's stamps); the latch went with the io_task, and the
    // link task's own account is its edge counters.
    //
    // This test does not replay a transcript. The M6 silicon capture it
    // was shaped after (`lp-emu/transcripts/esp32c6/usb-negative-control/
    // silicon-esp32c6-2026-09-07-b18360ea6.txt`) is what a host read after
    // opening the port on a pre-lp-link image; a desk re-capture on an
    // lp-link image is owed.
    //
    // Nothing reached a host, and nothing was written into the held packet.
    assert!(r.m.usb_sj().is_empty(), "{:?}", r.m.usb_sj().text());
    assert!(
        r.m.usb_sj_tried().is_empty(),
        "a write went into the held packet past the gate: {:?}",
        r.m.usb_sj_tried().text()
    );
    assert!(
        r.lines
            .iter()
            .any(|l| l.contains("wr_done: 28 bytes committed") && l.contains("port closed")),
        "the [INIT] line is held"
    );
    assert!(
        !r.lines.iter().any(|l| l.contains("delivered to the host")),
        "no delivery"
    );
    // esp-println latched after its one wait, like host-absent.
    assert_eq!(r.timed_out, (1, 1), "TIMED_OUT at 3 s and at the end");

    // The only commit is the held boot line.
    let mut commits: Vec<(Cycles, usize)> = Vec::new();
    let mut pushed = 0usize;
    for l in &r.lines {
        if l.contains("W4 USB_DEVICE+0x000 ep1 ") {
            pushed += 1;
        } else if l.contains("W4 USB_DEVICE+0x004 ep1_conf") && value_of(l) & 1 != 0 && pushed > 0 {
            commits.push((cycle_of(l), std::mem::take(&mut pushed)));
        }
    }
    assert_eq!(
        commits.iter().map(|c| c.1).collect::<Vec<_>>(),
        vec![28],
        "only the boot line is committed"
    );
    assert_eq!(pushed, 0, "bytes pushed and never committed");

    // The link task's frame writes wait on the gate (`serial_in_empty`
    // armed) and are abandoned by its 250 ms write bound; it counts each.
    let waits = r
        .lines
        .iter()
        .filter(|l| l.contains("W4 USB_DEVICE+0x010 int_ena"))
        .filter(|l| value_of(l) & SERIAL_IN_EMPTY != 0)
        .count();
    assert!(waits >= 2, "{waits} gated waits");
    let mut counter = |name: &str| {
        let sym = format!("fw_esp32_common::usb_link::usb_link_counters::{name}");
        r.m.peek_symbol(&sym)
            .unwrap_or_else(|| panic!("the image carries {sym}"))
            .1
    };
    let timeouts = counter("WRITE_TIMEOUTS");
    let (no_link, full) = (
        counter("REPLIES_DROPPED_NO_LINK"),
        counter("REPLIES_DROPPED_FULL"),
    );
    assert!(timeouts >= 1, "the link task never timed a write out");
    assert!(
        no_link >= 1 && full == 0,
        "the server's unsolicited hello and heartbeats go nowhere with no link, and are \
         counted as dropped; nothing was dropped for a full send budget"
    );
    assert_alive(&r);
    // The link task listens whatever the port's state: RX is armed.
    let rx_armed = r
        .lines
        .iter()
        .filter(|l| l.contains("W4 USB_DEVICE+0x010 int_ena"))
        .any(|l| value_of(l) & SERIAL_OUT_RECV_PKT != 0);
    assert!(
        rx_armed,
        "int_ena.serial_out_recv_pkt was never written set"
    );

    println!(
        "G2-3 {waits} gated waits, {timeouts} write timeouts; commits {:?}",
        commits.iter().map(|c| (c.0 / MS, c.1)).collect::<Vec<_>>()
    );
    println!(
        "G2-3 TIMED_OUT = {:?}, {} idle skips, delivered {} B, tried {} B",
        r.timed_out,
        r.m.idle_skips(),
        r.m.usb_sj().len(),
        r.m.usb_sj_tried().len()
    );
}

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn g2_4_two_attached_runs_are_the_same_run() {
    let (Some(a), Some(b)) = (
        run(UsbHost::Attached { draining: true }),
        run(UsbHost::Attached { draining: true }),
    ) else {
        return;
    };
    assert_eq!(a.m.usb_sj().bytes(), b.m.usb_sj().bytes());
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(a.m.idle_skips(), b.m.idle_skips());
    println!(
        "G2-4 sha256 {} / {}; cycles {} / {}",
        sha256(&a.m.usb_sj().bytes()),
        sha256(&b.m.usb_sj().bytes()),
        a.m.cycles(),
        b.m.cycles()
    );
}

/// **G4-4 (M6 P4): the shipped image, attached and draining, under
/// `--strict-grade documented`.**
///
/// The claim is narrow and it is stated narrowly. Every USB_DEVICE register
/// the shipped image touches in five and a half seconds is one the M6
/// transcripts measured or a document describes; not one of the twenty this
/// block still grades `modeled` is crossed. If a later change reaches for
/// one — a driver that starts reading `jfifo_st`, a line-coding path — this
/// run stops with the register's name, which is what the level is for.
///
/// It is **not** a claim about the chip, and it says which block it is
/// about: `--strict-grade-blocks USB_DEVICE`. Until 2026-09-08 the narrowing
/// was accidental — this was the only block that published a grade table, so
/// the level had nowhere else to apply. Then M2 P1 gave `GPIO` one, M3 P1
/// gave `UART0`/`UART1` theirs, and this branch gives every accept block one,
/// so a run that wants this claim has to name it and
/// `blocks_in_strict_grade_scope` reports the one block back. That is also
/// what stops this snapshot needing a hand-edit every time a block is graded.
///
/// What the wider run says is `the_boot_reads_registers_we_only_modelled`
/// below: the same level with no block named, which stops at the first
/// modelled register of the boot and prints it. That is the survey the
/// honest-peripheral policy wanted, and it is a different question from
/// this one.
#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn g4_4_the_shipped_image_crosses_no_modeled_usb_register() {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("usb_attached", &reason);
            return;
        }
    };
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .strict_grade(Some(RegGrade::Documented))
        .strict_grade_blocks(Some(vec!["USB_DEVICE"]))
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .build()
        .expect("the shipped-minus-flash image builds a machine");
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "a modeled USB register was crossed: {outcome:?}"
    );
    assert!(m.bus.first_strict_violation().is_none());

    // The scope, named — and it is ONE block because this run named it, not
    // because only one publishes a table. Four did before this branch
    // (`UART0`, `UART1`, `USB_DEVICE`, `GPIO`, from M3 P1 and M2 P1) and
    // twenty-six do after it. That is the churn `--strict-grade-blocks`
    // ends: this snapshot used to be a list every milestone that graded a
    // block had to remember to widen, and a widened list quietly changed
    // what G4-4 was claiming each time. It claims one block, so it says one.
    assert_eq!(m.bus.blocks_in_strict_grade_scope(), vec!["USB_DEVICE"]);
    // And the run really did do the whole boot, so the pass is not a pass by
    // never getting there.
    // (The boot marker is the last raw text; the heartbeat after it rides
    // lp-link since proto 30, which this crate cannot read.)
    assert!(
        m.usb_sj().text().contains("starting server loop... proto="),
        "the run reached the server loop"
    );
    assert!(
        m.idle_skips() > 1_000,
        "and idled in it: {}",
        m.idle_skips()
    );

    // The lists the README publishes, printed for the record.
    println!(
        "G4-4: {} modeled USB_DEVICE registers, none crossed in {GATE_US} us: {}",
        lp_emu_esp32c6::periph::usb_sj::modeled_registers().len(),
        lp_emu_esp32c6::periph::usb_sj::modeled_registers().join(", ")
    );
    let gpio = lp_emu_esp32c6::periph::gpio::Gpio::modeled_registers();
    println!(
        "G4-4: {} modeled GPIO registers, none crossed either: {}",
        gpio.len(),
        gpio.join(", ")
    );
}

/// The survey the accept-block grading made possible: run the shipped image
/// under `--strict-grade documented` over **every** block that publishes a
/// table, and name the first register it reads that we only modelled.
///
/// This is not a gate on a number and it is not expected to pass. A boot on
/// this machine reads registers whose behaviour is our reading of the PAC
/// and the drivers — that is what an emulator is — and the useful thing is
/// to say *which*, in a way a change can move. Before the accept blocks were
/// graded, `--strict-grade documented` passed over every one of them and the
/// flag measured how much of the chip had been graded rather than what a run
/// was allowed to trust.
///
/// The first one is `I2C_ANA_MST.i2c1_ctrl`: the analog transaction port,
/// which forces `busy` low and answers `regi2c` reads out of a store nobody
/// has measured. A register a block pretends about is a register it
/// modelled, and the grading rule says so without being told.
#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_boot_reads_registers_we_only_modelled_and_this_is_which() {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("usb_attached", &reason);
            return;
        }
    };
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .strict_grade(Some(RegGrade::Documented))
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .build()
        .expect("the shipped image builds a machine");
    // Not `.strict(true)`: an unmapped access is a different complaint, and
    // this run is only asking the grade question.
    let scope = m.bus.blocks_in_strict_grade_scope();
    assert!(
        scope.len() > 20,
        "the accept blocks should be in scope now, not just USB_DEVICE: {scope:?}"
    );
    assert!(scope.contains(&"USB_DEVICE"), "{scope:?}");
    assert!(scope.contains(&"PCR"), "{scope:?}");
    assert!(scope.contains(&"I2C_ANA_MST"), "{scope:?}");

    let outcome = m.run_until(&StopCondition::after_micros(GATE_US));
    let violation = m.bus.first_strict_violation().unwrap_or_else(|| {
        panic!("nothing below `documented` was read in {GATE_US} us: {outcome:?}")
    });
    assert_eq!(violation.grade, Some(RegGrade::Modeled));
    println!(
        "strict-grade documented over {} blocks: first modeled register read at \
         {:#010x} (pc {:#010x}, cycle {})",
        scope.len(),
        violation.address,
        violation.pc,
        violation.cycle
    );
    // Pinned, because a change that moves it is a change to what this boot
    // depends on being modelled — the point of the survey.
    assert_eq!(
        violation.address,
        lp_emu_esp32c6::memmap::periph::I2C_ANA_MST + 0x004,
        "the first modeled register the boot reads is the analog transaction port"
    );
}
