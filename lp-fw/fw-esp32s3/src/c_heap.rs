//! The heap's regions on a radio image, and the C heap: `malloc` and friends
//! for the radio blobs, placed in the radio's own region first.
//!
//! The S3's copy of `fw-esp32c6/src/c_heap.rs` (feature `radio`, and the
//! `radio_boot_probe` scaffolding). Both of the C6's radio-heap defects apply
//! to this chip as they are, so both of its answers do:
//!
//! - **Fragmentation.** The BLE link layer allocates every time advertising
//!   restarts (the advertised name follows the loaded project), and blocks
//!   that land in the main arena above a project split the space the next
//!   project needs
//!   (docs/defects/2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md).
//!   So the blobs' blocks go to a region of their own, [`HEAP_RADIO`].
//! - **Warm-reset DMA.** A radio keeps running across a warm reset of the
//!   system, and its DMA keeps writing into buffers the previous app gave
//!   it. On the C6 those writes crashed the second-stage bootloader
//!   (docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md)
//!   and could reach the split loader's RAM
//!   (docs/defects/2026-10-17-radio-heap-overflow-reached-the-split-loaders-ram.md).
//!   On this chip the boot stages are `lp_bootctl::S3_BOOT_STAGE_RAM`: the
//!   split loader's RAM, inside the core's `.bss` (where the main arena is),
//!   and the IDF bootloader's, which is `dram2_seg` and the top of
//!   `dram_seg`. So nothing the radio can DMA into may live in either.
//!
//! The regions are picked by capability. The S3 has PSRAM on some boards, but
//! this firmware never maps it, so `MemoryCapability::External` is borrowed as
//! a tag nothing else asks for, as on the C6:
//!
//! | region | tag | who lands there |
//! |---|---|---|
//! | main arena, below the boot stages | `Internal` | Rust first; C when the radio region is full |
//! | main arena, from the loader's RAM up | none ([`FENCED`]) | Rust only |
//! | `dram2_seg` (`crate::dram2_heap`) | none | Rust only (capability-free requests) |
//! | radio ([`HEAP_RADIO`]) | `Internal` and `External` ([`RADIO`]) | C first; Rust last of all |
//!
//! A request carrying `Internal` (the C fallback, esp-radio's own
//! `InternalMemory`, esp-rtos's thread stacks) therefore never reaches
//! `dram2_seg` nor the loader's RAM. Rust's global allocator asks for nothing
//! and tries the regions in registration order: main below the boot stages,
//! main from them up, `dram2_seg`, radio.
//!
//! Semantics are esp-alloc 0.10's `compat` ones (`malloc.rs`), which this
//! replaces (`compat` is off: its `malloc` asks for no capability): a 4-byte
//! size header in front of each block, 4-byte alignment.

use core::sync::atomic::{AtomicUsize, Ordering};

use enumset::EnumSet;
use esp_alloc::MemoryCapability;

/// The capability set that selects the radio's region alone.
pub const RADIO: EnumSet<MemoryCapability> =
    enumset::enum_set!(MemoryCapability::Internal | MemoryCapability::External);

/// The capability set of the main arena's part from the loader's RAM up: none,
/// so only a request that asks for nothing can land there.
pub const FENCED: EnumSet<MemoryCapability> = EnumSet::empty();

/// The radio region's size. BTDM's controller took 35,724 B at init on FS3
/// with esp-radio's defaults (M1's radio-boot transcript, `ble_new`; six
/// connections and an 8 KB controller task stack, which esp-rtos takes from
/// the heap with `Internal`, so from the main arena, not from here); this
/// image asks for two connections. The heartbeat's `[radio-heap]` line is the
/// figure this is checked against.
pub const HEAP_RADIO_BYTES: usize = 32 * 1024;

/// An arena the allocator gets whole, 8-aligned (a bare byte array can land
/// on an odd address).
#[repr(C, align(8))]
struct HeapArena<const N: usize>(core::mem::MaybeUninit<[u8; N]>);

static mut HEAP_MAIN: HeapArena<{ crate::HEAP_SIZE }> = HeapArena(core::mem::MaybeUninit::uninit());
static mut HEAP_RADIO: HeapArena<HEAP_RADIO_BYTES> = HeapArena(core::mem::MaybeUninit::uninit());

/// Where the radio's region is: `(start, size)`, set once by [`install`].
static RADIO_START: AtomicUsize = AtomicUsize::new(0);

/// Live C bytes (headers included) in the radio's region.
static RADIO_LIVE: AtomicUsize = AtomicUsize::new(0);
/// The most [`RADIO_LIVE`] has been.
static RADIO_HIGH_WATER: AtomicUsize = AtomicUsize::new(0);
/// C bytes the radio region could not hold, which went to the main arena.
static OVERFLOWED: AtomicUsize = AtomicUsize::new(0);
/// The high-water [`log_if_grown`] last reported.
#[cfg(feature = "radio")]
static LOGGED_HIGH_WATER: AtomicUsize = AtomicUsize::new(0);

/// Bytes of header in front of every C block: its total size.
const HEADER: usize = 4;

/// Register the main arena, cut where it reaches a boot stage's RAM; the
/// `Internal` part first. Replaces `esp_alloc::heap_allocator!` on a radio
/// image. Then `dram2_seg` (its own module), then [`install_radio`], so Rust
/// reaches the radio region only once the main arena and `dram2_seg` are
/// both full. Returns `(start, internal part, fenced part)`.
pub fn install_main() -> (usize, usize, usize) {
    let main = core::ptr::addr_of_mut!(HEAP_MAIN).cast::<u8>();
    let (below, fenced) =
        lp_bootctl::split_at_s3_boot_stage_ram(main as u32, crate::HEAP_SIZE as u32);
    let (below, fenced) = (below as usize, fenced as usize);
    // SAFETY: the array is handed to the allocator exactly once, here; its
    // two parts are disjoint and together the array.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            main,
            below,
            MemoryCapability::Internal.into(),
        ));
        if fenced != 0 {
            esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(main.add(below), fenced, FENCED));
        }
    }
    (main as usize, below, fenced)
}

/// Register the radio region, last of all (see [`install_main`]). It must lie
/// clear of every boot stage's RAM — it is the one region the radio's DMA
/// lands in — and that is a fact of the link, so it is checked here, at
/// boot, where the emulator's boot gates see it. Returns `(start, size)`.
pub fn install_radio() -> (usize, usize) {
    let radio = core::ptr::addr_of_mut!(HEAP_RADIO).cast::<u8>();
    let (clear, _) = lp_bootctl::split_at_s3_boot_stage_ram(radio as u32, HEAP_RADIO_BYTES as u32);
    assert!(
        clear as usize == HEAP_RADIO_BYTES,
        "the radio heap at {radio:p} reaches a boot stage's RAM"
    );
    RADIO_START.store(radio as usize, Ordering::Relaxed);
    // SAFETY: the array is handed to the allocator exactly once, here.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(radio, HEAP_RADIO_BYTES, RADIO));
    }
    radio_region()
}

/// The radio region: `(start, size)`.
pub fn radio_region() -> (usize, usize) {
    (RADIO_START.load(Ordering::Relaxed), HEAP_RADIO_BYTES)
}

/// Allocate `size` bytes for C: the radio's region first, then the main
/// arena below the boot stages — never `dram2_seg`, never the loader's RAM.
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
        RADIO_HIGH_WATER.fetch_max(live, Ordering::Relaxed);
    }
    if ptr.is_null() {
        return ptr;
    }
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
    // The radio's region and the main arena are both internal RAM.
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
/// last line — the number [`HEAP_RADIO_BYTES`] is checked against, the way
/// `stack_probe`'s `[stack]` line checks the main stack. The product radio's
/// heartbeat only (`radio`): the M1 probe's output stays as it was recorded.
#[cfg(feature = "radio")]
pub fn log_if_grown(tag: &str) {
    let high = RADIO_HIGH_WATER.load(Ordering::Relaxed);
    if high > LOGGED_HIGH_WATER.swap(high, Ordering::Relaxed) {
        log::info!(
            "[radio-heap] {tag}: high-water {high} B of {HEAP_RADIO_BYTES} B, {} B live, {} B \
             overflowed to main",
            RADIO_LIVE.load(Ordering::Relaxed),
            OVERFLOWED.load(Ordering::Relaxed),
        );
    }
}

/// Whether `block` lies in the radio's region.
fn in_radio_region(block: *mut u8) -> bool {
    let (start, size) = radio_region();
    (start..start + size).contains(&(block as usize))
}
