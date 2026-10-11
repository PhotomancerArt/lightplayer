//! The C6's LP SRAM as a fourth heap region, the last one Rust's allocator
//! tries (`lp_sram_heap`, off by default; research/ram-e03).
//!
//! LP SRAM (`RTC_FAST` in esp-hal's `memory.x`, 0x5000_0000, 16 KiB) holds
//! one tenant in the shipped image: the recovery region,
//! `.rtc_fast.persistent`, 1,024 B at its very bottom. This region is
//! **whatever the linker left above it**: from `_rtc_fast_persistent_end`
//! (the highest byte esp-hal's `rtc_fast.x` places) to the end of LP SRAM.
//! So it is never a static of its own, nothing that must survive a reset
//! moves (the recovery region stays at 0x5000_0000 whether this is on or
//! off, which matters across an update: the old and the new core read the
//! breadcrumbs at the same address), and a build that puts more into LP SRAM
//! (`heap_track_diag`'s 12 KB table) simply gets a smaller region.
//!
//! **No capability tag**, like `dram2_seg` ([`crate::c_heap::BOOTLOADER_RECLAIMED`]):
//! only a request that asks for nothing can land here, and only Rust's global
//! allocator asks for nothing. That keeps out
//!
//! - the radio blobs' C heap, which asks for the radio region's tag and then
//!   for `Internal` — their blocks hold DMA descriptors and buffers, and no
//!   DMA engine on this chip is documented to reach LP SRAM (esp-hal's DMA
//!   checks a buffer against `memory_range!("DRAM")`, 0x4080_0000..0x4088_0000);
//! - esp-radio's own `InternalMemory` (`Internal`);
//! - esp-rtos's task stacks, which it allocates with `InternalMemory` — hot
//!   memory that should never pay LP SRAM's access cost.
//!
//! Registered **last**, after main, `dram2_seg` and the radio region, so a
//! Rust allocation lands here only once every other region refused it: today
//! that is the moment the allocator would otherwise have returned null.
//!
//! **What it costs when it is used** (silicon, LC6, 2026-10-10): one extra
//! cycle per HP-CPU access, data or instruction fetch — 8 KiB of `lw` 1.19×
//! HP SRAM's cycles, `sw` 1.24×, a 7 KiB `memcpy` 1.69×, a loop executed from
//! it 1.99× (it is executable, so a JIT buffer that lands here runs, slower).
//! The emulator charges it nothing (t1/t2 have no memory cost model).

use core::sync::atomic::{AtomicUsize, Ordering};

/// End of LP SRAM: `RTC_FAST : ORIGIN = 0x50000000, LENGTH = 16K`
/// (esp-hal 1.1.1 `ld/esp32c6/memory.x`).
const LP_SRAM_END: usize = 0x5000_4000;

/// Below this the region is not worth a slot in the allocator's table (a
/// diagnostic build that fills LP SRAM leaves only a few hundred bytes).
const MIN_REGION: usize = 1024;

/// Where the region is: start address, size, and the allocator's index for
/// it (`usize::MAX` while it is not installed).
static START: AtomicUsize = AtomicUsize::new(0);
static SIZE: AtomicUsize = AtomicUsize::new(0);
static INDEX: AtomicUsize = AtomicUsize::new(usize::MAX);
/// The most the region has held at a heartbeat, and what the last
/// `[lp-heap]` line said.
static HIGH_WATER: AtomicUsize = AtomicUsize::new(0);
static LOGGED_USED: AtomicUsize = AtomicUsize::new(usize::MAX);

unsafe extern "C" {
    /// esp-hal's `rtc_fast.x`: one past the last byte of
    /// `.rtc_fast.persistent`, the last LP SRAM section it places.
    static _rtc_fast_persistent_end: u8;
}

/// The span the linker left free in LP SRAM, `(start, size)`, 8-aligned.
pub fn free_span() -> (usize, usize) {
    let end_of_sections = core::ptr::addr_of!(_rtc_fast_persistent_end) as usize;
    let start = (end_of_sections + 7) & !7;
    (start, LP_SRAM_END.saturating_sub(start))
}

/// Register the free span as the heap's last region. Call once, after every
/// other region is added.
pub fn install() {
    let (start, size) = free_span();
    if size < MIN_REGION {
        esp_println::println!(
            "[lp-heap] not installed: {size} B free in LP SRAM above 0x{start:08x}"
        );
        return;
    }
    let index = esp_alloc::HEAP
        .stats()
        .region_stats
        .iter()
        .filter(|r| r.is_some())
        .count();
    // SAFETY: nothing the linker places reaches above
    // `_rtc_fast_persistent_end`, and nothing else in the firmware names this
    // memory, so the allocator owns it alone from here on.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            start as *mut u8,
            size,
            enumset::EnumSet::empty(),
        ));
    }
    START.store(start, Ordering::Relaxed);
    SIZE.store(size, Ordering::Relaxed);
    INDEX.store(index, Ordering::Relaxed);
    esp_println::println!(
        "[lp-heap] region {index}: {size} B at 0x{start:08x}..0x{LP_SRAM_END:08x}, no capability tag"
    );
}

/// The region as `(start address, size)`; `(0, 0)` when it is not installed.
#[allow(dead_code, reason = "read only by the heap diagnostics")]
pub fn region() -> (usize, usize) {
    (START.load(Ordering::Relaxed), SIZE.load(Ordering::Relaxed))
}

/// Log `[lp-heap]` when the region's use has changed since the last line:
/// used and free as the allocator counts them, and the most it has held at
/// any heartbeat.
#[cfg_attr(
    fw_harness,
    allow(dead_code, reason = "the heartbeat that logs it is the product's")
)]
pub fn log_if_changed(tag: &str) {
    let index = INDEX.load(Ordering::Relaxed);
    if index == usize::MAX {
        return;
    }
    let stats = esp_alloc::HEAP.stats();
    let Some(Some(region)) = stats.region_stats.get(index) else {
        return;
    };
    let used = region.used;
    let high = HIGH_WATER.load(Ordering::Relaxed).max(used);
    HIGH_WATER.store(high, Ordering::Relaxed);
    if LOGGED_USED.swap(used, Ordering::Relaxed) != used {
        log::info!(
            "[lp-heap] {tag}: {used} B used, {} B free of {} B (heartbeat high-water {high} B)",
            region.free,
            region.size,
        );
    }
}
