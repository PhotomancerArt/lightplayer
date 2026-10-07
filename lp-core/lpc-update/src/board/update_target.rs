//! What the board session needs from the firmware: flash, placement, the
//! split image's format hooks, and the facts read once at start.
//!
//! The firmware (Part B) implements [`UpdateTarget`] over the split image's
//! `ota/` code and `lp-bootctl`; tests implement it over an in-memory NOR
//! model (`crate::testing`, feature `test-support`). It is small and
//! synchronous on purpose: flash calls block on the device anyway.
//!
//! **This crate never re-types the split image's formats.** The boot record,
//! the engine header and its commit word are reached only through the hooks
//! here ([`UpdateTarget::write_trial_record`],
//! [`UpdateTarget::prepare_uncommitted_header`],
//! [`UpdateTarget::commit_engine_header`],
//! [`UpdateTarget::erase_engine_header`]).

use alloc::string::String;
use core::ops::Range;

use crate::build_id::BUILD_ID_LEN;

/// A flash operation failed (or the target refused it: outside the region,
/// or over the running core). The session stops the transfer it was part of;
/// the progress record says how far it got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlashFault;

/// The firmware's side of an update.
pub trait UpdateTarget {
    /// Erase the 4 KiB sector at `addr` to `0xFF`.
    fn erase_sector(&mut self, addr: u32) -> Result<(), FlashFault>;
    /// The size of the target's block erase, if it has one: a power of two,
    /// a multiple of [`CHUNK`](crate::code_table::CHUNK). A NOR part erases
    /// a 64 KiB block in a fraction of what sixteen sector erases take, so
    /// the session erases a piece's whole blocks ahead of their chunks.
    /// `None` (the default): sector by sector.
    fn block_size(&self) -> Option<u32> {
        None
    }
    /// Erase the [`block_size`](Self::block_size) block at `addr` (aligned to
    /// it) to `0xFF`. Asked only when `block_size` is `Some`.
    fn erase_block(&mut self, addr: u32) -> Result<(), FlashFault> {
        let _ = addr;
        Err(FlashFault)
    }
    /// SHA-256 of `head` (held in RAM, if any) followed by the flash bytes
    /// `[from, to)`, when the target has a faster way to it than the
    /// session's software hash over [`read`](Self::read) — an accelerator.
    /// `None` (the default): the session hashes. The digest is the same
    /// either way, and it is still the check a piece commits on.
    fn sha256_flash(
        &mut self,
        head: Option<&[u8]>,
        from: u32,
        to: u32,
    ) -> Option<Result<[u8; 32], FlashFault>> {
        let _ = (head, from, to);
        None
    }
    /// Program `bytes` at `addr` (NOR: bits only go 1 → 0).
    fn program(&mut self, addr: u32, bytes: &[u8]) -> Result<(), FlashFault>;
    /// Read `buf.len()` bytes at `addr`.
    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashFault>;

    /// Where a new core of `core_len` bytes goes while this core runs (the
    /// split image's `next_core_offset`), or `None` if it does not fit.
    fn core_dest(&self, core_len: u32) -> Option<u32>;
    /// The most an engine may occupy for a core at `core_off`/`core_len`
    /// (the split image's `engine_room`). The session asks it for its own
    /// core, and for a new core's place before accepting a core install.
    fn engine_room_for(&self, core_off: u32, core_len: u32) -> Range<u32>;

    /// Clear the commit word in an engine header sector held in RAM, so the
    /// sector can be written uncommitted and committed separately.
    fn prepare_uncommitted_header(&self, sector0: &mut [u8]);
    /// Program the commit word of the engine header at `dest`.
    fn commit_engine_header(&mut self, dest: u32) -> Result<(), FlashFault>;
    /// Write the boot record that boots the new core at `dest` on trial.
    fn write_trial_record(
        &mut self,
        dest: u32,
        len: u32,
        build_hash: u32,
    ) -> Result<(), FlashFault>;
    /// Erase the running engine's header sector, so the next boot is
    /// core-only (DM13: "the flash is the update-pending state").
    fn erase_engine_header(&mut self) -> Result<(), FlashFault>;
}

/// Which half of the image the session runs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionMode {
    /// The engine runs; the session is its channel-3 hook (DM13).
    EngineRunning,
    /// Core-only: no engine runs; the session serves every link.
    CoreOnly,
}

/// The engine as the core found it at boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineStatus {
    /// A committed header of this core's build.
    Valid,
    /// No valid engine: core-only waits for its engine (E1, E13).
    Missing,
    /// A valid engine that keeps crashing (E10): core-only, reported, never
    /// healed in a loop; an explicit reinstall is still accepted.
    Crashing,
}

/// What the session knows about its board, read once at start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardFacts {
    pub mode: SessionMode,
    /// The chip code ([`crate::code_table`]) and its word.
    pub chip: u16,
    pub chip_word: String,
    pub layout: u16,
    pub loader: u16,
    /// The opaque target name, from the image's manifest core. For `M`
    /// only: the session never compares target names.
    pub target: String,
    pub version: String,
    /// The image's manifest-core `wireProto`.
    pub wire_proto: u32,
    /// This core's build id, zero-padded.
    pub build_id: [u8; BUILD_ID_LEN],
    /// This core's SHA-256 (the core rule; computed and cached by the
    /// firmware, DM24).
    pub core_sha256: [u8; 32],
    /// The core's digest slot: SHA-256 of the engine it needs.
    pub digest_slot: [u8; 32],
    pub core_off: u32,
    pub core_len: u32,
    /// The engine's length from its valid header, if the core can know it.
    pub engine_len: Option<u32>,
    /// Bytes in the region the core and engine share.
    pub region_len: u32,
    /// The boot state can be trusted (the split image's records read
    /// cleanly). `false` refuses every install `N`/`T`.
    pub trusted_boot: bool,
    /// This core runs on trial and has not yet been confirmed.
    pub on_trial: bool,
    /// The build hash the split image's rollback refused, if any.
    pub refused_build: Option<u32>,
    pub engine: EngineStatus,
    /// The progress record's sector (layout 1: `factory + 0x5000`).
    pub progress_record_addr: u32,
}

impl BoardFacts {
    /// This core's build hash ([`crate::build_id`]).
    #[must_use]
    pub fn build_hash(&self) -> u32 {
        crate::build_id::build_hash_of_field(&self.build_id)
    }
}
