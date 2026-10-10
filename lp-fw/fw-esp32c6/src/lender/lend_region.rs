//! The block's backing array and its registration with the heap.

use super::LEND_BYTES;

#[repr(C, align(8))]
struct LendArena(core::mem::MaybeUninit<[u8; LEND_BYTES]>);

static mut HEAP_LEND: LendArena = LendArena(core::mem::MaybeUninit::uninit());

/// Where the block is, `(start address, size)`.
#[allow(dead_code, reason = "read by the heap diagnostics")]
pub fn region() -> (usize, usize) {
    (core::ptr::addr_of!(HEAP_LEND) as usize, LEND_BYTES)
}

/// Register the block as the heap's fourth region and make it the lend
/// region. Call once, after the other three regions are added.
///
/// The region carries the main region's capability (`Internal`), so any
/// request the main region could serve may spill into it once every other
/// region is out.
pub fn install() {
    // SAFETY: the array is handed to the allocator exactly once, here, and
    // nothing else ever names it except to read its address.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            core::ptr::addr_of_mut!(HEAP_LEND).cast::<u8>(),
            LEND_BYTES,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
    esp_alloc::HEAP.set_lend_region(3);
}
