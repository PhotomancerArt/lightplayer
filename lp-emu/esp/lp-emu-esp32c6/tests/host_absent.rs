//! G6-3 / M6 G2-2: the shipped image with no host on USB-Serial-JTAG. The
//! observable is register and static state, not a log line
//! (`periph/usb_sj.rs`): esp-println's `TIMED_OUT` latches after its one
//! 50,000-iteration wait, the connected path never arms
//! `int_ena.serial_out_recv_pkt`, the **observation** log holds what the
//! guest tried to print while the **delivered** log is empty (nobody
//! received anything), and the machine is idle in `wfi` with the tick and
//! the RWDT feeds alive.
//!
//! The image is `esp32c6,server,radio,memory_fs` — the shipped set minus
//! flash (director note 2; the flash-backed one spins on `SPI1.cmd` until
//! M4). `#[ignore]`d for the usual reason (`test_support`).

use lp_emu_esp_common::pins::RouteSource;
use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, Outcome, StopCondition, TimeGrade};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};

const GATE_US: u64 = 5_000_000;
const USB_INT_ENA: u32 = memmap::periph::USB_DEVICE + 0x10;
const RMT_CH0_TX_CONF0: u32 = memmap::periph::RMT + 0x10;
const SERIAL_OUT_RECV_PKT: u32 = 1 << 2;

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn with_no_host_the_printer_times_out_once_and_the_rx_path_is_never_armed() {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("host_absent", &reason);
            return;
        }
    };
    let buf = SharedBuffer::new();
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .time_grade(TimeGrade::T1)
        .trace(
            Box::new(buf.clone()),
            vec!["USB_DEVICE".to_string(), "LP_WDT".to_string()],
        )
        .build()
        .expect("the shipped-minus-flash image builds a machine");
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "expected the emulated deadline, got {outcome:?}"
    );
    assert_eq!(
        m.bus.unmapped_reads() + m.bus.unmapped_writes(),
        0,
        "unmapped"
    );
    let lines = buf.lines();

    // `TIMED_OUT == 1`: the 50,000-spin latch, resolved by demangled path.
    let (_, timed_out) = m
        .peek_symbol("esp_println::serial_jtag_printer::TIMED_OUT")
        .expect("the image has esp-println's TIMED_OUT");
    assert_eq!(timed_out, 1, "TIMED_OUT");
    // And exactly one spin, esp-println's.
    let spins: Vec<&String> = lines.iter().filter(|l| l.contains(" SPIN ")).collect();
    assert_eq!(spins.len(), 1, "{spins:?}");
    assert!(spins[0].contains("USB_DEVICE+0x004 ep1_conf = 0x00000000 x10000"));

    // The connected path never armed RX: bit 2 of `int_ena` is clear at the
    // end and was never written set (esp-emu's `INT_ENA = 0x4`, inverted).
    let int_ena = m.peek_word(USB_INT_ENA).expect("USB_DEVICE.int_ena");
    assert_eq!(int_ena & SERIAL_OUT_RECV_PKT, 0, "int_ena = {int_ena:#x}");
    let armed: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("W4 USB_DEVICE+0x010 int_ena"))
        .filter(|l| {
            let v = u32::from_str_radix(l.rsplit("= 0x").next().unwrap(), 16).unwrap();
            v & SERIAL_OUT_RECV_PKT != 0
        })
        .collect();
    assert!(armed.is_empty(), "{armed:?}");
    // What the guest tried to print sits on the observation stream: the
    // first esp-println line, committed by its newline before the FIFO
    // filled. Nothing reached a host: the delivered log is empty.
    let usb = m.usb_sj_tried().text();
    assert!(usb.starts_with("[INIT] "), "{usb:?}");
    assert!(m.usb_sj().is_empty(), "{:?}", m.usb_sj().text());
    assert!(
        lines
            .iter()
            .any(|l| l.contains("USB_DEVICE wr_done:") && l.contains("no host will drain them"))
    );
    // `io_task`'s probe write after the seal is dropped and noted.
    assert!(
        lines
            .iter()
            .any(|l| l.contains("ep1 write with the IN FIFO committed (host absent)"))
    );

    // Idle in `wfi` with the tick and the RWDT feeds alive to the end.
    assert!(m.idle_skips() > 1_000, "{} idle skips", m.idle_skips());
    let feeds: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("W4 LP_WDT+0x014 wdtfeed = 0x80000000"))
        .collect();
    assert!(feeds.len() > 1_000, "{} feeds", feeds.len());
    let last_feed_cycle: u64 = feeds.last().unwrap().split_whitespace().next().unwrap()
        ["cyc=".len()..]
        .parse()
        .unwrap();
    assert!(
        last_feed_cycle > (GATE_US - 100_000) * memmap::CYCLES_PER_US,
        "last feed at {last_feed_cycle}"
    );
    assert!(!lines.iter().any(|l| l.contains("EXPIRED")));
    // UART0 is silent on the shipped image: the console is USB-Serial-JTAG.
    assert!(m.uart0().is_empty());

    // M6 P1b, gate G1b-4 — what the connection stamps say when there is no
    // host at all, and it is NOT what the phase brief predicted.
    //
    // The brief (and the M6 discovery's "absent" row) expected zero: no
    // enumeration, so `is_connected()` false, so no write, so no timeout, so
    // no latch. The machine says one, and the machine is right — the monitor
    // starts OPTIMISTIC (`host_draining = true`, `no_sof_count = 0`) and the
    // enumeration verdict needs three polls, while one blocked write costs
    // 250 ms and holds the loop for the whole of it. So:
    //
    //   poll 1 -> no_sof 1, still "enumerated", write attempted, 250 ms, timeout
    //   poll 2 -> no_sof 2, still "enumerated", write attempted, 250 ms, timeout
    //             => two in a row: LATCH, and the stamp fires
    //   poll 3 -> no_sof 3, NOT enumerated: the latch is reset to draining
    //             (a disconnect starts the next enumeration from a clean
    //             slate) and `is_connected()` is false from here on, so
    //             nothing is ever written or timed out again.
    //
    // The number that matters is therefore not the count but the PAIR. With
    // no host the link never recovers, because nothing ever drained it:
    // `HOST_DRAINING_AGAIN_MS` stays at the never-happened sentinel. That is
    // what makes the silicon figures mean something — a transcript showing
    // both stamps is showing a transition this configuration cannot produce.
    let mut stamp = |sym: &str| {
        m.peek_symbol(sym)
            .unwrap_or_else(|| panic!("the image carries {sym}"))
            .1
    };
    let silences = stamp("fw_esp32_common::serial::link_counters::NOT_DRAINING_COUNT");
    let latched_at = stamp("fw_esp32_common::serial::link_counters::HOST_NOT_DRAINING_MS");
    let resumed_at = stamp("fw_esp32_common::serial::link_counters::HOST_DRAINING_AGAIN_MS");
    const NEVER: u32 = u32::MAX;

    assert_eq!(
        resumed_at, NEVER,
        "nothing ever drained this link, so it can never have resumed"
    );
    assert_eq!(
        silences, 1,
        "exactly one latch, from the optimistic window before enumeration \
         lapses; after that `is_connected()` gates every write and no second \
         silence is possible"
    );
    assert_ne!(latched_at, NEVER, "the one latch stamped its instant");
    assert!(
        u64::from(latched_at) < GATE_US / 1_000,
        "the latch is inside the run it happened in: {latched_at} ms"
    );
    println!(
        "[P1b] host absent: notDrainingCount={silences} \
         hostNotDrainingMs={latched_at} hostDrainingAgainMs=NEVER"
    );

    // M5 P1 G1-2: the boot configures two RMT channels (`mem_size 1` each,
    // the shipped two-channel plan) and never starts one — no project is
    // loaded, so no frame is ever sent.
    assert_eq!(m.rmt_frames_ended(0), 0);
    assert_eq!(m.rmt_frames_ended(1), 0);
    assert!(m.rmt_words(0).is_empty() && m.rmt_pulses(0).is_empty());
    // M5 P2 G2-2: and no peripheral signal reaches a pad. The one pad the
    // boot routes is `init_board`'s plain GPIO output on gpio16, following
    // `GPIO_OUT` — not a waveform, and nothing decodable on it.
    let routed = m.routed_pads();
    assert!(
        routed
            .iter()
            .all(|(_, source)| *source == RouteSource::GpioOut),
        "a peripheral signal reached a pad: {routed:?}"
    );
    assert!(m.frames(18).is_empty());
    assert_eq!(m.pin_edges(18), 0);
    let conf0 = m.peek_word(RMT_CH0_TX_CONF0).expect("RMT.ch0_tx_conf0");
    assert_eq!(conf0 & (1 << 6), 1 << 6, "idle_out_en: {conf0:#010x}");
    assert_eq!(conf0 & (1 << 5), 0, "idle_out_lv 0: {conf0:#010x}");
    assert_eq!((conf0 >> 8) & 0xff, 1, "div_cnt 1: {conf0:#010x}");
    assert_eq!((conf0 >> 16) & 0x7, 1, "mem_size 1: {conf0:#010x}");
    assert_eq!(conf0 & 0x0100_0007, 0, "the strobes read 0: {conf0:#010x}");
}
