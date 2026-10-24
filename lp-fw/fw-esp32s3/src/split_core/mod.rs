//! The S3's side of the split image's update board side
//! (`fw_esp32_common::split_core`, `LP_SPLIT_LINK=1` builds): [`S3Split`],
//! the chip the shared code is generic over — layout 2 on 64 KiB pages, the
//! S3's flash MMU ([`engine_window`], `lp_bootctl::s3_mmu`'s plan),
//! esp-storage's low-level ROM flash calls, the SHA accelerator
//! ([`split_sha`]), the S3 reset table (`ResetKind::from_s3_reason`), its
//! watchdog and its update light ([`StatusLight`]). The C6's twin is
//! `fw-esp32c6/src/split_core/`.
//!
//! The boot sequence, the silicon rules and the update session are the
//! shared module's; this is only what names the S3.
//!
//! **Octal flash takes no update** (the S3 plan's Q7, A7): a part whose eFuse
//! says octal flash forbids re-programming a word in place, which every boot
//! record mark does, so [`S3Split::updates_off`] says so — the board
//! announces layout 0, writes nothing, and runs its engine as a plain image.

mod engine_window;
mod split_sha;
mod status_light;

use fw_esp32_common::split_core::{FeedWatchdog, SplitChip, SplitStatics};
use lp_bootctl::s3_mmu::{ENGINE_DROM, MMU_PAGE};
use lp_bootctl::{ResetKind, SplitLayout};

pub use status_light::StatusLight;

/// The S3, as the split core sees it.
pub struct S3Split;

static SPLIT: SplitStatics<S3Split> = SplitStatics::new();

impl SplitChip for S3Split {
    const LAYOUT_CODE: u16 = lpc_update::code_table::LAYOUT_2;
    const REGION_START: u32 = lp_bootctl::LAYOUT_2_REGION_START;
    /// The end of an 8 MB part.
    const FALLBACK_REGION_END: u32 = 0x80_0000;
    const ENGINE_DATA_VADDR: usize = ENGINE_DROM as usize;
    /// The S3 mounts the tree store too (the S3 plan's P9): a core that does
    /// not carry its mark could format it.
    const CORES_MUST_READ_TREE_STORE: bool = true;

    fn updates_off() -> Option<&'static str> {
        esp_hal::efuse::read_bit(esp_hal::efuse::FLASH_TYPE).then_some(
            "octal flash: a boot record's marks re-program words in place, which this part forbids",
        )
    }

    fn page_size() -> u32 {
        MMU_PAGE
    }

    fn layout(offset: u32, len: u32) -> Option<SplitLayout> {
        SplitLayout::layout_2(offset, len)
    }

    fn map_engine(start: u32, len: u32) {
        engine_window::map_engine(start, len);
    }

    fn running_core_offset() -> u32 {
        engine_window::running_core_offset()
    }

    type Scratch = engine_window::ScratchWindow;

    unsafe fn flash_read(at: u32, dst: *mut u32, len: u32) -> bool {
        // SAFETY: the caller's word-aligned buffer of `len` bytes.
        parked(|| unsafe { esp_storage::ll::spiflash_read(at, dst, len) }.is_ok())
    }

    unsafe fn flash_program(at: u32, src: *const u32, len: u32) -> bool {
        // SAFETY: the caller's word-aligned buffer, a span its fence allowed.
        parked(|| unsafe { esp_storage::ll::spiflash_write(at, src, len) }.is_ok())
    }

    unsafe fn flash_erase_sector(index: u32) -> bool {
        // SAFETY: a sector the caller's fence allowed.
        parked(|| unsafe { esp_storage::ll::spiflash_erase_sector(index) }.is_ok())
    }

    unsafe fn flash_erase_block(index: u32) -> bool {
        // SAFETY: a block the caller's fence allowed.
        parked(|| unsafe { esp_storage::ll::spiflash_erase_block(index) }.is_ok())
    }

    unsafe fn flash_unlock() -> bool {
        // SAFETY: the ROM's write-protect clear.
        parked(|| unsafe { esp_storage::ll::spiflash_unlock() }.is_ok())
    }

    type Sha = split_sha::BootSha256;

    fn reset_kind() -> ResetKind {
        unsafe extern "C" {
            fn rtc_get_reset_reason(cpu: u32) -> u32;
        }
        // SAFETY: a ROM routine reading a status register (the loader's
        // `rom::reset_reason`, the same call).
        ResetKind::from_s3_reason(unsafe { rtc_get_reset_reason(0) })
    }

    /// `esp_hal`'s software reset, which the loader reads as warm (the
    /// board's own reboot, `crate::reset_now`).
    fn reset_now() -> ! {
        crate::reset_now()
    }

    type Watchdog = crate::recovery::watchdog::WatchdogFeeder;
    type Light = StatusLight;

    fn heap_free() -> usize {
        esp_alloc::HEAP.free()
    }

    fn log_stack_if_grown(what: &'static str) {
        crate::stack_probe::log_if_grown(what);
    }

    /// The S3 has no first-light gate: its frame hook takes the core's hash
    /// a slice a frame from the first frame, and a host waiting on its
    /// `Hello` has the rest taken at once.
    fn first_light_decided() -> bool {
        true
    }

    fn statics() -> &'static SplitStatics<Self> {
        &SPLIT
    }
}

impl FeedWatchdog for crate::recovery::watchdog::WatchdogFeeder {
    fn feed(&mut self, now_ms: u64) {
        crate::recovery::watchdog::WatchdogFeeder::feed(self, now_ms);
    }
}

/// Run a raw ROM flash call with core 1 stalled (`two-core`): these bypass
/// esp-storage's `FlashStorage`, whose writes park core 1 themselves, and a
/// core-1 cache refill from flash during an erase or program hangs the chip.
/// Reads too: SPI1 owns the flash for the call.
#[inline(always)]
fn parked<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(s3_two_core)]
    return crate::core1::with_core1_parked(f);
    #[cfg(not(s3_two_core))]
    f()
}
