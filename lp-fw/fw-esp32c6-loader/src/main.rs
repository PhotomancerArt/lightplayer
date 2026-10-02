//! The split-link loader for the ESP32-C6.
//!
//! The IDF second-stage bootloader boots this image from the start of the
//! app partition, as it would any app. It runs entirely from RAM, and does
//! four things:
//!
//! 1. reads the two boot records and lets `lp_bootctl::choose` pick one (the
//!    newest valid record, or the one before it when the newest is a trial
//!    that ran and never confirmed);
//! 2. walks that core's ESP image: flash segments are **mapped** at their
//!    link addresses through the MMU, RAM segments are **read** straight from
//!    flash into place by the ROM's SPI routine;
//! 3. invalidates the flash cache;
//! 4. jumps to the core's entry point.
//!
//! It never writes flash. The core marks a trial attempted and confirmed
//! itself; this image only reads. Small and boring on purpose: it is the one
//! piece of the split image that is never updated over the air.

#![no_std]
#![no_main]

mod esp_image;
mod mmu;
mod rom;

use lp_bootctl::{BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootSlot, REGION_START, choose};

core::arch::global_asm!(
    ".section .text.entry, \"ax\"",
    ".global _start",
    "_start:",
    "  la sp, _stack_top",
    "  call loader_main",
    "1: j 1b",
);

/// The loader's own RAM: a core segment landing here would overwrite the code
/// doing the copying.
const LOADER_RAM: core::ops::Range<u32> = 0x4085_0000..0x4086_0000;

#[unsafe(no_mangle)]
extern "C" fn loader_main() -> ! {
    let page_shift = mmu::page_shift();

    let mut sectors = [None; 2];
    for (slot, at) in sectors.iter_mut().zip(BOOT_RECORD_SECTORS) {
        let mut buf = [0u32; BOOT_RECORD_READ_LEN / 4];
        if rom::flash_read(at, &mut buf) {
            *slot = BootSlot::decode(words_as_bytes(&buf));
        }
    }
    let (core_off, note) = match choose(sectors) {
        Some(c) if c.rolled_back => (c.slot.record.core_off, c"rolled back"),
        Some(c) if c.slot.record.trial && !c.slot.marks.confirmed => {
            (c.slot.record.core_off, c"trial")
        }
        Some(c) => (c.slot.record.core_off, c"proven"),
        // A board flashed before it ever had a record: the core is where the
        // first flash puts it.
        None => (REGION_START, c"no record"),
    };
    rom::print_core(core_off, note);

    match esp_image::load(core_off, page_shift, &LOADER_RAM) {
        Ok(entry) => {
            rom::invalidate_cache();
            // SAFETY: `entry` is the core image's own entry point, and every
            // segment it expects is now mapped or loaded.
            let entry: extern "C" fn() -> ! = unsafe { core::mem::transmute(entry as usize) };
            entry()
        }
        Err(why) => {
            rom::print_failure(core_off, why);
            loop {
                core::hint::spin_loop();
            }
        }
    }
}

fn words_as_bytes(words: &[u32]) -> &[u8] {
    // SAFETY: a `u32` slice is a valid byte slice four times as long.
    unsafe { core::slice::from_raw_parts(words.as_ptr().cast(), words.len() * 4) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
