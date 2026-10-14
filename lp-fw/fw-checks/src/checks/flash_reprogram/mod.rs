//! The `flash-reprogram` payload: does programming a torn word **again**,
//! with the value it was meant to hold, make its weak bits read solid?
//!
//! The tree store's mount settles each write head's tail by programming the
//! last record again with the bytes it read (PR #1136,
//! `lp-base/lp-tree-store/src/record_scan.rs`). That rests on a model —
//! `lp-nor-sim`'s weak bit programmed to 0 becomes a solid 0 — and not on
//! silicon. `flash-tears` (the sibling payload in `../flash_tears/`) found
//! weak bits after real power cuts on CX1, but only by watching: it never
//! programmed one again. This payload does.
//!
//! # What runs on the board
//!
//! The whole `lpfs` partition (176 sectors on the C6), destroyed:
//!
//! ```text
//! lpfs + 0      state: one 64-byte entry per boot, appended
//! lpfs + 4 KiB  timing scratch: erased and programmed once per boot
//! lpfs + 8 KiB  the region, 174 sectors, programmed front to back
//! ```
//!
//! **The work loop programs and does nothing else.** Every boot erases the
//! region (but for the one or two sectors it keeps, below) *before* its
//! scan-done line, and the loop then programs the region's sectors in order,
//! one 256-byte write at a time, with [`pattern::intended_word`] for the
//! boot's generation. The region takes about 0.9 s to fill, and the host cuts
//! well inside that, so every cut lands while the part is programming. That
//! is the bias toward program tears: in `flash-tears` 83 % of cuts landed in
//! an erase, because the erase is 81 % of a work cycle's time; here the
//! erases all run before the host is told it may cut. A region that filled
//! before the cut is reported (`frontier: null`) and costs a cut, nothing
//! else.
//!
//! **The boot**, before any write:
//!
//! 1. **Re-check** the words the previous boot re-programmed (step 5): they
//!    sit in the sectors it kept, which the loop skipped, so the cut that
//!    started this boot is the "further power cycle" they have now lived
//!    through. Each is read [`CONTROL_READS`] times.
//! 2. **Scan** the region once, in program order, for the frontier: the
//!    first word that is not what was meant, and the last that is not
//!    erased.
//! 3. **Find the torn words**: the bytes around the frontier read
//!    [`WINDOW_READS`] times. A bit that does not read the same every time is
//!    **weak**; a word with a weak bit, or that reads stably as neither its
//!    intended value nor erased, is **torn**.
//! 4. **Control**: each torn word read [`CONTROL_READS`] more times, in
//!    [`CONTROL_CHUNKS`] chunks [`CONTROL_GAP_US`] apart, *without*
//!    re-programming — how the weak bits behave left alone.
//! 5. **Re-program** each torn word's write from its start through the last
//!    torn word in it, with the intended bytes (one program call, as the tree
//!    store's settle is), then read it [`WINDOW_READS`] times and each torn
//!    word [`CONTROL_READS`] times: stable, and correct?
//! 6. Keep the sectors holding the torn words for step 1 of the next boot,
//!    erase every other dirty sector, time one erase and 16 writes in the
//!    scratch sector, and print [`SCAN_DONE_MARKER`].
//!
//! # Two loops, on alternate generations
//!
//! A program cut tears a word inside itself rarely: on CX1's first 250
//! cuts of this payload, the part stopped on a whole word 247 times. So
//! every even generation's loop **erases** instead ([`LoopMode::Erase`]):
//! the boot before it programs the first [`ERASE_SECTORS`] sectors of the
//! region with the generation's pattern, and the loop erases them one by
//! one. A cut in an erase leaves a sector caught on its way to `0xFF` — a
//! pre-program stopped inside a word, or cells part-way lifted, which in
//! `flash-tears` carried a median of ~90 weak bits a sector — and each such
//! sector is read, left alone, programmed to `0x00` (what the erase's own
//! pre-program would have written) and read again, at the sector's scale:
//! the same question, asked of far more cells than a torn program offers.
//!
//! Everything here is arithmetic over bytes and runs on the host against a
//! fake part (`runner`'s tests), with a part whose weak bits settle when
//! programmed and one whose do not. What stays in `fw-esp32c6` is the flash
//! driver, the clock, the USB link and the reset reason.

pub mod layout;
pub mod pattern;
pub mod records;
pub mod runner;
pub mod state;

/// The payload's name, as the header and the registries spell it. Not
/// `flash-tears-*`: its transcripts are not tear-shape histograms, and
/// `scripts/emu/flash-tears-analyze.py` reads every `flash-tears*` directory.
pub const PAYLOAD: &str = "flash-reprogram";

/// The line that ends one boot's scan: the host may cut power after it.
pub const SCAN_DONE_MARKER: &str = "[flash-reprogram] === SCAN DONE ===";

/// Printed once the host has sent a byte, before the header (the same
/// handshake as `flash-tears`, for the same reason: a boot that writes
/// before the host answers sends the host nothing).
pub const READY_LINE: &str = "[flash-reprogram] READY host answered, scanning";

/// Bytes in one erase sector of the part.
pub const SECTOR_SIZE: usize = 4096;

/// Bytes in one work-loop write: one esp-storage write, one call into the
/// mask ROM, which programs it as eight 32-byte commands.
pub const WRITE_SIZE: usize = 256;

/// The ROM's program command.
pub const COMMAND_SIZE: usize = 32;

/// Bytes in a word: the unit `flash-tears` found torn programs stop on.
pub const WORD_SIZE: usize = 4;

/// The state sector, from the start of the partition.
pub const STATE_SECTOR: u32 = 0;

/// The timing scratch sector.
pub const TIMING_SECTOR: u32 = 1;

/// The region's first sector.
pub const REGION_FIRST: u32 = 2;

/// The fewest region sectors the payload runs on.
pub const MIN_REGION_SECTORS: u32 = 16;

/// The most sectors the payload uses (the dirty-sector bitset's size).
pub const MAX_SECTORS: u32 = 256;

/// Sectors a boot keeps for the next boot's re-check.
pub const MAX_KEEP: usize = 2;

/// Torn words a boot follows through re-program and re-check. More are
/// counted in `fr-window`, not followed.
pub const MAX_TORN: usize = 6;

/// Writes a boot re-programs (each holds one or more torn words).
pub const MAX_GROUPS: usize = 3;

/// Reads of the bytes around the frontier, before and after the re-program.
/// A bit that does not read the same all 64 times is weak.
pub const WINDOW_READS: usize = 64;

/// The most bytes read around the frontier.
pub const MAX_WINDOW: usize = 4096;

/// Reads of one word in one control chunk.
pub const CONTROL_CHUNK_READS: usize = 512;

/// Control chunks per word.
pub const CONTROL_CHUNKS: usize = 8;

/// Reads of one word per stage (before, after, re-check).
pub const CONTROL_READS: usize = CONTROL_CHUNK_READS * CONTROL_CHUNKS;

/// The wait between two control chunks, so that one stage's reads span
/// about 150 ms rather than a few.
pub const CONTROL_GAP_US: u64 = 20_000;

/// Writes the boot's timed program makes in the scratch sector.
pub const TIMING_WRITES: usize = 16;

/// Sectors an erase loop erases (about 1.4 s of erasing on CX1, where the
/// host cuts 0.4-0.9 s in).
pub const ERASE_SECTORS: u32 = 64;

/// Reads of a whole sector in one chunk of an erase-mode stage.
pub const SECTOR_CHUNK_READS: usize = 64;

/// Reads of a whole sector per erase-mode stage (control, after, re-check),
/// in [`CONTROL_CHUNKS`] chunks [`CONTROL_GAP_US`] apart.
pub const SECTOR_READS: usize = SECTOR_CHUNK_READS * CONTROL_CHUNKS;

/// What a generation's work loop does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    /// Program the region, write by write.
    Program,
    /// Erase the first [`ERASE_SECTORS`] sectors of the region, one by one.
    Erase,
}

impl LoopMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::Erase => "erase",
        }
    }
}

/// The loop generation `generation` runs: odd generations program, even
/// ones erase.
pub const fn mode_of(generation: u32) -> LoopMode {
    if generation % 2 == 0 {
        LoopMode::Erase
    } else {
        LoopMode::Program
    }
}
