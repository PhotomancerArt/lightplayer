//! SPIKE (fps spike, feature `alloc-count`): esp-alloc's hooks count every
//! allocation and free into `stage_probe`, printed per frame beside `[stage]`.

use core::sync::atomic::Ordering;

use fw_esp32_common::stage_probe::{ALLOC_BYTES, ALLOCS, BIG, BIG_LAST, FREES};

#[unsafe(no_mangle)]
fn _esp_alloc_alloc(
    _heap: &esp_alloc::EspHeap,
    _caps: enumset::EnumSet<esp_alloc::MemoryCapability>,
    ptr: usize,
    size: usize,
) {
    if ptr != 0 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(size as u32, Ordering::Relaxed);
        if size >= 16 * 1024 {
            BIG.fetch_add(1, Ordering::Relaxed);
            BIG_LAST.store(size as u32, Ordering::Relaxed);
        }
    }
}

#[unsafe(no_mangle)]
fn _esp_alloc_dealloc(_heap: &esp_alloc::EspHeap, _ptr: usize, _size: usize) {
    FREES.fetch_add(1, Ordering::Relaxed);
}
