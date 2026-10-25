//! The C6's side of the split image's update board side
//! (`fw_esp32_common::split_core`, `LP_SPLIT_LINK=1` builds): [`C6Split`],
//! the chip the shared code is generic over — the C6's flash MMU
//! ([`engine_window`]), its ROM flash calls (esp-storage's low-level ones),
//! its SHA accelerator (`crate::hw_sha`), its reset, its watchdog and its
//! update light ([`StatusLight`]). Layout 1, 32 KiB pages.
//!
//! The boot sequence, the silicon rules and the update session are the
//! shared module's; this is only what names the C6.

mod engine_window;
mod status_light;

use fw_esp32_common::split_core::{FeedWatchdog, Sha256Stream, SplitChip, SplitStatics};
use lp_bootctl::{ResetKind, SplitLayout};

pub use engine_window::ENGINE_VADDR;
pub use status_light::StatusLight;

/// The C6, as the split core sees it.
pub struct C6Split;

static SPLIT: SplitStatics<C6Split> = SplitStatics::new();

impl SplitChip for C6Split {
    const LAYOUT_CODE: u16 = lpc_update::code_table::LAYOUT_1;
    const REGION_START: u32 = lp_bootctl::REGION_START;
    /// The end of a 4 MB chip.
    const FALLBACK_REGION_END: u32 = 0x40_0000;
    const ENGINE_DATA_VADDR: usize = ENGINE_VADDR;
    /// This board's files are in a tree store (or held in littlefs for the
    /// USB migration, which no over-the-air core may format either).
    const CORES_MUST_READ_TREE_STORE: bool = true;

    fn page_size() -> u32 {
        engine_window::page_size()
    }

    fn layout(offset: u32, len: u32) -> Option<SplitLayout> {
        SplitLayout::from_factory(offset, len, engine_window::page_size())
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
        unsafe { esp_storage::ll::spiflash_read(at, dst, len) }.is_ok()
    }

    unsafe fn flash_program(at: u32, src: *const u32, len: u32) -> bool {
        // SAFETY: the caller's word-aligned buffer, a span its fence allowed.
        unsafe { esp_storage::ll::spiflash_write(at, src, len) }.is_ok()
    }

    unsafe fn flash_erase_sector(index: u32) -> bool {
        // SAFETY: a sector the caller's fence allowed.
        unsafe { esp_storage::ll::spiflash_erase_sector(index) }.is_ok()
    }

    unsafe fn flash_erase_block(index: u32) -> bool {
        // SAFETY: a block the caller's fence allowed.
        unsafe { esp_storage::ll::spiflash_erase_block(index) }.is_ok()
    }

    unsafe fn flash_unlock() -> bool {
        // SAFETY: the ROM's write-protect clear.
        unsafe { esp_storage::ll::spiflash_unlock() }.is_ok()
    }

    type Sha = crate::hw_sha::BootSha256;

    fn reset_kind() -> ResetKind {
        unsafe extern "C" {
            fn rtc_get_reset_reason(cpu: u32) -> u32;
        }
        // SAFETY: a ROM routine reading a status register.
        ResetKind::from_c6_reason(unsafe { rtc_get_reset_reason(0) })
    }

    /// `esp_hal`'s software reset, which the loader reads as warm, with the
    /// radio quiet first like every other reset the firmware makes
    /// (`restart`): an update over Bluetooth, the LAN or the relay ends with
    /// the radio busy, and the loader then runs in RAM the app's heap held
    /// (docs/defects/2026-10-17-radio-heap-overflow-reached-the-split-loaders-ram.md).
    fn reset_now() -> ! {
        crate::board::esp32c6::restart::restart()
    }

    type Watchdog = crate::recovery::watchdog::WatchdogFeeder;
    type Light = StatusLight;

    fn heap_free() -> usize {
        esp_alloc::HEAP.free()
    }

    fn log_stack_if_grown(what: &'static str) {
        crate::stack_probe::log_if_grown(what);
    }

    fn first_light_decided() -> bool {
        crate::first_light::decided()
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

impl Sha256Stream for crate::hw_sha::BootSha256 {
    fn new() -> Self {
        crate::hw_sha::BootSha256::new()
    }

    fn update(&mut self, data: &[u8]) {
        crate::hw_sha::BootSha256::update(self, data);
    }

    fn finalize(self) -> [u8; 32] {
        crate::hw_sha::BootSha256::finalize(self)
    }
}
