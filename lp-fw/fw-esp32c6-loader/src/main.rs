//! The split-link loader for the ESP32-C6.
//!
//! The IDF second-stage bootloader boots this image from the start of the
//! app partition, as it would any app. It runs entirely from RAM, and does
//! four things:
//!
//! 1. reads the two boot records and lets `lp_bootctl::choose` pick one, by
//!    the reset reason (`lp_bootctl::ResetKind`): the newest valid record,
//!    or the one before it when the newest is a trial that failed;
//! 2. walks that core's ESP image, mapping no more of flash than the
//!    record's `core_len`: flash segments are **mapped** at their link
//!    addresses through the MMU, RAM segments are **copied** into place
//!    through a scratch mapping (never the ROM's SPI1 routines — see
//!    `flash_window`). When the chosen core does not load, it says why and
//!    tries the other record's core;
//! 3. invalidates the flash cache;
//! 4. jumps to the core's entry point.
//!
//! It never writes flash, and it reads no partition table: the records say
//! where the core is and how long it is. The core marks a trial attempted,
//! started and confirmed itself; this image only reads. Small and boring on
//! purpose: it is the one piece of the split image that is never updated
//! over the air. It carries its version word (`lp_bootctl::loader_identity`)
//! for a core to read.
//!
//! # Two flavours, one crate
//!
//! - **factory** (default): the image above, at `factory+0`, started by the
//!   ESP-IDF second stage (the *IDF door*). Linked by `loader.x`.
//! - **bootloader** (`--features bootloader`): the same boot policy as
//!   LightPlayer's own second stage at flash `0x0`, loaded by the mask ROM
//!   itself (*our door*). Linked by `bootloader.x`; RAM-only, no placeholder
//!   segments; runs a short platform prologue first (`prologue`: the state
//!   the IDF left that a core relies on — watchdogs, brownout, the flash
//!   clocks, 32 KiB MMU pages) and says which bootloader it is in one line.
//!   `app.bin` is byte-identical under both doors. ADR
//!   `docs/adr/2026-10-17-the-c6-boots-through-our-own-bootloader.md`.

#![no_std]
#![no_main]

mod esp_image;
mod flash_window;
mod mmu;
#[cfg(feature = "bootloader")]
mod prologue;
mod rom;

use flash_window::FlashWindow;
use lp_bootctl::loader_identity::LOADER_ID_LEN;
#[cfg(not(feature = "bootloader"))]
use lp_bootctl::loader_identity::loader_identity;
#[cfg(not(feature = "bootloader"))]
use lp_bootctl::LOADER_VERSION;
use lp_bootctl::{
    BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootSlot, REGION_START, ResetKind, choose,
};

#[cfg(not(feature = "bootloader"))]
core::arch::global_asm!(
    ".section .text.entry, \"ax\"",
    ".global _start",
    "_start:",
    "  la sp, _stack_top",
    "  call loader_main",
    "1: j 1b",
);

// The ROM jumps here with its own stack: take ours, zero `.bss` (the ROM
// loads only the image's bytes), then the prologue and the loader's job.
#[cfg(feature = "bootloader")]
core::arch::global_asm!(
    ".section .text.entry, \"ax\"",
    ".global _start",
    "_start:",
    "  la sp, _stack_top",
    "  la t0, _bss_start",
    "  la t1, _bss_end",
    "2: bgeu t0, t1, 3f",
    "  sw zero, 0(t0)",
    "  addi t0, t0, 4",
    "  j 2b",
    "3: call bootloader_main",
    "1: j 1b",
);

/// The loader's version word, in its flash placeholder segment (`loader.x`):
/// a core finds it by scanning the loader image's first 4 KiB.
#[cfg(not(feature = "bootloader"))]
#[unsafe(link_section = ".loader_identity")]
#[used]
static LOADER_IDENTITY: [u8; LOADER_ID_LEN] = loader_identity(LOADER_VERSION);

/// The bootloader's word, first in its RAM segment (`bootloader.x`), so it
/// lands at image offset `0x20` of flash `0x0`.
#[cfg(feature = "bootloader")]
#[unsafe(link_section = ".bootloader_identity")]
#[used]
static BOOTLOADER_IDENTITY: [u8; LOADER_ID_LEN] =
    lp_bootctl::bootloader_identity::bootloader_identity(lp_bootctl::BOOTLOADER_VERSION);

/// Our door: the prologue, our line, then the loader's job.
#[cfg(feature = "bootloader")]
#[unsafe(no_mangle)]
extern "C" fn bootloader_main() -> ! {
    prologue::run();
    rom::print_bootloader(lp_bootctl::BOOTLOADER_VERSION as u32);
    loader_main()
}

/// The loader's own RAM: a core segment landing here would overwrite the code
/// doing the copying.
const LOADER_RAM: core::ops::Range<u32> = 0x4085_0000..0x4086_0000;

/// One core the loader may boot.
#[derive(Clone, Copy)]
struct Candidate {
    core_off: u32,
    /// How much flash to map for it: the record's `core_len`, or the whole
    /// scratch window when there is no record.
    core_len: u32,
    note: &'static core::ffi::CStr,
}

#[unsafe(no_mangle)]
extern "C" fn loader_main() -> ! {
    let page_shift = mmu::page_shift();

    let mut sectors = [None; 2];
    if let Some(records) = FlashWindow::map(BOOT_RECORD_SECTORS[0], 0x2000, page_shift) {
        for (slot, at) in sectors.iter_mut().zip(BOOT_RECORD_SECTORS) {
            let mut buf = [0u8; BOOT_RECORD_READ_LEN];
            // SAFETY: a local buffer.
            if unsafe { records.copy(at, buf.as_mut_ptr(), buf.len() as u32) } {
                *slot = BootSlot::decode(&buf);
            }
        }
    }
    let reset = ResetKind::from_c6_reason(rom::reset_reason());
    let mut candidates = [None; 2];
    match choose(sectors, reset) {
        Some(c) => {
            let note = if c.rolled_back {
                c"rolled back"
            } else if c.slot.record.trial && !c.slot.marks.confirmed {
                c"trial"
            } else {
                c"proven"
            };
            candidates[0] = Some(Candidate {
                core_off: c.slot.record.core_off,
                core_len: c.slot.record.core_len,
                note,
            });
            // The other record's core, if the chosen one will not load.
            candidates[1] = sectors[1 - c.sector].map(|s| Candidate {
                core_off: s.record.core_off,
                core_len: s.record.core_len,
                note: c"fallback",
            });
        }
        // A board flashed before it ever had a record: the core is where the
        // first flash puts it.
        None => {
            candidates[0] = Some(Candidate {
                core_off: REGION_START,
                core_len: flash_window::SCRATCH_LEN,
                note: c"no record",
            });
        }
    }

    for candidate in candidates.into_iter().flatten() {
        let loaded = match FlashWindow::map(candidate.core_off, candidate.core_len, page_shift) {
            Some(w) => esp_image::load(&w, candidate.core_off, page_shift, &LOADER_RAM),
            None => Err(c"core does not fit the scratch window"),
        };
        match loaded {
            Ok(entry) => {
                rom::print_core(candidate.core_off, candidate.note);
                rom::invalidate_cache();
                // SAFETY: `entry` is the core image's own entry point, and
                // every segment it expects is now mapped or loaded.
                let entry: extern "C" fn() -> ! = unsafe { core::mem::transmute(entry as usize) };
                entry()
            }
            Err(why) => rom::print_skipped(candidate.core_off, why),
        }
    }
    rom::print_nothing_to_boot();
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
