//! `lp-ws281x` — the portable half of a multi-channel WS2811/WS2812 LED driver
//! built on the ESP32 family's RMT peripheral.
//!
//! The clockless WS281x protocol has no clock line: each bit is a pulse whose
//! high time carries the value, and a frame is only correct if every bit lands
//! within roughly ±150 ns. The RMT peripheral transmits such pulses from a small
//! RAM window — far too small to hold a frame — so a driver is really a *refill
//! race*: keep the half the transmitter just left full of fresh pulses, forever,
//! from an interrupt handler that competes with WiFi.
//!
//! This crate is the whole of that logic, with no chip in it:
//!
//! - [`timing`] — per-channel wire timing and byte order, compiled to RMT words
//!   for a given clock rate ([`ChannelTiming`], [`PulseCodes`], [`ColorOrder`]).
//! - [`blocks`] — [`BlockPlan`]: how the peripheral's memory blocks are shared
//!   out between channels, and which channels an extension makes unavailable;
//!   [`SharedBlockPlan`], the set-once slot a backend publishes the plan
//!   through at driver init.
//! - [`pulse`] — the RMT item format, including the all-zero STOP word the
//!   guard mechanism is built on.
//! - [`state`] — the atomics an interrupt handler and its caller share
//!   ([`ChannelState`], [`ChannelStats`]).
//! - [`driver`] — [`Ws281xDriver`]: ping-pong refill driven by a **bit cursor**,
//!   guard-word flicker protection, and frame accounting.
//! - [`hw`] — [`RmtHw`], the register-poking seam a chip backend implements.
//! - [`mock`] — [`MockRmt`], a transmitter simulation the host tests run
//!   against (default feature `mock`).
//!
//! ## Where the chip goes
//!
//! Nothing here knows a register name. A backend supplies seven operations —
//! write a RAM word, read the transmit pointer, set the threshold, start, stop,
//! report the window size, take the interrupt causes — and every decision about
//! *what* and *when* stays in [`Ws281xDriver`]. That is what lets the sequencing
//! be tested exhaustively on the host and reused unchanged across the classic
//! ESP32 (8 channels, 64-word blocks), the ESP32-S3 (4, 48) and the ESP32-C6
//! (2, 48).
//!
//! ## Scope
//!
//! Frames are `u8` RGB triplets. Colour processing — gamma, dithering,
//! white-point — belongs upstream (lightplayer's `DisplayPipeline`), not in a
//! transmitter. RGBW, 400 kHz parts and an async transaction API are future
//! work.
//!
//! ## Usage sketch
//!
//! ```
//! use lp_ws281x::{ChannelTiming, MockRmt, Pump, Ws281xDriver};
//!
//! // Four channels, one 48-word RMT block each (the ESP32-S3 shape).
//! let driver: Ws281xDriver<MockRmt, 4> = Ws281xDriver::new(MockRmt::new(4, 48));
//! driver.configure_default_clock(0, &ChannelTiming::WS2812).unwrap();
//!
//! let frame = [255, 0, 0, 0, 255, 0]; // two pixels, RGB in
//! // SAFETY: `frame` outlives the transmission — the pump below runs it to
//! // completion before the borrow ends.
//! unsafe { driver.start_frame(0, &frame).unwrap() };
//! Pump::default().run(&driver);
//!
//! assert!(driver.is_complete(0));
//! assert_eq!(driver.stats(0).frames, 1);
//! assert_eq!(driver.stats(0).guard_trips, 0);
//! ```
//!
//! ## Provenance
//!
//! Original code. It descends from the author's own single-channel ESP32-C6
//! driver in `lp2025` (`lp-fw/fw-esp32c6/src/output/rmt/`) — reworked here for
//! multiple channels, arbitrary half sizes, configurable timing, and without
//! that driver's start-of-frame guard race. No GPL source was consulted; see
//! `AGENTS.md` and `docs/adr/2026-07-28-license-provenance-discipline.md`.

#![no_std]

#[cfg(feature = "mock")]
extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod blocks;
pub mod driver;
pub mod hw;
pub mod pulse;
pub mod state;
pub mod timing;

#[cfg(feature = "mock")]
pub mod mock;

pub use blocks::{BlockPlan, BlockPlanError, SharedBlockPlan, SharedPlanError};
pub use driver::{ConfigError, FillResult, Half, StartError, Ws281xDriver};
pub use hw::{InterruptFlags, RamWindow, RmtHw};
pub use pulse::{pulse_code, Pulse, PulseItem, MAX_DURATION_TICKS, STOP_WORD};
pub use state::{
    lag_bucket, ChannelState, ChannelStats, BITS_PER_PIXEL, BYTES_PER_PIXEL, LAG_BUCKETS,
};
pub use timing::{ChannelTiming, ColorOrder, PulseCodes, TimingError};

#[cfg(feature = "mock")]
pub use mock::{MockRmt, Pump};
