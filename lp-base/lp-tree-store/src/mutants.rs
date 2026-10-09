//! Mutation switches (feature `mutants`, off by default): each [`Mutant`]
//! breaks one guarantee of the store on purpose, so the host testbed
//! (`lp-store-bench mutants`) can prove its drivers and oracle notice — a
//! mutant no driver catches marks a place the tests are blind. Nothing here
//! exists without the feature: every switch site is `mutant!(…)`, which is
//! `false` then, and the mutant-only paths below are compiled out. No
//! product build turns it on.
//!
//! One mutant at a time, process-wide ([`set_mutant`]).

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::flash::Flash;
use crate::object_hasher::ObjectHasher;
use crate::object_id::{IdTag, ObjectId};
use crate::ram_index::RecordLoc;
use crate::record_header::{HeaderRead, RECORD_HEADER_LEN, RecordHeader};
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::record_log::RecordLog;
use crate::root_record::RootRecord;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN};
use crate::store_error::StoreError;
use crate::tree_store::{Committed, Res, TreeStore};

/// One deliberately broken guarantee.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Mutant {
    /// Erase a sector without killing its header first.
    SkipKill = 1,
    /// Mount trusts a record header without checking its payload's CRC.
    TrustRecordHeaders,
    /// Mount indexes every record on flash, so writes dedup against any
    /// indexed id, closure complete or not (the prototype's first rule).
    DedupAgainstAllRecords,
    /// GC erases its victim before writing (and verifying) the copies.
    GcEraseBeforeCopy,
    /// A commit writes its root before the directories it names.
    RootBeforeDirs,
    /// Mount resumes a write head without checking its tail reads `0xFF`.
    ResumeWithoutTailCheck,
    /// A file write makes room (and refuses with `NoSpace`) only after its
    /// records are written.
    NoSpaceAfterWrite,
    /// GC's mark forgets the pending set (a transaction's file ids and the
    /// records a flush has in flight), so it may collect them.
    GcForgetsPending,
    /// A sector header is programmed whole, magic first (not magic last).
    MagicFirst,
    /// A sector header at a newer format version reads as untrusted (blank)
    /// instead of refusing the mount.
    NewerVersionUntrusted,
    /// A sector is opened without reading its erase back as all `0xFF`.
    OpenWithoutReadBack,
}

impl Mutant {
    pub const ALL: [Mutant; 11] = [
        Mutant::SkipKill,
        Mutant::TrustRecordHeaders,
        Mutant::DedupAgainstAllRecords,
        Mutant::GcEraseBeforeCopy,
        Mutant::RootBeforeDirs,
        Mutant::ResumeWithoutTailCheck,
        Mutant::NoSpaceAfterWrite,
        Mutant::GcForgetsPending,
        Mutant::MagicFirst,
        Mutant::NewerVersionUntrusted,
        Mutant::OpenWithoutReadBack,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Mutant::SkipKill => "skip_kill",
            Mutant::TrustRecordHeaders => "trust_record_headers",
            Mutant::DedupAgainstAllRecords => "dedup_against_all_records",
            Mutant::GcEraseBeforeCopy => "gc_erase_before_copy",
            Mutant::RootBeforeDirs => "root_before_dirs",
            Mutant::ResumeWithoutTailCheck => "resume_without_tail_check",
            Mutant::NoSpaceAfterWrite => "no_space_after_write",
            Mutant::GcForgetsPending => "gc_forgets_pending",
            Mutant::MagicFirst => "magic_first",
            Mutant::NewerVersionUntrusted => "newer_version_untrusted",
            Mutant::OpenWithoutReadBack => "open_without_read_back",
        }
    }

    pub fn from_name(name: &str) -> Option<Mutant> {
        Mutant::ALL.into_iter().find(|m| m.name() == name)
    }
}

static ACTIVE: AtomicU8 = AtomicU8::new(0);

/// Switch one mutant on (process-wide), or none.
pub fn set_mutant(m: Option<Mutant>) {
    ACTIVE.store(m.map_or(0, |m| m as u8), Ordering::SeqCst);
}

/// The mutant switched on, if any.
pub fn active_mutant() -> Option<Mutant> {
    let v = ACTIVE.load(Ordering::Relaxed);
    Mutant::ALL.into_iter().find(|&m| m as u8 == v)
}

pub(crate) fn on(m: Mutant) -> bool {
    ACTIVE.load(Ordering::Relaxed) == m as u8
}

/// [`Mutant::DedupAgainstAllRecords`]: after mount's closure walk, index
/// every other record the trusted sectors hold (newest sector first).
pub(crate) fn index_every_record<F: Flash, K>(
    log: &mut RecordLog<F>,
    sectors: &[(u32, u32, K)],
) -> Result<(), StoreError<F::Error>> {
    for &(_, s, _) in sectors.iter().rev() {
        let end = u32::from(log.sectors.end[s as usize]);
        let mut off = SECTOR_HEADER_LEN;
        while off + RECORD_HEADER_LEN <= end {
            let mut raw = [0u8; RECORD_HEADER_LEN as usize];
            log.read(log.addr(s, off), &mut raw)?;
            let h = match RecordHeader::parse(&raw) {
                HeaderRead::Record(h) => h,
                HeaderRead::Unknown { len } => {
                    off += RECORD_HEADER_LEN + u32::from(len);
                    continue;
                }
                HeaderRead::End | HeaderRead::Bad => break,
            };
            if !log.index.contains(h.id) {
                log.index.insert(
                    h.id,
                    RecordLoc {
                        sector: s,
                        offset: off,
                    },
                );
            }
            off += h.total_len();
        }
    }
    Ok(())
}

/// [`Mutant::GcEraseBeforeCopy`]: read the victim's live records into RAM,
/// kill and erase it, then write the copies.
pub(crate) fn collect_erasing_first<F: Flash>(
    log: &mut RecordLog<F>,
    victim: u32,
    items: Vec<(u32, ObjectId)>,
) -> Result<(), StoreError<F::Error>> {
    let mut held = Vec::new();
    for (_, id) in items {
        let (h, payload) = log.read_record(id)?;
        held.push((h, id, payload));
    }
    log.kill_and_erase(victim)?;
    for (h, id, payload) in held {
        log.append(HeadKind::Cold, h.kind, h.codec, id, &[&payload])?;
    }
    Ok(())
}

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// [`Mutant::RootBeforeDirs`]: work out the new directories' ids without
    /// writing them (a dry flush), write the root naming them, then write
    /// the directories. Room for the root is made with the old tree and the
    /// pending files marked, as for any write before the root exists.
    pub(crate) fn commit_root_first(&mut self) -> Res<(), F> {
        let old_work = self.work;
        let pending = self.delta.entries().to_vec();
        self.dry = true;
        let r = self.flush();
        self.dry = false;
        r?;
        let new_work = self.work;
        self.work = old_work;
        self.delta.restore(pending);
        let root = RootRecord {
            seq: self.max_root_seq.wrapping_add(1),
            cold_dir: new_work.cold,
            hot_dir: new_work.hot,
            retired: self.log.sectors.retired.clone(),
        };
        let payload = root.encode();
        let id = ObjectId::of(&mut self.hasher, IdTag::Root, &[&payload]);
        let len = RECORD_HEADER_LEN + payload.len() as u32;
        self.ensure_room(&[(HeadKind::Hot, len)])?;
        self.log.append(
            HeadKind::Hot,
            RecordKind::Root,
            ChunkCodec::Stored,
            id,
            &[&payload],
        )?;
        self.max_root_seq = root.seq;
        self.flush()?;
        if self.work != new_work {
            return Err(StoreError::Corrupt("mutant: dry flush disagreed"));
        }
        self.committed = Some(Committed { id, root });
        Ok(())
    }
}
