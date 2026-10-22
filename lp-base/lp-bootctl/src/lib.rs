//! Flash-persisted instructions to the next boot: the boot-control sector,
//! and the split image's formats (the ESP32-C6's layout 1, the ESP32-S3's
//! layout 2).
//!
//! This crate is the single definition of every on-flash format the boot
//! path shares between writers and readers. It is `no_std`, IO-free and
//! clock-free: flash bytes and reset reasons are passed in.
//!
//! # The split image's formats
//!
//! The C6's product image is one link split into a **core** and an
//! **engine**, booted by a RAM-only **loader**, all inside `factory`
//! (layout 1, [`SplitLayout`]):
//!
//! ```text
//! 0x10000  loader                 carries its version word (loader_identity)
//! 0x15000  progress record        an over-the-air update's (lpc-update's transfer_record)
//! 0x16000  boot record, sector 0  BootRecord + marks (boot_record)
//! 0x17000  boot record, sector 1
//! 0x18000  core                   an ESP image; carries the engine digest slot (engine_digest)
//!  page↑   engine                 the engine header first (engine_header)
//!   …      free to factory's end  (read from the flashed partition table)
//! ```
//!
//! - [`BootRecord`]/[`BootMarks`] and [`choose`]: which core the loader
//!   boots, the trial marks, and the rollback rules, by [`ResetKind`].
//! - [`EngineHeader`]: whether there is an engine the core may enter, and
//!   its committed length.
//! - [`engine_digest`]: SHA-256 of the engine exactly as flashed, in the core.
//! - [`loader_identity`]: the loader's version word.
//! - [`BOOT_STAGE_RAM`]: the loader's RAM, which no radio DMA may reach
//!   across a warm reset.
//! - [`bootloader_identity`]: our second-stage bootloader's word at flash
//!   `0x0` (absent under the IDF bootloader).
//!
//! These bind once a fielded core can install an update (see
//! `docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md`).
//!
//! # Layout 2: the ESP32-S3's
//!
//! The S3 ships the same split image with the region moved to the next
//! 64 KiB page, the only MMU page the S3 has ([`split_layout_2`],
//! [`SplitLayout::layout_2`]). Everything below `0x18000` is layout 1's
//! bytes at layout 1's offsets, so the records, `choose`, the engine header,
//! the progress record and the engine digest are the same formats:
//!
//! ```text
//! 0x10000  loader                 (layout 1's)
//! 0x15000  progress record        (layout 1's)
//! 0x16000  boot record, sector 0  (layout 1's)
//! 0x17000  boot record, sector 1  (layout 1's)
//! 0x18000  reserved               erased, never written (LAYOUT_2_RESERVED)
//! 0x20000  core                   the region, on 64 KiB pages
//!  page↑   engine
//!   …      free to factory's end
//! ```
//!
//! - [`s3_mmu`]: the S3's flash MMU plan — the core's, the engine's and the
//!   loader's entries, and what the loader claims of the MMU forever.
//! - [`ResetKind::from_s3_reason`]: the S3's reset codes in the C6's classes,
//!   cited to the S3 ROM's own name table.
//! - [`S3_BOOT_STAGE_RAM`]: the S3 loader's and the IDF bootloader's RAM,
//!   which no radio DMA may reach across a warm reset.
//!
//! Layout 2 binds from the first release whose S3 core can install an
//! update.
//!
//! # The boot-control sector
//!
//! One 4 KB flash sector carrying a small record that the firmware reads
//! **before** it auto-loads a project. Its purpose is to make a device
//! recoverable when its own project is what prevents it from running — a
//! too-bright project that browns the board out, a shader that hangs the
//! watchdog, anything that dies before the link is usable.
//!
//! # Why flash, and not the recovery region
//!
//! [`lp_recovery`](../lp_recovery/index.html)'s breadcrumb region lives in
//! RTC fast RAM. That survives software and watchdog resets but **not a
//! power cycle** — and unplugging the board is exactly what a person does to
//! a device that is misbehaving. A latch that a user can erase by doing the
//! obvious thing is not a latch. This sector is flash-resident so it
//! survives.
//!
//! # Two writers
//!
//! The sector is written from two directions, and the format is shared so
//! they cannot disagree:
//!
//! - **The host**, over esptool/espflash, while the device sits in ROM
//!   download mode. This is the path that works on a board that cannot boot
//!   far enough to talk to anything.
//! - **The firmware itself**, so a device that keeps failing can latch its
//!   own degraded state across a power cycle. (Not yet implemented — the
//!   firmware side currently only reads and clears. See the follow-up plan.)
//!
//! # Blank is safe
//!
//! A device that has never seen this feature has `0xFF` bytes here, and that
//! **must** decode to "boot normally". So must a bad magic, a bad CRC, a
//! future version, and a torn write. There is exactly one way to get a
//! non-default boot: a fully valid record that says so. Every other state,
//! including every corruption state, falls back to normal operation.
//!
//! # Torn writes
//!
//! The record is written to an erased sector in **one** operation, and its
//! integrity rests on the magic and the CRC rather than on write ordering.
//!
//! This is not the discipline [`lp_recovery`] uses — it publishes RTC-RAM
//! structures by flipping a single visibility word last. That trick is
//! unavailable here: every flash-write API that can reach this sector (the
//! ESP ROM/stub `FLASH_BEGIN`, hence both `espflash` and `esptool-js`)
//! **erases the sectors it is about to write**, so a second write meant to
//! publish a first one erases it instead.
//!
//! The CRC covers the magic, so any interrupted write fails either the magic
//! check or the checksum, and decodes as "no record". `encode_record` is the
//! whole API; see [`sector`] for the byte layout.

#![no_std]

#[cfg(test)]
extern crate std;

mod boot_choice;
mod boot_control;
mod boot_flags;
mod boot_record;
mod boot_stage_ram;
pub mod bootloader_identity;
pub mod engine_digest;
pub mod engine_header;
pub mod loader_identity;
mod reset_kind;
mod s3_boot_stage_ram;
pub mod s3_mmu;
mod sector;
mod split_layout;
pub mod split_layout_2;

pub use boot_choice::{BootChoice, booted_by, choose, cold_retry_to_count, failed, stray_trial};
pub use boot_record::{
    ATTEMPTED_MARK_OFFSET, BOOT_RECORD_LEN, BOOT_RECORD_READ_LEN, BOOT_RECORD_VERSION, BootMarks,
    BootRecord, BootSlot, COLD_RETRY_CAP, COLD_TALLY_OFFSET, CONFIRMED_MARK_OFFSET, RECORD_MAGIC,
    STARTED_MARK_OFFSET, build_hash,
};
pub use boot_stage_ram::{BOOT_STAGE_RAM, split_at_boot_stage_ram};
pub use bootloader_identity::{BOOTLOADER_VERSION, find_bootloader_version};
pub use engine_header::{EngineHeader, EngineHeaderError};
pub use loader_identity::{LOADER_VERSION, find_loader_version};
pub use reset_kind::ResetKind;
pub use s3_boot_stage_ram::{
    S3_BOOT_STAGE_RAM, S3_IDF_BOOTLOADER_RAM, S3_LOADER_IRAM, S3_LOADER_RAM, SRAM1_I_FROM_D,
    split_at_s3_boot_stage_ram,
};
pub use split_layout::{
    BOOT_RECORD_SECTORS, Extent, LOADER_MAX_LEN, LOADER_OFFSET, PROGRESS_RECORD_SECTOR,
    REGION_START, SplitLayout,
};
pub use split_layout_2::{LAYOUT_2_PAGE, LAYOUT_2_REGION_START, LAYOUT_2_RESERVED};

pub use boot_control::{BootAction, BootControl, DecodeOutcome, decode};
pub use boot_flags::BootFlags;
pub use sector::{
    BOOTCTL_PARTITION_OFFSET, BOOTCTL_PARTITION_SIZE, RECORD_LEN, SECTOR_MAGIC, SECTOR_VERSION,
    encode_record,
};
