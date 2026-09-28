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
//! Since wire proto 30 (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`)
//! the link is an lp-link: a frame that loses bytes fails its checksum and is
//! resent, so the question this test asks moved from "does a reply arrive
//! torn" to "does the link have to recover from damage at all" — and, for
//! the ungated image, "does the recovery keep the damage from the app". The
//! conversation is the product's own link host, in process, stepping the
//! machine in emulated time (it was a `--usb-script` of `M!` lines).
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
//! 2. **the timing**, reported: on the lp-link image the gate's check comes
//!    first (before proto 30 it came second, and step 2 asserted that);
//! 3. **a lag past the ungated write**: the ungated image's frames arrive
//!    damaged — counted by the host's link, never handed to the app — and,
//!    because the loss repeats on every resend, its replies do not get
//!    through at all: the link hides occasional damage, not a systematic
//!    one, so the gate is still needed. The gated image never writes into
//!    the lag, but on this image its check lands inside it and it loses the
//!    drain's wake (an open defect, pinned here: 2026-09-27, see step 3).
//!
//! The lag is chosen from step 1's measurements (just past the later of the
//! two), not written down, so a firmware change that moves either path moves
//! the lag with it.
//!
//! It lives in `lp-cli` because the link host is a product crate, which
//! nothing under `lp-emu/` may depend on (the MIT fence).
//!
//! `#[ignore]`d and run by `just test-emu-c6-cli`: it needs two built
//! `fw-esp32c6` ELFs (`LP_EMU_BUILD_FW=1`).

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::control::ControlCommand;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpc_wire::{ClientMessage, ClientRequest, LinkCounters};

/// The conversation, by emulated millisecond: the free lag set (which also
/// restarts the block's wake measurements, so boot is not in them), then a
/// Hello every [`EVERY_MS`]. A packed Hello is several packets, so every
/// reply gives esp-hal's loop wakes to write straight into.
const LAG_AT_MS: u64 = 1_900;
const FIRST_AT_MS: u64 = 2_000;
const EVERY_MS: u64 = 20;
const REQUESTS: u64 = 40;
const FIRST_ID: u64 = 10;

#[test]
#[ignore = "needs two built fw-esp32c6 ELFs; `just test-emu-c6-cli` runs it"]
fn a_free_lag_past_esp_hals_write_damages_only_the_ungated_images_frames() {
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
        eprintln!("no lag, {name}: {}", run.summary());
        assert_eq!(run.host.damaged, 0, "{name}: {}", run.summary());
        assert_eq!(run.tried, 0, "{name}: {} B refused", run.tried);
        assert_eq!(run.replies, REQUESTS as usize, "{name}: {}", run.summary());
    }

    // 2. The timing, reported. Before proto 30 the gate's free check came
    //    LATER than esp-hal's unchecked write, and a lag between the two
    //    separated the images. On the lp-link image the link task's gated
    //    path checks SOONER (measured at lp-emu:esp32c6:t1: the gate at
    //    ~8.6 us after a drain, esp-hal's write at ~9.3 us), so there is no
    //    "between" — and none is needed: the gate re-checks and waits for as
    //    long as the buffer is not free, whatever the lag. What separates the
    //    two images now is any lag reaching past the ungated write.
    let write = before
        .write_ns
        .expect("the ungated image wrote after a drain");
    let check = after
        .check_ns
        .expect("the gated image checked the buffer after a drain");
    // (The ungated image reads `ep1_conf` too, later: esp-hal's RX drain
    // tests `serial_out_ep_data_avail` in the same register. Not a free
    // check, so its `ep1_conf` span is not the comparison.)
    eprintln!(
        "esp-hal writes {write} ns after a drain at the soonest; the gate checks at {check} ns"
    );

    // 3. A lag past the ungated write. The ungated image's frames lose a few
    //    bytes at every packet boundary and arrive damaged; the host's link
    //    counts each and never hands the app a corrupt message — but the loss
    //    is SYSTEMATIC (every resend of a multi-packet frame is damaged the
    //    same way), so resending cannot get those replies through. The link
    //    hides occasional damage, not a driver that loses bytes on every
    //    packet. The gated image never writes into the lag — the gate's first
    //    job — but, on this image, its check lands inside the lag and it loses
    //    the drain's wake: docs/defects/2026-09-27-the-in-endpoint-gate-loses-
    //    the-drains-wake-inside-a-free-lag.md (open, and conditional on the
    //    lag hypothesis).
    let lag = write.max(check) + 1_000;
    let (before, after) = both(&ungated, &gated, lag);

    eprintln!("free lag {lag} ns, ungated: {}", before.summary());
    assert!(
        before.host.damaged > 0,
        "no frame arrived damaged: {}",
        before.summary()
    );
    // Reported, not gated: the bytes refused per damaged frame. A frame here
    // is several packets and loses a few bytes at each boundary.
    eprintln!(
        "{} B refused over {} damaged frames",
        before.tried, before.host.damaged
    );
    // The damage never reaches the app as a corrupt message...
    assert_eq!(before.host.payload_errors, 0, "{}", before.summary());
    assert_eq!(before.app_errors, 0, "{}", before.summary());
    // ...and resending cannot beat a loss on every packet.
    assert!(
        before.replies < REQUESTS as usize,
        "every reply got through a systematic loss — the gate would no longer be \
         needed, which is a finding to look at: {}",
        before.summary()
    );

    eprintln!("free lag {lag} ns, gated: {}", after.summary());
    // The gate's own claim: nothing it wrote landed in the lag.
    assert_eq!(after.tried, 0, "{} B refused", after.tried);
    // Every frame of the gated image that arrived damaged is one the board
    // itself gave up on: under the open defect below, a frame's write waits
    // out its 250 ms bound after its first packet(s) went out, and the host
    // holds that half frame until the next frame's opening `0x00` closes it
    // and the CRC fails. (Until the host's partial-frame wait went from the
    // 50 ms text idle to `LinkConfig::frame_abandon`'s 3 s, e726f7083, the
    // same half frame was dropped quietly as a stale partial and counted
    // there — the event is the same, the counter moved.) Damage the board did
    // not cause itself would exceed its own count of abandoned writes.
    assert!(
        after.host.damaged <= after.board_write_timeouts,
        "a gated frame arrived damaged that the board did not abandon: {}",
        after.summary()
    );
    assert_eq!(after.host.payload_errors, 0, "{}", after.summary());
    // Nothing reached the app corrupt, and the session never reset.
    assert_eq!(after.app_errors, 0, "{}", after.summary());
    // The open defect's signature, pinned so a fix is noticed: the gate's
    // writes wait out their bound instead of the drain. When the defect is
    // fixed this flips to `after.replies == REQUESTS` and no write timeout.
    assert!(
        after.board_write_timeouts > 0 && after.replies < REQUESTS as usize,
        "the gate no longer loses the drain's wake inside a free lag — the open defect \
         2026-09-27-the-in-endpoint-gate-loses-the-drains-wake-inside-a-free-lag.md looks \
         fixed: make this assert every reply and no write timeout, and close it: {}",
        after.summary()
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
    /// The host end's link counters.
    host: LinkCounters,
    /// Link resets and messages that did not parse, host side.
    app_errors: u32,
    /// Distinct Hellos of this conversation answered.
    replies: usize,
    /// Bytes the guest wrote and the block refused.
    tried: usize,
    /// The soonest `ep1` write after a drain, in ns.
    write_ns: Option<u64>,
    /// The soonest `ep1_conf` read after a drain, in ns.
    check_ns: Option<u64>,
    /// The board's own count of frame writes it gave up on.
    board_write_timeouts: u32,
}

impl Run {
    fn summary(&self) -> String {
        format!(
            "lp-emu:esp32c6:t1 — {} of {REQUESTS} Hellos answered, {} B refused; host link {} \
             damaged, {} stale partials, {} resent, {} resets, {} payload errors; board {} write \
             timeouts; \
             next write {:?} ns / next free check {:?} ns after a drain",
            self.replies,
            self.tried,
            self.host.damaged,
            self.host.stale_partials,
            self.host.resends,
            self.host.resets.total,
            self.host.payload_errors,
            self.board_write_timeouts,
            self.write_ns,
            self.check_ns,
        )
    }
}

/// The conversation on `elf`, in process over the product's link host, the
/// model's free lag set to `lag_ns` at [`LAG_AT_MS`].
fn converse(elf: &std::path::Path, lag_ns: u64) -> Run {
    let ms = 1_000 * memmap::CYCLES_PER_US;
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(FlashBacking::Blank)
        .strict(true)
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .usb_script(vec![(LAG_AT_MS * ms, ControlCommand::FreeLag(lag_ns))])
        .build()
        .expect("the image builds a machine");
    let mut host = EmuLinkHost::new(C6Board::new(machine).unwrap(), 0x0F4E_E1A6, true);
    for n in 0..REQUESTS {
        host.run_until((FIRST_AT_MS + n * EVERY_MS) * 1_000, None)
            .expect("the run");
        host.send(&ClientMessage {
            id: FIRST_ID + n,
            msg: ClientRequest::Hello,
        })
        .expect("the link takes a request");
    }
    host.run_until((FIRST_AT_MS + REQUESTS * EVERY_MS + 1_500) * 1_000, None)
        .expect("the run");

    let ids: std::collections::BTreeSet<u64> = host
        .messages
        .iter()
        .map(|m| m.id)
        .filter(|id| (FIRST_ID..FIRST_ID + REQUESTS).contains(id))
        .collect();
    let m = &mut host.board.machine;
    let stats = m.usb_in_wake_stats().expect("the USB block");
    let ns = |cycles: u64| cycles * 1_000 / stats.cycles_per_us;
    let write_ns = (stats.write.count > 0).then(|| ns(stats.write.min));
    let check_ns = (stats.free_read.count > 0).then(|| ns(stats.free_read.min));
    let tried = m.usb_sj_tried().len();
    let board_write_timeouts = m
        .peek_symbol("fw_esp32_common::usb_link::usb_link_counters::WRITE_TIMEOUTS")
        .expect("the image carries the link task's counters")
        .1;
    Run {
        host: host.counters(),
        app_errors: host.link_errors,
        replies: ids.len(),
        tried,
        write_ns,
        check_ns,
        board_write_timeouts,
    }
}
