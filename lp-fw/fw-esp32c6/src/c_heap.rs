//! The C heap: `malloc` and friends for the radio blobs, placed in the
//! radio's own region first.
//!
//! esp-radio's Wi-Fi and BLE controller blobs allocate through C symbols
//! (`malloc`, `free`, `calloc`, `realloc_internal`, …) that esp-alloc's
//! `compat` feature would provide against the global heap: main region first.
//! That placement split the main region. The BLE link layer allocates
//! (`r_ble_ll_mem_alloc`) every time advertising restarts — which it does
//! when the advertised name follows a newly loaded project — so its blocks
//! landed above the project's memory, were still there when the project was
//! stopped, and the next project was refused on contiguity with 200 KB free
//! (desk, 2026-09-24: live `r_ble_ll_mem_alloc` blocks of 296 and 80 B at
//! main-region offsets +187,663 and +127,621, largest free block 59,952 B;
//! docs/defects/2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md).
//!
//! So the radio's blocks get a region of their own. Until 2026-10-05 that
//! region was the reclaimed bootloader segment (`dram2_seg`). It cannot be:
//! these blocks hold the radio's DMA descriptors and buffers, the radio keeps
//! running across a warm reset of the HP system (the host's RTS, a requested
//! reboot), and `dram2_seg` is where the second-stage bootloader then loads
//! its code. On silicon the stray writes landed on the bootloader's
//! image-hash check and crashed it on 11 of 22 warm resets
//! (docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md).
//! The region is now `HEAP_RADIO` in main RAM's `.bss`, which no bootloader
//! loads into; when it is full a block falls back to the main region —
//! never to `dram2_seg`.
//!
//! The regions are picked by capability, and the C6 has no SPI RAM, so
//! `MemoryCapability::External` is borrowed as a tag nothing else asks for:
//!
//! | region | tag | who lands there |
//! |---|---|---|
//! | main | `Internal` | Rust first; C when the radio region is full |
//! | `dram2_seg` | none ([`BOOTLOADER_RECLAIMED`]) | Rust only (capability-free requests) |
//! | radio | `Internal` and `External` ([`RADIO`]) | C first; Rust last of all |
//!
//! A request carrying `Internal` (the C fallback, esp-radio's own
//! `InternalMemory`) therefore never reaches `dram2_seg`. Rust's global
//! allocator asks for nothing and tries the regions in registration order:
//! main, `dram2_seg`, radio.
//!
//! Semantics are esp-alloc 0.10's `compat` ones (`malloc.rs`): a 4-byte size
//! header in front of each block, 4-byte alignment.

use core::sync::atomic::{AtomicUsize, Ordering};

use enumset::EnumSet;
use esp_alloc::MemoryCapability;

/// The capability set that selects the radio's region alone.
pub const RADIO: EnumSet<MemoryCapability> =
    enumset::enum_set!(MemoryCapability::Internal | MemoryCapability::External);

/// The capability set `dram2_seg` is registered with: none, so only a request
/// that asks for nothing can land there.
pub const BOOTLOADER_RECLAIMED: EnumSet<MemoryCapability> = EnumSet::empty();

/// Live C bytes (headers included) in the radio's region.
static RADIO_LIVE: AtomicUsize = AtomicUsize::new(0);
/// The most [`RADIO_LIVE`] has been.
static RADIO_HIGH_WATER: AtomicUsize = AtomicUsize::new(0);
/// C bytes the radio region could not hold, which went to the main region.
static OVERFLOWED: AtomicUsize = AtomicUsize::new(0);
/// The high-water [`log_if_grown`] last reported.
static LOGGED_HIGH_WATER: AtomicUsize = AtomicUsize::new(0);

/// Bytes of header in front of every C block: its total size.
const HEADER: usize = 4;

/// Allocate `size` bytes for C: the radio's region first, then the main
/// region — never `dram2_seg`.
unsafe fn c_alloc(size: usize) -> *mut u8 {
    let total = size + HEADER;
    // SAFETY: HEADER > 0, so the layout is non-zero; align 4 is valid.
    let layout = unsafe { core::alloc::Layout::from_size_align_unchecked(total, 4) };
    let mut ptr = unsafe { esp_alloc::HEAP.alloc_caps(RADIO, layout) };
    if ptr.is_null() {
        ptr = unsafe { esp_alloc::HEAP.alloc_caps(MemoryCapability::Internal.into(), layout) };
        if !ptr.is_null() {
            OVERFLOWED.fetch_add(total, Ordering::Relaxed);
        }
    } else {
        let live = RADIO_LIVE.fetch_add(total, Ordering::Relaxed) + total;
        // A load and a store, not `fetch_max`: `amomaxu.w` is RV32A, but
        // lp-riscv-emu's decoder has no AMOMIN/AMOMAX family, so the emulated
        // board would fault here. Two radio threads racing can lose an update
        // by one block; this is a diagnostic high-water.
        if live > RADIO_HIGH_WATER.load(Ordering::Relaxed) {
            RADIO_HIGH_WATER.store(live, Ordering::Relaxed);
        }
    }
    if ptr.is_null() {
        return ptr;
    }
    #[cfg(feature = "radio_dma_diag")]
    crate::radio_dma_diag::record(ptr as usize, total);
    // SAFETY: the block is at least HEADER bytes and 4-aligned.
    unsafe {
        (ptr as *mut usize).write(total);
        ptr.add(HEADER)
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
    unsafe { c_alloc(size) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn malloc_internal(size: usize) -> *mut u8 {
    // The radio's region and the main region are both internal RAM.
    unsafe { c_alloc(size) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: `ptr` came from `c_alloc`, which put the block's total size
    // HEADER bytes in front of it; the global allocator frees in whichever
    // region the block is.
    unsafe {
        let block = ptr.sub(HEADER);
        let total = (block as *const usize).read();
        #[cfg(feature = "radio_dma_diag")]
        crate::radio_dma_diag::forget(block as usize);
        if in_radio_region(block) {
            RADIO_LIVE.fetch_sub(total, Ordering::Relaxed);
        }
        alloc::alloc::dealloc(
            block,
            core::alloc::Layout::from_size_align_unchecked(total, 4),
        );
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn free_internal(ptr: *mut u8) {
    unsafe { free(ptr) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn calloc(number: u32, size: usize) -> *mut u8 {
    let total = number as usize * size;
    let ptr = unsafe { c_alloc(total) };
    if !ptr.is_null() {
        // SAFETY: `ptr` has `total` writable bytes.
        unsafe { core::ptr::write_bytes(ptr, 0, total) };
    }
    ptr
}

#[unsafe(no_mangle)]
unsafe extern "C" fn calloc_internal(number: u32, size: usize) -> *mut u8 {
    unsafe { calloc(number, size) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn realloc_internal(ptr: *mut u8, new_size: usize) -> *mut u8 {
    let new = unsafe { c_alloc(new_size) };
    if !new.is_null() && !ptr.is_null() {
        // SAFETY: both blocks come from `c_alloc`; the old one's payload is
        // its total minus the header.
        unsafe {
            let old_len = (ptr.sub(HEADER) as *const usize).read() - HEADER;
            core::ptr::copy_nonoverlapping(ptr, new, old_len.min(new_size));
            free(ptr);
        }
    }
    new
}

#[unsafe(no_mangle)]
unsafe extern "C" fn get_free_internal_heap_size() -> usize {
    esp_alloc::HEAP.free_caps(MemoryCapability::Internal.into())
}

/// Log `[radio-heap]` when the radio region's high-water has grown since the
/// last line — the number its size is checked against, the way
/// `stack_probe`'s `[stack]` line checks the main stack.
#[cfg_attr(
    fw_harness,
    allow(dead_code, reason = "the heartbeat that logs it is the product's")
)]
pub fn log_if_grown(tag: &str) {
    let high = RADIO_HIGH_WATER.load(Ordering::Relaxed);
    if high > LOGGED_HIGH_WATER.swap(high, Ordering::Relaxed) {
        let (_, size) = crate::board::esp32c6::init::radio_region();
        log::info!(
            "[radio-heap] {tag}: high-water {high} B of {size} B, {} B live, {} B overflowed to main",
            RADIO_LIVE.load(Ordering::Relaxed),
            OVERFLOWED.load(Ordering::Relaxed),
        );
    }
}

/// Whether `block` lies in the radio's region.
fn in_radio_region(block: *mut u8) -> bool {
    let (start, size) = crate::board::esp32c6::init::radio_region();
    (start..start + size).contains(&(block as usize))
}
