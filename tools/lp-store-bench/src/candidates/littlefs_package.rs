//! **F2 — littlefs + one package file per project.** The same littlefs as
//! F1, but every path under `/projects/<slot>/` (except `.lp/…`) lives in one
//! file, `/projects/<slot>.pkg` (format: [`package_format`]). Members are
//! deflated on write (the host's job in the real design; `miniz_oxide` here)
//! and inflated with `lp-deflate`, the device's own decoder, on every read.
//!
//! - **Writes within a step are buffered** (already encoded) until `commit`,
//!   which writes the slot's new package to `<slot>.pkg.tmp` — surviving old
//!   members copied as stored bytes, never recompressed — and renames it over
//!   `<slot>.pkg`, which littlefs does in one metadata commit. A package is
//!   therefore old-or-new as a whole. The board's files and `.lp/` files stay
//!   plain littlefs files, written at `put`, and a step can touch several
//!   slots, so the step as a whole is **not** atomic (`step_atomic: false`).
//! - **Plain files are atomic per file.** An existing one is rewritten in
//!   place (littlefs keeps the old content until the close commits). A *new*
//!   one is written to `.f2-new.tmp` in its own directory and renamed into
//!   place, because littlefs commits a new file's entry at `open`, before its
//!   data (F1's empty-new-file result). The temporary sits beside its target
//!   on purpose: littlefs-rust 0.1.0 loses a source-directory entry when a
//!   cut lands late in a rename *across* directories (pinned in
//!   `littlefs_volume.rs`), and a same-directory rename is one commit.
//! - **`get` seeks**: it reads the header and index, then only that member.
//! - **Stale temporaries** (a cut mid-commit or mid-put) are removed at the
//!   first commit after a mount (one directory walk), or overwritten first;
//!   until then they only cost space.
//! - Plain files directly in `/projects/` named `*.pkg` / `*.pkg.tmp`, and
//!   any file named `.f2-new.tmp`, are reserved (refused).
//!
//! RAM: littlefs with two files open (the commit copies old → tmp), a 512 B
//! copy buffer, and the transient read path — the largest package index and
//! the largest member's stored bytes (`lp-deflate` decodes from a whole
//! input slice). The raw output is the caller's document buffer, exactly as
//! with F1's whole-file read, so neither adapter counts it. The step's
//! write buffer is a testbed simplification (a device would stream members
//! into the temporary as they arrive), so it is reported apart, not in
//! `ram_bytes`: `extra.step_buffer_peak_bytes`, the most this mount has
//! buffered, and `extra.step_buffer_push_bytes`, what pushing the largest
//! committed package again would buffer (its stored bytes) — the harness
//! reports a fresh mount, where the first is 0.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use littlefs_rust::{Error as LfsError, FileType, OpenFlags, SeekFrom};
use lp_nor_sim::NorFlashSim;

use crate::candidates::littlefs_volume::{CACHE_SIZE, LfsFile, LfsVolume, lfs_ram_bytes};
use crate::candidates::package_format::{
    MemberCodec, PKG_HEADER, PkgHeader, PkgMember, decode_index, decode_member, encode_head,
    encode_member, head_len,
};
use crate::{Candidate, CandidateConfig, CandidateReport, CandidateStore, StoreError};

const PROJECTS: &str = "/projects";
const COPY_BUF: usize = CACHE_SIZE as usize;
/// The name a new plain file is written under, in its own directory,
/// before it is renamed into place.
const NEW_TMP: &str = ".f2-new.tmp";

pub struct LittlefsPackage;

impl Candidate for LittlefsPackage {
    fn name(&self) -> &str {
        "f2"
    }

    fn format(&self, flash: &mut NorFlashSim, cfg: &CandidateConfig) -> Result<(), StoreError> {
        LfsVolume::format(flash, cfg)
    }

    fn mount(
        &self,
        flash: NorFlashSim,
        cfg: &CandidateConfig,
    ) -> Result<Box<dyn CandidateStore>, (StoreError, NorFlashSim)> {
        Ok(Box::new(PackageStore {
            vol: LfsVolume::mount(flash, cfg)?,
            pending: BTreeMap::new(),
            swept_tmp: false,
            buffer_peak: Cell::new(0),
            plain_deletes: Vec::new(),
            plain_rewritten: std::collections::BTreeSet::new(),
        }))
    }
}

/// A member as it will be written.
struct EncodedMember {
    codec: MemberCodec,
    stored: Vec<u8>,
    raw_len: u32,
    raw_crc: u32,
}

/// One slot's edits since the last commit.
#[derive(Default)]
struct SlotEdit {
    /// Every member of the committed package is gone (a prefix covered it).
    cleared: bool,
    /// rel → new member, or `None` = deleted.
    members: BTreeMap<String, Option<EncodedMember>>,
}

struct PackageStore {
    vol: LfsVolume,
    pending: BTreeMap<String, SlotEdit>,
    swept_tmp: bool,
    buffer_peak: Cell<u64>,
    /// Prefixes deleted this step over plain files: applied at `commit`, so a
    /// delete-then-rewrite (a re-push's `.lp/panel.json`) never passes
    /// through "missing". Until then such files read as gone unless put again.
    plain_deletes: Vec<String>,
    /// Plain files put this step (they survive this step's deletes).
    plain_rewritten: std::collections::BTreeSet<String>,
}

/// `/projects/<slot>/<rel>` with `rel` outside `.lp/` → (slot, rel).
fn member_of(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/projects/")?;
    let (slot, rel) = rest.split_once('/')?;
    if slot.is_empty() || rel.is_empty() || rel == ".lp" || rel.starts_with(".lp/") {
        return None;
    }
    Some((slot, rel))
}

fn pkg_path(slot: &str) -> String {
    format!("{PROJECTS}/{slot}.pkg")
}

fn tmp_path(slot: &str) -> String {
    format!("{PROJECTS}/{slot}.pkg.tmp")
}

/// A package or temporary file name, directly in `/projects/`.
fn is_package_file(path: &str) -> bool {
    path.strip_prefix("/projects/")
        .is_some_and(|n| !n.contains('/') && (n.ends_with(".pkg") || n.ends_with(".pkg.tmp")))
}

/// A name the store keeps for itself.
fn is_reserved(path: &str) -> bool {
    is_package_file(path) || path.rsplit('/').next() == Some(NEW_TMP)
}

/// The new-file temporary beside `path`.
fn new_tmp_for(path: &str) -> String {
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    format!("{dir}/{NEW_TMP}")
}

/// Can a path under this slot's directory start with `prefix`?
fn slot_reachable(slot: &str, prefix: &str) -> bool {
    let dir = format!("{PROJECTS}/{slot}/");
    dir.starts_with(prefix) || prefix.starts_with(&dir)
}

impl PackageStore {
    /// A plain file this step deleted and has not put again.
    fn plain_doomed(&self, path: &str) -> bool {
        !self.plain_rewritten.contains(path)
            && self
                .plain_deletes
                .iter()
                .any(|d| path.starts_with(d.as_str()))
    }

    fn pending_bytes(&self) -> u64 {
        self.pending
            .values()
            .flat_map(|e| e.members.values().flatten())
            .map(|m| m.stored.len() as u64)
            .sum()
    }

    /// Slots with a committed package, plus slots with edits.
    fn slots(&self) -> Result<BTreeSet<String>, StoreError> {
        let mut out: BTreeSet<String> = self.pending.keys().cloned().collect();
        out.extend(self.committed_slots()?);
        Ok(out)
    }

    fn committed_slots(&self) -> Result<Vec<String>, StoreError> {
        let entries = match self.vol.list_dir(PROJECTS) {
            Ok(e) => e,
            Err(LfsError::NoEntry) => return Ok(Vec::new()),
            Err(e) => return Err(self.vol.err(e)),
        };
        Ok(entries
            .into_iter()
            .filter(|e| e.file_type == FileType::File)
            .filter_map(|e| e.name.strip_suffix(".pkg").map(str::to_string))
            .collect())
    }

    /// The committed package's index, or `None` when it has none.
    fn read_index(&self, slot: &str) -> Result<Option<Vec<PkgMember>>, StoreError> {
        let f = match self.vol.open(&pkg_path(slot), OpenFlags::READ) {
            Ok(f) => f,
            Err(LfsError::NoEntry) => return Ok(None),
            Err(e) => return Err(self.vol.err(e)),
        };
        let idx = self.read_index_from(&f)?;
        f.close().map_err(|e| self.vol.err(e))?;
        Ok(Some(idx))
    }

    fn read_index_from(&self, f: &LfsFile<'_>) -> Result<Vec<PkgMember>, StoreError> {
        let mut h = [0u8; PKG_HEADER];
        read_exact(f, &mut h).map_err(|e| self.vol.err(e))?;
        let h = PkgHeader::parse(&h).map_err(StoreError::Corrupt)?;
        if h.index_len as usize > f.size() as usize {
            return Err(StoreError::Corrupt("package index past its end".into()));
        }
        let mut idx = vec![0u8; h.index_len as usize];
        read_exact(f, &mut idx).map_err(|e| self.vol.err(e))?;
        decode_index(&h, &idx).map_err(StoreError::Corrupt)
    }

    /// One committed member, by seeking.
    fn read_member(&self, slot: &str, rel: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let f = match self.vol.open(&pkg_path(slot), OpenFlags::READ) {
            Ok(f) => f,
            Err(LfsError::NoEntry) => return Ok(None),
            Err(e) => return Err(self.vol.err(e)),
        };
        let idx = self.read_index_from(&f)?;
        let Some(m) = idx.iter().find(|m| m.path == rel) else {
            return Ok(None);
        };
        if m.offset as u64 + m.stored_len as u64 > f.size() as u64 {
            return Err(StoreError::Corrupt(format!("member {rel} past its end")));
        }
        f.seek(SeekFrom::Start(m.offset))
            .map_err(|e| self.vol.err(e))?;
        let mut stored = vec![0u8; m.stored_len as usize];
        read_exact(&f, &mut stored).map_err(|e| self.vol.err(e))?;
        f.close().map_err(|e| self.vol.err(e))?;
        decode_member(m, &stored)
            .map(Some)
            .map_err(StoreError::Corrupt)
    }

    /// The paths a slot holds now: committed (unless cleared) with edits laid on.
    fn slot_paths(&self, slot: &str) -> Result<Vec<String>, StoreError> {
        let edit = self.pending.get(slot);
        let mut rels: BTreeSet<String> = BTreeSet::new();
        if !edit.is_some_and(|e| e.cleared)
            && let Some(idx) = self.read_index(slot)?
        {
            rels.extend(idx.into_iter().map(|m| m.path));
        }
        if let Some(e) = edit {
            for (rel, m) in &e.members {
                if m.is_some() {
                    rels.insert(rel.clone());
                } else {
                    rels.remove(rel);
                }
            }
        }
        Ok(rels
            .into_iter()
            .map(|r| format!("{PROJECTS}/{slot}/{r}"))
            .collect())
    }

    /// Write `slot`'s next package (or remove it when empty).
    fn commit_slot(&self, slot: &str, edit: &SlotEdit) -> Result<(), StoreError> {
        let old = if edit.cleared {
            None
        } else {
            self.read_index(slot)?
        };
        enum Src<'a> {
            Old(u32),
            New(&'a EncodedMember),
        }
        let mut members: Vec<(PkgMember, Src<'_>)> = Vec::new();
        for m in old.iter().flatten() {
            if !edit.members.contains_key(&m.path) {
                members.push((m.clone(), Src::Old(m.offset)));
            }
        }
        for (rel, m) in &edit.members {
            if let Some(m) = m {
                members.push((
                    PkgMember {
                        path: rel.clone(),
                        offset: 0,
                        stored_len: m.stored.len() as u32,
                        raw_len: m.raw_len,
                        raw_crc: m.raw_crc,
                        codec: m.codec,
                    },
                    Src::New(m),
                ));
            }
        }
        let (pkg, tmp) = (pkg_path(slot), tmp_path(slot));
        if members.is_empty() {
            return self.vol.remove(&pkg);
        }
        members.sort_by(|a, b| a.0.path.cmp(&b.0.path));
        let mut index: Vec<PkgMember> = members.iter().map(|(m, _)| m.clone()).collect();
        let mut off = head_len(&index) as u32;
        for m in &mut index {
            m.offset = off;
            off += m.stored_len;
        }
        let head = encode_head(&index);

        self.vol.ensure_parent_dirs(&tmp)?;
        self.vol.remove(&tmp)?;
        let err = |e| self.vol.err(e);
        let out = self
            .vol
            .open(
                &tmp,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNC,
            )
            .map_err(err)?;
        write_all(&out, &head).map_err(err)?;
        let src = if old.is_some() {
            Some(self.vol.open(&pkg, OpenFlags::READ).map_err(err)?)
        } else {
            None
        };
        let mut buf = [0u8; COPY_BUF];
        for (m, s) in &members {
            match s {
                Src::New(e) => write_all(&out, &e.stored).map_err(err)?,
                Src::Old(at) => {
                    let src = src.as_ref().expect("old package is open");
                    src.seek(SeekFrom::Start(*at)).map_err(err)?;
                    let mut left = m.stored_len as usize;
                    while left > 0 {
                        let n = left.min(COPY_BUF);
                        read_exact(src, &mut buf[..n]).map_err(err)?;
                        write_all(&out, &buf[..n]).map_err(err)?;
                        left -= n;
                    }
                }
            }
        }
        if let Some(src) = src {
            src.close().map_err(err)?;
        }
        out.close().map_err(err)?;
        self.vol.rename(&tmp, &pkg)
    }

    /// Remove every `*.pkg.tmp` (and the new-file temporary) a cut left
    /// behind.
    fn sweep_tmp(&self) -> Result<(), StoreError> {
        for p in self.vol.files_under("/")? {
            if p.rsplit('/').next() == Some(NEW_TMP) {
                self.vol.remove(&p)?;
            }
        }
        let entries = match self.vol.list_dir(PROJECTS) {
            Ok(e) => e,
            Err(LfsError::NoEntry) => return Ok(()),
            Err(e) => return Err(self.vol.err(e)),
        };
        for e in entries {
            if e.file_type == FileType::File && e.name.ends_with(".pkg.tmp") {
                self.vol.remove(&format!("{PROJECTS}/{}", e.name))?;
            }
        }
        Ok(())
    }

    /// The stored bytes of the largest committed package's members.
    fn largest_package_stored(&self) -> u64 {
        self.committed_slots()
            .unwrap_or_default()
            .iter()
            .filter_map(|slot| self.read_index(slot).ok().flatten())
            .map(|idx| idx.iter().map(|m| m.stored_len as u64).sum::<u64>())
            .max()
            .unwrap_or(0)
    }

    /// The read path's transient RAM: the largest index and stored member.
    fn read_path_ram(&self) -> u64 {
        let mut most = 0u64;
        for slot in self.committed_slots().unwrap_or_default() {
            if let Ok(Some(idx)) = self.read_index(&slot) {
                let index_bytes = head_len(&idx) as u64
                    + (idx.len() * std::mem::size_of::<PkgMember>()) as u64
                    + idx.iter().map(|m| m.path.len() as u64).sum::<u64>();
                // The compressed input buffer; the raw output is the caller's
                // document buffer, as with F1's whole-file read.
                let member = idx.iter().map(|m| m.stored_len as u64).max().unwrap_or(0);
                most = most.max(index_bytes + member);
            }
        }
        most
    }
}

pub(super) fn read_exact(f: &LfsFile<'_>, buf: &mut [u8]) -> Result<(), LfsError> {
    let mut got = 0;
    while got < buf.len() {
        let n = f.read(&mut buf[got..])? as usize;
        if n == 0 {
            return Err(LfsError::Corrupt);
        }
        got += n;
    }
    Ok(())
}

pub(super) fn write_all(f: &LfsFile<'_>, data: &[u8]) -> Result<(), LfsError> {
    let mut off = 0;
    while off < data.len() {
        off += f.write(&data[off..])? as usize;
    }
    Ok(())
}

impl CandidateStore for PackageStore {
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        if let Some((slot, rel)) = member_of(path) {
            let (codec, stored) = encode_member(bytes);
            let m = EncodedMember {
                codec,
                stored,
                raw_len: bytes.len() as u32,
                raw_crc: lp_crc32::crc32(bytes),
            };
            self.pending
                .entry(slot.into())
                .or_default()
                .members
                .insert(rel.into(), Some(m));
            let b = self.pending_bytes();
            self.buffer_peak.set(self.buffer_peak.get().max(b));
            return Ok(());
        }
        if is_reserved(path) {
            return Err(StoreError::Other(format!("{path} is a reserved name")));
        }
        self.plain_rewritten.insert(path.into());
        if self.vol.exists(path)? {
            return self.vol.write_file(path, bytes);
        }
        let tmp = new_tmp_for(path);
        self.vol.remove(&tmp)?;
        self.vol.write_file(&tmp, bytes)?;
        self.vol.rename(&tmp, path)
    }

    fn get(&mut self, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        if let Some((slot, rel)) = member_of(path) {
            if let Some(edit) = self.pending.get(slot) {
                match edit.members.get(rel) {
                    Some(Some(m)) => {
                        let pm = PkgMember {
                            path: rel.into(),
                            offset: 0,
                            stored_len: m.stored.len() as u32,
                            raw_len: m.raw_len,
                            raw_crc: m.raw_crc,
                            codec: m.codec,
                        };
                        return decode_member(&pm, &m.stored)
                            .map(Some)
                            .map_err(StoreError::Corrupt);
                    }
                    Some(None) => return Ok(None),
                    None if edit.cleared => return Ok(None),
                    None => {}
                }
            }
            return self.read_member(slot, rel);
        }
        if is_reserved(path) || self.plain_doomed(path) {
            return Ok(None);
        }
        self.vol.read_file(path)
    }

    fn delete_prefix(&mut self, prefix: &str) -> Result<(), StoreError> {
        self.plain_deletes.push(prefix.into());
        self.plain_rewritten.retain(|p| !p.starts_with(prefix));
        for slot in self.slots()? {
            if !slot_reachable(&slot, prefix) {
                continue;
            }
            if format!("{PROJECTS}/{slot}/").starts_with(prefix) {
                let e = self.pending.entry(slot).or_default();
                e.cleared = true;
                e.members.clear();
                continue;
            }
            let doomed: Vec<String> = self
                .slot_paths(&slot)?
                .into_iter()
                .filter(|p| p.starts_with(prefix))
                .collect();
            let e = self.pending.entry(slot.clone()).or_default();
            for p in doomed {
                if let Some((_, rel)) = member_of(&p) {
                    e.members.insert(rel.into(), None);
                }
            }
        }
        Ok(())
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let mut out: Vec<String> = self
            .vol
            .files_under(prefix)?
            .into_iter()
            .filter(|p| !is_reserved(p) && !self.plain_doomed(p))
            .collect();
        for slot in self.slots()? {
            if slot_reachable(&slot, prefix) {
                out.extend(
                    self.slot_paths(&slot)?
                        .into_iter()
                        .filter(|p| p.starts_with(prefix)),
                );
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    fn commit(&mut self) -> Result<(), StoreError> {
        if !self.swept_tmp {
            self.sweep_tmp()?;
            self.swept_tmp = true;
        }
        let pending = std::mem::take(&mut self.pending);
        for (slot, edit) in &pending {
            if let Err(e) = self.commit_slot(slot, edit) {
                self.pending = pending;
                return Err(e);
            }
        }
        let rewritten = std::mem::take(&mut self.plain_rewritten);
        for prefix in std::mem::take(&mut self.plain_deletes) {
            self.vol
                .delete_prefix(&prefix, &|p| is_package_file(p) || rewritten.contains(p))?;
        }
        Ok(())
    }

    fn into_flash(self: Box<Self>) -> NorFlashSim {
        self.vol.into_flash()
    }

    fn flash_snapshot(&self) -> NorFlashSim {
        self.vol.snapshot()
    }

    fn report(&self) -> CandidateReport {
        let mut extra = BTreeMap::new();
        extra.insert("open_files_max".into(), 2.0);
        extra.insert(
            "step_buffer_peak_bytes".into(),
            self.buffer_peak.get() as f64,
        );
        extra.insert(
            "step_buffer_push_bytes".into(),
            self.largest_package_stored() as f64,
        );
        CandidateReport {
            ram_bytes: lfs_ram_bytes(2) + COPY_BUF as u64 + self.read_path_ram(),
            used_sectors: self.vol.used_blocks(),
            step_atomic: false,
            extra,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::candidate_tests::{round_trip, small_sweep};

    #[test]
    fn members_and_plain_files_split_where_the_design_says() {
        assert_eq!(
            member_of("/projects/a/modules/x/shader.glsl"),
            Some(("a", "modules/x/shader.glsl"))
        );
        assert_eq!(member_of("/projects/a/.lp/panel.json"), None);
        assert_eq!(member_of("/projects/a.pkg"), None);
        assert_eq!(member_of("/hardware.json"), None);
        assert!(is_package_file("/projects/a.pkg"));
        assert!(is_package_file("/projects/a.pkg.tmp"));
        assert!(!is_package_file("/projects/a/b.pkg"));
        assert!(is_reserved("/.lp/.f2-new.tmp"));
        assert_eq!(new_tmp_for("/hardware.json"), "/.f2-new.tmp");
        assert_eq!(
            new_tmp_for("/projects/a/.lp/panel.json"),
            "/projects/a/.lp/.f2-new.tmp"
        );
    }

    #[test]
    fn round_trips_fault_free() {
        round_trip(&LittlefsPackage);
    }

    #[test]
    fn small_exhaustive_sweep_runs() {
        let s = small_sweep(&LittlefsPackage);
        eprintln!("f2 small sweep: {s:?}");
        assert!(s.cases > 0);
    }
}
