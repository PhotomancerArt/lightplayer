//! The S3 core's half of a split image (`LP_SPLIT_LINK=1` builds, made by
//! `lp-fw-split build --chip esp32s3`; the C6's twin is the split section
//! of `fw-esp32c6/src/main.rs`).
//!
//! One link, cut by reachability into a **core** — everything `core_boot`
//! and the RAM code reach — and an **engine**, reached only through
//! [`ENGINE_HEADER`]'s `entry`, which the core reads through a plain address.
//!
//! The S3's one MMU table serves both buses at one index (layout 2,
//! `lp_bootctl::s3_mmu`): the engine is one run of flash pages mapped at
//! `ENGINE_MMU_ENTRY`, its header and rodata read through the data view
//! (`ENGINE_DROM`), its code fetched through the instruction view of the same
//! entries (`ENGINE_IROM`).
//!
//! What this does not do yet (the S3 updates plan's P8, with the C6's update
//! board side moved into shared code): the boot-record marks
//! (attempted/started/confirmed), the engine guard, and a core-only mode
//! that serves the update channel. A board whose engine cannot be entered
//! stays reachable on its link and says why; nothing offers it an update,
//! because this image announces `ota_layout: 0`.

use lp_bootctl::engine_header::{ENGINE_BUILD_ID_LEN, ENGINE_HEADER_LEN};
use lp_bootctl::s3_mmu::{CORE_SCRATCH, ENGINE_DROM, ENGINE_MMU_ENTRY, MMU_PAGE, drom};
use lp_bootctl::{BOOT_RECORD_READ_LEN, BOOT_RECORD_SECTORS, BootSlot, SplitLayout};

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

/// This build's id, `"<version>+<commit>"`: core and engine carry the same
/// one because they come from the same link. Named, so the split tool reads
/// the core's copy and checks the engine header's against it.
#[unsafe(no_mangle)]
static LP_BUILD_ID: [u8; ENGINE_BUILD_ID_LEN] = lp_bootctl::engine_header::build_id_field(
    concat!(env!("LP_APP_VERSION"), "+", env!("LP_BUILD_COMMIT")).as_bytes(),
);

/// The engine digest slot: SHA-256 of this build's `engine.bin` exactly as
/// flashed, patched in by the packager (`lp_bootctl::engine_digest`). Named
/// so the split tool finds it, and a core root there.
#[unsafe(no_mangle)]
static LP_ENGINE_DIGEST: [u8; lp_bootctl::engine_digest::ENGINE_DIGEST_LEN] =
    lp_bootctl::engine_digest::ENGINE_DIGEST_UNPATCHED;

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

/// The S3 flash MMU's table: 512 words at `0x600C_5000`, entry `n` behind
/// both `0x3C00_0000 + n × 64 KiB` and `0x4200_0000 + n × 64 KiB`; bit 14
/// set means invalid, bits 13:0 the flash page. Read off the mask ROM's
/// `Cache_MMU_Init` / `Cache_*bus_MMU_Set` (lp-emu-esp32s3 `cache.rs`).
const MMU_TABLE: usize = 0x600C_5000;
const MMU_INVALID: u32 = 0x4000;
const MMU_PAGE_MASK: u32 = 0x3FFF;

unsafe extern "C" {
    fn Cache_Invalidate_DCache_All();
    fn Cache_Invalidate_ICache_All();
}

fn mmu_read(entry: u32) -> u32 {
    // SAFETY: a read of one MMU table word.
    unsafe { core::ptr::read_volatile((MMU_TABLE + 4 * entry as usize) as *const u32) }
}

fn mmu_write(entry: u32, value: u32) {
    // SAFETY: an entry nothing running is mapped through (the engine's and
    // the core's scratch entries, never the core's own).
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

/// The digest the core carries, read volatile (the packager patched it).
fn engine_digest() -> [u8; 32] {
    // SAFETY: a plain read of a static the packager patched in flash.
    let slot = unsafe { core::ptr::read_volatile(&raw const LP_ENGINE_DIGEST) };
    lp_bootctl::engine_digest::decode(&slot).unwrap_or([0; 32])
}

/// Where the running core is: the flash page behind MMU entry 0, which the
/// loader mapped to the core's first page (its app descriptor).
fn running_core_offset() -> Option<u32> {
    let e = mmu_read(0);
    (e & MMU_INVALID == 0).then_some((e & MMU_PAGE_MASK) * MMU_PAGE)
}

/// Both boot-record sectors, read through the cache at the core's scratch
/// entry — never through the ROM's SPI1 routines.
fn read_records() -> [Option<BootSlot>; 2] {
    let page = BOOT_RECORD_SECTORS[0] / MMU_PAGE * MMU_PAGE;
    let entry = CORE_SCRATCH.start;
    mmu_write(entry, page / MMU_PAGE);
    invalidate_caches();
    let base = drom(entry);
    let mut slots = [None; 2];
    for (slot, at) in slots.iter_mut().zip(BOOT_RECORD_SECTORS) {
        let mut buf = [0u8; BOOT_RECORD_READ_LEN];
        for (i, b) in buf.iter_mut().enumerate() {
            // SAFETY: the scratch entry maps the records' page.
            *b = unsafe { core::ptr::read_volatile((base + (at - page) + i as u32) as *const u8) };
        }
        *slot = BootSlot::decode(&buf);
    }
    mmu_write(entry, MMU_INVALID);
    slots
}

/// Map `len` bytes of flash from `start` (page-aligned) at the engine's
/// entries.
fn map_engine(start: u32, len: u32) {
    for k in 0..len.div_ceil(MMU_PAGE) {
        mmu_write(ENGINE_MMU_ENTRY + k, start / MMU_PAGE + k);
    }
    invalidate_caches();
}

/// Why the core does not enter an engine.
enum NoEngine {
    NoFactory,
    NotMapped,
    NoRecord,
    NoRoom,
    Header(lp_bootctl::EngineHeaderError),
}

impl NoEngine {
    fn describe(&self) -> &'static str {
        match self {
            Self::NoFactory => "no `factory` partition in the flashed table",
            Self::NotMapped => "MMU entry 0 is not mapped",
            Self::NoRecord => "no boot record names the running core",
            Self::NoRoom => "no room for an engine",
            Self::Header(e) => e.describe(),
        }
    }
}

/// The engine's entry, where it is and its length, if a committed engine
/// of this build is in the room layout 2 leaves the running core.
fn find_engine(factory: Option<(u32, u32)>) -> Result<(fn(CoreBoot), u32, u32), NoEngine> {
    let (offset, len) = factory.ok_or(NoEngine::NoFactory)?;
    let layout = SplitLayout::layout_2(offset, len).ok_or(NoEngine::NoFactory)?;
    let core_off = running_core_offset().ok_or(NoEngine::NotMapped)?;
    let records = read_records();
    let record = records
        .iter()
        .flatten()
        .find(|s| s.record.core_off == core_off)
        .ok_or(NoEngine::NoRecord)?;
    let room = layout.engine_room(core_off, record.record.core_len);
    if room.is_empty() {
        return Err(NoEngine::NoRoom);
    }
    map_engine(room.start, ENGINE_HEADER_LEN as u32);
    let mut bytes = [0u8; ENGINE_HEADER_LEN];
    for (i, b) in bytes.iter_mut().enumerate() {
        // SAFETY: the header's page is mapped (erased flash reads as 0xff).
        *b = unsafe { core::ptr::read_volatile((ENGINE_DROM as *const u8).add(i)) };
    }
    let header = lp_bootctl::EngineHeader::decode(&bytes).map_err(NoEngine::Header)?;
    header
        .validate(&LP_BUILD_ID, room.len())
        .map_err(NoEngine::Header)?;
    map_engine(room.start, header.len);
    // SAFETY: a committed header of this build, from this link: its entry is
    // `lp_engine_entry`'s address in the engine now mapped behind it.
    let entry: fn(CoreBoot) = unsafe { core::mem::transmute(header.entry as usize) };
    Ok((entry, room.start, header.len))
}

/// After `core_boot`: enter the engine the records and the header agree on,
/// or stay core-only — the link up and the watchdog fed, nothing served.
pub fn split_boot(core: CoreBoot) {
    let digest = engine_digest();
    match find_engine(core.factory) {
        Ok((entry, at, len)) => {
            log::info!(
                "[CORE] engine @{at:#x} ({len} B, digest {:02x}{:02x}{:02x}{:02x}), entering",
                digest[0],
                digest[1],
                digest[2],
                digest[3]
            );
            // The radio links serve the engine's server (the link mux). In
            // the same synchronous run as `core_boot`, so before the BLE task
            // has run at all; a connection that subscribes waits for it.
            #[cfg(s3_ble)]
            core.radio_port
                .decide_mode(fw_esp32_common::radio_link::RadioLinkMode::Serve);
            entry(core)
        }
        Err(why) => {
            // Core-only serves nothing over the air yet (the update session
            // on the radio links is the S3 updates plan's): the radio links'
            // mode stays undecided, so a Bluetooth central can connect and
            // its link never opens. USB is the way in, as before.
            log::error!("[CORE] no engine: {} — core-only", why.describe());
            let CoreBoot {
                spawner, watchdog, ..
            } = core;
            spawner.spawn(core_only_task(watchdog).unwrap());
        }
    }
}

/// Core-only: keep the watchdog fed while the link (its own thread) stays
/// up, so a board with no engine it may enter is still reachable over USB.
#[embassy_executor::task]
async fn core_only_task(mut watchdog: crate::recovery::watchdog::WatchdogFeeder) {
    loop {
        let now_ms = embassy_time::Instant::now().as_millis();
        watchdog.feed(now_ms);
        embassy_time::Timer::after(embassy_time::Duration::from_millis(500)).await;
    }
}
