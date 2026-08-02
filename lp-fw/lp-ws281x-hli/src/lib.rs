//! Contract and reference model for the classic-ESP32 **level-4
//! high-priority-interrupt WS281x refill** — the HLI experiment.
//!
//! # What this crate is
//!
//! `fw-esp32v3`'s `hli_refill` feature services the RMT `tx_thr_event`
//! interrupt from a **hand-written Xtensa level-4 handler** (call0 discipline,
//! no windowed ABI — ordinary Rust cannot run in that context). Assembly can
//! be proven correct on silicon, but its *algorithm* deserves a host test
//! suite like the one `lp-ws281x` has. This crate is how: it holds
//!
//! * [`HliChannel`] / [`HliBank`] — the exact `#[repr(C)]` state the vector
//!   reads and writes. The firmware derives every assembly field offset from
//!   these structs with `core::mem::offset_of!`, so the layout cannot drift
//!   from the code that consumes it.
//! * The **reference model** ([`service_flags`], [`service_threshold`],
//!   [`service_end`], [`fill_half`], [`start_frame_state`]) — the same
//!   algorithm in Rust, statement for statement. The firmware runs the model
//!   on the thread-side start path (prefill), and the host tests pin its
//!   behavior against `lp-ws281x`'s driver as the oracle; the assembly then
//!   only has to be equal to the model, which silicon verifies end to end (a
//!   divergence corrupts the LED stream, which the level-4 telemetry counts).
//!
//! # Provenance
//!
//! Clean-room work. The algorithm is a re-expression of this project's own
//! `lp-ws281x` ping-pong refill (`driver.rs::refill`/`fill_half`), restricted
//! to what fixed-work assembly can do: power-of-two windows, byte-aligned
//! halves, and a frame buffer already in **wire order** (the color-order
//! permutation happens once on the thread side, not per bit in the handler).
//! No copyleft source was consulted; WLED and NeoPixelBus were never opened
//! (see `docs/adr/2026-08-02-classic-hli-refill.md` and the provenance log in
//! the plan's notes). Register semantics referenced from the `esp32` PAC
//! field docs and the ESP32 TRM.
//!
//! # The contract, in one place
//!
//! The level-4 handler (and therefore this model) assumes, and
//! [`configure_channel`] enforces:
//!
//! * `ram_words` is a **power of two** (so `mod` is an AND with
//!   [`HliChannel::ram_mask`]) and `half_words = ram_words / 2` is a multiple
//!   of 8 (so a refill walks whole bytes — `total_bits` is always a multiple
//!   of 8 because a pixel is 24 bits).
//! * `frame_ptr` points at **wire-order** bytes (color order pre-applied) that
//!   stay valid and unmodified while `active != 0`.
//! * The classic ESP32's `CH_TX_LIM` holds a repeating **count**, not a
//!   position (see `fw-esp32v3`'s `v3_rmt::set_tx_threshold` for the measured
//!   history): the armed value is always `half_words`, and the software
//!   [`HliChannel::boundary`] alternates `half ↔ 0` to name the window
//!   position each event corresponds to.
//! * Single core, single writer: the handler never nests with itself
//!   (level 4 masks level 4), and the thread side only mutates a channel while
//!   `active == 0`. All fields are `AtomicU32` for the thread-side reads
//!   (counters, `complete`), with `Relaxed` orderings throughout — on the
//!   single-core ESP32 the interrupt/thread interleaving is program order.

#![no_std]

use core::sync::atomic::{AtomicU32, AtomicUsize};
use core::sync::atomic::Ordering::Relaxed;

/// An all-zero RMT word: the STOP marker (same value as
/// `lp_ws281x::pulse::STOP_WORD`, restated here so the contract crate stays
/// dependency-free).
pub const STOP_WORD: u32 = 0;

/// Histogram buckets, matching `lp_ws281x::LAG_BUCKETS`: eighths of a half,
/// plus the "at or beyond the half" overflow bucket.
pub const LAG_BUCKETS: usize = 9;

/// RMT slots the level-4 path services — the classic's four two-block
/// transmitters (silicon slots 0/2/4/6). The bank is indexed by *entry*, not
/// by slot: each entry carries its own interrupt-cause masks, so the handler
/// never needs the slot number.
pub const HLI_CHANNELS: usize = 4;

/// Per-channel state shared between the thread side and the level-4 vector.
///
/// `#[repr(C)]`, every field a 4-byte `AtomicU32`: the assembly addresses
/// fields as `base + offset_of!(...)`, passed to `global_asm!` as `const`
/// operands — except the pointer-valued fields (`*_addr`, `ram_base`,
/// `frame_ptr`), which are `AtomicUsize` so the model also runs on 64-bit
/// hosts; on the 32-bit target they are 4 bytes like everything else.
#[repr(C)]
#[derive(Debug)]
pub struct HliChannel {
    // ---- interrupt-cause masks (configure-time constants) ----
    /// `chN_tx_thr_event` bit for this entry's slot in the RMT `INT_*`
    /// registers (bit `24+N` on the classic).
    pub thr_mask: AtomicU32,
    /// `chN_tx_end` bit (bit `3N`).
    pub end_mask: AtomicU32,
    /// `chN_err` bit (bit `3N+2`).
    pub err_mask: AtomicU32,

    // ---- hardware addresses and window geometry (configure-time) ----
    /// Address of the slot's `CHnSTATUS` register; `mem_raddr_ex` is bits
    /// 12..=21 (the handler's one hard-coded field layout, from the `esp32`
    /// PAC).
    pub status_addr: AtomicUsize,
    /// Address of the slot's `CH_TX_LIM` register (9-bit count field).
    pub tx_lim_addr: AtomicUsize,
    /// Address of word 0 of the slot's RMT RAM window.
    pub ram_base: AtomicUsize,
    /// The window's first word, counted from the start of the whole RMT RAM —
    /// what `mem_raddr_ex` (an absolute offset) must have subtracted from it.
    pub window_start: AtomicU32,
    /// Words per ping-pong half.
    pub half_words: AtomicU32,
    /// `ram_words - 1`; `ram_words` is a power of two so this is the modulus.
    pub ram_mask: AtomicU32,
    /// `log2(half_words) - 3`: histogram bucket = `delay >> bucket_shift`
    /// (clamped to 8), equal to `lp_ws281x::lag_bucket` for power-of-two
    /// halves — the host suite proves that equivalence.
    pub bucket_shift: AtomicU32,

    // ---- pulse codes (configure-time) ----
    /// RMT word for a 0 bit.
    pub code_zero: AtomicU32,
    /// RMT word for a 1 bit.
    pub code_one: AtomicU32,
    /// RMT word for the end-of-frame latch.
    pub code_latch: AtomicU32,

    // ---- frame state ----
    /// Non-zero while a frame is in flight. The handler ignores inactive
    /// channels (their causes are still acknowledged, so a level-triggered
    /// line can never storm).
    pub active: AtomicU32,
    /// Wire-order frame bytes; valid while `active != 0`.
    pub frame_ptr: AtomicUsize,
    /// Bits this frame transmits (`(bytes / 3) * 24` — always a multiple
    /// of 8).
    pub total_bits: AtomicU32,
    /// Next bit to encode. Multiple of 8 whenever the handler runs.
    pub bit_cursor: AtomicU32,
    /// The latch word has been written (once per frame).
    pub latch_written: AtomicU32,
    /// Window position (words) the *armed* threshold event corresponds to:
    /// alternates `half_words ↔ 0`. Already reduced mod `ram_words`.
    pub boundary: AtomicU32,
    /// Set by the handler at `tx_end`; the thread side polls it.
    pub complete: AtomicU32,

    // ---- counters (same meanings as `lp_ws281x::ChannelStats`) ----
    /// Frames that reached `tx_end`, truncated or not.
    pub frames: AtomicU32,
    /// Frames that ended short of `total_bits` — a lost/late refill hit the
    /// guard word.
    pub trips: AtomicU32,
    /// Refills that ended with no guard planted: the read pointer sat on the
    /// guard slot at entry **and** was still there after the fill. (The
    /// level-3 driver counts pre-fill misses only; this path retries after
    /// the fill because a zero-entry-delay service is its normal case.)
    pub skips: AtomicU32,
    /// `chN_err` causes seen.
    pub errors: AtomicU32,
    /// Worst interrupt-to-service latency, in words.
    pub entry_max: AtomicU32,
    /// Sum of refill lags (words the reader advanced during the refill).
    pub lag_sum: AtomicU32,
    /// Refills performed (the `refills=` telemetry field).
    pub lag_count: AtomicU32,
    /// Worst refill lag, in words.
    pub lag_max: AtomicU32,
    /// Services where the reader's half disagreed with the armed boundary's
    /// half — the signature of a missed or duplicated threshold event (the
    /// boundary bookkeeping and the silicon got out of step). Diagnostic for
    /// the stress trips question.
    pub sel_mismatch: AtomicU32,
    /// Entry-delay histogram, eighths of a half + overflow.
    pub entry_hist: [AtomicU32; LAG_BUCKETS],
    /// Refill-lag histogram, same edges.
    pub lag_hist: [AtomicU32; LAG_BUCKETS],
}

impl HliChannel {
    /// An all-zero channel: inactive, unconfigured.
    pub const fn new() -> Self {
        // `AtomicU32::new(0)` is not `Copy`; spell the arrays out via a const
        // block, the same trick `lp-ws281x`'s driver uses for its state array.
        Self {
            thr_mask: AtomicU32::new(0),
            end_mask: AtomicU32::new(0),
            err_mask: AtomicU32::new(0),
            status_addr: AtomicUsize::new(0),
            tx_lim_addr: AtomicUsize::new(0),
            ram_base: AtomicUsize::new(0),
            window_start: AtomicU32::new(0),
            half_words: AtomicU32::new(0),
            ram_mask: AtomicU32::new(0),
            bucket_shift: AtomicU32::new(0),
            code_zero: AtomicU32::new(0),
            code_one: AtomicU32::new(0),
            code_latch: AtomicU32::new(0),
            active: AtomicU32::new(0),
            frame_ptr: AtomicUsize::new(0),
            total_bits: AtomicU32::new(0),
            bit_cursor: AtomicU32::new(0),
            latch_written: AtomicU32::new(0),
            boundary: AtomicU32::new(0),
            complete: AtomicU32::new(0),
            frames: AtomicU32::new(0),
            trips: AtomicU32::new(0),
            skips: AtomicU32::new(0),
            errors: AtomicU32::new(0),
            entry_max: AtomicU32::new(0),
            lag_sum: AtomicU32::new(0),
            lag_count: AtomicU32::new(0),
            lag_max: AtomicU32::new(0),
            sel_mismatch: AtomicU32::new(0),
            entry_hist: [const { AtomicU32::new(0) }; LAG_BUCKETS],
            lag_hist: [const { AtomicU32::new(0) }; LAG_BUCKETS],
        }
    }
}

impl Default for HliChannel {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything the level-4 vector touches, in one `static`.
#[repr(C)]
#[derive(Debug)]
pub struct HliBank {
    /// Address of the RMT `INT_ST` register (`raw & ena` — causes this
    /// firmware never enabled cannot appear).
    pub int_st_addr: AtomicUsize,
    /// Address of the RMT `INT_CLR` register (write-1-to-clear).
    pub int_clr_addr: AtomicUsize,
    /// Union of every entry's three cause masks: the causes this handler owns.
    /// Everything pending under this mask is acknowledged in one write at
    /// entry, whether or not the channel is active — storm safety for a
    /// level-triggered line.
    pub all_mask: AtomicU32,
    /// Level-4 entries taken (diagnostic; includes entries that found no
    /// cause).
    pub isr_entries: AtomicU32,
    /// The four serviced transmitters.
    pub channels: [HliChannel; HLI_CHANNELS],
}

impl HliBank {
    /// An empty bank; [`configure_channel`] and the firmware's install step
    /// fill it in.
    pub const fn new() -> Self {
        Self {
            int_st_addr: AtomicUsize::new(0),
            int_clr_addr: AtomicUsize::new(0),
            all_mask: AtomicU32::new(0),
            isr_entries: AtomicU32::new(0),
            channels: [const { HliChannel::new() }; HLI_CHANNELS],
        }
    }
}

impl Default for HliBank {
    fn default() -> Self {
        Self::new()
    }
}

/// The three memory-mapped operations the model performs, abstracted so host
/// tests can substitute a simulated transmitter. The firmware's implementation
/// is three volatile accesses on the addresses stored in the channel.
pub trait HliPort {
    /// `mem_raddr_ex`: the read pointer as an absolute word offset into the
    /// whole RMT RAM.
    fn read_pos_abs(&mut self) -> u32;
    /// Write the `CH_TX_LIM` count field.
    fn write_tx_lim(&mut self, words: u32);
    /// Write one word of the channel's window (`word` is window-relative).
    fn write_ram(&mut self, word: u32, value: u32);
}

/// Why [`configure_channel`] refused a geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HliConfigError {
    /// `ram_words` is not a power of two — the handler's AND-modulus needs it.
    RamNotPowerOfTwo,
    /// `half_words` is not a multiple of 8 — the byte-walking refill needs it.
    HalfNotByteAligned,
    /// The window is too small to hold one byte per half.
    RamTooSmall,
}

/// Bind geometry, pulse codes and register addresses to a channel. Thread
/// side, channel inactive.
///
/// This is the only place the contract's numeric preconditions are checked;
/// everything downstream (model and assembly alike) assumes them.
#[expect(
    clippy::too_many_arguments,
    reason = "a flat configure call keeps the contract crate free of a builder \
              type the firmware would use exactly once"
)]
pub fn configure_channel(
    ch: &HliChannel,
    masks: (u32, u32, u32),
    status_addr: usize,
    tx_lim_addr: usize,
    ram_base: usize,
    window_start: u32,
    ram_words: u32,
    codes: (u32, u32, u32),
) -> Result<(), HliConfigError> {
    if !ram_words.is_power_of_two() {
        return Err(HliConfigError::RamNotPowerOfTwo);
    }
    let half = ram_words / 2;
    if half < 8 {
        return Err(HliConfigError::RamTooSmall);
    }
    if half % 8 != 0 {
        return Err(HliConfigError::HalfNotByteAligned);
    }
    let (thr, end, err) = masks;
    let (zero, one, latch) = codes;
    ch.thr_mask.store(thr, Relaxed);
    ch.end_mask.store(end, Relaxed);
    ch.err_mask.store(err, Relaxed);
    ch.status_addr.store(status_addr, Relaxed);
    ch.tx_lim_addr.store(tx_lim_addr, Relaxed);
    ch.ram_base.store(ram_base, Relaxed);
    ch.window_start.store(window_start, Relaxed);
    ch.half_words.store(half, Relaxed);
    ch.ram_mask.store(ram_words - 1, Relaxed);
    // half = 2^k, k >= 3 here; bucket = delay >> (k - 3) — proven equal to
    // lp_ws281x::lag_bucket by the host suite.
    ch.bucket_shift.store(half.trailing_zeros() - 3, Relaxed);
    ch.code_zero.store(zero, Relaxed);
    ch.code_one.store(one, Relaxed);
    ch.code_latch.store(latch, Relaxed);
    Ok(())
}

/// Histogram bucket for `value` on a channel: eighths of the half, overflow
/// at or beyond it. The shift form the assembly uses.
#[inline]
fn bucket(ch: &HliChannel, value: u32) -> usize {
    let half = ch.half_words.load(Relaxed);
    if value >= half {
        LAG_BUCKETS - 1
    } else {
        (value >> ch.bucket_shift.load(Relaxed)) as usize
    }
}

/// Write one half of the window from the bit cursor onwards — the model of
/// the handler's fill loop, byte-granular by contract.
///
/// # Safety
///
/// `ch.frame_ptr` must point at at least `total_bits / 8` readable bytes and
/// stay valid for the duration of the call.
pub unsafe fn fill_half(ch: &HliChannel, port: &mut impl HliPort, start_word: u32) {
    let half = ch.half_words.load(Relaxed);
    let end = start_word + half;
    let total = ch.total_bits.load(Relaxed);
    let code0 = ch.code_zero.load(Relaxed);
    let code1 = ch.code_one.load(Relaxed);
    let frame = ch.frame_ptr.load(Relaxed) as *const u8;
    let mut cursor = ch.bit_cursor.load(Relaxed);
    let mut word = start_word;

    // Data: whole bytes. `cursor`, `total`, `word` and `end` are all
    // multiples of 8 here (the contract), so the two exit conditions can be
    // checked once per byte, exactly as the assembly does.
    loop {
        if cursor >= total {
            break;
        }
        // SAFETY: cursor < total, and the caller guarantees total/8 readable
        // bytes behind frame_ptr.
        let byte = unsafe { frame.add((cursor >> 3) as usize).read() };
        for k in 0..8u32 {
            let one = byte & (0x80 >> k) != 0;
            port.write_ram(word + k, if one { code1 } else { code0 });
        }
        word += 8;
        cursor += 8;
        if word >= end {
            // The half filled with data alone; more refills to come.
            ch.bit_cursor.store(cursor, Relaxed);
            return;
        }
    }

    // Tail: the data ended inside this half (or in an earlier one). Emit the
    // latch exactly once, then STOP-fill to the boundary. If the data ended
    // exactly on the previous half boundary, `word < end` still holds here
    // and the latch lands as this half's first word — the driver-core
    // behavior this mirrors puts it at the start of the *next* half in that
    // case, which is exactly where `word` is.
    if ch.latch_written.load(Relaxed) == 0 {
        port.write_ram(word, ch.code_latch.load(Relaxed));
        word += 1;
        ch.latch_written.store(1, Relaxed);
    }
    while word < end {
        port.write_ram(word, STOP_WORD);
        word += 1;
    }
    ch.bit_cursor.store(cursor, Relaxed);
}

/// Service one `tx_thr_event` for `ch` — the model of the handler's refill:
/// entry-delay telemetry, boundary flip, re-arm, guard word, fill, lag
/// telemetry. Statement order mirrors `lp_ws281x::Ws281xDriver::refill`.
///
/// # Safety
///
/// As [`fill_half`]: the frame bytes must be valid while the channel is
/// active.
pub unsafe fn service_threshold(ch: &HliChannel, port: &mut impl HliPort) {
    if ch.active.load(Relaxed) == 0 {
        return;
    }
    let half = ch.half_words.load(Relaxed);
    let mask = ch.ram_mask.load(Relaxed);
    if half == 0 {
        return;
    }

    let window_start = ch.window_start.load(Relaxed);
    let pos = port.read_pos_abs().wrapping_sub(window_start) & mask;

    // Entry delay against the boundary the armed event corresponds to,
    // before anything else moves.
    let boundary = ch.boundary.load(Relaxed);
    let delay = pos.wrapping_sub(boundary) & mask;
    if delay > ch.entry_max.load(Relaxed) {
        ch.entry_max.store(delay, Relaxed);
    }
    ch.entry_hist[bucket(ch, delay)].fetch_add(1, Relaxed);

    // Selection-mismatch diagnostic: the armed boundary names the half the
    // reader should be inside when this event is serviced (boundary `half` ->
    // second half, boundary 0 -> first). Disagreement means an event was
    // missed or duplicated somewhere between the silicon and this bookkeeping.
    let expected_in_second = boundary != 0;
    if (pos >= half) != expected_in_second {
        ch.sel_mismatch.fetch_add(1, Relaxed);
    }

    // The transmitter is inside one half; the other is free.
    let (free_start, guard_slot, new_boundary) = if pos >= half {
        (0, half, 0)
    } else {
        (half, 0, half)
    };

    // Arm the next event first — the fill is the long part. The classic's
    // `CH_TX_LIM` is a repeating count, so the written value is always the
    // half size; only the software boundary alternates.
    ch.boundary.store(new_boundary, Relaxed);
    port.write_tx_lim(half);

    // Guard placement. The level-4 handler routinely arrives with the reader
    // still ON the boundary slot (entry delay 0 — measured on silicon, where
    // `skips == refills` before this retry existed), so a pre-fill-only
    // guard would leave most refills unguarded. Plant it up front when the
    // slot is already clear; otherwise retry after the fill, by which point
    // the reader has moved on. `skips` counts only refills left unguarded by
    // BOTH attempts.
    let deferred_guard = if pos == guard_slot {
        Some(guard_slot)
    } else {
        port.write_ram(guard_slot, STOP_WORD);
        None
    };

    // SAFETY: forwarded contract.
    unsafe { fill_half(ch, port, free_start) };

    let pos_after = port.read_pos_abs().wrapping_sub(window_start) & mask;
    let advanced = pos_after.wrapping_sub(pos) & mask;
    ch.lag_sum.fetch_add(advanced, Relaxed);
    ch.lag_count.fetch_add(1, Relaxed);
    if advanced > ch.lag_max.load(Relaxed) {
        ch.lag_max.store(advanced, Relaxed);
    }
    ch.lag_hist[bucket(ch, advanced)].fetch_add(1, Relaxed);

    if let Some(slot) = deferred_guard {
        let pos_now = port.read_pos_abs().wrapping_sub(window_start) & mask;
        if pos_now == slot {
            ch.skips.fetch_add(1, Relaxed);
        } else {
            port.write_ram(slot, STOP_WORD);
        }
    }
}

/// Service one `tx_end` for `ch` — the model of the handler's finish: classify
/// truncation, count the frame, publish completion, deactivate.
pub fn service_end(ch: &HliChannel) {
    if ch.active.load(Relaxed) == 0 {
        return;
    }
    if ch.bit_cursor.load(Relaxed) < ch.total_bits.load(Relaxed) {
        ch.trips.fetch_add(1, Relaxed);
    }
    ch.frames.fetch_add(1, Relaxed);
    ch.active.store(0, Relaxed);
    ch.complete.store(1, Relaxed);
}

/// Dispatch one snapshot of this channel's causes, with the handler's
/// precedence: an error is counted independently; `tx_end` wins over
/// `tx_thr_event` (a finished frame has nothing to refill).
///
/// # Safety
///
/// As [`fill_half`].
pub unsafe fn service_flags(
    ch: &HliChannel,
    port: &mut impl HliPort,
    thr: bool,
    end: bool,
    err: bool,
) {
    if err {
        ch.errors.fetch_add(1, Relaxed);
    }
    if end {
        service_end(ch);
    } else if thr {
        // SAFETY: forwarded contract.
        unsafe { service_threshold(ch, port) };
    }
}

/// Thread-side frame arming: bind the (wire-order) frame, prefill both
/// halves, arm the first threshold, and mark the channel active. The caller
/// starts the transmitter immediately after (and only after) this returns.
///
/// # Safety
///
/// `frame_ptr` must point at `frame_bytes` readable bytes that stay valid,
/// in place and unmodified until [`HliChannel::complete`] reads non-zero or
/// the channel is aborted (`active` stored 0 **before** the transmitter is
/// stopped).
pub unsafe fn start_frame_state(
    ch: &HliChannel,
    port: &mut impl HliPort,
    frame_ptr: usize,
    frame_bytes: u32,
) {
    let half = ch.half_words.load(Relaxed);
    ch.frame_ptr.store(frame_ptr, Relaxed);
    // Whole pixels only, mirroring the driver core: 24 bits per 3-byte
    // triplet, trailing partial triplets ignored.
    ch.total_bits.store((frame_bytes / 3) * 24, Relaxed);
    ch.bit_cursor.store(0, Relaxed);
    ch.latch_written.store(0, Relaxed);
    ch.complete.store(0, Relaxed);

    // SAFETY: forwarded contract.
    unsafe {
        fill_half(ch, port, 0);
        fill_half(ch, port, half);
    }

    ch.boundary.store(half, Relaxed);
    port.write_tx_lim(half);
    ch.active.store(1, Relaxed);
}

#[cfg(test)]
mod tests;
