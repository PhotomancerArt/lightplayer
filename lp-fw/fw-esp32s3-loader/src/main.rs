//! The split-image loader for the ESP32-S3.
//!
//! The IDF second-stage bootloader boots this image from the start of the
//! app partition, as it would any app. It runs entirely from RAM, and does
//! the C6 loader's four things on the S3's machine (layout 2,
//! `lp_bootctl::split_layout_2`):
//!
//! 1. reads the two boot records and lets `lp_bootctl::choose` pick one, by
//!    the reset reason (`lp_bootctl::ResetKind::from_s3_reason`): the newest
//!    valid record, or the one before it when the newest is a trial that
//!    failed;
//! 2. walks that core's ESP image, mapping no more of flash than the
//!    record's `core_len`: flash segments are **mapped** at their link
//!    addresses through the MMU (one table for both buses), RAM segments are
//!    **copied** into place through the scratch window (never the ROM's SPI1
//!    routines — see `flash_window`). When the chosen core does not load, it
//!    says why and tries the other record's core;
//! 3. unmaps its scratch window and invalidates both caches;
//! 4. jumps to the core's entry point.
//!
//! It never writes flash, and it reads no partition table: the records say
//! where the core is and how long it is. The core marks a trial attempted,
//! started and confirmed itself; this image only reads. Small and boring on
//! purpose: it is the one piece of the split image that is never updated
//! over the air. It carries its version word (`lp_bootctl::loader_identity`)
//! for a core to read.
//!
//! What it claims of every core it will ever boot is ruling L4
//! (`lp_bootctl::s3_mmu`): entry 0 is the core's first page, a core's flash
//! segments stay below entry `0x1C0`, the scratch entries `0x1C0..0x200` are
//! the loader's and are empty at the jump, and its RAM
//! (`lp_bootctl::S3_LOADER_RAM`) is inside every core's `.bss`.
//!
//! # One flavour today
//!
//! This is the **factory** loader at `factory+0`, started by the ESP-IDF
//! second stage (the *IDF door*) and linked by `loader.x`. A
//! **bootloader** flavour (LightPlayer's own second stage at flash `0x0`, as
//! the C6 has) is future work; it would add a feature, a linker script, its
//! own stack and a platform prologue in front of [`loader_main`], and change
//! nothing below it.

#![no_std]
#![no_main]

mod esp_image;
mod flash_window;
mod mmu;
mod rom;

use flash_window::FlashWindow;
use lp_bootctl::loader_identity::{LOADER_ID_LEN, loader_identity};
use lp_bootctl::{
    BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootSlot, LAYOUT_2_REGION_START, LOADER_VERSION,
    ResetKind, choose,
};

/// The loader's version word, in its data-view placeholder segment
/// (`loader.x`): a core finds it by scanning the loader image's first 4 KiB.
#[unsafe(link_section = ".loader_identity")]
#[used]
static LOADER_IDENTITY: [u8; LOADER_ID_LEN] = loader_identity(LOADER_VERSION);

/// The IDF bootloader calls this on its own stack (the mask ROM's, inside
/// `lp_bootctl::S3_IDF_BOOTLOADER_RAM`, which no core segment may reach).
#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    loader_main()
}

/// One core the loader may boot.
#[derive(Clone, Copy)]
struct Candidate {
    core_off: u32,
    /// How much flash to map for it: the record's `core_len`, or the whole
    /// scratch window when there is no record.
    core_len: u32,
    note: &'static core::ffi::CStr,
}

fn loader_main() -> ! {
    let mut sectors = [None; 2];
    if let Some(records) = FlashWindow::map(BOOT_RECORD_SECTORS[0], 0x2000) {
        for (slot, at) in sectors.iter_mut().zip(BOOT_RECORD_SECTORS) {
            let mut buf = [0u8; BOOT_RECORD_READ_LEN];
            // SAFETY: a local buffer.
            if unsafe { records.copy(at, buf.as_mut_ptr(), buf.len() as u32) } {
                *slot = BootSlot::decode(&buf);
            }
        }
    }
    let reset = ResetKind::from_s3_reason(rom::reset_reason());
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
                core_off: LAYOUT_2_REGION_START,
                core_len: flash_window::SCRATCH_LEN,
                note: c"no record",
            });
        }
    }

    for candidate in candidates.into_iter().flatten() {
        let loaded = match FlashWindow::map(candidate.core_off, candidate.core_len) {
            Some(w) => esp_image::load(&w, candidate.core_off),
            None => Err(c"core does not fit the scratch window"),
        };
        match loaded {
            Ok(entry) => {
                rom::print_core(candidate.core_off, candidate.note);
                // The scratch window goes; the core maps what it needs.
                mmu::unmap_scratch();
                rom::invalidate_caches();
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
