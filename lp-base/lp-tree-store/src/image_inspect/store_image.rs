//! [`StoreImage`]: a raw image of a tree-store partition, read without
//! mounting it. It repeats mount's decisions (FORMAT.md "Finding the
//! committed state") over the bytes instead of a `Flash`, so it can answer
//! where mount would refuse: which sectors refuse it, and whether the
//! sectors that do not hold a complete store anyway. It never writes.
//!
//! The format's decoders are the store's own (`SectorHeader::decode`,
//! `RecordHeader::parse`, `RootRecord::decode`, `decode_dir`,
//! `parse_multi`, `decode_blob_into`); only the walk over an in-memory
//! image is new, and the crate's tests hold it to the real mount's choice
//! of root and live bytes on stores the real writer made.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;

use super::image_report::{
    EntryKindReport, ImageReport, MountVerdict, RecordKindReport, RecordStatus, RootOutcome,
    RootReport, SectorSizeFrom, SectorState, TreeEntryReport,
};
use super::image_scan::{detect_sector_size, scan_sector};
use crate::blob_codec::decode_blob_into;
use crate::dir_node::{DirEntry, EntryKind, decode_dir};
use crate::multi_node::{multi_child, parse_multi};
use crate::record_kind::ChunkCodec;
use crate::root_record::RootRecord;

/// Why an image cannot be opened at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// Fewer bytes than one 512-byte sector.
    TooSmall,
    /// A sector size the format does not allow (a power of two from 512 to
    /// 32768).
    BadSectorSize(u32),
}

/// Where a kept record is: sector index and position in its record list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Loc {
    pub sector: u32,
    pub rec: usize,
}

/// A read-only view of a raw partition image.
pub struct StoreImage<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) report: ImageReport,
    /// The copy of each id that mount keeps: the one in the sector with the
    /// highest sequence.
    pub(super) kept: BTreeMap<u64, Loc>,
    /// Ids the chosen root reaches.
    pub(super) live: BTreeSet<u64>,
    /// The chosen root's decoded record.
    pub(super) root: Option<RootRecord>,
}

impl<'a> StoreImage<'a> {
    /// Read `bytes`. `sector_size` = the caller's choice; `None` reads it
    /// from the headers (FORMAT.md "The medium"), and assumes 4096 when no
    /// header gives one.
    pub fn open(bytes: &'a [u8], sector_size: Option<u32>) -> Result<Self, ImageError> {
        let (size, from) = match sector_size {
            Some(s) if (512..=32768).contains(&s) && s.is_power_of_two() => {
                (s, SectorSizeFrom::Given)
            }
            Some(s) => return Err(ImageError::BadSectorSize(s)),
            None => match detect_sector_size(bytes) {
                Some(s) => (s, SectorSizeFrom::Headers),
                None => (4096, SectorSizeFrom::Assumed),
            },
        };
        let count = (bytes.len() / size as usize) as u32;
        if count == 0 {
            return Err(ImageError::TooSmall);
        }
        let mut sectors: Vec<_> = (0..count).map(|s| scan_sector(bytes, s, size)).collect();

        // The trusted sectors in the order mount reads them.
        let mut order: Vec<(u32, u32)> = sectors
            .iter()
            .filter_map(|s| s.header.map(|h| (h.seq, s.index)))
            .collect();
        order.sort_unstable();

        // The kept copy of each id: the latest sector's, and inside one
        // sector the first (mount's `locate` reads headers in order).
        let mut kept: BTreeMap<u64, Loc> = BTreeMap::new();
        for &(_, s) in &order {
            for (i, r) in sectors[s as usize].records.iter().enumerate() {
                if matches!(r.status, RecordStatus::Untrusted | RecordStatus::Unknown) {
                    continue;
                }
                let here = Loc { sector: s, rec: i };
                match kept.get(&r.id) {
                    Some(old) if old.sector == s => {}
                    _ => {
                        kept.insert(r.id, here);
                    }
                }
            }
        }

        let mut img = StoreImage {
            bytes,
            report: ImageReport {
                image_bytes: bytes.len() as u64,
                sector_size: size,
                sector_size_from: from,
                sector_count: count,
                trailing_bytes: (bytes.len() % size as usize) as u32,
                sectors: Vec::new(),
                roots: Vec::new(),
                chosen: None,
                mount: MountVerdict::NoStore { why: "" },
                tree: Vec::new(),
                live_bytes: 0,
                garbage_bytes: 0,
            },
            kept,
            live: BTreeSet::new(),
            root: None,
        };

        // Root candidates: CRC-good roots that decode, newest first. A
        // root's id on flash twice is one candidate, at its kept copy.
        let mut roots: Vec<RootReport> = Vec::new();
        for &(_, s) in &order {
            for r in &sectors[s as usize].records {
                let (RecordKindReport::Root, Some(seq)) = (r.kind, r.root_seq) else {
                    continue;
                };
                if roots.iter().any(|c| c.id == r.id) {
                    continue;
                }
                let at = img.kept[&r.id];
                let (sector, rec) = (at.sector, &sectors[at.sector as usize].records[at.rec]);
                let decoded = RootRecord::decode(img.payload_in(&sectors, at, size))
                    .expect("scan checked it decodes");
                roots.push(RootReport {
                    seq,
                    id: r.id,
                    sector,
                    offset: rec.offset,
                    cold_dir: decoded.cold_dir.0,
                    hot_dir: decoded.hot_dir.0,
                    retired: decoded.retired,
                    outcome: RootOutcome::NotTried,
                });
            }
        }
        // Newest first; among equal sequences the later sector first.
        roots.reverse();
        roots.sort_by(|a, b| b.seq.cmp(&a.seq));

        img.report.sectors = core::mem::take(&mut sectors);
        img.report.roots = roots;
        img.choose_root();
        img.finish();
        Ok(img)
    }

    /// The report.
    pub fn report(&self) -> &ImageReport {
        &self.report
    }

    pub fn sector_size(&self) -> u32 {
        self.report.sector_size
    }

    // ---- record access ----------------------------------------------------

    fn payload_in(
        &self,
        sectors: &[super::image_report::SectorReport],
        at: Loc,
        size: u32,
    ) -> &'a [u8] {
        let r = &sectors[at.sector as usize].records[at.rec];
        let start = at.sector as usize * size as usize + r.offset as usize + 16;
        let bytes: &'a [u8] = self.bytes;
        &bytes[start..start + usize::from(r.len)]
    }

    pub(super) fn record(&self, at: Loc) -> &super::image_report::RecordReport {
        &self.report.sectors[at.sector as usize].records[at.rec]
    }

    pub(super) fn payload(&self, at: Loc) -> &'a [u8] {
        self.payload_in(&self.report.sectors, at, self.report.sector_size)
    }

    /// The kept record of `id`, and its payload.
    pub(super) fn get(&self, id: u64) -> Option<(&super::image_report::RecordReport, &'a [u8])> {
        let at = *self.kept.get(&id)?;
        Some((self.record(at), self.payload(at)))
    }

    // ---- mount's choice ---------------------------------------------------

    /// Try the two newest roots, newest first; the first whose closure is
    /// complete is the committed state (FORMAT.md mount step 3).
    fn choose_root(&mut self) {
        let mut chosen = None;
        for i in 0..self.report.roots.len().min(2) {
            let id = self.report.roots[i].id;
            match self.walk(id) {
                Ok(closed) => {
                    self.report.roots[i].outcome = RootOutcome::Chosen;
                    chosen = Some((i, closed));
                    break;
                }
                Err(why) => self.report.roots[i].outcome = RootOutcome::Unusable { why },
            }
        }
        if let Some((i, closed)) = chosen {
            self.report.chosen = Some(i);
            self.live = closed.ids;
            self.report.tree = closed.entries;
            self.root = Some(closed.root);
        }
    }

    /// Statuses, live and garbage bytes, retirements and the mount verdict.
    fn finish(&mut self) {
        let retired = self.root.as_ref().map(|r| r.retired.clone());
        let mut live_total = 0u64;
        let mut garbage_total = 0u64;
        for s in 0..self.report.sectors.len() {
            let mut live = 0u32;
            let mut garbage = 0u32;
            for i in 0..self.report.sectors[s].records.len() {
                let r = &self.report.sectors[s].records[i];
                let status = match r.status {
                    RecordStatus::Untrusted => RecordStatus::Untrusted,
                    RecordStatus::Unknown => RecordStatus::Unknown,
                    _ if !self.live.contains(&r.id) => RecordStatus::Garbage,
                    _ if self.kept.get(&r.id)
                        == Some(&Loc {
                            sector: s as u32,
                            rec: i,
                        }) =>
                    {
                        RecordStatus::Live
                    }
                    _ => RecordStatus::Copy,
                };
                let total = r.total_len();
                match status {
                    RecordStatus::Live => live += total,
                    RecordStatus::Untrusted => {}
                    _ => garbage += total,
                }
                self.report.sectors[s].records[i].status = status;
            }
            let sector = &mut self.report.sectors[s];
            sector.live_bytes = live;
            sector.garbage_bytes = garbage;
            sector.retired = retired
                .as_ref()
                .is_some_and(|r| r.binary_search(&(s as u16)).is_ok());
            live_total += u64::from(live);
            garbage_total += u64::from(garbage);
        }
        self.report.live_bytes = live_total;
        self.report.garbage_bytes = garbage_total;

        let refusing = self.report.refusing_sectors();
        let any_trusted = self.report.sectors.iter().any(|s| s.header.is_some());
        let all_blank = self
            .report
            .sectors
            .iter()
            .all(|s| s.state == SectorState::Blank);
        self.report.mount = if !refusing.is_empty() {
            MountVerdict::Refused {
                sectors: refusing,
                rest_is_complete_store: self.report.chosen.is_some(),
            }
        } else if self.report.chosen.is_some() {
            MountVerdict::Mounts
        } else if all_blank {
            MountVerdict::NoStore {
                why: "every sector is erased",
            }
        } else if !any_trusted {
            MountVerdict::NoStore {
                why: "no sector has a header of this format",
            }
        } else if self.report.roots.is_empty() {
            MountVerdict::NoStore {
                why: "no root record",
            }
        } else {
            MountVerdict::NoStore {
                why: "no complete root",
            }
        };
    }

    // ---- the committed tree ------------------------------------------------

    /// The closure of root `root_id` as mount builds it: every record
    /// present, every root, directory and multi decoding, every directory
    /// node (a multi's chunks too) readable. Nothing deeper than mount
    /// checks (that is [`StoreImage::check`]).
    fn walk(&self, root_id: u64) -> Result<Closure, &'static str> {
        let (rec, payload) = self.get(root_id).ok_or("missing record")?;
        if rec.kind != RecordKindReport::Root {
            return Err("record kind");
        }
        let root = RootRecord::decode(payload).ok_or("root")?;
        let mut c = Closure {
            ids: BTreeSet::new(),
            entries: Vec::new(),
            root: root.clone(),
        };
        c.ids.insert(root_id);
        let mut stack = Vec::new();
        self.walk_dir(root.cold_dir.0, &mut Vec::new(), false, &mut c, &mut stack)?;
        self.walk_dir(root.hot_dir.0, &mut Vec::new(), true, &mut c, &mut stack)?;
        Ok(c)
    }

    fn walk_dir(
        &self,
        id: u64,
        path: &mut Vec<u8>,
        hot: bool,
        c: &mut Closure,
        stack: &mut Vec<u64>,
    ) -> Result<(), &'static str> {
        if stack.contains(&id) || stack.len() > 64 {
            return Err("directory cycle");
        }
        let entries = self.dir_entries(id)?;
        c.ids.insert(id);
        if let Some((rec, payload)) = self.get(id) {
            if rec.kind == RecordKindReport::Multi {
                for i in 0..usize::from(u16::from_le_bytes([payload[5], payload[6]])) {
                    self.walk_node(multi_child(payload, i).0, c)?;
                }
            }
        }
        stack.push(id);
        for e in entries {
            let len = path.len();
            // The hot directory's names are full paths; everything else
            // joins names with `/`.
            if !(hot && stack.len() == 1) {
                path.push(b'/');
            }
            path.extend_from_slice(&e.name);
            c.entries.push(TreeEntryReport {
                path: String::from_utf8_lossy(path).into_owned(),
                kind: match e.kind {
                    EntryKind::File => EntryKindReport::File,
                    EntryKind::Dir => EntryKindReport::Dir,
                },
                size: e.size,
                id: e.id.0,
                hot,
                name_problem: name_problem(&e.name, hot && stack.len() == 1),
            });
            match e.kind {
                EntryKind::File => self.walk_node(e.id.0, c)?,
                EntryKind::Dir => self.walk_dir(e.id.0, path, hot, c, stack)?,
            }
            path.truncate(len);
        }
        stack.pop();
        Ok(())
    }

    /// A file node or chunk: present, a blob or a multi that parses, and so
    /// on down.
    fn walk_node(&self, id: u64, c: &mut Closure) -> Result<(), &'static str> {
        if !c.ids.insert(id) {
            return Ok(());
        }
        let (rec, payload) = self.get(id).ok_or("missing record")?;
        match rec.kind {
            RecordKindReport::Blob => Ok(()),
            RecordKindReport::Multi => {
                let m = parse_multi(payload).ok_or("multi")?;
                for i in 0..m.count {
                    self.walk_node(multi_child(payload, i).0, c)?;
                }
                Ok(())
            }
            _ => Err("record kind"),
        }
    }

    /// A directory node's entries: a `Dir` record's payload, or a multi
    /// with the directory flag whose chunks are the bytes.
    pub(super) fn dir_entries(&self, id: u64) -> Result<Vec<DirEntry>, &'static str> {
        let (rec, payload) = self.get(id).ok_or("missing record")?;
        let bytes = match rec.kind {
            RecordKindReport::Dir => payload.to_vec(),
            RecordKindReport::Multi => {
                if !parse_multi(payload).ok_or("multi")?.dir {
                    return Err("dir multi without the dir flag");
                }
                self.node_bytes(id)?
            }
            _ => return Err("record kind"),
        };
        decode_dir(&bytes).ok_or("dir")
    }

    /// Node `id`'s logical bytes, as `node_read::read_node_into` reads them:
    /// chunks decoded, a multi's levels and lengths checked.
    pub(super) fn node_bytes(&self, id: u64) -> Result<Vec<u8>, &'static str> {
        let mut out = Vec::new();
        let (rec, payload) = self.get(id).ok_or("missing record")?;
        match rec.kind {
            RecordKindReport::Blob => self.decode_chunk(rec.codec, payload, &mut out)?,
            RecordKindReport::Dir => out.extend_from_slice(payload),
            RecordKindReport::Multi => {
                let total = self.multi_into(payload, None, &mut out)?;
                if out.len() != total as usize {
                    return Err("multi length");
                }
            }
            _ => return Err("root is not a node"),
        }
        Ok(out)
    }

    fn multi_into(
        &self,
        payload: &[u8],
        level: Option<u8>,
        out: &mut Vec<u8>,
    ) -> Result<u32, &'static str> {
        let m = parse_multi(payload).ok_or("multi")?;
        if level.is_some_and(|l| l != m.level) {
            return Err("multi level");
        }
        let start = out.len();
        for i in 0..m.count {
            let (rec, p) = self
                .get(multi_child(payload, i).0)
                .ok_or("missing record")?;
            match (m.level, rec.kind) {
                (0, RecordKindReport::Blob) => self.decode_chunk(rec.codec, p, out)?,
                (l, RecordKindReport::Multi) if l > 0 => {
                    self.multi_into(p, Some(l - 1), out)?;
                }
                _ => return Err("multi child"),
            }
        }
        if out.len() - start != m.total_len as usize {
            return Err("multi length");
        }
        Ok(m.total_len)
    }

    pub(super) fn decode_chunk(
        &self,
        codec: u8,
        payload: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<(), &'static str> {
        let codec = ChunkCodec::from_u8(codec).ok_or("chunk does not decode")?;
        decode_blob_into(codec, payload, out).ok_or("chunk does not decode")
    }
}

/// What a walk of a root reaches.
struct Closure {
    ids: BTreeSet<u64>,
    entries: Vec<TreeEntryReport>,
    root: RootRecord,
}

/// The writer's name rules (FORMAT.md "Dir"): UTF-8, non-empty, no `/`.
/// In the hot directory (`full_path`) a name is a whole path, so `/` is
/// fine there.
pub(super) fn name_problem(name: &[u8], full_path: bool) -> Option<&'static str> {
    if name.is_empty() {
        Some("empty name")
    } else if !full_path && name.contains(&b'/') {
        Some("name contains '/'")
    } else if core::str::from_utf8(name).is_err() {
        Some("name is not UTF-8")
    } else {
        None
    }
}
