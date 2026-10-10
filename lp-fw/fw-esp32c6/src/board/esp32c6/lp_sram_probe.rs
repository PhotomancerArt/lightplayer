//! RESEARCH (research/ram-e03; never shipped): how fast the HP CPU reaches
//! LP SRAM, beside HP SRAM, measured once at boot.
//!
//! Runs from `init_board` after the main, `dram2_seg` and radio regions are
//! up and before LP SRAM is handed to the heap, on the span
//! [`super::lp_sram_heap::free_span`] names (nothing else uses it yet). The
//! HP buffers are ordinary heap allocations, so they land in the main region:
//! the memory a Rust allocation gets today.
//!
//! Each figure is the fewest `mpccr` cycles (`cycle_counter`) of
//! [`REPEATS`] runs, taken before the RTOS starts, so nothing preempts it.
//!
//! | kernel | what one run does |
//! |---|---|
//! | `read32` | 2,048 volatile `lw` over 8 KiB, summed |
//! | `write32` | 2,048 volatile `sw` over 8 KiB |
//! | `read8` | 8,192 volatile `lbu` over 8 KiB, summed |
//! | `memcpy` | `copy_nonoverlapping` of 7,168 B (two halves of LP SRAM fit) |
//! | `exec` | `addi a0,a0,-1; bnez a0,.; ret` with `a0` = 20,000, copied into the memory and called |
//!
//! `exec` answers the question the JIT asks: its code buffers are heap
//! allocations executed where they are written (`lpvm_native::exec_addr`,
//! identity on RV32), so a buffer that landed in LP SRAM would run from it.
//! A fetch fault there would panic at boot; so that a fault cannot loop, the
//! exec kernel runs only when the reset was a power-on or a host's USB reset,
//! never after a software reset (which is what the panic path makes).
//!
//! The results are printed at boot (`[e03-probe]`) and again on the first
//! [`HEARTBEAT_REPEATS`] heartbeats, since a host that attaches after the
//! boot misses the first print.

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use super::cycle_counter;

/// Runs per kernel; the fewest cycles is the figure.
const REPEATS: usize = 5;
/// Bytes the read/write kernels walk.
const RW_BYTES: usize = 8 * 1024;
/// Bytes one `memcpy` moves: two of these fit in LP SRAM's free span.
const COPY_BYTES: usize = 7 * 1024;
/// Iterations of the exec loop (two instructions each).
const EXEC_ITERS: u32 = 20_000;
/// Heartbeats that repeat the results.
const HEARTBEAT_REPEATS: usize = 5;

/// `addi a0,a0,-1` / `bnez a0,-4` / `ret`, uncompressed so each word is one
/// instruction and the copy is word-aligned.
const LOOP_CODE: [u32; 3] = [0xfff5_0513, 0xfe05_1ee3, 0x0000_8067];

/// The kernels, in print order.
const KERNELS: [&str; 9] = [
    "read32", "write32", "read8", "memcpy", "memcpy_hp_to_lp", "memcpy_lp_to_hp", "exec",
    "exec_rwtext", "exec_ran",
];

/// Cycles per kernel for HP SRAM and LP SRAM (`exec_rwtext` uses the HP
/// column; `exec_ran` is 1 when the exec kernels ran).
static HP: [AtomicU32; 9] = [const { AtomicU32::new(0) }; 9];
static LP: [AtomicU32; 9] = [const { AtomicU32::new(0) }; 9];
static PRINTED: AtomicUsize = AtomicUsize::new(0);

/// The loop's words in `.rwtext`, the linker's HP SRAM code section: the
/// reference an IRAM function would give.
#[unsafe(link_section = ".rwtext")]
static RWTEXT_LOOP: [u32; 3] = LOOP_CODE;

/// Measure, record and print. `lp` is LP SRAM's free span and must hold at
/// least 2 × [`COPY_BYTES`] and [`RW_BYTES`].
pub fn run(lp: (usize, usize)) {
    let (lp_start, lp_len) = lp;
    if lp_len < (2 * COPY_BYTES).max(RW_BYTES) {
        esp_println::println!("[e03-probe] skipped: only {lp_len} B of LP SRAM free");
        return;
    }
    cycle_counter::setup();
    let mut hp_a: Vec<u32> = alloc::vec![0; RW_BYTES / 4];
    let mut hp_b: Vec<u32> = alloc::vec![0; RW_BYTES / 4];
    let hp = hp_a.as_mut_ptr();
    let hp2 = hp_b.as_mut_ptr();
    let lp = lp_start as *mut u32;
    let lp2 = (lp_start + COPY_BYTES) as *mut u32;

    record(0, best(|| read32(hp)), best(|| read32(lp)));
    record(1, best(|| write32(hp)), best(|| write32(lp)));
    record(2, best(|| read8(hp.cast())), best(|| read8(lp.cast())));
    record(3, best(|| copy(hp, hp2)), best(|| copy(lp, lp2)));
    record(4, best(|| copy(hp, lp)), 0);
    record(5, 0, best(|| copy(lp, hp)));

    if exec_is_safe() {
        // SAFETY: both spans are at least 12 B, word-aligned, writable and
        // owned by this probe; `place` writes the loop and syncs the fetch
        // path before the call.
        let hp_fn = unsafe { place(hp) };
        let lp_fn = unsafe { place(lp) };
        record(6, best(|| hp_fn(EXEC_ITERS)), best(|| lp_fn(EXEC_ITERS)));
        // SAFETY: `RWTEXT_LOOP` is the loop's words in an executable section.
        let rw_fn: extern "C" fn(u32) -> u32 =
            unsafe { core::mem::transmute(core::ptr::addr_of!(RWTEXT_LOOP)) };
        record(7, best(|| rw_fn(EXEC_ITERS)), 0);
        record(8, 1, 1);
    }
    // Leave LP SRAM as the heap will find it; the heap does not care, but a
    // dump of the region should not show the probe's bytes as live data.
    // SAFETY: the probe owns the span until `lp_sram_heap::install`.
    unsafe { core::ptr::write_bytes(lp_start as *mut u8, 0, lp_len) };
    drop(hp_a);
    drop(hp_b);
    print("boot");
}

/// Print the results on the first few heartbeats.
#[cfg_attr(
    fw_harness,
    allow(dead_code, reason = "the heartbeat that calls it is the product's")
)]
pub fn heartbeat() {
    if PRINTED.load(Ordering::Relaxed) < HEARTBEAT_REPEATS {
        print("heartbeat");
    }
}

fn print(tag: &str) {
    PRINTED.fetch_add(1, Ordering::Relaxed);
    let lp_span = super::lp_sram_heap::free_span();
    log_line(format_args!(
        "[e03-probe] {tag}: cycles, fewest of {REPEATS}; rw {RW_BYTES} B, memcpy {COPY_BYTES} B, exec {EXEC_ITERS} iters; LP span 0x{:08x}+{}",
        lp_span.0, lp_span.1
    ));
    for (i, name) in KERNELS.iter().enumerate() {
        let hp = HP[i].load(Ordering::Relaxed);
        let lp = LP[i].load(Ordering::Relaxed);
        // Ratio in hundredths, integer only.
        let ratio = if hp > 0 && lp > 0 {
            (lp as u64 * 100 / hp as u64) as u32
        } else {
            0
        };
        log_line(format_args!(
            "[e03-probe] {tag}: {name} hp={hp} lp={lp} lp/hp={}.{:02}",
            ratio / 100,
            ratio % 100
        ));
    }
}

/// Before the logger is up (boot) the line goes to the console directly.
fn log_line(args: core::fmt::Arguments<'_>) {
    if log::max_level() == log::LevelFilter::Off {
        esp_println::println!("{args}");
    } else {
        log::info!("{args}");
    }
}

fn record(i: usize, hp: u32, lp: u32) {
    HP[i].store(hp, Ordering::Relaxed);
    LP[i].store(lp, Ordering::Relaxed);
}

/// Fewest cycles of [`REPEATS`] runs of `f`.
fn best<R>(mut f: impl FnMut() -> R) -> u32 {
    let mut fewest = u32::MAX;
    for _ in 0..REPEATS {
        let t0 = cycle_counter::read();
        core::hint::black_box(f());
        let dt = cycle_counter::read().wrapping_sub(t0);
        fewest = fewest.min(dt);
    }
    fewest
}

#[inline(never)]
fn read32(p: *const u32) -> u32 {
    let mut sum = 0u32;
    for i in 0..RW_BYTES / 4 {
        // SAFETY: `p` spans RW_BYTES readable bytes.
        sum = sum.wrapping_add(unsafe { p.add(i).read_volatile() });
    }
    sum
}

#[inline(never)]
fn write32(p: *mut u32) {
    for i in 0..RW_BYTES / 4 {
        // SAFETY: `p` spans RW_BYTES writable bytes.
        unsafe { p.add(i).write_volatile(i as u32) };
    }
}

#[inline(never)]
fn read8(p: *const u8) -> u32 {
    let mut sum = 0u32;
    for i in 0..RW_BYTES {
        // SAFETY: `p` spans RW_BYTES readable bytes.
        sum = sum.wrapping_add(unsafe { p.add(i).read_volatile() } as u32);
    }
    sum
}

#[inline(never)]
fn copy(src: *const u32, dst: *mut u32) {
    // SAFETY: both span COPY_BYTES and do not overlap (callers pass distinct
    // buffers, or the two halves of LP SRAM's span).
    unsafe { core::ptr::copy_nonoverlapping(src.cast::<u8>(), dst.cast::<u8>(), COPY_BYTES) };
}

/// Write the loop at `dst` and make it fetchable.
unsafe fn place(dst: *mut u32) -> extern "C" fn(u32) -> u32 {
    for (i, word) in LOOP_CODE.iter().enumerate() {
        // SAFETY: the caller's span holds the three words.
        unsafe { dst.add(i).write_volatile(*word) };
    }
    // SAFETY: `fence.i` only orders this hart's fetches after its stores.
    unsafe { core::arch::asm!("fence.i") };
    // SAFETY: `dst` now holds a complete leaf function of this signature.
    unsafe { core::mem::transmute(dst) }
}

/// The exec kernels run only after a power-on or a host's USB reset, never
/// after a software reset — the reset a panic ends in — so a fetch fault in
/// LP SRAM costs one boot, not a loop of them.
fn exec_is_safe() -> bool {
    use esp_hal::rtc_cntl::SocResetReason as R;
    matches!(
        esp_hal::system::reset_reason(),
        Some(R::ChipPowerOn | R::CoreUsbUart | R::CoreUsbJtag)
    )
}
