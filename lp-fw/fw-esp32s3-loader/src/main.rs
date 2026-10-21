//! SPIKE (`spike-s3-split-image`): the split-image loader for the ESP32-S3,
//! factory flavour — the app the IDF second-stage bootloader starts at
//! `factory+0`. The C6 loader's policy (`fw-esp32c6-loader`) on the S3's
//! machine:
//!
//! 1. read both boot records through a scratch MMU window and let
//!    `lp_bootctl::choose` pick one;
//! 2. walk that core's ESP image: flash segments are **mapped** (data-bus
//!    segments at `0x3C00_0000 + n·64 KiB`, instruction-bus segments at
//!    `0x4200_0000 + n·64 KiB` — one table, one index for both), RAM
//!    segments **copied** through the scratch window, IRAM ones through
//!    their data-bus alias (IRAM takes only 32-bit stores);
//! 3. invalidate both caches and jump to the core's entry.
//!
//! Left out against the C6's: the fallback to the other record, the
//! reset-reason table for the S3 (`ResetKind::from_c6_reason` is the C6's;
//! every reset reads as cold here), and the version word.

#![no_std]
#![no_main]

use lp_bootctl::split_layout_s3::{S3_PAGE, S3_REGION_START, SCRATCH_MMU_ENTRY};
use lp_bootctl::{BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootSlot, ResetKind, choose};

unsafe extern "C" {
    fn ets_printf(fmt: *const core::ffi::c_char, ...) -> i32;
    fn Cache_Invalidate_ICache_All();
    fn Cache_Invalidate_DCache_All();
}

/// The S3 flash MMU (lp-emu-esp32s3 `cache.rs`, from the mask ROM).
const MMU_TABLE: usize = 0x600C_5000;
const MMU_INVALID: u32 = 0x4000;
const DROM: core::ops::Range<u32> = 0x3C00_0000..0x3E00_0000;
const IROM: core::ops::Range<u32> = 0x4200_0000..0x4400_0000;
/// Internal SRAM1's instruction-bus view, and its distance to the data bus.
const IRAM1: core::ops::Range<u32> = 0x4037_8000..0x403E_0000;
const IRAM_TO_DRAM: u32 = 0x6F_0000;
/// The scratch window: entries 0x1C0..0x200 (4 MiB), data-bus view.
const SCRATCH_PAGES: u32 = 0x40;
const SCRATCH: u32 = 0x3C00_0000 + SCRATCH_MMU_ENTRY * S3_PAGE;
/// The loader's own RAM (D view), which no copied segment may touch.
const LOADER_DRAM: core::ops::Range<u32> = 0x3FCB_0000..0x3FCC_0000;

fn mmu_set(entry: u32, page: u32) {
    // SAFETY: one MMU table word; nothing runs from flash here.
    unsafe { core::ptr::write_volatile((MMU_TABLE + 4 * entry as usize) as *mut u32, page) }
}

fn invalidate() {
    // SAFETY: ROM routines, no arguments.
    unsafe {
        Cache_Invalidate_DCache_All();
        Cache_Invalidate_ICache_All();
    }
}

/// Flash `[base, base + len)` (page-rounded) behind the scratch window.
struct Window {
    base: u32,
    len: u32,
}

impl Window {
    fn map(at: u32, len: u32) -> Option<Self> {
        let base = at - at % S3_PAGE;
        let pages = (at + len - base).div_ceil(S3_PAGE);
        if pages > SCRATCH_PAGES {
            return None;
        }
        for k in 0..pages {
            mmu_set(SCRATCH_MMU_ENTRY + k, base / S3_PAGE + k);
        }
        invalidate();
        Some(Self {
            base,
            len: pages * S3_PAGE,
        })
    }

    fn byte(&self, at: u32) -> u8 {
        // SAFETY: inside the mapped window (callers check `has`).
        unsafe { core::ptr::read_volatile((SCRATCH + (at - self.base)) as *const u8) }
    }

    fn has(&self, at: u32, len: u32) -> bool {
        at >= self.base && at + len <= self.base + self.len
    }

    fn word(&self, at: u32) -> Option<u32> {
        if !self.has(at, 4) {
            return None;
        }
        Some(u32::from_le_bytes([
            self.byte(at),
            self.byte(at + 1),
            self.byte(at + 2),
            self.byte(at + 3),
        ]))
    }
}

/// Map and load the ESP image at flash `at`; its entry point.
fn load(at: u32, len: u32) -> Result<u32, &'static core::ffi::CStr> {
    let w = Window::map(at, len).ok_or(c"core does not fit the scratch window")?;
    let (Some(first), Some(entry)) = (w.word(at), w.word(at + 4)) else {
        return Err(c"flash read failed");
    };
    let b = first.to_le_bytes();
    let segments = b[1];
    if b[0] != 0xE9 || segments == 0 || segments > 16 {
        return Err(c"no image there");
    }
    let mut off = at + 24;
    for _ in 0..segments {
        let (Some(addr), Some(slen)) = (w.word(off), w.word(off + 4)) else {
            return Err(c"flash read failed");
        };
        let data = off + 8;
        let window = if DROM.contains(&addr) {
            Some(DROM.start)
        } else if IROM.contains(&addr) {
            Some(IROM.start)
        } else {
            None
        };
        if let Some(bus) = window {
            if data % S3_PAGE != addr % S3_PAGE {
                return Err(c"segment not page-congruent");
            }
            let mut v = addr - addr % S3_PAGE;
            let mut p = data - data % S3_PAGE;
            while v < addr + slen {
                mmu_set((v - bus) / S3_PAGE, p / S3_PAGE);
                v += S3_PAGE;
                p += S3_PAGE;
            }
        } else {
            if !w.has(data, slen) {
                return Err(c"flash read failed");
            }
            // IRAM through its data-bus alias: byte stores are fine there.
            let dest = if IRAM1.contains(&addr) {
                addr - IRAM_TO_DRAM
            } else {
                addr
            };
            if dest < LOADER_DRAM.end && dest + slen > LOADER_DRAM.start {
                return Err(c"segment overlaps the loader");
            }
            for i in 0..slen {
                // SAFETY: the core's RAM, free until it starts; checked
                // against the loader's own above.
                unsafe { core::ptr::write_volatile((dest + i) as *mut u8, w.byte(data + i)) };
            }
        }
        off = data + slen;
    }
    Ok(entry)
}

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    // SAFETY: a C format string, no arguments.
    unsafe { ets_printf(c"[LOADER] LightPlayer S3 loader (spike)\n".as_ptr()) };
    let mut sectors = [None; 2];
    if let Some(w) = Window::map(BOOT_RECORD_SECTORS[0], 0x2000) {
        for (slot, at) in sectors.iter_mut().zip(BOOT_RECORD_SECTORS) {
            let mut buf = [0u8; BOOT_RECORD_READ_LEN];
            for (i, b) in buf.iter_mut().enumerate() {
                *b = w.byte(at + i as u32);
            }
            *slot = BootSlot::decode(&buf);
        }
    }
    let (core_off, core_len, note) = match choose(sectors, ResetKind::Cold) {
        Some(c) => (c.slot.record.core_off, c.slot.record.core_len, c"record"),
        None => (S3_REGION_START, 0x30_0000, c"no record"),
    };
    match load(core_off, core_len) {
        Ok(entry) => {
            // SAFETY: a C format string with matching arguments.
            unsafe {
                ets_printf(
                    c"[LOADER] core @0x%x (%s) entry 0x%x\n".as_ptr(),
                    core_off,
                    note.as_ptr(),
                    entry,
                )
            };
            // The scratch window goes; the core maps what it needs.
            for k in 0..SCRATCH_PAGES {
                mmu_set(SCRATCH_MMU_ENTRY + k, MMU_INVALID);
            }
            invalidate();
            // SAFETY: the core image's entry; its segments are in place.
            let entry: extern "C" fn() -> ! = unsafe { core::mem::transmute(entry as usize) };
            entry()
        }
        Err(why) => {
            // SAFETY: as above.
            unsafe {
                ets_printf(
                    c"[LOADER] core @0x%x skipped: %s\n".as_ptr(),
                    core_off,
                    why.as_ptr(),
                )
            };
        }
    }
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
