//! `heap_peak_diag` — OFF by default, never shipped: the heap's peak between
//! two heartbeats, for the tree store's RAM figures (plan
//! `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, D5 / Q15).
//!
//! The heartbeat samples memory every 5 s and so misses a compile's or a
//! commit's peak, and esp-alloc's `max_usage` is a lifetime maximum with no
//! reset. This keeps its own: esp-alloc's allocation hooks add and remove
//! each block's requested size (`live`), and `peak` is the most `live` reached
//! since the last line. One line per heartbeat, after the heartbeat:
//!
//! ```text
//! [heap] peak=<B> live=<B> max_usage=<B> largest=<B>
//! ```
//!
//! `peak` then restarts from `live`, so a line's `peak` is the window since
//! the line before it: a test reads a line, runs one operation, and reads the
//! next — the operation's transient is `peak` minus the first line's `live`.
//! `peak`/`live` count requested bytes (no allocator overhead); `max_usage`
//! is esp-alloc's own (with it); `largest` is the largest free block at the
//! line, by probe (a sample, not a minimum over the window). The wire is
//! unchanged.
//!
//! The largest-block probe (the heartbeat's, the read gate's, this line's)
//! allocates and frees blocks up to the whole free heap; it runs
//! [`untracked`], so its asks never reach `peak`.
//!
//! Uses the same two hook symbols as `heap_map_diag`'s tracker, so the two
//! cannot be built together.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
/// A probe is running: `live` still moves (so it never drifts), `peak` does
/// not.
static PROBING: AtomicBool = AtomicBool::new(false);

#[unsafe(no_mangle)]
fn _esp_alloc_alloc(
    _heap: &esp_alloc::EspHeap,
    _caps: enumset::EnumSet<esp_alloc::MemoryCapability>,
    ptr: usize,
    size: usize,
) {
    if ptr == 0 {
        return;
    }
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    if !PROBING.load(Ordering::Relaxed) {
        PEAK.fetch_max(live, Ordering::Relaxed);
    }
}

#[unsafe(no_mangle)]
fn _esp_alloc_dealloc(_heap: &esp_alloc::EspHeap, ptr: usize, size: usize) {
    if ptr == 0 {
        return;
    }
    LIVE.fetch_sub(size, Ordering::Relaxed);
}

/// Run `f` (the largest-block probe) with its allocations kept out of
/// `peak`.
pub fn untracked<T>(f: impl FnOnce() -> T) -> T {
    PROBING.store(true, Ordering::Relaxed);
    let out = f();
    PROBING.store(false, Ordering::Relaxed);
    out
}

/// The `[heap]` line, and the window's restart.
pub fn log_line() {
    let live = LIVE.load(Ordering::Relaxed);
    let peak = PEAK.swap(live, Ordering::Relaxed).max(live);
    let max_usage = esp_alloc::HEAP.stats().max_usage;
    let largest = crate::recovery::panic_path::largest_free_block();
    log::info!("[heap] peak={peak} live={live} max_usage={max_usage} largest={largest}");
}
