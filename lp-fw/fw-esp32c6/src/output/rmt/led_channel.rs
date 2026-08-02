//! The hardware harnesses' single-channel LED API, on top of [`lp_ws281x`].
//!
//! `test_rmt`, `test_dither`, `test_usb`, `test_json` and `test_fluid_demo`
//! each drive one strip directly, without a server, a manifest or a registry:
//! take the RMT peripheral, name a pin, push RGB bytes, wait. That shape came
//! from the legacy driver's [`LedChannel`], which this replaces — the legacy
//! implementation (its own ISR, its own ping-pong refill, and
//! `fw-esp32-common`'s `rmt_state`) went away with the migration to
//! `lp-ws281x`, but the harnesses' call sites are unchanged.
//!
//! So this is a shim, not a driver: it configures RMT slot 0 through the same
//! [`super::shared_driver::DRIVER`] and [`super::c6_rmt`] the app path uses, so
//! a harness exercises the shipping refill machinery rather than a parallel
//! copy of it.
//!
//! Two differences from the legacy type, both deliberate:
//!
//! * **The transmission is blocking.** `lp-ws281x` owns frame completion
//!   internally, so [`LedChannel::start_transmission`] sends and waits, and
//!   [`LedTransaction::wait_complete`] just hands the channel back. Every
//!   harness calls the two back to back, so wall-clock behaviour is the same.
//! * **The frame is built once per call, in wire order.** The legacy path
//!   reordered RGB into GRB inside its interrupt handler; `lp-ws281x` transmits
//!   the bytes it is given, so the swap happens here — the harnesses keep
//!   passing RGB and the strips keep showing the same colours.
//!
//! Harness-only by construction: `output/mod.rs` compiles this module for the
//! five harness features above and for nothing else, so the shipping image
//! never carries it.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use esp_hal::Blocking;
use esp_hal::gpio::Level;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::rmt::{
    Channel, ConfigError as RmtConfigError, Rmt, Tx, TxChannelConfig, TxChannelCreator,
};
use esp_hal::time::Instant;
use lp_ws281x::ChannelTiming;

use super::c6_rmt::{
    self, BLOCKS_PER_CHANNEL, CHANNEL_WORDS, SLOT_STRIDE, TX_BLOCKS, USABLE_CHANNELS,
    slot_for_index,
};
use super::shared_driver::{DRIVER, FRAME_TIMEOUT, install_isr, report_telemetry_if_due};

/// Manifest channel the harnesses stand in for. There is no manifest here, so
/// they take the first one — whichever RMT slot the block plan gives it.
const HARNESS_INDEX: usize = 0;

/// RMT slot the harnesses drive. Index 0 owns memory under every block plan,
/// so the `unwrap_or` is unreachable; it exists so a hand-edited plan degrades
/// to slot 0 instead of failing to compile.
const HARNESS_CHANNEL: u8 = match slot_for_index(HARNESS_INDEX) {
    Some(slot) => slot,
    None => 0,
};

/// One WS2812 strip on RMT slot 0, for the hardware harnesses.
pub struct LedChannel<'ch> {
    /// Kept alive for its pin binding; the frame path talks to [`DRIVER`].
    _channel: Channel<'ch, Blocking, Tx>,
    /// Wire-order (GRB) bytes for one frame, sized at construction.
    frame: Vec<u8>,
    num_leds: usize,
}

/// A finished transmission, kept so the harnesses' `wait_complete()` call sites
/// still read the way they did against the legacy driver.
#[must_use = "transactions must be waited on to get the channel back"]
pub struct LedTransaction<'ch> {
    channel: LedChannel<'ch>,
}

impl<'ch> LedChannel<'ch> {
    /// Configure RMT slot 0 for `num_leds` WS2812 LEDs on `pin`.
    ///
    /// Takes the RMT peripheral because the harness has nothing else to do
    /// with it, and installs the shared interrupt handler on the way through.
    pub fn new<O>(
        mut rmt: Rmt<'ch, Blocking>,
        pin: O,
        num_leds: usize,
    ) -> Result<Self, RmtConfigError>
    where
        O: PeripheralOutput<'ch>,
    {
        // The app's boot line, in harness form: a capture should say which
        // window the run was measured against, not just that it ran.
        log::info!(
            "LedChannel::new: RMT slot {HARNESS_CHANNEL} of {USABLE_CHANNELS} usable, \
             {num_leds} LEDs (blocks/channel={BLOCKS_PER_CHANNEL} slot_stride={SLOT_STRIDE} \
             window_words={CHANNEL_WORDS} half_words={})",
            CHANNEL_WORDS / 2,
        );

        install_isr(&mut rmt);

        let config = TxChannelConfig::default()
            .with_clk_divider(1)
            .with_idle_output(true)
            .with_idle_output_level(Level::Low)
            .with_carrier_modulation(false)
            .with_memsize(BLOCKS_PER_CHANNEL);
        let channel = rmt.channel0.configure_tx(&config)?.with_pin(pin);

        c6_rmt::enable_tx_interrupts(HARNESS_CHANNEL);
        // All-STOP until the first frame prefills the window, so a spurious
        // start transmits nothing.
        c6_rmt::clear_ram(&TX_BLOCKS, HARNESS_CHANNEL);
        if let Err(error) = DRIVER.configure_default_clock(HARNESS_CHANNEL, &ChannelTiming::WS2812)
        {
            log::error!("LedChannel::new: timing configuration failed: {error:?}");
        }

        Ok(Self {
            _channel: channel,
            frame: vec![0u8; num_leds * 3],
            num_leds,
        })
    }

    /// Send `rgb_bytes` (R,G,B per LED) and wait for the frame to finish.
    ///
    /// Short input lights the leading LEDs and blanks the rest, which is what
    /// the legacy implementation did; extra input is ignored.
    pub fn start_transmission(mut self, rgb_bytes: &[u8]) -> LedTransaction<'ch> {
        let leds = (rgb_bytes.len() / 3).min(self.num_leds);
        self.frame.fill(0);
        for led in 0..leds {
            let src = led * 3;
            let dst = led * 3;
            // WS2812 wire order is GRB.
            self.frame[dst] = rgb_bytes[src + 1];
            self.frame[dst + 1] = rgb_bytes[src];
            self.frame[dst + 2] = rgb_bytes[src + 2];
        }

        // Same hang detector as the app path: a frame that outlives its
        // deadline is aborted and reported rather than wedging a harness loop.
        let started = Instant::now();
        let mut timed_out = false;
        let result = DRIVER.send_blocking(HARNESS_CHANNEL, &self.frame, || {
            if !timed_out && started.elapsed() > FRAME_TIMEOUT {
                timed_out = true;
                DRIVER.abort(HARNESS_CHANNEL);
            }
        });
        // A no-op unless `ws281x_telemetry` is on; a harness that turns it on
        // gets the same `[WS281X]` counters the app path prints.
        report_telemetry_if_due();

        if let Err(error) = result {
            log::warn!("LedChannel::start_transmission: frame failed to start: {error:?}");
        } else if timed_out {
            log::warn!(
                "LedChannel::start_transmission: frame did not complete within {} ms",
                FRAME_TIMEOUT.as_millis()
            );
        }

        LedTransaction { channel: self }
    }
}

impl<'ch> LedTransaction<'ch> {
    /// Hand the channel back. The frame already completed inside
    /// [`LedChannel::start_transmission`].
    pub fn wait_complete(self) -> LedChannel<'ch> {
        self.channel
    }
}
