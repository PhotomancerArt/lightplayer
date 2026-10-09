//! The store's fsck: read-only, never repairs. [`StoreImage::check`]
//! verifies the committed state far deeper than mount does (mount only
//! checks that every id a root reaches is present and parses): every
//! reachable record's id is the hash of what it holds, every chunk inflates
//! to its logical length, every multi's levels and lengths add up, every
//! file entry's size is the node's, every name obeys the writer's rules, and
//! the rest of the flash is accounted for (orphans, older copies that must
//! be identical, sectors that refuse the mount or read differently twice).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use serde::Serialize;

use super::image_report::{MountVerdict, RecordKindReport, RecordStatus, RootOutcome, SectorState};
use super::store_image::{Loc, StoreImage, name_problem};
use crate::dir_node::{DirEntry, EntryKind};
use crate::multi_node::{multi_child, parse_multi};
use crate::object_hasher::ObjectHasher;
use crate::object_id::{IdTag, ObjectId};
use crate::tree_store::{MAX_DEPTH, is_hot, valid_path};

/// How much a finding matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Worth knowing, nothing wrong (orphans, older copies, killed sectors).
    Note,
    /// A torn write the store recovers from by design.
    Warning,
    /// An inconsistency: the tool exits non-zero.
    Error,
}

/// One thing `check` found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub severity: Severity,
    /// A stable kebab-case name for scripts.
    pub code: &'static str,
    pub sector: Option<u32>,
    pub message: String,
}

/// `check`'s answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CheckReport {
    pub findings: Vec<Finding>,
    pub errors: u32,
    pub warnings: u32,
    /// Files whose bytes were read, inflated and hashed.
    pub files_verified: u32,
    /// Chunk, multi, directory and root records whose id was recomputed.
    pub records_verified: u32,
    /// Records no chosen root reaches, and their bytes.
    pub orphan_records: u32,
    pub orphan_bytes: u64,
}

impl CheckReport {
    /// No error-level finding.
    pub fn is_consistent(&self) -> bool {
        self.errors == 0
    }
}

impl<'a> StoreImage<'a> {
    /// Verify the image. `reread` = a second read of the same flash (same
    /// length): sectors that differ between the two reads are weak.
    pub fn check<H: ObjectHasher>(&self, hasher: &mut H, reread: Option<&[u8]>) -> CheckReport {
        let mut c = Checker {
            img: self,
            hasher,
            out: CheckReport {
                findings: Vec::new(),
                errors: 0,
                warnings: 0,
                files_verified: 0,
                records_verified: 0,
                orphan_records: 0,
                orphan_bytes: 0,
            },
            nodes: BTreeMap::new(),
        };
        c.medium(reread);
        c.mount_and_sectors();
        c.committed_tree();
        c.copies_and_orphans();
        c.out
    }
}

struct Checker<'i, 'a, H> {
    img: &'i StoreImage<'a>,
    hasher: &'i mut H,
    out: CheckReport,
    /// Node id -> its verified logical length, or why it failed.
    nodes: BTreeMap<u64, Result<u32, String>>,
}

impl<H: ObjectHasher> Checker<'_, '_, H> {
    fn add(
        &mut self,
        severity: Severity,
        code: &'static str,
        sector: Option<u32>,
        message: String,
    ) {
        match severity {
            Severity::Error => self.out.errors += 1,
            Severity::Warning => self.out.warnings += 1,
            Severity::Note => {}
        }
        self.out.findings.push(Finding {
            severity,
            code,
            sector,
            message,
        });
    }

    // ---- the medium --------------------------------------------------------

    fn medium(&mut self, reread: Option<&[u8]>) {
        let r = &self.img.report;
        if r.trailing_bytes != 0 {
            self.add(
                Severity::Warning,
                "trailing-bytes",
                None,
                format!(
                    "the image ends {} bytes past its last whole {}-byte sector",
                    r.trailing_bytes, r.sector_size
                ),
            );
        }
        if r.sector_size_from == super::image_report::SectorSizeFrom::Assumed {
            self.add(
                Severity::Warning,
                "sector-size-assumed",
                None,
                format!(
                    "no sector header gave the sector size; {} assumed (pass it to override)",
                    r.sector_size
                ),
            );
        }
        let Some(second) = reread else { return };
        let (size, count) = (r.sector_size as usize, r.sector_count as usize);
        if second.len() != self.img.bytes.len() {
            self.add(
                Severity::Error,
                "reread-length",
                None,
                format!(
                    "the second read is {} bytes, the first {}",
                    second.len(),
                    self.img.bytes.len()
                ),
            );
            return;
        }
        for s in 0..count {
            let range = s * size..(s + 1) * size;
            if self.img.bytes[range.clone()] != second[range.clone()] {
                let differing = self.img.bytes[range.clone()]
                    .iter()
                    .zip(&second[range])
                    .filter(|(a, b)| a != b)
                    .count();
                self.add(
                    Severity::Error,
                    "weak-sector",
                    Some(s as u32),
                    format!("two reads of the sector differ in {differing} bytes: weak cells"),
                );
            }
        }
    }

    // ---- the mount and the sectors -------------------------------------------

    fn mount_and_sectors(&mut self) {
        let img = self.img;
        let (sectors, verdict, roots, chosen) = (
            &img.report.sectors,
            img.report.mount.clone(),
            &img.report.roots,
            img.report.chosen,
        );
        for s in sectors {
            match s.state {
                SectorState::Newer { version } => self.add(
                    Severity::Error,
                    "newer-sector",
                    Some(s.index),
                    format!(
                        "header has the magic and format version {version} (this tool reads \
                         version {}): the store refuses to mount",
                        crate::FORMAT_VERSION
                    ),
                ),
                SectorState::Unsupported { why } => self.add(
                    Severity::Error,
                    "unsupported-sector",
                    Some(s.index),
                    format!("header refuses the mount: {why}"),
                ),
                SectorState::NeedsErase { why } => self.add(
                    Severity::Note,
                    "needs-erase",
                    Some(s.index),
                    format!("not a trusted header ({why}); the writer erases it before use"),
                ),
                _ => {}
            }
            if let Some(why) = s.closed {
                self.add(
                    Severity::Warning,
                    "closed-sector",
                    Some(s.index),
                    format!(
                        "reading stopped at offset {}: {why}; nothing after it is read",
                        s.records_end
                    ),
                );
            }
            if s.header.is_some() && s.closed.is_none() && !s.tail_erased {
                self.add(
                    Severity::Note,
                    "dirty-tail",
                    Some(s.index),
                    format!(
                        "bytes after the last record (offset {}) are not all erased; a writer \
                         will not append here",
                        s.records_end
                    ),
                );
            }
            if s.retired {
                self.add(
                    Severity::Note,
                    "retired-sector",
                    Some(s.index),
                    "retired by the committed root: never opened, erased or collected again".into(),
                );
            }
        }
        match verdict {
            MountVerdict::Mounts => {}
            MountVerdict::Refused {
                sectors: refusing,
                rest_is_complete_store,
            } => {
                let list: Vec<String> = refusing.iter().map(|s| format!("{s}")).collect();
                let trusted = sectors.iter().filter(|s| s.header.is_some()).count();
                if rest_is_complete_store {
                    let seq = chosen.map_or(0, |i| roots[i].seq);
                    self.add(
                        Severity::Note,
                        "rest-complete",
                        None,
                        format!(
                            "mount refuses on sector(s) {}, but the other {trusted} trusted \
                             sector(s) hold a complete version-{} store (root seq {seq}): \
                             `extract` reads it",
                            list.join(", "),
                            crate::FORMAT_VERSION
                        ),
                    );
                } else {
                    self.add(
                        Severity::Error,
                        "rest-incomplete",
                        None,
                        format!(
                            "mount refuses on sector(s) {}, and the other {trusted} trusted \
                             sector(s) do not hold a complete store",
                            list.join(", ")
                        ),
                    );
                }
            }
            MountVerdict::NoStore { why } => self.add(
                Severity::Error,
                "no-store",
                None,
                format!("no store: {why}"),
            ),
        }
        // The newest root being unusable means the committed state fell back.
        for r in roots {
            if let RootOutcome::Unusable { why } = r.outcome {
                let fell_back = chosen.is_some();
                self.add(
                    Severity::Error,
                    "root-unusable",
                    Some(r.sector),
                    format!(
                        "root seq {} (sector {}, offset {}) has an incomplete closure ({why}){}",
                        r.seq,
                        r.sector,
                        r.offset,
                        if fell_back {
                            "; mount fell back to the root before it, losing that commit"
                        } else {
                            ""
                        }
                    ),
                );
            }
        }
    }

    // ---- the committed tree -------------------------------------------------

    fn committed_tree(&mut self) {
        let Some(root) = self.img.root.clone() else {
            return;
        };
        let chosen = self
            .img
            .report
            .chosen_root()
            .expect("a root was chosen")
            .clone();
        // The root record: its id hashes its payload; retired sectors exist.
        if let Some((_, payload)) = self.img.get(chosen.id) {
            self.id_matches(chosen.id, IdTag::Root, payload, "root");
        }
        for &s in &root.retired {
            if u32::from(s) >= self.img.report.sector_count {
                self.add(
                    Severity::Error,
                    "retired-range",
                    None,
                    format!("the root retires sector {s}, past the partition's end"),
                );
            }
        }
        self.check_dir(root.cold_dir.0, &mut Vec::new(), false, 0);
        self.check_dir(root.hot_dir.0, &mut Vec::new(), true, 0);
    }

    fn id_matches(&mut self, id: u64, tag: IdTag, bytes: &[u8], what: &str) -> bool {
        self.out.records_verified += 1;
        let want = ObjectId::of(self.hasher, tag, &[bytes]).0;
        if want != id {
            self.add(
                Severity::Error,
                "id-mismatch",
                self.img.kept.get(&id).map(|l| l.sector),
                format!("{what} {id:016x} holds bytes that hash to {want:016x}"),
            );
            return false;
        }
        true
    }

    fn check_dir(&mut self, id: u64, path: &mut Vec<u8>, hot: bool, depth: usize) {
        let shown = if path.is_empty() {
            String::from(if hot { "<hot directory>" } else { "/" })
        } else {
            String::from_utf8_lossy(path).into_owned()
        };
        let entries = match self.verify_dir(id) {
            Ok(e) => e,
            Err(why) => {
                self.add(
                    Severity::Error,
                    "dir-node",
                    self.img.kept.get(&id).map(|l| l.sector),
                    format!("directory {shown} ({id:016x}): {why}"),
                );
                return;
            }
        };
        if entries.is_empty() && depth > 0 {
            self.add(
                Severity::Error,
                "empty-dir",
                None,
                format!("directory {shown} is empty (only the root's directories may be)"),
            );
        }
        for w in entries.windows(2) {
            if (&w[0].name[..], w[0].kind) >= (&w[1].name[..], w[1].kind) {
                self.add(
                    Severity::Error,
                    "dir-order",
                    None,
                    format!(
                        "directory {shown}: entries {:?} and {:?} are not sorted by (name, kind)",
                        String::from_utf8_lossy(&w[0].name),
                        String::from_utf8_lossy(&w[1].name)
                    ),
                );
            }
        }
        for e in &entries {
            let len = path.len();
            if !(hot && depth == 0) {
                path.push(b'/');
            }
            path.extend_from_slice(&e.name);
            self.check_entry(e, path, hot, depth);
            path.truncate(len);
        }
    }

    fn check_entry(&mut self, e: &DirEntry, path: &mut Vec<u8>, hot: bool, depth: usize) {
        let shown = String::from_utf8_lossy(path).into_owned();
        let sector = self.img.kept.get(&e.id.0).map(|l| l.sector);
        // In the hot directory a name is a full path, with `/` in it.
        if let Some(why) = name_problem(&e.name, hot && depth == 0) {
            self.add(
                Severity::Error,
                "dir-name",
                sector,
                format!("{shown:?}: {why} (the writer's name rules; `list` reports it corrupt)"),
            );
        }
        if path.len() > usize::from(u16::MAX) || (!hot && depth + 1 > MAX_DEPTH) {
            self.add(
                Severity::Error,
                "path-limits",
                sector,
                format!("{shown:?}: deeper than {MAX_DEPTH} components or longer than 65535 bytes"),
            );
        }
        if hot {
            // The hot directory: flat, every entry a file at a full hot path.
            let hot_ok = e.kind == EntryKind::File
                && core::str::from_utf8(&e.name).is_ok_and(|p| valid_path(p) && is_hot(p));
            if !hot_ok {
                self.add(
                    Severity::Error,
                    "hot-entry",
                    sector,
                    format!(
                        "{shown:?} in the hot directory is not a file at a full path ending in \
                         /.lp/panel.json"
                    ),
                );
            }
        }
        match e.kind {
            EntryKind::File => {
                if !hot && is_hot(&shown) {
                    self.add(
                        Severity::Error,
                        "hot-file-in-cold-tree",
                        sector,
                        format!("{shown} is a hot file listed in the cold tree"),
                    );
                }
                match self.verify_file(e.id.0) {
                    Ok(n) if n == e.size => {
                        self.out.files_verified += 1;
                    }
                    Ok(n) => self.add(
                        Severity::Error,
                        "file-size",
                        sector,
                        format!(
                            "{shown}: the entry says {} bytes, the node holds {n}",
                            e.size
                        ),
                    ),
                    Err(why) => self.add(
                        Severity::Error,
                        "file-node",
                        sector,
                        format!("{shown} ({:016x}): {why}", e.id.0),
                    ),
                }
            }
            EntryKind::Dir => {
                if hot {
                    // Already flagged; do not descend into a hot "directory".
                    return;
                }
                self.check_dir(e.id.0, path, hot, depth + 1);
            }
        }
    }

    // ---- verified nodes ------------------------------------------------------

    /// A directory node: a one-record `Dir` or a directory multi; its id
    /// hashes what it holds and its entries decode.
    fn verify_dir(&mut self, id: u64) -> Result<Vec<DirEntry>, String> {
        let (rec, payload) = self
            .img
            .get(id)
            .ok_or_else(|| String::from("missing record"))?;
        match rec.kind {
            RecordKindReport::Dir => {
                self.verify_node(id)?;
            }
            RecordKindReport::Multi => {
                let m = parse_multi(payload).ok_or_else(|| String::from("multi does not parse"))?;
                if !m.dir {
                    return Err("a multi without the directory flag".into());
                }
                self.verify_node(id)?;
            }
            _ => return Err(format!("a {} record is not a directory", rec.kind.label())),
        }
        self.img.dir_entries(id).map_err(String::from)
    }

    /// A file node: a blob, or a multi without the directory flag.
    fn verify_file(&mut self, id: u64) -> Result<u32, String> {
        if let Some((rec, payload)) = self.img.get(id) {
            if rec.kind == RecordKindReport::Multi && parse_multi(payload).is_some_and(|m| m.dir) {
                return Err("a directory multi where a file is named".into());
            }
            if matches!(rec.kind, RecordKindReport::Dir | RecordKindReport::Root) {
                return Err(format!("a {} record is not a file", rec.kind.label()));
            }
        }
        self.verify_node(id)
    }

    /// A node and everything under it, hashed: the logical length, or what
    /// is wrong. Checked once per id.
    fn verify_node(&mut self, id: u64) -> Result<u32, String> {
        if let Some(done) = self.nodes.get(&id) {
            return done.clone();
        }
        let done = self.verify_node_uncached(id);
        self.nodes.insert(id, done.clone());
        done
    }

    fn verify_node_uncached(&mut self, id: u64) -> Result<u32, String> {
        let (rec, payload) = self
            .img
            .get(id)
            .ok_or_else(|| String::from("missing record"))?;
        match rec.kind {
            RecordKindReport::Blob => {
                let mut bytes = Vec::new();
                self.img
                    .decode_chunk(rec.codec, payload, &mut bytes)
                    .map_err(|_| {
                        format!("chunk {id:016x} does not inflate to its stated length")
                    })?;
                if !self.id_matches(id, IdTag::Blob, &bytes, "chunk") {
                    return Err(format!("chunk {id:016x} does not hash to its id"));
                }
                Ok(bytes.len() as u32)
            }
            RecordKindReport::Dir => {
                if !self.id_matches(id, IdTag::Dir, payload, "directory") {
                    return Err("directory record does not hash to its id".into());
                }
                Ok(payload.len() as u32)
            }
            RecordKindReport::Multi => {
                let m = parse_multi(payload).ok_or_else(|| String::from("multi does not parse"))?;
                if !self.id_matches(id, IdTag::Multi, payload, "multi") {
                    return Err(format!("multi {id:016x} does not hash to its id"));
                }
                let mut sum = 0u64;
                for i in 0..m.count {
                    let child = multi_child(payload, i).0;
                    let (crec, cpayload) = self
                        .img
                        .get(child)
                        .ok_or_else(|| String::from("missing record"))?;
                    match (m.level, crec.kind) {
                        (0, RecordKindReport::Blob) => {}
                        (l, RecordKindReport::Multi) if l > 0 => {
                            let ch = parse_multi(cpayload)
                                .ok_or_else(|| String::from("multi does not parse"))?;
                            if ch.level != l - 1 || ch.dir != m.dir {
                                return Err(format!(
                                    "multi {id:016x} (level {l}) has child {child:016x} of \
                                     level {} or the wrong directory flag",
                                    ch.level
                                ));
                            }
                        }
                        _ => {
                            return Err(format!(
                                "multi {id:016x} (level {}) names a {} record as a child",
                                m.level,
                                crec.kind.label()
                            ));
                        }
                    }
                    sum += u64::from(self.verify_node(child)?);
                }
                if sum != u64::from(m.total_len) {
                    return Err(format!(
                        "multi {id:016x} says {} bytes, its children hold {sum}",
                        m.total_len
                    ));
                }
                Ok(m.total_len)
            }
            RecordKindReport::Root => Err("a root is not a node".into()),
            RecordKindReport::Unknown { .. } => Err("an unknown record".into()),
        }
    }

    // ---- the rest of the flash -------------------------------------------------

    fn copies_and_orphans(&mut self) {
        let img = self.img;
        let sectors = &img.report.sectors;
        let mut by_id: BTreeMap<u64, Vec<Loc>> = BTreeMap::new();
        let (mut orphan_records, mut orphan_bytes) = (0u32, 0u64);
        for s in sectors {
            for (i, r) in s.records.iter().enumerate() {
                match r.status {
                    RecordStatus::Untrusted | RecordStatus::Unknown => {}
                    RecordStatus::Garbage => {
                        orphan_records += 1;
                        orphan_bytes += u64::from(r.total_len());
                        by_id.entry(r.id).or_default().push(Loc {
                            sector: s.index,
                            rec: i,
                        });
                    }
                    _ => by_id.entry(r.id).or_default().push(Loc {
                        sector: s.index,
                        rec: i,
                    }),
                }
            }
        }
        self.out.orphan_records = orphan_records;
        self.out.orphan_bytes = orphan_bytes;

        // Copies of one id are byte-identical by construction (FORMAT.md
        // "Record"), a stored and a deflated chunk by their logical bytes.
        let mut copies = 0u32;
        let mut differing = Vec::new();
        for (id, locs) in &by_id {
            if locs.len() < 2 {
                continue;
            }
            copies += locs.len() as u32 - 1;
            let first = self.content_of(locs[0]);
            for &other in &locs[1..] {
                let (a, b) = (&first, &self.content_of(other));
                if let (Some(a), Some(b)) = (a, b) {
                    if a != b {
                        differing.push((*id, locs[0], other));
                    }
                }
            }
        }
        for (id, a, b) in differing {
            self.add(
                Severity::Error,
                "copies-differ",
                Some(b.sector),
                format!(
                    "record {id:016x} is on flash in sector {} and sector {} with different \
                     contents",
                    a.sector, b.sector
                ),
            );
        }
        if copies > 0 {
            self.add(
                Severity::Note,
                "older-copies",
                None,
                format!(
                    "{copies} record(s) are on flash more than once (GC copies); the copies agree"
                ),
            );
        }
        if orphan_records > 0 {
            self.add(
                Severity::Note,
                "orphans",
                None,
                format!(
                    "{orphan_records} record(s), {orphan_bytes} bytes, are not reachable from \
                     the committed root (garbage until GC collects them)"
                ),
            );
        }
    }

    /// A record's identity-bearing contents: `(kind, bytes)`, a blob's
    /// bytes decoded; `None` when a blob does not decode.
    fn content_of(&self, at: Loc) -> Option<(&'static str, Vec<u8>)> {
        let rec = self.img.record(at);
        let payload = self.img.payload(at);
        if rec.kind == RecordKindReport::Blob {
            let mut out = Vec::new();
            self.img.decode_chunk(rec.codec, payload, &mut out).ok()?;
            Some(("blob", out))
        } else {
            Some((rec.kind.label(), payload.to_vec()))
        }
    }
}
