//! OTA split-link spike (S3): the core's update path. Spike code, not product.
//!
//! Flash layout (`partitions.csv` on the spike branch):
//!
//! ```text
//! ota_0  0x010000  1.1875 MiB   core slot A
//! ota_1  0x140000  1.1875 MiB   core slot B
//! engine 0x270000  640 KiB      the engine's tail
//! lpfs   0x310000  960 KiB      unchanged
//! ```
//!
//! The engine lives in whichever core slot is NOT running, followed by the
//! tail, and the core stitches those pages into one contiguous window at
//! `ENGINE_VADDR`. Updating the core therefore destroys the engine first, by
//! construction; the new core then fetches a matching engine into the old
//! core's slot.
//!
//! The update channel (lp-link channel 3) speaks the host's `OtaServe` words:
//! `O` offer, `Q` query, `R` request, `D` data (see `lp-cli`'s `link_host.rs`).

use alloc::boxed::Box;
use alloc::vec::Vec;

use embedded_storage::nor_flash::NorFlash;
use esp_bootloader_esp_idf::partitions::{AppPartitionSubType, PARTITION_TABLE_MAX_LEN};
use fw_esp32_common::usb_link::UsbLinkShared;
use lpc_wire::lp_link::{CH_UPDATE, LinkEvent};

pub const SLOT_A: u32 = 0x01_0000;
pub const SLOT_B: u32 = 0x14_0000;
pub const SLOT_LEN: u32 = 0x13_0000;
pub const TAIL: u32 = 0x27_0000;
pub const TAIL_LEN: u32 = 0x0A_0000;
pub const ENGINE_VADDR: usize = 0x4240_0000;
pub const ENGINE_MAX_BYTES: u32 = SLOT_LEN + TAIL_LEN;

const SECTOR: u32 = 4096;
const SPI0: usize = 0x6000_2000;
const MMU_ITEM_CONTENT: usize = SPI0 + 0x37c;
const MMU_ITEM_INDEX: usize = SPI0 + 0x380;
const MMU_POWER_CTRL: usize = SPI0 + 0x384;
const MMU_VALID: u32 = 1 << 9;
const MMU_PAGE_MASK: u32 = MMU_VALID - 1;

// ---------------------------------------------------------------------------
// Where things are
// ---------------------------------------------------------------------------

/// log2 of the MMU page size the bootloader chose (`mmu_power_ctrl[4:3]`).
fn page_shift() -> u32 {
    // SAFETY: a read of an SPI0 register.
    16 - ((unsafe { core::ptr::read_volatile(MMU_POWER_CTRL as *const u32) } >> 3) & 3)
}

/// The slot this core runs from: the flash page behind `0x4200_0000`.
pub fn running_slot() -> u32 {
    // SAFETY: selects and reads MMU entry 0, which the bootloader set.
    let entry = unsafe {
        core::ptr::write_volatile(MMU_ITEM_INDEX as *mut u32, 0);
        core::ptr::read_volatile(MMU_ITEM_CONTENT as *const u32)
    };
    let phys = (entry & MMU_PAGE_MASK) << page_shift();
    if (SLOT_B..TAIL).contains(&phys) { SLOT_B } else { SLOT_A }
}

pub fn inactive_slot() -> u32 {
    if running_slot() == SLOT_A { SLOT_B } else { SLOT_A }
}

/// Engine byte offset → flash address: the inactive slot, then the tail.
pub fn engine_phys(inactive: u32, off: u32) -> u32 {
    if off < SLOT_LEN { inactive + off } else { TAIL + (off - SLOT_LEN) }
}

/// Map the engine window: one MMU entry per page, stitched across the
/// inactive slot and the tail, at the page size the bootloader chose.
pub fn map_engine() {
    let shift = page_shift();
    let inactive = inactive_slot();
    let first_entry = ((ENGINE_VADDR - 0x4200_0000) >> shift) as u32;
    for k in 0..(ENGINE_MAX_BYTES >> shift) {
        let phys = engine_phys(inactive, k << shift);
        // SAFETY: entries of the engine window only, which nothing has
        // touched; the code doing it runs from the core's own pages.
        unsafe {
            core::ptr::write_volatile(MMU_ITEM_INDEX as *mut u32, first_entry + k);
            core::ptr::write_volatile(MMU_ITEM_CONTENT as *mut u32, (phys >> shift) | MMU_VALID);
        }
    }
}

// ---------------------------------------------------------------------------
// Resets, and "update on the next boot"
// ---------------------------------------------------------------------------

/// A system reset, the way the ROM's `software_reset` does it: set
/// `LP_AON.sys_cfg.hpsys_sw_reset` (bit 31 of `0x600B_1034`). On silicon the
/// chip is gone before the next instruction. The spin is for the emulator,
/// which does not act on that bit yet
/// (`docs/defects/2026-09-29-the-emulated-c6-does-not-perform-a-software-reset.md`)
/// and so reboots on the RTC watchdog a few seconds later — instead of
/// falling out of a `-> !` call into whatever code follows it.
pub fn reset() -> ! {
    const LP_AON_SYS_CFG: usize = 0x600B_1034;
    // SAFETY: the reset request register; nothing runs after it on silicon.
    unsafe {
        let v = core::ptr::read_volatile(LP_AON_SYS_CFG as *const u32);
        core::ptr::write_volatile(LP_AON_SYS_CFG as *mut u32, v | 1 << 31);
    }
    loop {
        core::hint::spin_loop();
    }
}

/// Installed into the engine's link transport: an offer of a different build
/// while the engine runs erases the engine's header sector and resets. The
/// flash itself is the "update pending" state — no flag, nothing in RTC RAM,
/// and a cut right after the erase still boots into core-only. (Called from
/// engine code; lives in core, which is fine — the engine may call core.)
pub fn on_update_while_running(data: &[u8]) {
    if let Some(offer) = Offer::parse(data)
        && offer.build_id != crate::build_id()
    {
        // SAFETY: called from the server loop between frames, so lpfs (the
        // other owner of the SPI flash) is not mid-operation.
        let mut flash =
            esp_storage::FlashStorage::new(unsafe { esp_hal::peripherals::FLASH::steal() });
        erase(&mut flash, engine_phys(inactive_slot(), 0));
        esp_println::println!("[OTA] offer of a different build — engine erased, resetting into core-only");
        reset();
    }
}

// ---------------------------------------------------------------------------
// The core-only loop
// ---------------------------------------------------------------------------

struct Offer {
    core_len: u32,
    engine_len: u32,
    build_id: [u8; 48],
}

impl Offer {
    fn parse(m: &[u8]) -> Option<Self> {
        if m.len() != 57 || m[0] != b'O' {
            return None;
        }
        let mut build_id = [0u8; 48];
        build_id.copy_from_slice(&m[9..57]);
        Some(Self {
            core_len: u32::from_le_bytes(m[1..5].try_into().ok()?),
            engine_len: u32::from_le_bytes(m[5..9].try_into().ok()?),
            build_id,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Plan {
    Idle,
    /// Writing the offered core into the inactive slot, front to back.
    Core { len: u32, next: u32 },
    /// Writing the engine into the inactive slot + tail: sector 1 onward,
    /// then sector 0 (the header) LAST, so a cut never leaves a valid magic
    /// over a partial engine.
    Engine { len: u32, next: u32 },
}

#[repr(C, align(4))]
struct Sector([u8; SECTOR as usize]);

/// The core-only loop: keep the board reachable, take an offer, and do what
/// it asks. Never returns; every completed step ends in a reset.
pub async fn core_only(
    usb_link: &'static UsbLinkShared,
    mut watchdog: crate::recovery::watchdog::WatchdogFeeder,
) -> ! {
    // SAFETY: the engine never ran this boot, so nothing else drives the SPI
    // flash; lpfs (mounted on the real peripheral) is idle in core-only mode.
    let mut flash = esp_storage::FlashStorage::new(unsafe { esp_hal::peripherals::FLASH::steal() });
    let mut buf = Box::new(Sector([0xff; SECTOR as usize]));
    let inactive = inactive_slot();
    let mut plan = Plan::Idle;
    let mut queried = false;
    // Core-only is a complete boot: the recovery ladder must not count the
    // update's own resets as a crash loop.
    lp_recovery::mark_boot_complete();
    esp_println::println!("[OTA] core-only: running slot {:#x}", running_slot());
    loop {
        watchdog.feed(embassy_time::Instant::now().as_millis());
        if !queried && usb_link.is_established() {
            queried = send(usb_link, &[b'Q']);
        }
        while let Some(event) = usb_link.with_link(|link| link.recv()) {
            match event {
                LinkEvent::Up { .. } => queried = false,
                LinkEvent::Message { channel, data } if channel == CH_UPDATE => {
                    plan = step(plan, &data, inactive, &mut flash, &mut buf, usb_link);
                }
                _ => {}
            }
        }
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}

fn step(
    plan: Plan,
    msg: &[u8],
    inactive: u32,
    flash: &mut esp_storage::FlashStorage<'_>,
    buf: &mut Sector,
    usb_link: &'static UsbLinkShared,
) -> Plan {
    if let Some(offer) = Offer::parse(msg) {
        if plan != Plan::Idle {
            return plan; // a re-offer mid-transfer (link re-up): carry on
        }
        if offer.build_id != crate::build_id() {
            // The engine dies first: its header sector is the first sector
            // the new core overwrites anyway, but erasing it now means no cut
            // from here on can leave this core starting an engine whose core
            // is half-replaced.
            erase(flash, inactive);
            esp_println::println!("[OTA] new build offered: core {} B — engine erased", offer.core_len);
            request(usb_link, b'C', 0, offer.core_len);
            return Plan::Core { len: offer.core_len, next: 0 };
        }
        // Same build, and core-only means there is no valid engine.
        erase(flash, engine_phys(inactive, 0));
        esp_println::println!("[OTA] same build, no engine: fetching {} B", offer.engine_len);
        let first = if offer.engine_len > SECTOR { SECTOR } else { 0 };
        request(usb_link, b'E', first, offer.engine_len);
        return Plan::Engine { len: offer.engine_len, next: first };
    }
    if msg.len() < 6 || msg[0] != b'D' {
        return plan;
    }
    let kind = msg[1];
    let off = u32::from_le_bytes(msg[2..6].try_into().unwrap_or_default());
    let data = &msg[6..];
    match plan {
        Plan::Core { len, next } if kind == b'C' && off == next => {
            write_sector(flash, buf, inactive + off, data);
            let next = off + data.len() as u32;
            if next < len {
                request(usb_link, b'C', next, len);
                return Plan::Core { len, next };
            }
            activate(flash, inactive);
            esp_println::println!("[OTA] core written ({len} B) and activated — rebooting into it");
            reset();
        }
        Plan::Engine { len, next } if kind == b'E' && off == next => {
            write_sector(flash, buf, engine_phys(inactive, off), data);
            if off == 0 {
                esp_println::println!("[OTA] engine written ({len} B), header last — rebooting");
                reset();
            }
            let after = off + data.len() as u32;
            let next = if after < len { after } else { 0 };
            request(usb_link, b'E', next, len);
            Plan::Engine { len, next }
        }
        _ => plan,
    }
}

fn request(usb_link: &'static UsbLinkShared, kind: u8, off: u32, total: u32) {
    let len = SECTOR.min(total - off);
    let mut m = Vec::with_capacity(10);
    m.extend_from_slice(&[b'R', kind]);
    m.extend_from_slice(&off.to_le_bytes());
    m.extend_from_slice(&len.to_le_bytes());
    send(usb_link, &m);
}

fn send(usb_link: &'static UsbLinkShared, m: &[u8]) -> bool {
    let ok = usb_link.with_link(|link| link.send(CH_UPDATE, m).is_ok());
    usb_link.ring();
    ok
}

fn erase(flash: &mut esp_storage::FlashStorage<'_>, at: u32) {
    if let Err(e) = flash.erase(at, at + SECTOR) {
        esp_println::println!("[OTA] erase {at:#x} failed: {e:?}");
    }
}

fn write_sector(flash: &mut esp_storage::FlashStorage<'_>, buf: &mut Sector, at: u32, data: &[u8]) {
    erase(flash, at);
    buf.0.fill(0xff);
    buf.0[..data.len()].copy_from_slice(data);
    let len = (data.len() + 3) & !3;
    if let Err(e) = flash.write(at, &buf.0[..len]) {
        esp_println::println!("[OTA] write {at:#x} failed: {e:?}");
    }
}

/// Select the slot just written in `otadata` (two sectors, alternating, with
/// a CRC each: a cut mid-write leaves the previous selection valid).
fn activate(flash: &mut esp_storage::FlashStorage<'_>, slot: u32) {
    let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
    let mut updater = match esp_bootloader_esp_idf::ota_updater::OtaUpdater::new(flash, &mut table) {
        Ok(u) => u,
        Err(e) => {
            esp_println::println!("[OTA] no OTA partitions: {e:?}");
            return;
        }
    };
    let target = if slot == SLOT_B { AppPartitionSubType::Ota1 } else { AppPartitionSubType::Ota0 };
    let result = updater.ota_data().and_then(|mut ota| {
        // Blank otadata reads as "factory", which this table does not have;
        // the crate's sequence arithmetic from there is fragile, so select
        // ota_0 (what the bootloader booted) explicitly first.
        if ota.current_app_partition()? == AppPartitionSubType::Factory {
            ota.set_current_app_partition(AppPartitionSubType::Ota0)?;
        }
        ota.set_current_app_partition(target)
    });
    if let Err(e) = result {
        esp_println::println!("[OTA] otadata write failed: {e:?}");
    }
}
