//! The emulated C6 lost bytes inside a packed frame the way the real one did
//! — **under a hypothesis** — until the IN-endpoint gate stopped it, and
//! since the esp-hal back-port (#855) esp-hal's own write stops it too.
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
//! again (the model's *free lag*, `--usb-in-free-lag <ns>`). Stock esp-hal
//! 1.1.1's `write_async` wrote a frame's next 64-byte packet the moment its
//! future woke, with no free check, so its first few bytes landed inside the
//! lag and were refused — a loss of a few bytes, not a packet, with nothing
//! logged.
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
//! **Since the esp-hal back-port** (upstream #6104 in `third_party/esp-hal`,
//! README-LP.md's third diff) esp-hal no longer writes straight out of its
//! wake either: after every `wr_done` it waits for a new `serial_in_empty`
//! and then re-reads `serial_in_ep_data_free`, waiting again while it is
//! clear. So the ungated image writes nothing into the lag any more, and the
//! loss this test was built to show is gone from both images. What a lag
//! costs now is the same on both: the free bit returns with no second edge,
//! and a wait on that edge sits out its 250 ms write bound — the open defect
//! 2026-09-27 (step 3), which since the back-port is esp-hal's own wait as
//! much as the gate's.
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
//! 1. **no lag**: neither loses a byte in the conversation, which is the
//!    finding that the model's default has no path to this loss. The block
//!    measures how soon after a drain each image touches the endpoint: the
//!    ungated one's next `ep1` write, the gated one's next `ep1_conf` read;
//! 2. **the timing**, reported: on the lp-link image the gate's check comes
//!    first (before proto 30 it came second, and step 2 asserted that);
//! 3. **a lag past the ungated write**: neither image writes a byte into
//!    the lag. Both lose the drain's wake inside it and wait out their write
//!    bound (an open defect, pinned here: 2026-09-27, see step 3); the half
//!    frames those abandoned writes leave are the only damage the host's
//!    link sees, and none reaches the app.
//!
//! The lag is chosen from step 1's measurements (just past the later of the
//! two), not written down, so a firmware change that moves either path moves
//! the lag with it. What it is chosen from is each image's **typical** wake,
//! not its soonest: the median, over the 40 requests, of each request's
//! soonest `ep1` write and `ep1_conf` read after a drain. The soonest over
//! the whole run is the wrong statistic. Almost every drain finds the CPU
//! idle and takes the same path (1,503 cycles to the ungated image's check),
//! but now and then a drain lands while the CPU is already running, and that
//! one wake is shorter (769–1,317 cycles, measured on PR #894's images).
//! Whether any drain lands like that is a matter of phase between the guest's
//! other work and the host's drain cadence, so it moves with any change to
//! the image, and differs between a desk build and CI's of one commit.
//! On #894 the run's soonest became one of those and put the lag below the
//! wake nearly every drain takes, so the test no longer covered the path it
//! is about.
//!
//! **Boot has two writers** since the C6's link task got a thread of its own
//! (`io-thread`, plan `lp2025/2026-10-01-1756-c6-link-io-thread`): the link
//! thread sends its SYNs while the main thread is still printing the boot
//! text raw through esp-println, so "one writer on the IN endpoint" holds
//! only once boot is over. The ungated image writes a SYN frame onto a busy
//! endpoint there and the block refuses it (23 B, measured at
//! lp-emu:esp32c6:t1); the gated image waits for the buffer and refuses
//! nothing. That is the gate doing its job on a path that needs no
//! hypothesis, so the gated image is held to zero over the whole run, and
//! the lag questions below count only the conversation.
//!
//! It lives in `lp-cli` because the link host is a product crate, which
//! nothing under `lp-emu/` may depend on (the MIT fence).
//!
//! `#[ignore]`d and run by `just test-emu-c6-cli`: it needs two built
//! `fw-esp32c6` ELFs (`LP_EMU_BUILD_FW=1`).

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp32c6::control::{ControlCommand, ControlReply};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, Esp32C6Machine, TimeGrade, UsbHost};
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
fn a_free_lag_tears_neither_image_once_esp_hal_checks_the_free_bit() {
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
    // Boot's two writers (module docs): the gate keeps the shipped image's
    // SYNs off a busy endpoint.
    assert_eq!(
        after.tried_in_boot, 0,
        "gated: {} B refused during boot",
        after.tried_in_boot
    );

    // 2. The timing, reported. Before proto 30 the gate's free check came
    //    LATER than esp-hal's unchecked write, and a lag between the two
    //    separated the images. On the lp-link image the link task's gated
    //    path checks SOONER (measured at lp-emu:esp32c6:t1: the gate at
    //    ~8.6 us after a drain, esp-hal's write at ~9.3 us, before the esp-hal
    //    back-port; ~9.6 us and ~9.7 us after it), so there is no "between" —
    //    and none is needed: the gate re-checks and waits for as long as the
    //    buffer is not free, whatever the lag. Since the back-port esp-hal
    //    does the same, so no lag separates the two images any more (step 3).
    let write = before
        .write_ns
        .expect("the ungated image wrote after a drain");
    let check = after
        .check_ns
        .expect("the gated image checked the buffer after a drain");
    // (The ungated image reads `ep1_conf` too, and since the back-port that
    // includes esp-hal's own free check, before its write. It is printed in
    // the summary, not compared: esp-hal's RX drain reads the same register
    // for `serial_out_ep_data_avail`, so an `ep1_conf` read is no proof of a
    // free check. Step 3 is the proof.)
    eprintln!(
        "esp-hal writes {write} ns after a drain at the soonest; the gate checks at {check} ns"
    );
    // The typical wake, which the lag is chosen from (module docs).
    let typical_write = before
        .typical_write_ns
        .expect("the ungated image wrote after a drain");
    let typical_check = after
        .typical_check_ns
        .expect("the gated image checked the buffer after a drain");
    eprintln!(
        "typically, esp-hal writes {typical_write} ns after a drain; the gate checks at \
         {typical_check} ns"
    );

    // 3. A lag past the ungated write. Before the esp-hal back-port the
    //    ungated image wrote each packet straight out of its wake, lost a few
    //    bytes at every packet boundary into the lag, and resending could not
    //    beat a loss on every packet. esp-hal now re-reads the free bit after
    //    every wake (upstream #6104), so the ungated image writes nothing into
    //    the lag either — the gate's first job, now done twice. Both images
    //    then lose the drain's wake inside the lag, esp-hal's wait as well as
    //    the gate's: docs/defects/2026-09-27-the-in-endpoint-gate-loses-the-
    //    drains-wake-inside-a-free-lag.md (open, and conditional on the lag
    //    hypothesis).
    let lag = typical_write.max(typical_check) + 1_000;
    let (before, after) = both(&ungated, &gated, lag);
    for (name, run) in [("ungated", &before), ("gated", &after)] {
        eprintln!("free lag {lag} ns, {name}: {}", run.summary());
        // Nothing either image wrote landed in the lag.
        assert_eq!(run.tried, 0, "{name}: {} B refused", run.tried);
        // Every frame that arrived damaged is one the board itself gave up
        // on: under the open defect below, a frame's write waits out its
        // 250 ms bound after its first packet(s) went out, and the host
        // holds that half frame until the next frame's opening `0x00` closes
        // it and the CRC fails. (Until the host's partial-frame wait went from
        // the 50 ms text idle to `LinkConfig::frame_abandon`'s 3 s, e726f7083,
        // the same half frame was dropped quietly as a stale partial and
        // counted there — the event is the same, the counter moved.) Damage
        // the board did not cause itself would exceed its own count of
        // abandoned writes.
        assert!(
            run.host.damaged <= run.board_write_timeouts,
            "{name}: a frame arrived damaged that the board did not abandon: {}",
            run.summary()
        );
        assert_eq!(run.host.payload_errors, 0, "{name}: {}", run.summary());
        // Nothing reached the app corrupt, and the session never reset.
        assert_eq!(run.app_errors, 0, "{name}: {}", run.summary());
        // The open defect's signature, pinned so a fix is noticed: the
        // writes wait out their bound instead of the drain. When the defect
        // is fixed this flips to `run.replies == REQUESTS` and no write
        // timeout. A fix in the gate alone would not flip the ungated image:
        // esp-hal's own post-`wr_done` wait has the same shape.
        assert!(
            run.board_write_timeouts > 0 && run.replies < REQUESTS as usize,
            "{name}: the image no longer loses the drain's wake inside a free lag — the \
             open defect 2026-09-27-the-in-endpoint-gate-loses-the-drains-wake-inside-a-\
             free-lag.md looks fixed: make this assert every reply and no write timeout, \
             and close it: {}",
            run.summary()
        );
    }
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
    /// Bytes the guest wrote and the block refused, from the first request
    /// on.
    tried: usize,
    /// Bytes the block refused before the first request: boot, where the
    /// link thread and esp-println both write (module docs).
    tried_in_boot: usize,
    /// The soonest `ep1` write after a drain, in ns.
    write_ns: Option<u64>,
    /// The soonest `ep1_conf` read after a drain, in ns.
    check_ns: Option<u64>,
    /// The median over the requests of each one's soonest `ep1` write after
    /// a drain, in ns: the typical wake (module docs).
    typical_write_ns: Option<u64>,
    /// The same for `ep1_conf` reads.
    typical_check_ns: Option<u64>,
    /// The board's own count of frame writes it gave up on.
    board_write_timeouts: u32,
}

impl Run {
    fn summary(&self) -> String {
        format!(
            "lp-emu:esp32c6:t1 — {} of {REQUESTS} Hellos answered, {} B refused ({} B in boot); \
             host link {} damaged, {} stale partials, {} resent, {} resets, {} payload errors; board {} write \
             timeouts; \
             next write {:?} ns / next free check {:?} ns after a drain at the soonest, \
             {:?} ns / {:?} ns typically",
            self.replies,
            self.tried,
            self.tried_in_boot,
            self.host.damaged,
            self.host.stale_partials,
            self.host.resends,
            self.host.resets.total,
            self.host.payload_errors,
            self.board_write_timeouts,
            self.write_ns,
            self.check_ns,
            self.typical_write_ns,
            self.typical_check_ns,
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
    let mut tried_in_boot = 0;
    // One window of wake stats per request, so one odd drain moves one
    // window's soonest, not the run's (module docs).
    let mut windows = Vec::new();
    for n in 0..REQUESTS {
        host.run_until((FIRST_AT_MS + n * EVERY_MS) * 1_000, None)
            .expect("the run");
        if n == 0 {
            tried_in_boot = host.board.machine.usb_sj_tried().len();
        } else {
            windows.push(soonest_wake(&mut host.board.machine));
        }
        restart_wake_stats(&mut host.board.machine, lag_ns);
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
    windows.push(soonest_wake(m));
    let soonest = |pick: fn(&(Option<u64>, Option<u64>)) -> Option<u64>| {
        windows.iter().filter_map(pick).min()
    };
    let typical = |pick: fn(&(Option<u64>, Option<u64>)) -> Option<u64>| {
        let mut v: Vec<u64> = windows.iter().filter_map(pick).collect();
        v.sort_unstable();
        v.get(v.len() / 2).copied()
    };
    let write_ns = soonest(|w| w.0);
    let check_ns = soonest(|w| w.1);
    let typical_write_ns = typical(|w| w.0);
    let typical_check_ns = typical(|w| w.1);
    let tried = m.usb_sj_tried().len() - tried_in_boot;
    let board_write_timeouts = m
        .peek_symbol("fw_esp32_common::usb_link::usb_link_counters::WRITE_TIMEOUTS")
        .expect("the image carries the link task's counters")
        .1;
    Run {
        host: host.counters(),
        app_errors: host.link_errors,
        replies: ids.len(),
        tried,
        tried_in_boot,
        write_ns,
        check_ns,
        typical_write_ns,
        typical_check_ns,
        board_write_timeouts,
    }
}

/// The soonest `ep1` write and `ep1_conf` read after a drain since the
/// stats last restarted, in ns.
fn soonest_wake(m: &mut Esp32C6Machine) -> (Option<u64>, Option<u64>) {
    let stats = m.usb_in_wake_stats().expect("the USB block");
    let ns = |cycles: u64| cycles * 1_000 / stats.cycles_per_us;
    (
        (stats.write.count > 0).then(|| ns(stats.write.min)),
        (stats.free_read.count > 0).then(|| ns(stats.free_read.min)),
    )
}

/// Restart the wake stats. Setting the lag is what restarts them, and
/// setting the lag already in force changes nothing else the guest sees.
fn restart_wake_stats(m: &mut Esp32C6Machine, lag_ns: u64) {
    let reply = m.control_line(&format!("free-lag {lag_ns}"));
    assert!(
        matches!(reply, ControlReply::Ok { .. }),
        "free-lag {lag_ns}: {reply:?}"
    );
}
