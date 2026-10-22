//! One chip's side of the split image: what [`super`] cannot do without
//! naming a chip — its flash MMU, its raw flash, its SHA, its reset, and the
//! board pieces core-only drives.
//!
//! Associated functions and types only, never `self`: an image has one chip,
//! so every generic over a [`SplitChip`] is instantiated once, and under the
//! firmware's release profile (fat LTO, one codegen unit) each call is the
//! direct call it was when this code lived in the chip crate. The two
//! statics that hold chip types are the chip's ([`SplitChip::statics`]).
//!
//! The C6's is `fw-esp32c6/src/split_core/`; the S3's, `fw-esp32s3`'s.

use lp_bootctl::{ResetKind, SplitLayout};
use lpc_update::BoardState;

use super::split_statics::SplitStatics;

pub trait SplitChip: Sized + 'static {
    /// The layout code the board manifest announces
    /// (`lpc_update::code_table`).
    const LAYOUT_CODE: u16;
    /// The region's first byte, and the low end's core offset, in this
    /// chip's layout.
    const REGION_START: u32;
    /// Bounds an engine header's length when the flashed table could not be
    /// read (nothing is written in that state): the end of the chip's flash.
    const FALLBACK_REGION_END: u32;
    /// Where the engine's first byte reads through the data view (the
    /// header, the engine guard's hash).
    const ENGINE_DATA_VADDR: usize;
    /// Every core installed over the air must carry the tree-store mark
    /// (`crate::fs_tree_core_guard`): this board's files are in a tree store.
    const CORES_MUST_READ_TREE_STORE: bool;

    /// The flash MMU's page, as this boot runs it.
    fn page_size() -> u32;
    /// This chip's layout over a `factory` partition at `offset`, `len`
    /// bytes long, or `None` when `factory` cannot hold it.
    fn layout(offset: u32, len: u32) -> Option<SplitLayout>;
    /// Map `len` bytes of flash from `start` (page-aligned) behind the
    /// engine's views.
    fn map_engine(start: u32, len: u32);
    /// The flash offset of the page behind MMU entry 0: where the running
    /// core really is.
    fn running_core_offset() -> u32;
    /// A one-off read window over MMU entries nothing else uses.
    type Scratch: ScratchWindow;

    /// Read `len` bytes (a multiple of 4) at `at` (word-aligned) into `dst`
    /// (word-aligned). The only flash path besides the one `FlashStorage`:
    /// never a second one (the split image ADR's §7).
    ///
    /// # Safety
    /// `dst` is valid for `len` bytes of writes.
    unsafe fn flash_read(at: u32, dst: *mut u32, len: u32) -> bool;
    /// Program `len` bytes (a multiple of 4) at `at` (word-aligned) from
    /// `src`. Unfenced: [`super::split_flash`] is the fence.
    ///
    /// # Safety
    /// `src` is valid for `len` bytes of reads.
    unsafe fn flash_program(at: u32, src: *const u32, len: u32) -> bool;
    /// Erase 4 KiB sector number `index`. Unfenced.
    ///
    /// # Safety
    /// The caller's fence allowed it.
    unsafe fn flash_erase_sector(index: u32) -> bool;
    /// Erase 64 KiB block number `index`. Unfenced.
    ///
    /// # Safety
    /// The caller's fence allowed it.
    unsafe fn flash_erase_block(index: u32) -> bool;
    /// Clear the part's write protection.
    ///
    /// # Safety
    /// Only before a fenced write.
    unsafe fn flash_unlock() -> bool;

    /// SHA-256 as the chip does it best (its accelerator, when free).
    type Sha: Sha256Stream;
    /// This boot's reset, classified as the loader classified it.
    fn reset_kind() -> ResetKind;
    /// The chip reset an update ends with: a software reset (warm), with the
    /// radio quiet first.
    fn reset_now() -> !;
    /// The watchdog core-only keeps fed.
    type Watchdog: FeedWatchdog;
    /// The update light core-only drives.
    type Light: UpdateLight;
    /// Free heap, for core-only's log lines.
    fn heap_free() -> usize;
    /// Log the main stack's high-water mark if it grew (`what` names the
    /// moment).
    fn log_stack_if_grown(what: &'static str);
    /// The boot has lit its first frame, or given up lighting.
    fn first_light_decided() -> bool;
    /// The chip's one [`SplitStatics`].
    fn statics() -> &'static SplitStatics<Self>;
}

/// Flash mapped through the cache into MMU entries nothing else uses,
/// unmapped again when dropped.
pub trait ScratchWindow: Sized {
    /// Map `len` bytes of flash from `start` (any alignment), or `None` when
    /// they would need an entry already in use.
    fn map(start: u32, len: u32) -> Option<Self>;
    /// The mapped bytes, read through the cache.
    fn bytes(&self) -> &[u8];
}

/// A SHA-256 in progress.
pub trait Sha256Stream: Sized {
    fn new() -> Self;
    fn update(&mut self, data: &[u8]);
    fn finalize(self) -> [u8; 32];
}

/// The chip's watchdog, as core-only feeds it.
pub trait FeedWatchdog {
    fn feed(&mut self, now_ms: u64);
}

/// The update light: the strip the engine recorded, lit for a board state.
pub trait UpdateLight {
    /// Light the strip for `state`, if that changed what it shows.
    fn show(&mut self, state: BoardState);
}
