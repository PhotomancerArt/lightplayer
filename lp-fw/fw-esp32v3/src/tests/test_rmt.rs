//! The `rmt-chase` payload's **classic** harness — the C6's twin
//! (`fw-esp32c6/src/tests/test_rmt.rs`), on a chip that has no
//! `output::LedChannel`.
//!
//! Everything portable — the chase pattern, the FNV-1a checksum, the
//! `rmt-frame` record, the done marker — lives in
//! [`fw_checks::checks::rmt_chase`] and is unit-tested on the host. What is
//! here is the half that needs a chip: `init_board`, the RMT peripheral at
//! [`shared_driver::RMT_CLOCK`], the block plan, one channel routed onto
//! **IO18**, and the 10 ms gap between frames.
//!
//! # Why this drives the product's own backend (M4 ruling R5)
//!
//! The C6 harness opens an `output::LedChannel`; this chip has no such type.
//! Rather than port one — a second driver path whose bugs would be nobody
//! else's — the harness drives what the product drives:
//! [`shared_driver::DRIVER`] (the one [`lp_ws281x::Ws281xDriver`] on this
//! chip) over the [`v3_rmt`] backend, opened with the *same* register
//! sequence `Esp32V3RmtWs281xDriver::new` uses — plan, ISR, `configure_tx`,
//! `enable_tx_interrupts`, `clear_ram`, `configure_default_clock`, `init_tx`
//! — and the same `route_rmt_to_gpio` the slot pool uses per transmission.
//! Every line of that sequence is a call into the product module, never a
//! copy of one, so a change to the backend either compiles here or breaks
//! here.
//!
//! What the harness does *not* take from the product is the wire/slot pool,
//! the admission cap and the mailbox: one wire on one slot needs none of it,
//! and the payload is meant to be the simplest frame path this chip has.
//!
//! # Single core, deliberately
//!
//! **The APP core is never started.** `start_app_core_isr` is not called, so
//! `isr_on_app_core()` stays false, [`shared_driver::install_isr`] binds the
//! RMT handler on the PRO core, and the frame path is: fill both halves,
//! `tx_start`, spin, refill from the ISR, `tx_end`. Adding the second core
//! would add the wire pusher's slot pooling and its doorbell to a payload
//! whose whole job is to put one known frame on one pad and say what it was.
//! Do not "fix" this by starting core 1 — the dual-core path has its own
//! instruments (`ws281x_telemetry`, `[WS281X-WIRE]`), and a payload that
//! measured them would be measuring two things at once.
//!
//! # The record comes after the send
//!
//! `write_frame_record` runs once [`lp_ws281x::Ws281xDriver::send_blocking`]
//! has returned, which is after the channel reported complete — so the record
//! is the claim that this frame *reached the wire*, not that it was queued.
//! That is what makes the emulator's gate meaningful: the decoder reads the
//! pad, the guest says what it believes it sent, and both sides compute the
//! same [`fw_checks::checks::rmt_chase::fnv1a`] over the same bytes.
//!
//! The header, the records and the done marker all go through
//! `esp_println::Printer` rather than `log`, for the reason
//! `fw-checks/src/header.rs` gives and for the reason every other classic
//! payload harness does it: the payload's output must not depend on a logger
//! being installed. A logger *is* installed all the same, because esp-hal's
//! own `debug!`/`info!` output is part of what a boot capture is.

use esp_hal::delay::Delay;
use esp_hal::gpio::Level;
use esp_hal::rmt::{Rmt, TxChannelConfig, TxChannelCreator};
use fw_checks::checks::rmt_chase::{
    CHASES, FRAMES, FrameRecord, LEDS, chase_frame, frame_bytes, write_done, write_frame_record,
};
use log::info;
use lp_ws281x::ChannelTiming;

use crate::board::esp32v3::init::init_board;
use crate::output::rmt::shared_driver::{self, DRIVER};
use crate::output::rmt::v3_rmt::{self, BLOCK_WORDS, TX_PLAN};

/// The pad. IO18 is the DOM-Z-102's "data channel 1" and the wire M4's walk
/// uses; it is also the number the C6 harness drives, so the two payloads
/// name one pad between them.
const PAD: u8 = 18;

/// The RMT slot. Slot 0 owns the first memory block, and
/// `plan_for_declared(1)` gives it all four this chip's plan allows — a
/// 256-word window with 128-word halves, the widest refill geometry the cap
/// permits and the easiest one to reason about with a single transmitter.
const SLOT: u8 = 0;

/// Declared WS281x wires, for the block plan. **One**: this harness opens one
/// channel, and the manifest is the sole authority on channel count in the
/// product (`v3_rmt::plan_for_declared`), so the harness states its own.
const DECLARED_WIRES: usize = 1;

/// The gap between frames, matching the C6 harness's `Timer::after(10 ms)`.
///
/// Blocking rather than an `embassy_time::Timer`: this harness starts no
/// executor at all (see the module docs — it is `#[esp_hal::main]`, not
/// `#[esp_rtos::main]`), and a payload that needed a runtime to put a frame
/// on a wire would be measuring the runtime too.
const FRAME_GAP_MS: u32 = 10;

/// Run the `rmt-chase` payload. Never returns: a harness that fell off the
/// end would be parked by the runtime with nothing said about why.
pub fn run() -> ! {
    // The product's own init: `esp_hal::init` at `CpuClock::max()`, the FPU
    // arm, and — load-bearing for every line below — UART0 at 921,600, which
    // is what programs the baud divisor `esp-println` depends on and never
    // sets. Bound, not dropped.
    //
    // The APP core's `CPU_CTRL` is taken and dropped on the floor: see the
    // module docs on why core 1 never starts.
    let (_sw_int, _timg0, uart0, _flash, rmt_peripheral, _cpu_ctrl) = init_board();
    let _uart0 = uart0;

    // After `init_board`, not before: until it has programmed the divisor,
    // anything printed here goes out at the ROM's rate and is unreadable.
    esp_println::logger::init_logger(log::LevelFilter::Info);

    // The transcript header, first, before any record.
    let _ = fw_checks::write_header(
        &mut esp_println::Printer,
        &fw_checks::PayloadHeader {
            payload: "rmt-chase",
            chip: super::CHIP,
            firmware_commit: env!("LP_BUILD_COMMIT"),
            firmware_features: env!("LP_BUILD_FEATURES"),
            firmware_dirty: fw_checks::str_is_true(env!("LP_BUILD_DIRTY")),
        },
    );

    // Exactly 80 MHz: esp-hal's classic `validate_clock` accepts only the
    // source frequency itself, and the per-channel divider of 1 then makes
    // one tick 12.5 ns — what `lp_ws281x::PulseCodes::DEFAULT_CLOCK_HZ`
    // assumes. `shared_driver::RMT_CLOCK` is that constant; the number is
    // never written here.
    let mut rmt = Rmt::new(rmt_peripheral, shared_driver::RMT_CLOCK).expect("RMT at 80 MHz");

    // The plan, once, before the ISR and before any configure — the order
    // `Esp32V3RmtWs281xDriver::new` establishes, because windows must never
    // change after init (`RmtHw::ram_words`'s contract).
    let plan = v3_rmt::plan_for_declared(DECLARED_WIRES).expect("a plan for one declared wire");
    TX_PLAN
        .init(plan)
        .expect("the block plan is published once");

    // Single core, so this binds the RMT handler on the PRO core (the M4
    // fallback shape). The handler is the product's own trampoline into
    // `DRIVER.on_interrupt()`.
    shared_driver::install_isr(&mut rmt);

    let config = TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output(true)
        .with_idle_output_level(Level::Low)
        .with_carrier_modulation(false);
    // Held for the whole run: dropping it would let esp-hal tear the channel
    // down under the driver.
    let _tx = rmt
        .channel0
        .configure_tx(&config.with_memsize(TX_PLAN.blocks(SLOT)))
        .expect("RMT slot 0 configures");
    v3_rmt::enable_tx_interrupts(SLOT);
    // All-STOP until the first frame prefills it, so a spurious start can
    // only transmit nothing.
    v3_rmt::clear_ram(SLOT);
    DRIVER
        .configure_default_clock(SLOT, &ChannelTiming::WS2812)
        .expect("WS2812 timing compiles at 80 MHz");
    // The wrap bit is global on this chip and load-bearing: without it the
    // transmitter runs off the end of the window instead of wrapping onto the
    // half the ISR has just refilled. Set after esp-hal has finished touching
    // `APB_CONF` in `configure_tx`, exactly as the product does.
    v3_rmt::init_tx();
    // The pad. One wire, one slot, so this is done once rather than per
    // transmission — the product's `acquire_slot` is the pooling half and
    // there is nothing here to pool.
    v3_rmt::route_rmt_to_gpio(SLOT, PAD);

    info!(
        "[rmt-chase] {LEDS} LEDs on gpio{PAD}, {CHASES} chases ({FRAMES} frames), \
         slot {SLOT} blocks={} window_words={} half_words={}",
        TX_PLAN.blocks(SLOT),
        TX_PLAN.window_words(SLOT, BLOCK_WORDS),
        TX_PLAN.window_words(SLOT, BLOCK_WORDS) / 2,
    );

    let delay = Delay::new();
    let mut data = [0u8; frame_bytes(LEDS)];
    for n in 0..FRAMES {
        let lit = chase_frame(n, LEDS, &mut data);
        // The safe form: the borrow of `data` provably outlives the
        // transmission, and the channel is aborted on any exit path. The spin
        // is a plain `spin_loop` — the refills come from the ISR.
        DRIVER
            .send_blocking(SLOT, &data, core::hint::spin_loop)
            .expect("frame starts");
        // After the send, not before: the record claims the frame reached the
        // wire.
        let _ = write_frame_record(
            &mut esp_println::Printer,
            &FrameRecord::of(n, LEDS, lit, &data),
        );
        delay.delay_millis(FRAME_GAP_MS);
    }

    let _ = write_done(&mut esp_println::Printer);

    loop {
        core::hint::spin_loop();
    }
}
