//! Moving one piece (DM10–DM13): the core stage and the engine stage, chunk
//! by chunk, then the hash and the commit.
//!
//! - **Core stage** (a core install): the progress record (kind `C`); the
//!   engine header erased if it was still valid (an engine-crashing board),
//!   so a cut can never leave a valid-looking engine over bytes the new core
//!   overwrote; then each chunk in order. When the last is in, the whole
//!   piece is hashed **from flash** against the offer's `core_sha256`; on a
//!   match the trial record is written and the board resets.
//! - **Engine stage** (an engine install by hashes: a heal, a reinstall, or
//!   a new core fetching its engine): the record (kind `E`); the header
//!   sector erased; sectors 1..n in order; then sector 0 held in RAM. The
//!   piece is hashed — sector 0 from RAM, the rest from flash — against the
//!   core's **digest slot**, never the offer's claim. On a match sector 0 is
//!   written with its commit word cleared and read back, the commit word is
//!   programmed, the record erased, and the board resets.
//! - On a hash mismatch the record is dropped, the owner is told `N`/`H`,
//!   and the session goes idle.
//!
//! **Inside one chunk** the order is erase → program → read back and compare
//! → program its mark. A cut anywhere in that sequence leaves either an
//! unmarked chunk (it is written again) or a marked one that really was
//! written.
//!
//! **Send-ahead:** a `D`/`Z` for any chunk but the one waited for, or from a
//! link that does not own the transfer, is ignored. **`Z`** is decoded
//! through [`UpdateWindow`]; a `Z` that does not decode to exactly the
//! chunk is asked for again with the flag clear (raw).

use crate::chunk::{ChunkEncoding, ChunkRef};
use crate::code_table::CHUNK;
use crate::flag_rule::REQUEST_TAKES_ENCODING_1;
use crate::piece_kind::PieceKind;
use crate::refusal::Refusal;
use crate::request::Request;
use crate::transfer_record::{MarkSet, TransferRecord, mark_position};

use super::board_link::LinkId;
use super::board_session::BoardSession;
use super::piece_hash::hash_flash;
use super::session_output::Effect;
use super::transfer::Transfer;
use super::transfer_plan::{chunk_len, next_chunk};
use super::update_target::{FlashFault, UpdateTarget};
use super::update_window::{UpdateWindow, WindowError};

/// Read-back mismatches on one chunk before the session gives the piece up.
const MAX_CHUNK_RETRIES: u8 = 3;

/// What became of one chunk.
enum ChunkOutcome {
    /// Written, read back and equal.
    Written,
    /// The engine header, held in RAM.
    HeldHeader,
    /// Ask for it again (raw if `raw`).
    Again {
        raw: bool,
    },
    /// The flash did not take it.
    Mismatch,
    Fault,
}

impl BoardSession {
    /// Start a new transfer for `link`: the record first (which also erases
    /// any foreign record), then what must go before the first chunk.
    pub(super) fn start<T: UpdateTarget>(
        &mut self,
        target: &mut T,
        link: LinkId,
        record: TransferRecord,
    ) {
        let addr = self.facts.progress_record_addr;
        let written = target
            .erase_sector(addr)
            .and_then(|()| target.program(addr, &record.encode_header()));
        if written.is_err() {
            return self.fault();
        }
        let marks = MarkSet::none(record.chunks() as usize);
        self.transfer = Some(Transfer::new(record, marks));
        self.window = None;
        self.resume(target, link);
    }

    /// `link` drives the transfer from its first unwritten chunk.
    pub(super) fn resume<T: UpdateTarget>(&mut self, target: &mut T, link: LinkId) {
        let Some(t) = &mut self.transfer else {
            return;
        };
        t.owner = Some(link);
        t.raw_next = false;
        t.retries = 0;
        t.sector0 = None;
        let (kind, dest, at) = (t.kind(), t.record.dest, t.record.resume_at(&t.marks));
        // Before any chunk: nothing may look like a valid engine while bytes
        // under it change (both are no-ops on a resume that already did it).
        let cleared = match kind {
            PieceKind::Core if self.engine_valid => target.erase_engine_header(),
            PieceKind::Engine => target.erase_sector(dest),
            PieceKind::Core => Ok(()),
        };
        if cleared.is_err() {
            return self.fault();
        }
        self.engine_valid = false;
        let Some(t) = &mut self.transfer else {
            return;
        };
        t.waiting = at;
        if at.is_some() {
            self.request();
        } else {
            self.finish(target);
        }
    }

    /// Ask the owner for the waiting chunk.
    pub(super) fn request(&mut self) {
        let Some(t) = &self.transfer else {
            return;
        };
        let (Some(owner), Some(idx)) = (t.owner, t.waiting) else {
            return;
        };
        let flags = if self.config.takes_encoding_1 && !t.raw_next {
            REQUEST_TAKES_ENCODING_1
        } else {
            0
        };
        let r = Request {
            kind: t.kind(),
            off: idx * CHUNK,
            len: chunk_len(t.record.len, idx),
            flags,
        };
        self.send(owner, r.encode());
    }

    /// A `D` or `Z` from `link`.
    pub(super) fn on_chunk<T: UpdateTarget>(
        &mut self,
        target: &mut T,
        link: LinkId,
        chunk: &ChunkRef<'_>,
    ) {
        let Some(t) = &self.transfer else {
            return;
        };
        if t.owner != Some(link) || chunk.kind != t.kind() || chunk.off % CHUNK != 0 {
            return;
        }
        let idx = chunk.off / CHUNK;
        if t.waiting != Some(idx) {
            return; // ahead of (or behind) the chunk waited for: ignored
        }
        let (kind, dest, len) = (t.kind(), t.record.dest, t.record.len);
        let clen = chunk_len(len, idx) as usize;

        let outcome = 'outcome: {
            let bytes: &[u8] = match chunk.encoding {
                ChunkEncoding::Raw if chunk.payload.len() == clen => chunk.payload,
                ChunkEncoding::Raw => break 'outcome ChunkOutcome::Again { raw: false },
                ChunkEncoding::Encoding1 => {
                    let window = self.window.get_or_insert_with(UpdateWindow::new);
                    match window.decode(target, kind, dest, chunk.off, clen, chunk.payload) {
                        Ok(bytes) => bytes,
                        Err(WindowError::Decode) => {
                            break 'outcome ChunkOutcome::Again { raw: true };
                        }
                        Err(WindowError::Flash(_)) => break 'outcome ChunkOutcome::Fault,
                    }
                }
            };
            if kind == PieceKind::Engine && idx == 0 {
                if let Some(t) = &mut self.transfer {
                    t.sector0 = Some(bytes.to_vec());
                }
                break 'outcome ChunkOutcome::HeldHeader;
            }
            match write_and_verify(target, &mut self.sector_buf, dest + chunk.off, bytes) {
                Err(FlashFault) => ChunkOutcome::Fault,
                Ok(false) => ChunkOutcome::Mismatch,
                Ok(true) => ChunkOutcome::Written,
            }
        };

        match outcome {
            ChunkOutcome::Fault => self.fault(),
            ChunkOutcome::Again { raw } => {
                if let Some(t) = &mut self.transfer {
                    t.raw_next |= raw;
                }
                self.request();
            }
            ChunkOutcome::Mismatch => {
                self.window = None;
                let give_up = self.transfer.as_mut().is_some_and(|t| {
                    t.retries += 1;
                    t.raw_next = true;
                    t.retries >= MAX_CHUNK_RETRIES
                });
                if give_up {
                    self.abort_hash_mismatch(target);
                } else {
                    self.request();
                }
            }
            ChunkOutcome::HeldHeader => {
                if let Some(t) = &mut self.transfer {
                    t.waiting = None;
                }
                self.finish(target);
            }
            ChunkOutcome::Written => {
                // A raw chunk keeps the window in step, so the next `Z` needs
                // no flash read.
                if chunk.encoding == ChunkEncoding::Raw
                    && let Some(window) = &mut self.window
                {
                    window.note_written(kind, dest, chunk.off, chunk.payload);
                }
                self.mark_written(target, idx);
            }
        }
    }

    /// Program chunk `idx`'s mark, then ask for the next chunk or finish.
    fn mark_written<T: UpdateTarget>(&mut self, target: &mut T, idx: u32) {
        let (byte, bit) = mark_position(idx);
        let at = self.facts.progress_record_addr + byte as u32;
        if target.program(at, &[!bit]).is_err() {
            return self.fault();
        }
        let Some(t) = &mut self.transfer else {
            return;
        };
        t.marks.set(idx);
        t.retries = 0;
        t.raw_next = false;
        t.waiting = next_chunk(t.kind(), t.record.len, idx);
        if t.waiting.is_some() {
            self.request();
        } else {
            self.finish(target);
        }
    }

    /// Every chunk is in (the engine's header in RAM): hash, then commit.
    fn finish<T: UpdateTarget>(&mut self, target: &mut T) {
        let Some(t) = &mut self.transfer else {
            return;
        };
        let record = t.record;
        match record.kind {
            PieceKind::Core => {
                let end = record.dest + record.len;
                let Ok(hash) = hash_flash(target, None, record.dest, end, &mut self.sector_buf)
                else {
                    return self.fault();
                };
                if hash != record.sha256 {
                    return self.abort_hash_mismatch(target);
                }
                if target
                    .write_trial_record(record.dest, record.len, record.build)
                    .is_err()
                {
                    return self.fault();
                }
                self.commit_done(target);
            }
            PieceKind::Engine => {
                let Some(mut header) = t.sector0.take() else {
                    // Every body sector is in: ask for the header.
                    t.waiting = Some(0);
                    return self.request();
                };
                let end = record.dest + record.len;
                let body = (record.dest + CHUNK).min(end);
                let Ok(hash) = hash_flash(target, Some(&header), body, end, &mut self.sector_buf)
                else {
                    return self.fault();
                };
                if hash != self.facts.digest_slot {
                    return self.abort_hash_mismatch(target);
                }
                target.prepare_uncommitted_header(&mut header);
                match write_and_verify(target, &mut self.sector_buf, record.dest, &header) {
                    Ok(true) => {}
                    Ok(false) => return self.abort_hash_mismatch(target),
                    Err(FlashFault) => return self.fault(),
                }
                if target.commit_engine_header(record.dest).is_err() {
                    return self.fault();
                }
                self.engine_valid = true;
                self.commit_done(target);
            }
        }
    }

    /// The piece is committed: erase the record and reset.
    fn commit_done<T: UpdateTarget>(&mut self, target: &mut T) {
        // A record a cut leaves behind here is foreign to the next core (a
        // core transfer) or stale beside a valid engine (an engine transfer),
        // so a failed erase is not a failed update.
        let _ = target.erase_sector(self.facts.progress_record_addr);
        self.transfer = None;
        self.reset();
    }

    /// The piece did not hash to what it must: drop the record, tell the
    /// owner `N`/`H`, and go idle.
    fn abort_hash_mismatch<T: UpdateTarget>(&mut self, target: &mut T) {
        let owner = self.transfer.take().and_then(|t| t.owner);
        self.window = None;
        if target
            .erase_sector(self.facts.progress_record_addr)
            .is_err()
        {
            self.push_effect(Effect::FlashFault);
        }
        if let Some(owner) = owner {
            self.refuse(owner, Refusal::HashMismatch);
        }
    }

    /// A flash operation failed: stop where the record says, until an offer
    /// resumes.
    fn fault(&mut self) {
        if let Some(t) = &mut self.transfer {
            t.waiting = None;
            t.sector0 = None;
        }
        self.window = None;
        self.push_effect(Effect::FlashFault);
    }
}

/// Erase the sector at `addr`, program `bytes`, read them back through
/// `buf` and compare. `Ok(false)`: the flash did not take them.
pub(super) fn write_and_verify<T: UpdateTarget>(
    target: &mut T,
    buf: &mut [u8],
    addr: u32,
    bytes: &[u8],
) -> Result<bool, FlashFault> {
    target.erase_sector(addr)?;
    target.program(addr, bytes)?;
    let back = &mut buf[..bytes.len()];
    target.read(addr, back)?;
    Ok(back == bytes)
}
