//! SPIKE (`spike-s3-split-image`): the S3 core's half of a split image.
//!
//! A split build (`LP_SPLIT_LINK=1`, `tools/lp-fw-split --chip esp32s3`) is
//! one link cut by reachability into a **core** — everything `core_boot` and
//! the RAM code reach — and an **engine**, reached only through
//! [`ENGINE_HEADER`]'s `entry`, which the core reads through a plain address.
//!
//! The S3's one MMU table serves both buses at one index, so the engine is
//! one run of flash pages mapped at `ENGINE_MMU_ENTRY`: the header and the
//! engine's rodata are read through the data-bus view (`ENGINE_DROM`), its
//! code fetched through the instruction-bus view (`ENGINE_IROM`) of the same
//! entries (`lp_bootctl::split_layout_s3`).
//!
//! What this spike leaves out, against the C6's `ota/`: the boot-record
//! marks (attempted/started/confirmed), the engine guard (a hash at the first
//! boot), core-only serving the update channel, and the write fence. It
//! reads the records, finds the engine, and enters it.

use lp_bootctl::engine_header::{ENGINE_BUILD_ID_LEN, ENGINE_HEADER_LEN};
use lp_bootctl::split_layout_s3::{ENGINE_DROM, ENGINE_MMU_ENTRY, S3_PAGE, S3SplitLayout};
use lp_bootctl::{BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootSlot, EngineHeaderError};

use crate::{CoreBoot, lp_engine_entry};

/// The engine's header, the first bytes of the engine region: the
/// `#[repr(C)]` mirror of `lp_bootctl::engine_header`'s v1 layout (the C6's
/// `EngineHeader`). The core never names it.
#[repr(C)]
struct EngineHeader {
    magic: u32,
    version: u16,
    header_len: u16,
    entry: fn(CoreBoot),
    /// Patched by the packager: `engine.bin`'s length.
    len: u32,
    build_id: [u8; ENGINE_BUILD_ID_LEN],
    /// Patched by the packager: CRC-32 of the bytes before it.
    crc: u32,
    commit: u32,
}

const _: () = {
    use core::mem::{offset_of, size_of};
    use lp_bootctl::engine_header as eh;
    assert!(offset_of!(EngineHeader, entry) == eh::ENGINE_ENTRY_OFFSET);
    assert!(offset_of!(EngineHeader, len) == eh::ENGINE_LEN_OFFSET);
    assert!(offset_of!(EngineHeader, build_id) == eh::ENGINE_BUILD_ID_OFFSET);
    assert!(offset_of!(EngineHeader, crc) == eh::ENGINE_CRC_OFFSET);
    assert!(offset_of!(EngineHeader, commit) == eh::ENGINE_COMMIT_OFFSET);
    assert!(size_of::<EngineHeader>() == eh::ENGINE_HEADER_LEN);
};

/// This build's id, `"<version>+<commit>"`, named so the split tool can
/// check the engine header's against the core's.
#[unsafe(no_mangle)]
static LP_BUILD_ID: [u8; ENGINE_BUILD_ID_LEN] = lp_bootctl::engine_header::build_id_field(
    concat!(env!("LP_APP_VERSION"), "+", env!("LP_BUILD_COMMIT")).as_bytes(),
);

#[unsafe(link_section = ".engine_header")]
#[used]
static ENGINE_HEADER: EngineHeader = EngineHeader {
    magic: lp_bootctl::engine_header::ENGINE_MAGIC,
    version: lp_bootctl::engine_header::ENGINE_HEADER_VERSION,
    header_len: ENGINE_HEADER_LEN as u16,
    entry: lp_engine_entry,
    len: 0,
    build_id: lp_bootctl::engine_header::build_id_field(
        concat!(env!("LP_APP_VERSION"), "+", env!("LP_BUILD_COMMIT")).as_bytes(),
    ),
    crc: 0,
    commit: lp_bootctl::engine_header::ENGINE_COMMITTED,
};

/// The S3 flash MMU: 512 entries at `0x600C_5000`, entry `n` behind both
/// `0x3C00_0000 + n × 64 KiB` and `0x4200_0000 + n × 64 KiB`; bit 14 set
/// means invalid, bits 13:0 are the flash page (lp-emu-esp32s3 `cache.rs`,
/// read off the mask ROM's `Cache_MMU_Init`/`Cache_*bus_MMU_Set`).
const MMU_TABLE: usize = 0x600C_5000;
const MMU_INVALID: u32 = 0x4000;
const MMU_PAGE_MASK: u32 = 0x3FFF;
/// The core's own scratch entry (the top one), for reading the records.
const SCRATCH_ENTRY: u32 = 0x1FF;
/// `factory`'s end on today's table (`partitions.csv`); a product core reads
/// it from the flashed table, as the C6's does.
const FACTORY_END: u32 = 0x61_0000;

unsafe extern "C" {
    fn Cache_Invalidate_DCache_All();
    fn Cache_Invalidate_ICache_All();
}

fn mmu_read(entry: u32) -> u32 {
    // SAFETY: a read of one MMU table word.
    unsafe { core::ptr::read_volatile((MMU_TABLE + 4 * entry as usize) as *const u32) }
}

fn mmu_write(entry: u32, value: u32) {
    // SAFETY: an entry nothing running is mapped through.
    unsafe { core::ptr::write_volatile((MMU_TABLE + 4 * entry as usize) as *mut u32, value) }
}

fn invalidate_caches() {
    // SAFETY: ROM routines with no arguments; flash pages are read-only, so
    // nothing dirty is lost.
    unsafe {
        Cache_Invalidate_DCache_All();
        Cache_Invalidate_ICache_All();
    }
}

/// Where the running core is: the flash page behind MMU entry 0, which the
/// loader mapped to the core's first page (its app descriptor).
fn running_core_offset() -> Option<u32> {
    let e = mmu_read(0);
    (e & MMU_INVALID == 0).then_some((e & MMU_PAGE_MASK) * S3_PAGE)
}

/// Both boot-record sectors, read through the scratch entry.
fn read_records() -> [Option<BootSlot>; 2] {
    let page = BOOT_RECORD_SECTORS[0] / S3_PAGE * S3_PAGE;
    mmu_write(SCRATCH_ENTRY, page / S3_PAGE);
    invalidate_caches();
    let base = 0x3C00_0000 + SCRATCH_ENTRY * S3_PAGE;
    let mut slots = [None; 2];
    for (slot, at) in slots.iter_mut().zip(BOOT_RECORD_SECTORS) {
        let mut buf = [0u8; BOOT_RECORD_READ_LEN];
        for (i, b) in buf.iter_mut().enumerate() {
            // SAFETY: the scratch entry maps the records' page.
            *b = unsafe { core::ptr::read_volatile((base + (at - page) + i as u32) as *const u8) };
        }
        *slot = BootSlot::decode(&buf);
    }
    mmu_write(SCRATCH_ENTRY, MMU_INVALID);
    slots
}

/// Map `len` bytes of flash from `start` (page-aligned) at the engine's
/// entries.
fn map_engine(start: u32, len: u32) {
    for k in 0..len.div_ceil(S3_PAGE) {
        mmu_write(ENGINE_MMU_ENTRY + k, start / S3_PAGE + k);
    }
    invalidate_caches();
}

/// The engine's entry, if a committed engine of this build is in the room.
fn find_engine() -> Result<(fn(CoreBoot), u32, u32), &'static str> {
    let core_off = running_core_offset().ok_or("MMU entry 0 is not mapped")?;
    let layout =
        S3SplitLayout::from_factory(lp_bootctl::LOADER_OFFSET, FACTORY_END - 0x1_0000).unwrap();
    let records = read_records();
    let record = records
        .iter()
        .flatten()
        .find(|s| s.record.core_off == core_off)
        .ok_or("no boot record names the running core")?;
    let room = layout.engine_room(core_off, record.record.core_len);
    if room.is_empty() {
        return Err("no room for an engine");
    }
    map_engine(room.start, ENGINE_HEADER_LEN as u32);
    let mut bytes = [0u8; ENGINE_HEADER_LEN];
    for (i, b) in bytes.iter_mut().enumerate() {
        // SAFETY: the header's page is mapped (erased flash reads as 0xff).
        *b = unsafe { core::ptr::read_volatile((ENGINE_DROM as *const u8).add(i)) };
    }
    let header = lp_bootctl::EngineHeader::decode(&bytes).map_err(EngineHeaderError::describe)?;
    header
        .validate(&LP_BUILD_ID, room.len())
        .map_err(EngineHeaderError::describe)?;
    map_engine(room.start, header.len);
    // SAFETY: a committed header of this build, from this link: its entry is
    // `lp_engine_entry`'s address in the engine now mapped behind it.
    let entry: fn(CoreBoot) = unsafe { core::mem::transmute(header.entry as usize) };
    Ok((entry, room.start, header.len))
}

/// After `core_boot`: enter the engine the records and the header agree on,
/// or stay core-only (link up, nothing served).
pub fn split_boot(core: CoreBoot) {
    match find_engine() {
        Ok((entry, at, len)) => {
            log::info!("[CORE] engine @{at:#x} ({len} B), entering");
            esp_println::println!("[CORE] engine @{at:#x} ({len} B), entering");
            entry(core)
        }
        Err(why) => {
            log::error!("[CORE] no engine: {why} — core-only");
            esp_println::println!("[CORE] no engine: {why} — core-only");
            // Core-only on the C6 serves the update channel; this spike
            // only keeps the link and the watchdog alive by leaking them.
            core::mem::forget(core);
        }
    }
}
