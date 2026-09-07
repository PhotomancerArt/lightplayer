//! M6 P2's machine-level gates on the shipped image minus flash
//! (`esp32c6,server,radio,memory_fs`), strict `t1`, 5.5 s:
//!
//! - **G2-1** attached-draining from boot: every `[INIT]` line, the hello,
//!   the boot lines, one heartbeat and the stack line reach the **delivered**
//!   `usb-sj` log in order; `TIMED_OUT` never latches; the connected path
//!   arms `int_ena.serial_out_recv_pkt`; no `SPIN` on `USB_DEVICE`. The 5 s
//!   `freeBytes` is printed for the PR body (DD26/DD30's first same-link
//!   data point) — recorded, not gated.
//! - **G2-3** attached-idle from boot (`Attached { draining: false }`): the
//!   vehicle-neutral signature of the firmware's "host not draining" latch —
//!   two `wr_done` commits ≥ 250 ms apart with nothing delivered before 2 s,
//!   then one-byte (`0x0a`) probe commits every ≈ 2 s; `TIMED_OUT == 1`; the
//!   delivered log empty; liveness from `idle_skips` and the RWDT feeds.
//! - **G2-4** determinism: G2-1 twice → identical delivered logs and cycle
//!   counts.
//!
//! G2-2 (host absent) is `tests/host_absent.rs`. `#[ignore]`d for the usual
//! reason (`test_support`); `just test-emu-c6` runs them.

use lp_emu_core::sched::Cycles;
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

/// The delivered log's markers, in order (the P6 hello gate's list plus the
/// esp-println line that precedes everything).
const DELIVERED_IN_ORDER: &[&str] = &[
    "[INIT] Initializing board...\n",
    "\nM!{\"id\":0,\"msg\":{\"hello\":{\"proto\":20,",
    "\"boardId\":\"seeed/xiao-esp32-c6\"",
    "\"baseMac\":\"a0:f2:62:87:b4:8c\"",
    "Esp32C6RmtWs281xDriver: 2 WS281x channels for 2 declared",
    "ESP-NOW radio ready",
    "[RECOVERY] boot complete",
    "M!{\"id\":0,\"msg\":{\"heartbeat\":{",
    "\"totalBytes\":325536",
    "[stack] heartbeat: high-water",
];

struct Run {
    m: Esp32C6Machine,
    lines: Vec<String>,
    /// `TIMED_OUT` at 3 s and at the end.
    timed_out: (u32, u32),
}

fn run(host: UsbHost) -> Option<Run> {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED_NO_FLASH) {
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
fn g2_1_attached_and_draining_from_boot_delivers_the_boot_the_hello_and_a_heartbeat() {
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
    let hello_at = text.find("M!{\"id\":0,\"msg\":{\"hello\"").unwrap();
    let last_init_at = text.rfind("[INIT] ").unwrap();
    assert!(
        last_init_at < hello_at,
        "every [INIT] line precedes the hello"
    );
    assert!(
        r.m.usb_sj_tried().is_empty(),
        "tried: {:?}",
        r.m.usb_sj_tried().text()
    );
    assert_eq!(
        text.matches("\"heartbeat\":{").count(),
        1,
        "one heartbeat in 5.5 s"
    );

    // esp-println never timed out: the host drained every packet.
    assert_eq!(r.timed_out, (0, 0), "TIMED_OUT at 3 s and at the end");
    let usb_spins: Vec<&String> = r
        .lines
        .iter()
        .filter(|l| l.contains(" SPIN ") && l.contains("USB_DEVICE"))
        .collect();
    assert!(usb_spins.is_empty(), "{usb_spins:?}");

    // The connected path armed RX at least once (read_serial's future).
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

    // For the PR body: the first 20 delivered lines, the heap trio's third
    // figure, and the log's digest.
    let free = text
        .split("\"freeBytes\":")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .expect("freeBytes in the heartbeat");
    println!("G2-1 delivered log, first 20 lines:");
    for l in text.lines().take(20) {
        println!("  | {l}");
    }
    println!(
        "G2-1 freeBytes at 5 s = {free} (UART0-link 266688; esp-emu 266792) — recorded, not gated"
    );
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
fn g2_3_attached_idle_from_boot_shows_the_not_draining_signature_and_the_probe() {
    let Some(r) = run(UsbHost::Attached { draining: false }) else {
        return;
    };
    // Nothing reached a host. The first `[INIT]` line is committed and
    // HELD in the endpoint for the whole run (a closed port never takes
    // it), so it is on neither log; what was merely tried starts with the
    // hello's first 64-byte chunk, dropped into the committed FIFO.
    assert!(r.m.usb_sj().is_empty(), "{:?}", r.m.usb_sj().text());
    let tried = r.m.usb_sj_tried().text();
    assert!(
        tried.starts_with("\nM!{\"id\":0,\"msg\":{\"hello\":{\"proto\":20,"),
        "{tried:?}"
    );
    assert!(!tried.contains("[INIT]"), "{tried:?}");
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

    // The commits: `wr_done` writes with the `ep1` writes since the previous
    // one. A `wr_done` with nothing pushed is esp-println's `Printer::flush`
    // on the `TIMED_OUT` path (a flush of nothing, one per formatted
    // fragment) — counted, not a commit.
    let mut commits: Vec<(Cycles, usize, Vec<u8>)> = Vec::new();
    let mut empty_flushes = 0usize;
    let mut pushed: Vec<u8> = Vec::new();
    for l in &r.lines {
        if l.contains("W4 USB_DEVICE+0x000 ep1 ") {
            pushed.push(value_of(l) as u8);
        } else if l.contains("W4 USB_DEVICE+0x004 ep1_conf") && value_of(l) & 1 != 0 {
            if pushed.is_empty() {
                empty_flushes += 1;
            } else {
                commits.push((cycle_of(l), pushed.len(), std::mem::take(&mut pushed)));
            }
        }
    }
    // Before 2 s: the boot line, then at least two protocol commits with
    // the 250 ms write timeout between them — the two timeouts that latch
    // "not draining".
    let early: Vec<&(Cycles, usize, Vec<u8>)> =
        commits.iter().filter(|c| c.0 < 2_000 * MS).collect();
    assert!(early.len() >= 3, "{} commits before 2 s", early.len());
    let timeouts = early
        .windows(2)
        .filter(|w| w[1].0 - w[0].0 >= 250 * MS && w[1].0 - w[0].0 < 260 * MS)
        .count();
    assert!(
        timeouts >= 2,
        "fewer than two 250 ms write timeouts before 2 s: {:?}",
        early.iter().map(|c| (c.0 / MS, c.1)).collect::<Vec<_>>()
    );
    // After the latch: one-byte `\n` probes, ≈ 2 s apart, and nothing else.
    let late: Vec<&(Cycles, usize, Vec<u8>)> =
        commits.iter().filter(|c| c.0 >= 2_500 * MS).collect();
    assert!(!late.is_empty(), "no probe after 2.5 s");
    for c in &late {
        assert_eq!(
            (c.1, c.2.as_slice()),
            (1, &b"\n"[..]),
            "a non-probe commit at {} ms",
            c.0 / MS
        );
    }
    for w in late.windows(2) {
        let gap = (w[1].0 - w[0].0) / MS;
        assert!((1_800..=2_300).contains(&gap), "probe gap {gap} ms");
    }
    assert_alive(&r);
    // io_task is sequential: while its writes time out it never reaches
    // `read_serial`, and after the latch it is not connected — so the RX
    // path is never armed here, exactly as with no host.
    let rx_armed = r
        .lines
        .iter()
        .filter(|l| l.contains("W4 USB_DEVICE+0x010 int_ena"))
        .any(|l| value_of(l) & SERIAL_OUT_RECV_PKT != 0);
    assert!(!rx_armed, "int_ena.serial_out_recv_pkt was written set");

    println!("G2-3 commits (ms, bytes), plus {empty_flushes} empty flushes:");
    for c in &commits {
        println!(
            "  {:>5} ms  {} byte(s){}",
            c.0 / MS,
            c.1,
            if c.1 == 1 && c.2 == b"\n" && c.0 >= 2_500 * MS {
                "  (probe)"
            } else if c.1 == 1 && c.2 == b"\n" {
                "  (a log line's leading newline, timed out)"
            } else {
                ""
            }
        );
    }
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
