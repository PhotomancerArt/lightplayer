//! The chip-agnostic driver: ping-pong refill, guard word, frame accounting.
//!
//! # How a frame is transmitted
//!
//! A channel's RMT RAM window is split into two equal **halves**. The hardware
//! reads the window as a ring (wrap mode) while the driver races ahead of it,
//! refilling the half the transmitter has just left:
//!
//! ```text
//!   words:  0 ............ half-1 | half ............ ram-1
//!           |<---- half A ------->|<---- half B ------->|
//!   start:  prefill A             prefill B              (no guard planted)
//!   thr #1: refill A              <-- reading            guard -> B[0]
//!   thr #2: <-- reading           refill B               guard -> A[0]
//!   ...
//! ```
//!
//! The threshold (`tx_lim`) alternates between `half` and `ram_words` so a
//! `tx_thr_event` fires at each crossing between halves; [`RmtHw::read_pos`]
//! then says which half the transmitter is in and therefore which one is free.
//!
//! # The bit cursor
//!
//! Refill position is tracked in **bits**, not LEDs. The lp2025 ancestor
//! counted LEDs and assumed a half held a whole number of them (96 words = 4
//! LEDs). That assumption dies as soon as channels get one memory block each:
//! a 48-word block halves into 24 words = exactly one LED, and the classic
//! ESP32's 64-word block halves into 32 words = 1⅓ LEDs. A bit cursor makes
//! every half size work on every chip, and makes `blocks_per_channel` a free
//! tuning knob (more blocks ⇔ fewer channels ⇔ lower interrupt rate).
//!
//! # The guard word
//!
//! An all-zero RMT word is a STOP marker. After each refill the driver plants
//! one at the **first word of the half the transmitter is currently reading** —
//! a slot the transmitter has already consumed, and the slot it would next
//! re-read if the following refill interrupt never arrived. The effect is that
//! a lost interrupt truncates the frame instead of replaying a stale half
//! forever, which is the difference between one dim frame and visible flicker.
//! A refill overwrites the previous guard as part of filling its half, so in
//! healthy operation the guard is never reached.
//!
//! Two behaviours here differ deliberately from lp2025:
//!
//! * **No guard at start.** lp2025 planted one at word 0 immediately after
//!   `tx_start` and hoped ("with any luck") the transmitter had already passed
//!   it. It is a real race and it kills the frame when lost. Here, `start_frame`
//!   prefills both halves and plants nothing; the first threshold interrupt
//!   plants the first guard, safely behind the read pointer. The cost is that a
//!   lost *first* interrupt replays the initial buffer once — which is why
//!   truncation is detected by cursor accounting at `tx_end`
//!   ([`ChannelStats::guard_trips`]) rather than by pretending the window is
//!   zero.
//! * **The guard slot is checked.** The threshold event marks the boundary
//!   between halves, so the read pointer has passed the guard slot by the time
//!   the handler runs (hardware read-ahead plus interrupt latency). If it
//!   somehow has not, planting the guard would terminate a perfectly good
//!   frame, so the driver skips it and counts a
//!   [`ChannelStats::guard_skips`] instead.

#[cfg(feature = "test_hooks")]
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use crate::blocks::BlockPlan;
use crate::hw::RmtHw;
use crate::pulse::STOP_WORD;
use crate::state::{ChannelState, ChannelStats, BITS_PER_PIXEL, BYTES_PER_PIXEL};
use crate::timing::{ChannelTiming, PulseCodes, TimingError};

/// Which ping-pong half of a channel's RAM window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    /// Words `0..half_words`.
    First,
    /// Words `half_words..ram_words`.
    Second,
}

/// What a single half-buffer refill produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillResult {
    /// The half does not terminate the frame — it is all data bits, or the
    /// latch still did not fit. Another refill is required.
    More,
    /// The last data bits, the latch, and the terminating STOP fill were
    /// written here; the transmitter will stop inside this half.
    Tail,
    /// The frame was already fully written; this half is pure STOP fill.
    Drained,
}

/// A channel could not be configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// `ch` is not in `0..N`.
    ChannelOutOfRange,
    /// `ch` owns no memory blocks under the driver's
    /// [`BlockPlan`](crate::BlockPlan): a lower-numbered channel extended over
    /// them, or the plan left the channel out.
    ChannelUnavailable,
    /// The backend reported fewer than 4 words for the channel.
    RamTooSmall,
    /// The backend reported an odd word count; the window must split evenly.
    OddRamWords,
    /// The timing cannot be expressed at this clock rate.
    Timing(TimingError),
}

impl From<TimingError> for ConfigError {
    fn from(e: TimingError) -> Self {
        ConfigError::Timing(e)
    }
}

/// A frame could not be started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartError {
    /// `ch` is not in `0..N`.
    ChannelOutOfRange,
    /// [`Ws281xDriver::configure`] has not been called for this channel.
    NotConfigured,
    /// The previous frame on this channel is still in flight.
    Busy,
}

/// A multi-channel WS281x transmitter over an [`RmtHw`] backend.
///
/// `N` is the number of channels the backend exposes — 8 on the classic
/// ESP32, 4 on the ESP32-S3, 2 on the ESP32-C6. All state is atomic, so the
/// natural deployment is a `static` shared between thread context and the
/// interrupt handler:
///
/// ```ignore
/// static DRIVER: Ws281xDriver<Esp32S3Rmt, 4> = Ws281xDriver::new(Esp32S3Rmt::new());
///
/// extern "C" fn rmt_isr() {
///     DRIVER.on_interrupt();
/// }
/// ```
#[derive(Debug)]
pub struct Ws281xDriver<H, const N: usize> {
    hw: H,
    channels: [ChannelState; N],
    blocks: BlockPlan<N>,
    /// Test hook (feature `test_hooks`), per channel: threshold interrupts to
    /// service normally before the drop counter starts consuming them.
    #[cfg(feature = "test_hooks")]
    hook_threshold_skip: [AtomicU32; N],
    /// Test hook (feature `test_hooks`), per channel: threshold interrupts to
    /// swallow, as if the interrupt had been lost.
    #[cfg(feature = "test_hooks")]
    hook_threshold_drop: [AtomicU32; N],
}

impl<H: RmtHw, const N: usize> Ws281xDriver<H, N> {
    /// Wrap a backend, one memory block per channel. No hardware is touched
    /// until [`Self::configure`].
    pub const fn new(hw: H) -> Self {
        Self::with_blocks(hw, BlockPlan::one_per_channel())
    }

    /// Wrap a backend with an explicit [`BlockPlan`].
    ///
    /// The plan decides which channels exist at all: a channel whose memory
    /// block was absorbed by a lower-numbered one is rejected by
    /// [`Self::configure`] with [`ConfigError::ChannelUnavailable`] instead of
    /// quietly transmitting into RAM it does not own.
    ///
    /// The backend must be built from the *same* plan — it is a `Copy` value
    /// precisely so that both halves can share one — because
    /// [`RmtHw::ram_words`] remains the authority on window size.
    pub const fn with_blocks(hw: H, blocks: BlockPlan<N>) -> Self {
        Self {
            hw,
            channels: [const { ChannelState::new() }; N],
            blocks,
            #[cfg(feature = "test_hooks")]
            hook_threshold_skip: [const { AtomicU32::new(0) }; N],
            #[cfg(feature = "test_hooks")]
            hook_threshold_drop: [const { AtomicU32::new(0) }; N],
        }
    }

    /// The backend.
    pub fn hw(&self) -> &H {
        &self.hw
    }

    /// The memory-block allocation this driver was built with.
    pub const fn block_plan(&self) -> &BlockPlan<N> {
        &self.blocks
    }

    /// Number of channels this driver manages, available or not.
    pub const fn channel_count(&self) -> usize {
        N
    }

    /// Per-channel state, for polling completion and reading counters.
    pub fn channel(&self, ch: u8) -> Option<&ChannelState> {
        self.channels.get(ch as usize)
    }

    /// Counters for `ch`, or all-zero if `ch` is out of range.
    pub fn stats(&self, ch: u8) -> ChannelStats {
        self.channel(ch)
            .map(ChannelState::stats)
            .unwrap_or_default()
    }

    /// Compile `timing` for `clock_hz` and bind it to `ch`.
    ///
    /// Call once per channel while it is idle. The RAM window size is taken
    /// from the backend at this point and cached, so the interrupt path never
    /// has to ask again.
    pub fn configure(
        &self,
        ch: u8,
        timing: &ChannelTiming,
        clock_hz: u32,
    ) -> Result<(), ConfigError> {
        let state = self
            .channels
            .get(ch as usize)
            .ok_or(ConfigError::ChannelOutOfRange)?;
        if !self.blocks.is_available(ch) {
            return Err(ConfigError::ChannelUnavailable);
        }

        let ram_words = self.hw.ram_words(ch);
        if ram_words < 4 {
            return Err(ConfigError::RamTooSmall);
        }
        if ram_words % 2 != 0 {
            return Err(ConfigError::OddRamWords);
        }

        let codes = PulseCodes::new(timing, clock_hz)?;
        state.set_timing(&codes, timing.color_order);
        state.ram_words.store(ram_words, Relaxed);
        state.half_words.store(ram_words / 2, Relaxed);
        Ok(())
    }

    /// Compile `timing` for the standard 80 MHz RMT clock and bind it to `ch`.
    pub fn configure_default_clock(
        &self,
        ch: u8,
        timing: &ChannelTiming,
    ) -> Result<(), ConfigError> {
        self.configure(ch, timing, PulseCodes::DEFAULT_CLOCK_HZ)
    }

    /// Number of pixels a frame of `bytes` bytes describes.
    pub const fn pixels_for(bytes: usize) -> usize {
        bytes / BYTES_PER_PIXEL
    }

    /// Begin transmitting `frame` (RGB triplets) on `ch`.
    ///
    /// Trailing bytes that do not complete a triplet are ignored. Both halves
    /// of the RAM window are filled before the transmitter is started, and no
    /// guard word is planted — see the module docs.
    ///
    /// # Safety
    ///
    /// The interrupt handler keeps reading `frame` for as long as the frame is
    /// in flight, through a raw pointer that outlives this borrow. The caller
    /// must keep the referenced bytes **alive, in place, and unmodified** until
    /// [`ChannelState::is_complete`] reports `true` for `ch` (or
    /// [`Self::abort`] has been called). [`Self::send_blocking`] is the safe
    /// wrapper that enforces this.
    pub unsafe fn start_frame(&self, ch: u8, frame: &[u8]) -> Result<(), StartError> {
        let state = self
            .channels
            .get(ch as usize)
            .ok_or(StartError::ChannelOutOfRange)?;
        if !state.is_configured() {
            return Err(StartError::NotConfigured);
        }
        if state.is_busy() {
            return Err(StartError::Busy);
        }

        // SAFETY: forwarding this function's own contract — the caller
        // guarantees the bytes stay valid and unmodified for the whole
        // transmission.
        unsafe { state.arm(frame.as_ptr(), frame.len()) };

        self.fill_half(ch, state, Half::First);
        self.fill_half(ch, state, Half::Second);

        let half = state.half_words.load(Relaxed);
        self.hw.set_tx_threshold(ch, half as u16);
        self.hw.start_tx(ch);
        Ok(())
    }

    /// Transmit `frame` on `ch` and wait for it to finish, calling `spin`
    /// between polls.
    ///
    /// This is the safe form of [`Self::start_frame`]: the borrow of `frame`
    /// provably outlives the transmission because the function does not return
    /// until the channel reports complete, and the channel is aborted if `spin`
    /// unwinds.
    ///
    /// `spin` is where a firmware yields (`delay_us`, WFI, …) — and where a
    /// host test advances its mock and dispatches [`Self::on_interrupt`].
    pub fn send_blocking(
        &self,
        ch: u8,
        frame: &[u8],
        mut spin: impl FnMut(),
    ) -> Result<(), StartError> {
        // SAFETY: `frame` is borrowed for the whole call, and the call cannot
        // return (normally or by unwind) while the frame is in flight: the
        // guard below aborts the channel on any exit path before the borrow
        // ends, and abort stops the transmitter and clears the frame pointer.
        unsafe { self.start_frame(ch, frame)? };

        let guard = AbortGuard { driver: self, ch };
        while !self.is_complete(ch) {
            spin();
        }
        drop(guard);
        Ok(())
    }

    /// Transmit several channels' frames **concurrently** and wait for all of
    /// them, calling `spin` between polls.
    ///
    /// The channels are started back to back — a few register writes apart, so
    /// for a strip they are simultaneous — and the call returns when the last
    /// one completes. This is the multi-channel form of [`Self::send_blocking`]
    /// and is safe for the same reason: `frames` is borrowed for the whole
    /// call, and every started channel is aborted on any exit path before the
    /// borrow ends.
    ///
    /// If a channel fails to start, the ones already started are aborted and
    /// the error is returned.
    ///
    /// ```
    /// # use lp_ws281x::{ChannelTiming, MockRmt, Pump, Ws281xDriver};
    /// let driver: Ws281xDriver<MockRmt, 4> = Ws281xDriver::new(MockRmt::new(4, 48));
    /// for ch in 0..2 {
    ///     driver.configure_default_clock(ch, &ChannelTiming::WS2812).unwrap();
    /// }
    /// let (red, green) = ([255, 0, 0], [0, 255, 0]);
    /// driver
    ///     .send_blocking_all(&[(0, &red[..]), (1, &green[..])], || {
    ///         driver.hw().advance_all(1);
    ///         driver.on_interrupt();
    ///     })
    ///     .unwrap();
    /// assert_eq!(driver.stats(0).frames, 1);
    /// assert_eq!(driver.stats(1).frames, 1);
    /// ```
    pub fn send_blocking_all(
        &self,
        frames: &[(u8, &[u8])],
        mut spin: impl FnMut(),
    ) -> Result<(), StartError> {
        let mut guard = AbortSetGuard {
            driver: self,
            started: 0,
        };
        for (ch, frame) in frames {
            // SAFETY: every `frame` is borrowed for the duration of this call,
            // and `guard` aborts every channel started so far on any exit path
            // — including the `?` below and an unwind out of `spin` — before
            // the borrow ends.
            unsafe { self.start_frame(*ch, frame)? };
            if *ch < 32 {
                guard.started |= 1u32 << *ch;
            }
        }

        while frames.iter().any(|(ch, _)| !self.is_complete(*ch)) {
            spin();
        }
        drop(guard);
        Ok(())
    }

    /// Stop `ch` at once and mark its frame complete.
    ///
    /// Safe to call at any time; a no-op if no frame is in flight.
    pub fn abort(&self, ch: u8) {
        let Some(state) = self.channels.get(ch as usize) else {
            return;
        };
        self.hw.stop_tx(ch);
        state.disarm();
    }

    /// Has `ch` finished (or never started) a frame?
    ///
    /// Out-of-range channels report `true` so polling loops terminate.
    pub fn is_complete(&self, ch: u8) -> bool {
        self.channels
            .get(ch as usize)
            .is_none_or(ChannelState::is_complete)
    }

    /// The RMT interrupt entry point. Services every flagged channel.
    ///
    /// Causes are taken (read **and** cleared) from the backend in one shot, so
    /// nothing is lost between the read and the acknowledgement. `tx_end` takes
    /// precedence over `tx_thr_event` on the same channel: once the frame is
    /// over there is nothing to refill.
    ///
    /// # Coincident causes
    ///
    /// The peripheral raises one interrupt line for **all** channels, and with
    /// several channels running the flags routinely name more than one of them
    /// in the same snapshot — with one block each every channel wants a refill
    /// every 24 bits, so alignment is not a rare accident but the steady state
    /// of any two channels started together. A handler that serviced only the
    /// lowest-numbered flagged channel would leave the others' causes cleared
    /// but unserved, and they would run into their guard words. So this is a
    /// single pass over the whole snapshot: every flagged channel is serviced
    /// before returning, and one entry can perform `N` refills.
    ///
    /// Channels are served in index order, which means the highest-numbered
    /// channel of a coincident group waits for all the others' refills. That
    /// cost is measurable — it is exactly what
    /// [`ChannelStats::refill_lag_sum`] records — and it is the reason the
    /// deadline budget in the crate README is stated per *group* rather than
    /// per channel.
    pub fn on_interrupt(&self) {
        let flags = self.hw.take_interrupts();
        #[cfg(feature = "test_hooks")]
        let flags = self.apply_threshold_suppression(flags);
        if flags.is_empty() {
            return;
        }

        for (idx, state) in self.channels.iter().enumerate() {
            let ch = idx as u8;
            if flags.error_for(ch) {
                state.errors.fetch_add(1, Relaxed);
            }
            if flags.end_for(ch) {
                self.finish(state);
            } else if flags.threshold_for(ch) {
                self.refill(ch, state);
            }
        }
    }

    /// Test hook — deliberately lose `ch`'s threshold interrupts.
    ///
    /// The next `skip` `tx_thr_event` causes flagged for `ch` are serviced
    /// normally; the `drop` after those are swallowed as if the interrupt had
    /// never fired (the hardware cause is still acknowledged, so the peripheral
    /// keeps running — exactly the shape of a threshold interrupt lost to
    /// masking or preemption). Other channels flagged in the same snapshot are
    /// unaffected, which is what makes the multi-channel isolation test on
    /// silicon possible.
    ///
    /// This exists so a hardware self-test can prove the guard word truncates a
    /// frame **on silicon**; see the plan's P3/P4 truncation tests. Counters
    /// are consumed from the interrupt handler without atomicity across the
    /// load/subtract pair, which is sound in the single-ISR deployments this
    /// hook targets.
    #[cfg(feature = "test_hooks")]
    pub fn suppress_thresholds_on(&self, ch: u8, skip: u32, drop: u32) {
        let Some(skip_slot) = self.hook_threshold_skip.get(ch as usize) else {
            return;
        };
        skip_slot.store(skip, Relaxed);
        self.hook_threshold_drop[ch as usize].store(drop, Relaxed);
    }

    /// [`Self::suppress_thresholds_on`] applied to every channel.
    #[cfg(feature = "test_hooks")]
    pub fn suppress_thresholds(&self, skip: u32, drop: u32) {
        for ch in 0..N {
            self.suppress_thresholds_on(ch as u8, skip, drop);
        }
    }

    /// Consume [`Self::suppress_thresholds_on`]'s counters against `flags`,
    /// channel by channel.
    #[cfg(feature = "test_hooks")]
    fn apply_threshold_suppression(
        &self,
        mut flags: crate::hw::InterruptFlags,
    ) -> crate::hw::InterruptFlags {
        if flags.threshold == 0 {
            return flags;
        }
        for ch in 0..N {
            if !flags.threshold_for(ch as u8) {
                continue;
            }
            if self.hook_threshold_skip[ch].load(Relaxed) > 0 {
                self.hook_threshold_skip[ch].fetch_sub(1, Relaxed);
            } else if self.hook_threshold_drop[ch].load(Relaxed) > 0 {
                self.hook_threshold_drop[ch].fetch_sub(1, Relaxed);
                flags.clear_threshold(ch as u8);
            }
        }
        flags
    }

    /// Handle `tx_end`: classify the frame and publish completion.
    fn finish(&self, state: &ChannelState) {
        if state.is_complete() {
            // Spurious or duplicated end event.
            return;
        }
        // A cursor short of `total_bits` means the transmitter stopped on a
        // guard word instead of on the frame's own STOP fill: a refill arrived
        // too late. The frame is still reported complete — the caller gets a
        // truncated frame and a counter, not an error.
        if state.bit_cursor.load(Acquire) < state.total_bits.load(Relaxed) {
            state.guard_trips.fetch_add(1, Relaxed);
        }
        state.frames.fetch_add(1, Relaxed);
        state.frame_complete.store(true, Release);
    }

    /// Handle `tx_thr_event`: plant the guard, refill the free half, flip the
    /// threshold, and record how far the read pointer moved meanwhile.
    fn refill(&self, ch: u8, state: &ChannelState) {
        if state.is_complete() {
            return;
        }
        let half = state.half_words.load(Relaxed);
        let ram = state.ram_words.load(Relaxed);
        if half == 0 {
            return;
        }

        let pos_before = self.hw.read_pos(ch) as usize;
        let in_second_half = pos_before >= half;

        // The transmitter is inside one half; the other one is free.
        let (guard_slot, free_half, next_threshold) = if in_second_half {
            (half, Half::First, ram)
        } else {
            (0, Half::Second, half)
        };

        // Flip the threshold first: the next event must be armed before the
        // refill, which is the long part of this handler.
        self.hw.set_tx_threshold(ch, next_threshold as u16);

        if pos_before == guard_slot {
            // The read pointer has not passed the slot yet — writing a STOP
            // there would end a healthy frame. See the module docs.
            state.guard_skips.fetch_add(1, Relaxed);
        } else {
            self.hw.write_ram(ch, guard_slot, STOP_WORD);
        }

        self.fill_half(ch, state, free_half);

        let pos_after = self.hw.read_pos(ch) as usize;
        let advanced = (pos_after + ram - pos_before) % ram;
        state.record_lag(advanced, half);
    }

    /// Write one half of the RAM window from the bit cursor onwards.
    ///
    /// Walks bits — not LEDs — so any half size works. When the pixel data runs
    /// out the latch is emitted exactly once and the remainder of the half is
    /// STOP-filled, which is what actually ends the transmission.
    fn fill_half(&self, ch: u8, state: &ChannelState, half: Half) -> FillResult {
        let half_words = state.half_words.load(Relaxed);
        let start = match half {
            Half::First => 0,
            Half::Second => half_words,
        };
        let end = start + half_words;

        let codes = state.codes();
        let order = state.color_order();
        let total = state.total_bits.load(Relaxed);
        let mut cursor = state.bit_cursor.load(Acquire);
        let mut word = start;

        let mut byte = 0u8;
        let mut byte_loaded = false;
        while word < end && cursor < total {
            let bit_in_byte = cursor % 8;
            if !byte_loaded || bit_in_byte == 0 {
                let pixel = cursor / BITS_PER_PIXEL;
                let slot = (cursor % BITS_PER_PIXEL) / 8;
                byte = state.frame_byte(pixel * BYTES_PER_PIXEL + order.source_index(slot));
                byte_loaded = true;
            }
            // WS281x is MSB-first within each byte.
            let one = byte & (0x80 >> bit_in_byte) != 0;
            self.hw.write_ram(ch, word, codes.bit(one));
            word += 1;
            cursor += 1;
        }
        state.bit_cursor.store(cursor, Release);

        if cursor < total {
            // The half filled up with data alone.
            return FillResult::More;
        }

        let mut result = FillResult::Drained;
        if !state.latch_written.load(Relaxed) {
            if word == end {
                // The data ended exactly on the half boundary; the latch is the
                // first word of the next half.
                return FillResult::More;
            }
            self.hw.write_ram(ch, word, codes.latch);
            word += 1;
            state.latch_written.store(true, Relaxed);
            result = FillResult::Tail;
        }

        while word < end {
            self.hw.write_ram(ch, word, STOP_WORD);
            word += 1;
        }
        result
    }
}

/// Aborts the channel unless it is dismissed — makes [`Ws281xDriver::send_blocking`]
/// sound in the presence of unwinding.
struct AbortGuard<'a, H: RmtHw, const N: usize> {
    driver: &'a Ws281xDriver<H, N>,
    ch: u8,
}

impl<H: RmtHw, const N: usize> Drop for AbortGuard<'_, H, N> {
    fn drop(&mut self) {
        self.driver.abort(self.ch);
    }
}

/// [`AbortGuard`] for a set of channels — makes
/// [`Ws281xDriver::send_blocking_all`] sound however it exits.
struct AbortSetGuard<'a, H: RmtHw, const N: usize> {
    driver: &'a Ws281xDriver<H, N>,
    /// One bit per started channel.
    started: u32,
}

impl<H: RmtHw, const N: usize> Drop for AbortSetGuard<'_, H, N> {
    fn drop(&mut self) {
        for ch in 0..N.min(32) {
            if self.started & (1 << ch) != 0 {
                self.driver.abort(ch as u8);
            }
        }
    }
}
