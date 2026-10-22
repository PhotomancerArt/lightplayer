//! The ESP32-S3's flash MMU plan for the split image: which table entries
//! the core, the engine, the loader and the core's own windows use — the one
//! definition the split tool, the loader and the core share.
//!
//! # The table
//!
//! One table of [`MMU_ENTRIES`] (512) entries serves both buses: entry `n`
//! maps one 64 KiB flash page ([`MMU_PAGE`]) at [`drom`]`(n)` =
//! `0x3C00_0000 + n × 64 KiB` (the data bus) **and** at [`irom`]`(n)` =
//! `0x4200_0000 + n × 64 KiB` (the instruction bus) — the mask ROM's
//! `Cache_Dbus_MMU_Set`/`Cache_Ibus_MMU_Set` index both by
//! `(vaddr >> 16) & 0x1ff` (`lp-emu-esp32s3`'s `cache.rs` cites the ROM's
//! code). So one run of flash pages mapped once is readable at one address
//! and executable at another: the core's rodata and code (esp-hal's
//! `.rotext_dummy`), and the engine's ([`ENGINE_DROM`], [`ENGINE_IROM`]).
//!
//! # The plan
//!
//! ```text
//! entries          data bus                 what
//! 0x000..0x100     0x3C00_0000..0x3D00_0000 the core (entry 0 = its first page)
//! 0x100..0x1C0     0x3D00_0000..0x3DC0_0000 the engine (12 MiB window)
//! 0x1C0..0x200     0x3DC0_0000..0x3E00_0000 the loader's scratch; after the
//!                                           jump, the core's two windows:
//!   0x1C0..0x1E0   0x3DC0_0000..0x3DE0_0000   lpfs (the tree store's reads)
//!   0x1E0..0x200   0x3DE0_0000..0x3E00_0000   the core's scratch (records, hash)
//! ```
//!
//! # What the loader claims, forever
//!
//! The loader is never updated over the air, so what it assumes of the MMU
//! binds every core a board will ever boot (the S3 plan's ruling L4):
//!
//! - **entry 0 is the core's first page**: the loader maps the core from
//!   entry 0 up, both buses;
//! - **a core's flash segments stay below entry [`LOADER_SCRATCH`]`.start`**
//!   ([`CORE_ENTRY_LIMIT`]): the loader reads records and headers through
//!   [`LOADER_SCRATCH`], so a core linked into it cannot boot;
//! - **the loader's scratch is [`LOADER_SCRATCH`]**, and every entry of it is
//!   unmapped before the jump, so a core starts with that window empty;
//! - **the loader's RAM** is [`crate::S3_LOADER_RAM`], which every core's
//!   `.bss` covers (that is the boot-stage RAM's business, not the MMU's).
//!
//! Everything else here is a **per-build** choice the core makes for itself
//! after it starts: [`CORE_ENTRIES`]'s upper end (a core today needs a few
//! MiB; up to [`CORE_ENTRY_LIMIT`] is allowed), the engine's window, and
//! how the core splits the scratch into [`LPFS_WINDOW`] and
//! [`CORE_SCRATCH`]. A later core may move them without asking the loader.

use core::ops::Range;

/// The S3's only MMU page: 64 KiB (the ROM refuses any other).
pub const MMU_PAGE: u32 = 0x1_0000;
/// Entries in the table, shared by both buses.
pub const MMU_ENTRIES: u32 = 0x200;
/// The data bus's first mapped address (entry 0).
pub const DROM_BASE: u32 = 0x3C00_0000;
/// The instruction bus's first mapped address (entry 0).
pub const IROM_BASE: u32 = 0x4200_0000;

/// Where entry `n` is read on the data bus.
pub const fn drom(entry: u32) -> u32 {
    DROM_BASE + entry * MMU_PAGE
}

/// Where entry `n` is fetched on the instruction bus.
pub const fn irom(entry: u32) -> u32 {
    IROM_BASE + entry * MMU_PAGE
}

/// The entry an address on either bus is served by, if it is in a bus's
/// window.
pub const fn entry_of(vaddr: u32) -> Option<u32> {
    let window = MMU_ENTRIES * MMU_PAGE;
    if vaddr >= DROM_BASE && vaddr < DROM_BASE + window {
        Some((vaddr - DROM_BASE) / MMU_PAGE)
    } else if vaddr >= IROM_BASE && vaddr < IROM_BASE + window {
        Some((vaddr - IROM_BASE) / MMU_PAGE)
    } else {
        None
    }
}

/// The core's entries in this build's plan (per-build; entry 0 is claimed).
pub const CORE_ENTRIES: Range<u32> = 0x000..0x100;
/// The loader's claim: no core flash segment at or above this entry.
pub const CORE_ENTRY_LIMIT: u32 = LOADER_SCRATCH.start;

/// The engine's entries (per-build).
pub const ENGINE_ENTRIES: Range<u32> = 0x100..0x1C0;
/// The engine's first entry.
pub const ENGINE_MMU_ENTRY: u32 = ENGINE_ENTRIES.start;
/// Where the engine's header and rodata are read: `0x3D00_0000`.
pub const ENGINE_DROM: u32 = drom(ENGINE_MMU_ENTRY);
/// Where the engine's code is fetched, through the same entries:
/// `0x4300_0000`.
pub const ENGINE_IROM: u32 = irom(ENGINE_MMU_ENTRY);
/// The most the engine window holds: 12 MiB.
pub const ENGINE_WINDOW: u32 = (ENGINE_ENTRIES.end - ENGINE_ENTRIES.start) * MMU_PAGE;

/// The loader's scratch (claimed): it maps records and headers here, and
/// unmaps every entry of it before the jump.
pub const LOADER_SCRATCH: Range<u32> = 0x1C0..0x200;
/// The loader's scratch on the data bus: `0x3DC0_0000`.
pub const LOADER_SCRATCH_DROM: u32 = drom(LOADER_SCRATCH.start);

/// The core's `lpfs` window, inside the loader's scratch once the core
/// runs (per-build): the tree store's cached reads. 2 MiB; `lpfs` is 1.5 MB.
pub const LPFS_WINDOW: Range<u32> = 0x1C0..0x1E0;
/// The `lpfs` window on the data bus: `0x3DC0_0000`.
pub const LPFS_WINDOW_DROM: u32 = drom(LPFS_WINDOW.start);
/// The most the `lpfs` window holds: 2 MiB.
pub const LPFS_WINDOW_LEN: u32 = (LPFS_WINDOW.end - LPFS_WINDOW.start) * MMU_PAGE;

/// The core's own scratch, inside the loader's once the core runs
/// (per-build): its record reads and its own hash.
pub const CORE_SCRATCH: Range<u32> = 0x1E0..0x200;
/// The core's scratch on the data bus: `0x3DE0_0000`.
pub const CORE_SCRATCH_DROM: u32 = drom(CORE_SCRATCH.start);

// The plan tiles the table: core, engine, scratch; the core's two windows
// tile the scratch.
const _: () = {
    assert!(CORE_ENTRIES.start == 0);
    assert!(CORE_ENTRIES.end == ENGINE_ENTRIES.start);
    assert!(CORE_ENTRIES.end <= CORE_ENTRY_LIMIT);
    assert!(ENGINE_ENTRIES.end == LOADER_SCRATCH.start);
    assert!(LOADER_SCRATCH.end == MMU_ENTRIES);
    assert!(LPFS_WINDOW.start == LOADER_SCRATCH.start);
    assert!(LPFS_WINDOW.end == CORE_SCRATCH.start);
    assert!(CORE_SCRATCH.end == LOADER_SCRATCH.end);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_views_share_one_mmu_entry() {
        assert_eq!(ENGINE_DROM, 0x3D00_0000);
        assert_eq!(ENGINE_IROM, 0x4300_0000);
        assert_eq!((ENGINE_DROM >> 16) & 0x1ff, (ENGINE_IROM >> 16) & 0x1ff);
        assert_eq!(entry_of(ENGINE_DROM), entry_of(ENGINE_IROM));
        assert_eq!(entry_of(ENGINE_DROM), Some(ENGINE_MMU_ENTRY));
    }

    #[test]
    fn the_windows_sit_where_the_plan_says() {
        assert_eq!(drom(0), 0x3C00_0000);
        assert_eq!(irom(0), 0x4200_0000);
        assert_eq!(ENGINE_WINDOW, 12 << 20);
        assert_eq!(LOADER_SCRATCH_DROM, 0x3DC0_0000);
        assert_eq!(LPFS_WINDOW_DROM, 0x3DC0_0000);
        assert_eq!(CORE_SCRATCH_DROM, 0x3DE0_0000);
        assert_eq!(drom(MMU_ENTRIES), 0x3E00_0000);
        assert_eq!(CORE_ENTRY_LIMIT, 0x1C0);
    }

    #[test]
    fn the_lpfs_window_holds_lpfs() {
        // The S3's `lpfs`: 1.5 MB (`lp-fw/fw-esp32s3/partitions.csv`).
        assert!(LPFS_WINDOW_LEN >= 0x18_0000);
    }

    #[test]
    fn entry_of_reads_both_buses_and_nothing_else() {
        assert_eq!(entry_of(0x3C00_0000), Some(0));
        assert_eq!(entry_of(0x4200_FFFF), Some(0));
        assert_eq!(entry_of(0x3DFF_FFFF), Some(0x1FF));
        assert_eq!(entry_of(0x43FF_FFFF), Some(0x1FF));
        assert_eq!(entry_of(0x3E00_0000), None);
        assert_eq!(entry_of(0x4400_0000), None);
        assert_eq!(entry_of(0x3FCC_5000), None);
        assert_eq!(entry_of(0x3BFF_FFFF), None);
    }
}
