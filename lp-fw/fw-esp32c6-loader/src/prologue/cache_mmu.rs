//! The flash MMU (contract C1): 32 KiB pages and every entry unmapped,
//! before the loader maps the core.
//!
//! The ROM hands over with the cache **enabled** (`l1_cache_ctrl = 0`, its
//! last write before `entry`), 64 KiB pages (`mmu_power_ctrl[4:3] = 0`) and
//! one entry mapped (the page it read this image through). The IDF sets the
//! page mode to 32 KiB (`mmu_power_ctrl[4:3] = 1`, `0x1320_0004` →
//! `0x1320_000C`) and unmaps every entry; so do we, with the cache
//! suspended around the change, then invalidated and resumed. Every packaged
//! and fielded core is laid out on 32 KiB pages.

use super::mmio::modify;
use crate::{mmu, rom};

const MMU_POWER_CTRL: u32 = 0x6000_2384;
const PAGE_MODE: u32 = 0b11 << 3;
const PAGE_MODE_32K: u32 = 0b01 << 3;

pub unsafe fn page_mode_32k_and_unmap_all() {
    let autoload = rom::suspend_cache();
    // SAFETY: SPI0's MMU registers; this stage runs from RAM.
    unsafe { modify(MMU_POWER_CTRL, PAGE_MODE, PAGE_MODE_32K) };
    mmu::unmap_all();
    rom::invalidate_cache();
    rom::resume_cache(autoload);
}
