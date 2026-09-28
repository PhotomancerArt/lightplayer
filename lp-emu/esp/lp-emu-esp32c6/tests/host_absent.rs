//! G6-3 / M6 G2-2: the shipped image with no host on USB-Serial-JTAG. The
//! observable is register and static state, not a log line
//! (`periph/usb_sj.rs`): esp-println's `TIMED_OUT` latches after its one
//! 50,000-iteration wait, the **observation** log holds what the guest tried
//! to print while the **delivered** log is empty (nobody received anything),
//! and the machine is idle in `wfi` with the tick and the RWDT feeds alive.
//!
//! Since wire proto 30 the USB port belongs to the lp-link task
//! (`fw_esp32_common::usb_link`, plan `lp-link-usb-cutover`), and two claims
//! changed with it, both on purpose. The RX path IS armed now
//! (`int_ena.serial_out_recv_pkt`): the link task listens from boot, because
//! a host that plugs in later opens the link with a handshake the board has
//! to hear — the old `io_task` only armed it once it believed a host was
//! connected. And the old not-draining latch's stamps are gone with the
//! latch; what stands in their place is the link task's own edge counters:
//! every frame it would have sent (its handshake, retried on its timer) was
//! DISCARDED for want of a host (no SOF), none was written, and no write
//! timed out.
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
fn with_no_host_the_printer_times_out_once_and_the_link_task_discards_its_frames() {
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

    // The link task listens from boot: RX is armed (bit 2 of `int_ena`),
    // with no host to hear. (Before proto 30 the `io_task` armed it only for
    // a host it believed connected, and this asserted the opposite.)
    let int_ena = m.peek_word(USB_INT_ENA).expect("USB_DEVICE.int_ena");
    assert_eq!(
        int_ena & SERIAL_OUT_RECV_PKT,
        SERIAL_OUT_RECV_PKT,
        "int_ena = {int_ena:#x}"
    );
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
    // The link task never writes into the sealed endpoint. Until 2026-09-24
    // the `io_task`'s first write went straight into esp-println's committed
    // packet and the model dropped and noted it (`ep1 write with the IN FIFO
    // committed (host absent)`) — esp-hal 1.1.1's `write_async` reads no free
    // bit. The TX half is behind the IN-endpoint gate
    // (`fw_esp32_common::serial::in_endpoint`, PR #805, ported to the C6 for
    // docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md),
    // and since proto 30 the link task does not even reach it with no SOF:
    // it discards the frame (below). This is not a transcript comparison — the
    // `usb-host-absent` transcripts are text a host would see, and with no
    // host both images show nothing — so no capture is owed for this line.
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("ep1 write with the IN FIFO committed")),
        "the io_task wrote into the sealed endpoint past the IN-endpoint gate"
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

    // What the link task says about a board with no host at all. It
    // retries its handshake on its own timer; with no SOF those frames are
    // discarded before they reach the endpoint. The server's boot hello and
    // heartbeats have no link to ride, and are dropped and counted.
    // (Until proto 30 this read the not-draining latch's stamps, which went
    // with the latch.)
    let mut counter = |name: &str| {
        let sym = format!("fw_esp32_common::usb_link::usb_link_counters::{name}");
        m.peek_symbol(&sym)
            .unwrap_or_else(|| panic!("the image carries {sym}"))
            .1
    };
    let discarded = counter("FRAMES_DISCARDED_NO_HOST");
    let timeouts = counter("WRITE_TIMEOUTS");
    let (no_link, full) = (
        counter("REPLIES_DROPPED_NO_LINK"),
        counter("REPLIES_DROPPED_FULL"),
    );
    assert!(
        discarded > 0,
        "the link task retries its handshake, and with no host every frame is discarded"
    );
    // Before the task has watched long enough to see there is no SOF, its
    // first write or two wait on the gate and time out (the old monitor had
    // the same optimistic window); after that every frame is discarded.
    assert!(
        timeouts < discarded,
        "{timeouts} write timeouts against {discarded} discards: the link task kept \
         writing into an endpoint no host drains"
    );
    assert!(
        no_link >= 1 && full == 0,
        "the server's unsolicited hello and heartbeats go nowhere with no link, and are \
         counted as dropped; nothing was dropped for a full send budget"
    );
    println!(
        "[lp-link] host absent: {discarded} frame(s) discarded with no host, \
         {timeouts} write timeouts, {no_link} unsolicited message(s) dropped with no link"
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
