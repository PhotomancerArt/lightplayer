//! G6-3: the shipped image with no host on USB-Serial-JTAG. The observable
//! is register and static state, not a log line (`periph/usb_sj.rs`):
//! esp-println's `TIMED_OUT` latches after its one 50,000-iteration wait,
//! the connected path never arms `int_ena.serial_out_recv_pkt`, the sink
//! holds what the guest tried to print, and the machine is idle in `wfi`
//! with the tick and the RWDT feeds alive.
//!
//! The image is `esp32c6,server,radio,memory_fs` — the shipped set minus
//! flash (director note 2; the flash-backed one spins on `SPI1.cmd` until
//! M4). `#[ignore]`d for the usual reason (`test_support`).

use lp_emu_esp_common::trace::SharedBuffer;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, Outcome, StopCondition, TimeGrade};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};

const GATE_US: u64 = 5_000_000;
const USB_INT_ENA: u32 = memmap::periph::USB_DEVICE + 0x10;
const SERIAL_OUT_RECV_PKT: u32 = 1 << 2;

#[test]
#[ignore = "needs the fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn with_no_host_the_printer_times_out_once_and_the_rx_path_is_never_armed() {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED_NO_FLASH) {
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
    // What the guest tried to print sits in the observation sink: the first
    // esp-println line, committed by its newline before the FIFO filled.
    let usb = m.usb_sj().text();
    assert!(usb.starts_with("[INIT] "), "{usb:?}");
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
}
