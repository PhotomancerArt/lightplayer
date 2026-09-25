//! The C heap: `malloc` and friends for the radio blobs, placed in the
//! reclaimed bootloader segment first.
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
//! So the C heap asks for the reclaimed segment (`dram2_seg`) first and falls
//! back to the whole heap only when it is full. That segment is 64 KiB, and a
//! block in it can never reach the project-load gate's 64 KiB anyway, so
//! whatever the radio puts there costs the gate nothing; what it no longer
//! puts in the main region is contiguity the gate gets back.
//!
//! The segment is picked by capability: `init_board` registers it with
//! [`RECLAIMED`] (`MemoryCapability::External` beside `Internal`). The C6 has
//! no SPI RAM, so nothing else ever asks for `External`; the capability is
//! borrowed as a tag that only the reclaimed segment carries. Rust allocations
//! (the global allocator, esp-radio's own `InternalMemory`) ask for nothing or
//! for `Internal`, and still fill the main region first.
//!
//! Semantics are esp-alloc 0.10's `compat` ones (`malloc.rs`): a 4-byte size
//! header in front of each block, 4-byte alignment.

use enumset::EnumSet;
use esp_alloc::MemoryCapability;

/// The capability set that selects the reclaimed segment alone.
pub const RECLAIMED: EnumSet<MemoryCapability> =
    enumset::enum_set!(MemoryCapability::Internal | MemoryCapability::External);

/// Bytes of header in front of every C block: its total size.
const HEADER: usize = 4;

/// Allocate `size` bytes for C: the reclaimed segment first, then anywhere.
unsafe fn c_alloc(size: usize) -> *mut u8 {
    let total = size + HEADER;
    // SAFETY: HEADER > 0, so the layout is non-zero; align 4 is valid.
    let layout = unsafe { core::alloc::Layout::from_size_align_unchecked(total, 4) };
    let mut ptr = unsafe { esp_alloc::HEAP.alloc_caps(RECLAIMED, layout) };
    if ptr.is_null() {
        ptr = unsafe { esp_alloc::HEAP.alloc_caps(EnumSet::empty(), layout) };
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
    // Both regions are internal RAM.
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
