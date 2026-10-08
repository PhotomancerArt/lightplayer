//! The `flash-tears` payload: what a real NOR part looks like after the power
//! goes out in the middle of an erase or a program.
//!
//! Every power-cut test the storage work has run so far trusts tear models
//! somebody guessed (`lp-nor-sim`'s `Clean`, `BytePrefix`, `RandomBits`, and
//! its three torn-erase shapes). This payload measures real ones, on a board
//! allocated to be destroyed, so that the models can be checked and corrected.
//!
//! # What runs on the board
//!
//! A test region at the start of the `lpfs` partition (it destroys the
//! board's filesystem; the build feature `test_flash_tears` and the desk
//! driver's tag check are the only gates — DD6 of the M4 brief):
//!
//! ```text
//! lpfs + 0      journal copy 0   (256 slots of 16 B)
//! lpfs + 4 KiB  journal copy 1
//! lpfs + 8 KiB  region sector 0
//! …             …
//! lpfs + 72 KiB region sector 15
//! ```
//!
//! **The work loop** runs forever and logs nothing: for cycle `c` it writes
//! `c` to both journal copies ([`journal`]), erases region sector
//! `c % REGION_SECTORS`, and programs it page by page with
//! [`pattern::fill_pattern`]. The cut is the point, so nothing is printed
//! while it runs.
//!
//! **The boot scan** runs on every boot, before any write: both journal copies
//! name the cycle that was in flight, every region sector is read
//! [`SCAN_READS`] times, and each is classified against what it should hold
//! ([`analysis`]). Then the in-flight sector is repaired, one timed cycle runs,
//! and [`SCAN_DONE_MARKER`] says the host may cut again.
//!
//! # Why the patterns alternate
//!
//! A sector's pattern in one generation is the bitwise complement of its
//! pattern in the previous one ([`pattern`]). So the bits the old content had
//! at 0 and the bits the new content wants at 0 are disjoint, and every stable
//! 0 bit in a torn sector says which operation put it there: an old 0 not yet
//! erased, or a new 0 programmed. A sector with both is neither model's shape
//! and is reported as `mixed` rather than forced into one.
//!
//! Everything here is arithmetic over bytes and runs on the host under test,
//! against a small fake part with cuts (`runner`'s tests). What stays in
//! `fw-esp32c6` is the flash driver, the clock, the USB link and the reset
//! reason.

pub mod analysis;
pub mod journal;
pub mod layout;
pub mod pattern;
pub mod records;
pub mod runner;

/// The payload's name, as the header and the registries spell it.
pub const PAYLOAD: &str = "flash-tears";

/// The line that ends one boot's scan: the host may cut power after it.
///
/// It is a readiness line, not a done marker — the payload never finishes,
/// it goes back to its work loop — so the `fw-checks` registry declares no
/// `done_marker` and the host registry carries it as `Sentinel::Ready`.
pub const SCAN_DONE_MARKER: &str = "[flash-tears] === SCAN DONE ===";

/// Printed every [`READY_PERIOD_MS`] until the host sends any byte.
///
/// After a power cut the board boots before the host has re-opened the port,
/// and a USB-Serial-JTAG write nobody reads is dropped; so the scan waits for
/// the host to say it is listening.
pub const READY_LINE: &str = "[flash-tears] READY send any byte to start the scan";

/// How often [`READY_LINE`] is repeated while nobody has answered.
pub const READY_PERIOD_MS: u64 = 250;

/// Bytes in one erase sector of the part.
pub const SECTOR_SIZE: usize = 4096;

/// Bytes in one program page (the NOR page; a longer program is several).
pub const PAGE_SIZE: usize = 256;

/// Program pages in a sector.
pub const PAGES_PER_SECTOR: usize = SECTOR_SIZE / PAGE_SIZE;

/// Sectors the work loop cycles through.
pub const REGION_SECTORS: u32 = 16;

/// Copies of the journal (each its own sector).
pub const JOURNAL_COPIES: u32 = 2;

/// Sectors the payload owns at the start of `lpfs`.
pub const LAYOUT_SECTORS: u32 = JOURNAL_COPIES + REGION_SECTORS;

/// Reads of each region sector per scan. A bit that does not read the same
/// all eight times is **weak**.
pub const SCAN_READS: usize = 8;
