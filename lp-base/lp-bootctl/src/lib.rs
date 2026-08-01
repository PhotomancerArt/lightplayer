//! The boot-control sector: a flash-persisted instruction to the next boot.
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

mod boot_control;
mod boot_flags;
mod crc32;
mod sector;

pub use boot_control::{BootAction, BootControl, DecodeOutcome, decode};
pub use boot_flags::BootFlags;
pub use sector::{
    BOOTCTL_PARTITION_OFFSET, BOOTCTL_PARTITION_SIZE, RECORD_LEN, SECTOR_MAGIC, SECTOR_VERSION,
    encode_record,
};
