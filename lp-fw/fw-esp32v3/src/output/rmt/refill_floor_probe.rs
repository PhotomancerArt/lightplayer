//! Boot-time cycle probe: what does one 64-word RMT RAM refill *have* to cost?
//!
//! The dual-core cap-4 measurement (plan `2026-08-04-1845-dualcore-rmt-isr`,
//! p5 matrix) starved the two last-serviced wires because four coincident
//! refills sum to ~94 % of the 80 µs half deadline. Whether code can win that
//! margin back depends on where the ~15-word-time refill cost actually goes:
//! the APB bus, or the per-word bookkeeping in the fill path. This probe
//! measures the floor on silicon — 64 back-to-back raw `write_volatile`
//! stores into channel 0's RMT RAM half — next to the same 64 stores through
//! the layers the real refill uses, so the gap between "bus" and "code" is a
//! printed number instead of an argument.
//!
//! Runs once at driver init (nothing is transmitting yet), on the PRO core
//! with interrupts masked, from IRAM (`#[esp_hal::ram]`) so flash-cache
//! misses don't pollute the floor. Only compiled under `ws281x_telemetry` —
//! the same feature the capacity sweeps flash with — and it leaves channel
//! 0's window all-STOP, which is the state `clear_ram` would put it in
//! anyway.
//!
//! Variants, each timed over the same 64 words (one half of the DOM-Z-102's
//! 128-word window):
//!
//! * `raw8` — eight `write_volatile` stores per loop iteration through one
//!   hoisted base pointer: the floor. If this alone is near the deadline the
//!   refill is bus-bound and no Rust (or asm) rewrite can reach cap 4.
//! * `raw1` — the same stores, one per iteration: loop overhead visibility.
//! * `ram_word` — [`RmtHw::write_ram`] per word, exactly as `fill_half` calls
//!   it today: block-plan lookup + two bounds checks per word.
//! * `fill_emu` — a local replica of `fill_half`'s inner loop: per-word bit
//!   math, the per-byte double atomic load + deref of `frame_byte`, and
//!   `write_ram`. The closest host-side stand-in for the true refill cost
//!   (which telemetry's `lag_avg` measures in word-times on the wire).
//!
//! Each variant runs [`TRIALS`] times; the **min** is the number to read
//! (cold-cache and stray-stall trials land in the max).

use core::ptr::null_mut;
use core::sync::atomic::Ordering::Relaxed;
use core::sync::atomic::{AtomicPtr, AtomicUsize};

use lp_ws281x::RmtHw;

use super::v3_rmt::{BLOCK_WORDS, RAM_BASE, TX_PLAN, V3Rmt};

/// Trials per variant. The min converges within a few once caches are warm.
const TRIALS: usize = 8;

/// Words per measured refill — one half of channel 0's two-block window.
const HALF_WORDS: usize = 64;

/// CPU cycles per microsecond (`CpuClock::max()` = 240 MHz on this chip).
const CYCLES_PER_US: u32 = 240;

/// Emulated frame descriptor for `fill_emu`, shaped exactly like
/// `ChannelState`'s: the two atomic loads per frame byte are the cost being
/// measured, so they must be real atomics, not locals the compiler can hoist.
static PROBE_FRAME_LEN: AtomicUsize = AtomicUsize::new(0);
static PROBE_FRAME_PTR: AtomicPtr<u8> = AtomicPtr::new(null_mut());

/// The bytes `fill_emu` reads. Never written through the pointer.
static PROBE_FRAME: [u8; 24] = [0xA5; 24];

#[inline]
fn cycles() -> u32 {
    esp_hal::xtensa_lx::timer::get_cycle_count()
}

/// The floor: 64 raw stores through a hoisted base pointer, 8 per iteration.
#[esp_hal::ram]
fn trial_raw8(base: *mut u32) -> u32 {
    let t0 = cycles();
    let mut i = 0;
    while i < HALF_WORDS {
        // SAFETY: `base` points at channel 0's RMT RAM window (≥ 128 words on
        // this board's plan); `i + 7 < HALF_WORDS = 64` stays inside it.
        // Volatile: MMIO, the stores must all be issued.
        unsafe {
            base.add(i).write_volatile(0);
            base.add(i + 1).write_volatile(0);
            base.add(i + 2).write_volatile(0);
            base.add(i + 3).write_volatile(0);
            base.add(i + 4).write_volatile(0);
            base.add(i + 5).write_volatile(0);
            base.add(i + 6).write_volatile(0);
            base.add(i + 7).write_volatile(0);
        }
        i += 8;
    }
    cycles().wrapping_sub(t0)
}

/// The floor with per-store loop overhead left in.
#[esp_hal::ram]
fn trial_raw1(base: *mut u32) -> u32 {
    let t0 = cycles();
    let mut i = 0;
    while i < HALF_WORDS {
        // SAFETY: as `trial_raw8` — in-window, volatile MMIO store.
        unsafe { base.add(i).write_volatile(0) };
        i += 1;
    }
    cycles().wrapping_sub(t0)
}

/// 64 words through [`RmtHw::write_ram`] — today's per-word path.
#[esp_hal::ram]
fn trial_ram_word(hw: &V3Rmt) -> u32 {
    let t0 = cycles();
    let mut i = 0;
    while i < HALF_WORDS {
        hw.write_ram(0, i, 0);
        i += 1;
    }
    cycles().wrapping_sub(t0)
}

/// `frame_byte` as `ChannelState` implements it: bounds check against an
/// atomic length, atomic pointer load, null check, deref.
///
/// `#[inline]`, no RAM section of its own: it inlines into `trial_fill_emu`'s
/// IRAM body, exactly as the real `frame_byte` inlines into `fill_half`.
#[inline]
fn probe_frame_byte(index: usize) -> u8 {
    if index >= PROBE_FRAME_LEN.load(Relaxed) {
        return 0;
    }
    let base = PROBE_FRAME_PTR.load(Relaxed);
    if base.is_null() {
        return 0;
    }
    // SAFETY: the only publisher stores `PROBE_FRAME`'s pointer and length,
    // and the bounds check just passed.
    unsafe { *base.add(index) }
}

/// A replica of `fill_half`'s inner loop over 64 data words.
#[esp_hal::ram]
fn trial_fill_emu(hw: &V3Rmt, total_bits: usize) -> u32 {
    // GRB source order, as the real driver runs. `black_box` keeps the
    // compiler from folding the table into the loop.
    let order: [usize; 3] = core::hint::black_box([1, 0, 2]);
    let t0 = cycles();
    let mut cursor = 0usize;
    let mut word = 0usize;
    let mut byte = 0u8;
    let mut byte_loaded = false;
    while word < HALF_WORDS && cursor < total_bits {
        let bit_in_byte = cursor % 8;
        if !byte_loaded || bit_in_byte == 0 {
            let pixel = cursor / 24;
            let slot = (cursor % 24) / 8;
            byte = probe_frame_byte(pixel * 3 + order[slot]);
            byte_loaded = true;
        }
        // Distinct pulse words per bit value, as the real fill writes — with
        // equal values the compiler deletes the whole byte-extraction path.
        // The window is re-cleared after the trials.
        let one = byte & (0x80 >> bit_in_byte) != 0;
        hw.write_ram(0, word, if one { 0x8010_4008 } else { 0x8004_8010 });
        word += 1;
        cursor += 1;
    }
    cycles().wrapping_sub(t0)
}

/// The hoisted `fill_half` shape (descriptor snapshot once, incremental byte
/// addressing, raw window base) — the after picture to `trial_fill_emu`'s
/// before.
#[esp_hal::ram]
fn trial_fill_hoisted(base: *mut u32, total_bits: usize) -> u32 {
    let order: [usize; 3] = core::hint::black_box([1, 0, 2]);
    let t0 = cycles();
    let len = PROBE_FRAME_LEN.load(Relaxed);
    let ptr = PROBE_FRAME_PTR.load(Relaxed);
    let len = if ptr.is_null() { 0 } else { len };
    let mut cursor = 0usize;
    let mut word = 0usize;
    let mut pixel_base = 0usize;
    let mut slot = 0usize;
    let mut mask = 0x80u8;
    while word < HALF_WORDS && cursor < total_bits {
        let index = pixel_base + order[slot];
        let byte = if index < len {
            // SAFETY: publisher stored `PROBE_FRAME`'s pointer/length; bounds
            // checked above.
            unsafe { *ptr.add(index) }
        } else {
            0
        };
        while mask != 0 && word < HALF_WORDS && cursor < total_bits {
            let one = byte & mask != 0;
            // SAFETY: as `trial_raw8` — in-window, volatile MMIO store.
            unsafe {
                base.add(word)
                    .write_volatile(if one { 0x8010_4008 } else { 0x8004_8010 })
            };
            word += 1;
            cursor += 1;
            mask >>= 1;
        }
        if mask == 0 {
            mask = 0x80;
            slot += 1;
            if slot == 3 {
                slot = 0;
                pixel_base += 3;
            }
        }
    }
    cycles().wrapping_sub(t0)
}

/// Run every variant and print one `[PROBE]` line each.
///
/// Call once from driver init, after the block plan is published and
/// `init_tx` has run, before any output can open.
pub fn run() {
    if TX_PLAN.window_words(0, BLOCK_WORDS) < HALF_WORDS {
        esp_println::println!("[PROBE] refill floor: channel 0 window too small; skipped");
        return;
    }
    let base = (RAM_BASE as *mut u32).wrapping_add(TX_PLAN.window_start(0, BLOCK_WORDS));
    let hw = V3Rmt::new();
    PROBE_FRAME_LEN.store(PROBE_FRAME.len(), Relaxed);
    PROBE_FRAME_PTR.store(PROBE_FRAME.as_ptr().cast_mut(), Relaxed);

    report("raw8", run_trials(|| trial_raw8(base)));
    report("raw1", run_trials(|| trial_raw1(base)));
    report("ram_word", run_trials(|| trial_ram_word(&hw)));
    report("fill_emu", run_trials(|| trial_fill_emu(&hw, HALF_WORDS)));
    report(
        "fill_hoist",
        run_trials(|| trial_fill_hoisted(base, HALF_WORDS)),
    );

    // `fill_emu` wrote real pulse words; leave the window all-STOP as the
    // driver expects an untouched channel to be.
    super::v3_rmt::clear_ram(0);
    PROBE_FRAME_PTR.store(null_mut(), Relaxed);
    PROBE_FRAME_LEN.store(0, Relaxed);
}

/// `(min, max)` cycles over [`TRIALS`] runs, each with interrupts masked.
fn run_trials(mut trial: impl FnMut() -> u32) -> (u32, u32) {
    let mut min = u32::MAX;
    let mut max = 0;
    for _ in 0..TRIALS {
        let dt = esp_hal::xtensa_lx::interrupt::free(|| trial());
        min = min.min(dt);
        max = max.max(dt);
    }
    (min, max)
}

fn report(name: &str, (min, max): (u32, u32)) {
    // Integer µs*100 so no float formatter is linked.
    let us_x100 = min * 100 / CYCLES_PER_US;
    let ns_per_word = min * 1000 / CYCLES_PER_US / HALF_WORDS as u32;
    esp_println::println!(
        "[PROBE] refill floor: {name} min={min} max={max} cycles for {HALF_WORDS} words \
         => {}.{:02} us, ~{ns_per_word} ns/word",
        us_x100 / 100,
        us_x100 % 100,
    );
}
