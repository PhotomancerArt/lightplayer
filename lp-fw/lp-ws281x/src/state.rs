//! Per-channel state shared between thread context and the interrupt handler.
//!
//! Everything here is atomic. There are no critical sections and no locks: the
//! ISR must never wait on the caller, and disabling interrupts around a refill
//! would defeat the point of the driver. The division of labour is strict —
//!
//! * thread context writes the configuration and the frame descriptor **once**,
//!   before `arm` publishes them, and afterwards only reads;
//! * the ISR owns the bit cursor and the counters.
//!
//! # Memory ordering — the cross-core contract
//!
//! Thread context and the interrupt handler may run on **different cores**
//! (fw-esp32v3 binds the RMT ISR on the classic ESP32's APP core), so nothing
//! here may lean on "the handler cannot be running while thread code runs".
//! The contract has three legs:
//!
//! * **Publish (arm → ISR).** `arm` stores the frame descriptor (pointer +
//!   length + total bits) and then `frame_complete.store(false, Release)`;
//!   the ISR's first act on a channel is an [`Self::is_complete_sync`] load,
//!   which pairs with it and orders every prior store — including the
//!   configuration written by `configure`, which happens-before `arm` on the
//!   caller's thread. `bit_cursor` is Release-stored / Acquire-loaded because
//!   the *frame buffer bytes* are read through it: an `Acquire` load of the
//!   cursor is what guarantees a refill sees the caller's pixel writes.
//!   `threshold_boundary` is stored after the `arm` publish and therefore
//!   carries its own Release/Acquire pair (it only feeds the entry-delay
//!   statistic, but a stale read would file garbage).
//!
//! * **Teardown (disarm → handshake).** `disarm` publishes
//!   `frame_complete = true` with `SeqCst`, and the driver's abort path then
//!   waits until the ISR is provably out of service (the `isr_seq` handshake
//!   in `driver.rs` — see the soundness argument there). An ISR pass that
//!   began before the disarm may keep reading the old descriptor through
//!   `Relaxed` loads in [`Self::frame_byte`]; that is sound **only** because
//!   the handshake keeps the caller from freeing or reusing the bytes until
//!   that pass has exited. The `SeqCst` on `frame_complete` (store here, load
//!   in the ISR's check) is one half of the store/load ("Dekker") pattern the
//!   handshake needs; plain Release/Acquire would permit both sides to miss
//!   each other.
//!
//! * **Ownership.** Exactly **one core services the ISR** — the handler never
//!   nests and never runs concurrently with itself. That single-writer fact
//!   is what keeps the counters plain `Relaxed` (staleness in a cross-core
//!   stats snapshot is harmless) and the load/compare/store maxima in
//!   [`Self::record_lag`] / [`Self::record_entry_delay`] lossless.

use core::sync::atomic::Ordering::{Acquire, Relaxed, Release, SeqCst};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU32, AtomicU8, AtomicUsize};

use crate::timing::{ColorOrder, PulseCodes};

/// Bits on the wire per pixel: three bytes of eight.
pub const BITS_PER_PIXEL: usize = 24;

/// Bytes per pixel in the caller's frame buffer (RGB).
pub const BYTES_PER_PIXEL: usize = 3;

/// Number of buckets in the refill-lag histogram.
///
/// Buckets `0..8` divide the deadline — one ping-pong **half** — into eighths;
/// bucket `8` is everything at or beyond the half. That last bucket is the one
/// that matters: a refill that consumed a whole half of read pointer while it
/// ran had no margin left, and the next one would have hit the guard word. The
/// edges are expressed as fractions of the half rather than as fixed word
/// counts precisely so the "half exhausted" edge stays explicit on every chip
/// (24 words on the S3/C6 with one block per channel, 32 on the classic ESP32).
pub const LAG_BUCKETS: usize = 9;

/// The bucket index [`ChannelState::record_lag`] would file `advanced` under,
/// for a channel whose half is `half_words` words.
///
/// The same scheme buckets the entry delay
/// ([`ChannelState::record_entry_delay`]), so the two histograms print with
/// identical edges and can be read side by side.
///
/// `half_words == 0` (an unconfigured channel, which never actually refills)
/// files everything in the overflow bucket rather than dividing by zero.
#[inline]
pub const fn lag_bucket(advanced: usize, half_words: usize) -> usize {
    if half_words == 0 || advanced >= half_words {
        return LAG_BUCKETS - 1;
    }
    (advanced * (LAG_BUCKETS - 1)) / half_words
}

/// A read-only snapshot of a channel's counters.
///
/// Cheap to take (a handful of relaxed loads) and safe to take at any time;
/// the fields are not sampled atomically with respect to each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChannelStats {
    /// Frames that reached `tx_end`, truncated or not.
    pub frames: usize,
    /// Frames that ended early because the transmitter hit a guard word — i.e.
    /// a refill interrupt arrived too late (or not at all).
    ///
    /// Detected by comparing the refill cursor against the frame length at
    /// `tx_end`. Because the cursor can over-count by up to one half after the
    /// transmitter has already stopped (see [`ChannelState::bits_emitted`]), a
    /// truncation in the *last* half of a frame can go uncounted; every earlier
    /// one is caught. It never over-reports.
    pub guard_trips: usize,
    /// Refills that declined to plant a guard because the read pointer had not
    /// yet passed the guard slot. Expected to be zero in practice; a non-zero
    /// value means interrupt latency is *lower* than the model assumes and the
    /// frame was protected from the driver rather than by it.
    pub guard_skips: usize,
    /// `tx_err` events observed (transmitter under-run).
    pub errors: usize,
    /// Sum of the read-pointer advance measured across each refill, in words.
    pub refill_lag_sum: i32,
    /// Number of refills contributing to [`Self::refill_lag_sum`].
    pub refill_lag_count: i32,
    /// Largest single-refill read-pointer advance seen, in words.
    ///
    /// This is the margin number: the deadline is one half (24 or 32 words
    /// depending on the chip), so `half_words - refill_lag_max` is how much
    /// slack the worst refill of the run had left.
    pub refill_lag_max: i32,
    /// Distribution of per-refill lag; see [`LAG_BUCKETS`].
    pub lag_hist: [u32; LAG_BUCKETS],
    /// Largest **entry delay** seen, in words: how far past the threshold
    /// boundary the transmitter had already run when the handler reached this
    /// channel.
    ///
    /// This is interrupt-to-service latency measured in the only clock the
    /// driver shares with the wire — one word is one bit, ≈1.25 µs at 800 kHz.
    /// It is the *other half* of the refill budget from
    /// [`Self::refill_lag_max`]: the lag counts what the refill itself cost,
    /// this counts what was spent before the refill started (interrupt
    /// dispatch, a higher-priority handler, and — with several channels
    /// flagged in one snapshot — the refills of every lower-numbered channel
    /// serviced first). `entry_delay_max + refill_lag_max` is an upper bound on
    /// the worst total occupancy of the deadline, which is one half.
    pub entry_delay_max: i32,
    /// Distribution of per-service entry delay, bucketed exactly like
    /// [`Self::lag_hist`]; see [`LAG_BUCKETS`].
    pub entry_delay_hist: [u32; LAG_BUCKETS],
}

impl ChannelStats {
    /// Mean read-pointer advance per refill, in words (one word = one bit on
    /// the wire ≈ 1.25 µs at 800 kHz). `None` before the first refill.
    pub fn mean_refill_lag(&self) -> Option<f32> {
        if self.refill_lag_count == 0 {
            None
        } else {
            Some(self.refill_lag_sum as f32 / self.refill_lag_count as f32)
        }
    }

    /// Frames that went out whole — every bit of the frame reached the wire.
    ///
    /// The complement of [`Self::guard_trips`], which counts the truncated
    /// ones. Note the caveat there: on a chip that re-raises an unacknowledged
    /// `tx_thr_event` (measured on the ESP32-S3, not on the classic ESP32 or
    /// the C6) a truncation in the frame's final half can go uncounted, so this
    /// is an upper bound on those chips.
    pub const fn complete_frames(&self) -> usize {
        self.frames.saturating_sub(self.guard_trips)
    }

    /// Refills that consumed a whole half or more — the last histogram bucket.
    ///
    /// A refill in this bucket finished with no margin at all; the next one was
    /// racing the guard word.
    pub const fn lag_over_half(&self) -> u32 {
        self.lag_hist[LAG_BUCKETS - 1]
    }

    /// Services that arrived a whole half or more after their threshold — the
    /// last entry-delay bucket.
    ///
    /// A service in this bucket had already lost the entire deadline before it
    /// wrote its first word; the frame was truncated whatever the refill cost.
    pub const fn entry_delay_over_half(&self) -> u32 {
        self.entry_delay_hist[LAG_BUCKETS - 1]
    }

    /// Number of services that contributed an entry-delay sample.
    ///
    /// Equal to [`Self::refill_lag_count`] in a healthy run — both are recorded
    /// once per serviced threshold — but derived from the histogram so a
    /// snapshot torn between the two is self-consistent.
    pub const fn entry_delay_count(&self) -> u32 {
        let mut total = 0u32;
        let mut i = 0;
        while i < LAG_BUCKETS {
            total = total.saturating_add(self.entry_delay_hist[i]);
            i += 1;
        }
        total
    }
}

/// Everything the interrupt handler needs to know about one channel.
#[derive(Debug)]
pub struct ChannelState {
    // --- configuration (written by `configure`, read everywhere) ---
    /// Total RMT RAM words assigned to this channel; 0 until configured.
    pub(crate) ram_words: AtomicUsize,
    /// `ram_words / 2` — the ping-pong half size, in words.
    pub(crate) half_words: AtomicUsize,
    pulse_zero: AtomicU32,
    pulse_one: AtomicU32,
    pulse_latch: AtomicU32,
    color_order: AtomicU8,

    // --- frame descriptor (published by `arm`) ---
    frame_ptr: AtomicPtr<u8>,
    frame_len: AtomicUsize,
    pub(crate) total_bits: AtomicUsize,

    // --- live transmission state (owned by the ISR after `arm`) ---
    pub(crate) bit_cursor: AtomicUsize,
    pub(crate) latch_written: AtomicBool,
    pub(crate) frame_complete: AtomicBool,
    /// The RAM word the currently armed `tx_lim` fires at.
    ///
    /// The driver alternates the threshold between `half_words` and
    /// `ram_words`, and the peripheral raises `tx_thr_event` as the read
    /// pointer reaches that word (`ram_words` meaning the wrap back to word
    /// 0). Recording it when it is armed is what lets the handler measure its
    /// own entry delay: the boundary is where the transmitter *was* when the
    /// cause appeared, and [`crate::RmtHw::read_pos`] says where it is now.
    ///
    /// Written by the driver only — from thread context at `start_frame`
    /// (before `start_tx`, so the two writers still never overlap even
    /// cross-core: the ISR cannot fire for a frame that has not started) and
    /// from the ISR at each threshold flip. The thread-side store is
    /// `Release` and the ISR's read `Acquire` because this field is stored
    /// *after* `arm`'s publish and would otherwise reach a cross-core ISR
    /// stale; it only feeds the entry-delay statistic, but a stale boundary
    /// files garbage samples.
    pub(crate) threshold_boundary: AtomicUsize,

    // --- counters ---
    pub(crate) frames: AtomicUsize,
    pub(crate) guard_trips: AtomicUsize,
    pub(crate) guard_skips: AtomicUsize,
    pub(crate) errors: AtomicUsize,
    pub(crate) refill_lag_sum: AtomicI32,
    pub(crate) refill_lag_count: AtomicI32,
    pub(crate) refill_lag_max: AtomicI32,
    pub(crate) lag_hist: [AtomicU32; LAG_BUCKETS],
    pub(crate) entry_delay_max: AtomicI32,
    pub(crate) entry_delay_hist: [AtomicU32; LAG_BUCKETS],
}

impl ChannelState {
    /// An unconfigured, idle channel.
    pub const fn new() -> Self {
        Self {
            ram_words: AtomicUsize::new(0),
            half_words: AtomicUsize::new(0),
            pulse_zero: AtomicU32::new(0),
            pulse_one: AtomicU32::new(0),
            pulse_latch: AtomicU32::new(0),
            color_order: AtomicU8::new(ColorOrder::Grb as u8),
            frame_ptr: AtomicPtr::new(core::ptr::null_mut()),
            frame_len: AtomicUsize::new(0),
            total_bits: AtomicUsize::new(0),
            bit_cursor: AtomicUsize::new(0),
            latch_written: AtomicBool::new(true),
            frame_complete: AtomicBool::new(true),
            threshold_boundary: AtomicUsize::new(0),
            frames: AtomicUsize::new(0),
            guard_trips: AtomicUsize::new(0),
            guard_skips: AtomicUsize::new(0),
            errors: AtomicUsize::new(0),
            refill_lag_sum: AtomicI32::new(0),
            refill_lag_count: AtomicI32::new(0),
            refill_lag_max: AtomicI32::new(0),
            lag_hist: [const { AtomicU32::new(0) }; LAG_BUCKETS],
            entry_delay_max: AtomicI32::new(0),
            entry_delay_hist: [const { AtomicU32::new(0) }; LAG_BUCKETS],
        }
    }

    /// File one refill's read-pointer advance into the counters. ISR only.
    ///
    /// One division and one bucket increment per refill — cheap next to the
    /// half-buffer fill that precedes them, and the histogram is the only way
    /// to tell "comfortably ahead" from "made it by one word". An average
    /// cannot, and the average is what lp2025 had.
    ///
    /// The running maximum is a plain load/compare/store rather than
    /// `fetch_max`. Originally that was forced: `AtomicI32::fetch_max` did not
    /// compile for Xtensa on the then-pinned `esp` toolchain (rustc
    /// 1.88.0-nightly / LLVM 18) — its `S32C1I` compare-exchange loop made the
    /// backend emit `error: Undefined temporary symbol` for the whole crate
    /// graph, on both the ESP32 and the ESP32-S3. **That is fixed as of the
    /// `esp` channel's rustc 1.95.0-nightly (2026-04-15), verified 2026-07-29
    /// by compiling `fetch_max` for `xtensa-esp32s3-none-elf`.** The
    /// load/compare/store is kept because it is also cheaper than a CAS loop on
    /// the ISR path, not because it is required. Losing an update would
    /// require a second writer, and there is none: `record_lag` runs only in
    /// the interrupt handler, which never nests with itself. (A concurrent
    /// `reset_stats` from thread context can lose one reset, which is
    /// harmless — the same is true of every other counter here.)
    #[inline]
    #[cfg_attr(feature = "isr-in-ram", link_section = ".rwtext")]
    pub(crate) fn record_lag(&self, advanced: usize, half_words: usize) {
        let advanced_i32 = advanced.min(i32::MAX as usize) as i32;
        self.refill_lag_sum.fetch_add(advanced_i32, Relaxed);
        self.refill_lag_count.fetch_add(1, Relaxed);
        if self.refill_lag_max.load(Relaxed) < advanced_i32 {
            self.refill_lag_max.store(advanced_i32, Relaxed);
        }
        self.lag_hist[lag_bucket(advanced, half_words)].fetch_add(1, Relaxed);
    }

    /// File one service's **entry delay** into the counters. ISR only.
    ///
    /// `delay` is `(read_pos - threshold_boundary) mod ram_words` sampled at
    /// the top of the channel's service — the words the transmitter got
    /// through between raising `tx_thr_event` and the handler looking at it.
    /// See [`ChannelStats::entry_delay_max`] for why that number and the
    /// refill lag are the two halves of one budget.
    ///
    /// Cheaper than [`Self::record_lag`] and the same shape — one division,
    /// one compare, one increment — and the same single-writer argument (the
    /// interrupt handler never nests with itself) makes the load/compare/store
    /// maximum sound.
    #[inline]
    #[cfg_attr(feature = "isr-in-ram", link_section = ".rwtext")]
    pub(crate) fn record_entry_delay(&self, delay: usize, half_words: usize) {
        let delay_i32 = delay.min(i32::MAX as usize) as i32;
        if self.entry_delay_max.load(Relaxed) < delay_i32 {
            self.entry_delay_max.store(delay_i32, Relaxed);
        }
        self.entry_delay_hist[lag_bucket(delay, half_words)].fetch_add(1, Relaxed);
    }

    /// Store the compiled timing. Thread context only, while idle.
    pub(crate) fn set_timing(&self, codes: &PulseCodes, order: ColorOrder) {
        self.pulse_zero.store(codes.zero, Relaxed);
        self.pulse_one.store(codes.one, Relaxed);
        self.pulse_latch.store(codes.latch, Relaxed);
        self.color_order.store(order.as_u8(), Relaxed);
    }

    /// The compiled timing as written by [`Self::set_timing`].
    #[cfg_attr(feature = "isr-in-ram", link_section = ".rwtext")]
    pub(crate) fn codes(&self) -> PulseCodes {
        PulseCodes {
            zero: self.pulse_zero.load(Relaxed),
            one: self.pulse_one.load(Relaxed),
            latch: self.pulse_latch.load(Relaxed),
        }
    }

    /// The configured byte order.
    #[cfg_attr(feature = "isr-in-ram", link_section = ".rwtext")]
    pub(crate) fn color_order(&self) -> ColorOrder {
        // The only writer stores a valid discriminant, so the fallback is
        // unreachable; it exists so the ISR path has no panic.
        ColorOrder::from_u8(self.color_order.load(Relaxed)).unwrap_or(ColorOrder::Grb)
    }

    /// True once the channel has been given RAM and timing.
    pub(crate) fn is_configured(&self) -> bool {
        self.half_words.load(Relaxed) != 0
    }

    /// Publish a frame and reset the per-frame state.
    ///
    /// # Safety
    ///
    /// `ptr` must point to `len` readable bytes that stay valid and immutable
    /// until the frame completes (see [`crate::driver::Ws281xDriver::start_frame`]).
    pub(crate) unsafe fn arm(&self, ptr: *const u8, len: usize) {
        let pixels = len / BYTES_PER_PIXEL;
        self.frame_ptr.store(ptr.cast_mut(), Relaxed);
        self.frame_len.store(pixels * BYTES_PER_PIXEL, Relaxed);
        self.total_bits.store(pixels * BITS_PER_PIXEL, Relaxed);
        self.latch_written.store(false, Relaxed);
        self.bit_cursor.store(0, Release);
        // Release: publishes every store above to the ISR's Acquire load.
        self.frame_complete.store(false, Release);
    }

    /// Drop the frame descriptor and mark the channel idle.
    ///
    /// Length is cleared before the pointer so a racing [`Self::frame_byte`]
    /// that sees the new state piecemeal fails its bounds check first, and the
    /// pointer store is `Release` as belt-and-braces — but neither is the
    /// guarantee. A cross-core ISR pass that entered before this call may
    /// still be dereferencing the **old** pointer through `Relaxed` loads;
    /// what makes that sound is the `isr_seq` handshake in
    /// [`crate::Ws281xDriver::abort`], which keeps the caller from freeing or
    /// reusing the frame bytes until that pass has exited. The `SeqCst` on
    /// `frame_complete` is the store half of the store/load pattern that
    /// handshake's soundness argument requires (see
    /// [`Self::is_complete_sync`]).
    pub(crate) fn disarm(&self) {
        self.frame_len.store(0, Relaxed);
        self.frame_ptr.store(core::ptr::null_mut(), Release);
        self.frame_complete.store(true, SeqCst);
    }

    /// Read the frame byte at `index`, or `0` if the index is out of range.
    ///
    /// The bounds check makes the raw read sound even if a caller violated the
    /// `arm` contract about `len`; it costs one compare per byte on the refill
    /// path.
    ///
    /// The loads are deliberately `Relaxed` even though a cross-core `disarm`
    /// can race them: this function runs **per frame byte** on the refill
    /// path, and an acquiring (`memw`-fenced) load here would tax every byte
    /// of every refill. Soundness does not depend on seeing the disarm — a
    /// pass that misses it reads the *old* descriptor, whose bytes the
    /// `isr_seq` handshake in [`crate::Ws281xDriver::abort`] keeps alive
    /// until this pass exits. A torn view (new `len`, old `ptr`, or vice
    /// versa) yields zeros or old-frame bytes on a frame that is being
    /// aborted anyway — never a dangling read.
    #[inline]
    #[cfg_attr(feature = "isr-in-ram", link_section = ".rwtext")]
    pub(crate) fn frame_byte(&self, index: usize) -> u8 {
        if index >= self.frame_len.load(Relaxed) {
            return 0;
        }
        let base = self.frame_ptr.load(Relaxed);
        if base.is_null() {
            return 0;
        }
        // SAFETY: `arm` promised `frame_ptr` addresses at least `frame_len`
        // readable bytes for the whole transmission, and `index < frame_len`
        // was just checked. The frame is `u8`, so the offset cannot violate
        // alignment, and `len` fits in an `isize` because it came from a slice.
        unsafe { *base.add(index) }
    }

    /// True while a frame is in flight.
    pub fn is_busy(&self) -> bool {
        !self.frame_complete.load(Acquire)
    }

    /// True once the in-flight frame has reached `tx_end`.
    pub fn is_complete(&self) -> bool {
        self.frame_complete.load(Acquire)
    }

    /// [`Self::is_complete`] with `SeqCst` — the ISR's entry check.
    ///
    /// The interrupt handler must use this form, not the `Acquire` one, at
    /// the top of a channel's service. It is one half of a store/load
    /// ("Dekker") pattern with [`crate::Ws281xDriver::abort`]: the ISR does
    /// *increment `isr_seq`, then load `frame_complete`*; abort does *store
    /// `frame_complete` (in [`Self::disarm`]), then load `isr_seq`*. The
    /// forbidden outcome — the ISR missing the disarm **and** abort missing
    /// the in-service increment, which would let abort's caller free frame
    /// bytes a refill is about to read — is excluded only in the `SeqCst`
    /// total order; Release/Acquire alone permits it (the classic
    /// store-buffer litmus). On Xtensa every one of these compiles to the
    /// same `memw`-fenced access an `Acquire` load costs, and it runs once
    /// per channel service, not per byte.
    #[cfg_attr(feature = "isr-in-ram", link_section = ".rwtext")]
    pub(crate) fn is_complete_sync(&self) -> bool {
        self.frame_complete.load(SeqCst)
    }

    /// Bits **written into RMT RAM** so far for the current frame.
    ///
    /// In healthy operation this is one window ahead of the wire and reaches
    /// `total_bits()` exactly. After a guard trip it is *not* a measure of what
    /// reached the strip: the transmitter latches the guard word ahead of the
    /// `tx_end` that reports it, and a refill serviced in that window still
    /// advances the cursor even though the words it writes are never read. On
    /// an ESP32-S3 that was measured at one half (24 bits) of over-count —
    /// the hardware re-raises a `tx_thr_event` that went unacknowledged, and
    /// the replacement refill lands inside the gap. Use the counters, or a
    /// receiver, to say what actually went out.
    pub fn bits_emitted(&self) -> usize {
        self.bit_cursor.load(Acquire)
    }

    /// Bits the current frame consists of.
    pub fn total_bits(&self) -> usize {
        self.total_bits.load(Relaxed)
    }

    /// The ping-pong half size in words — the refill deadline the lag figures
    /// are measured against. Zero until [`crate::Ws281xDriver::configure`].
    pub fn half_words(&self) -> usize {
        self.half_words.load(Relaxed)
    }

    /// Snapshot of the counters.
    pub fn stats(&self) -> ChannelStats {
        let mut lag_hist = [0u32; LAG_BUCKETS];
        for (slot, cell) in lag_hist.iter_mut().zip(self.lag_hist.iter()) {
            *slot = cell.load(Relaxed);
        }
        let mut entry_delay_hist = [0u32; LAG_BUCKETS];
        for (slot, cell) in entry_delay_hist
            .iter_mut()
            .zip(self.entry_delay_hist.iter())
        {
            *slot = cell.load(Relaxed);
        }
        ChannelStats {
            frames: self.frames.load(Relaxed),
            guard_trips: self.guard_trips.load(Relaxed),
            guard_skips: self.guard_skips.load(Relaxed),
            errors: self.errors.load(Relaxed),
            refill_lag_sum: self.refill_lag_sum.load(Relaxed),
            refill_lag_count: self.refill_lag_count.load(Relaxed),
            refill_lag_max: self.refill_lag_max.load(Relaxed),
            lag_hist,
            entry_delay_max: self.entry_delay_max.load(Relaxed),
            entry_delay_hist,
        }
    }

    /// Zero every counter. Thread context only.
    pub fn reset_stats(&self) {
        self.frames.store(0, Relaxed);
        self.guard_trips.store(0, Relaxed);
        self.guard_skips.store(0, Relaxed);
        self.errors.store(0, Relaxed);
        self.refill_lag_sum.store(0, Relaxed);
        self.refill_lag_count.store(0, Relaxed);
        self.refill_lag_max.store(0, Relaxed);
        for cell in self.lag_hist.iter() {
            cell.store(0, Relaxed);
        }
        self.entry_delay_max.store(0, Relaxed);
        for cell in self.entry_delay_hist.iter() {
            cell.store(0, Relaxed);
        }
    }
}

impl Default for ChannelState {
    fn default() -> Self {
        Self::new()
    }
}
