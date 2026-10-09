//! **F3 — littlefs + one package file per pattern** (the control candidate).
//! The same littlefs volume as F1 and F2, but each pattern's files
//! (`/projects/<slot>/modules/<p>/…`) live in one package,
//! `/projects/<slot>/modules/<p>.pkg` (format: [`package_format`], members
//! deflated on write, inflated with `lp-deflate`). Everything else — the
//! project's top-level files, `.lp/`, the board's files — stays a plain
//! littlefs file. A save that edits one shader rewrites that pattern's
//! package (a few KB), not the whole project as F2 does.
//!
//! - **Packages are written at `commit`.** Puts into a pattern are buffered
//!   (already encoded); `commit` writes each touched pattern's next package
//!   to `<p>.pkg.tmp` — surviving old members copied as stored bytes — and
//!   renames it over `<p>.pkg`. The temporary sits beside its target, in the
//!   same `modules/` directory, so the rename is one metadata commit and never
//!   crosses directories (littlefs-rust 0.1.0 loses a source-directory entry
//!   when a cut lands late in a cross-directory rename,
//!   `docs/defects/2026-10-08-littlefs-rust-cross-directory-rename-cut-loses-the-next-entry.md`).
//!   Each package is old-or-new as a whole; a step can touch several
//!   packages and plain files, so the step is **not** atomic
//!   (`step_atomic: false`).
//! - **Plain files are atomic per file**, as in F2: an existing one is
//!   rewritten in place; a new one is written to `.f3-new.tmp` in its own
//!   directory and renamed into place.
//! - **Deletes are applied at `commit`**: a prefix delete marks package
//!   members gone in the step's edits and queues the plain-file delete, so a
//!   delete-then-rewrite (a re-push) never passes through "missing".
//! - **`block_cycles` defaults to 100** (littlefs's metadata-pair wear
//!   levelling; the value that levelled F1's hot pair from 21,602 to 711
//!   erases in 30 simulated days). `f3@block_cycles=-1` runs it off, as the
//!   firmware ships. At 100 littlefs runs its relocation path, which a cut can
//!   leave unmountable
//!   (`docs/defects/2026-10-08-littlefs-rust-relocation-cut-leaves-lpfs-unmountable.md`):
//!   a cut failure there is a result about littlefs-rust, not this adapter.
//! - **Stale temporaries** (a cut mid-commit or mid-put) are removed at the
//!   first commit after a mount, or overwritten first.
//! - Plain files directly in a `modules/` directory named `*.pkg` /
//!   `*.pkg.tmp`, and any file named `.f3-new.tmp`, are reserved (refused).
//!
//! RAM, as F2 counts it: littlefs with two files open (the commit copies
//! old → tmp), a 512 B copy buffer, and the read path's transient — the
//! largest package index and the largest member's stored bytes. The step's
//! write buffer is a testbed simplification and is reported apart
//! (`extra.step_buffer_peak_bytes`, `extra.step_buffer_push_bytes` — the
//! largest single package's stored bytes, what a device streaming one
//! package at a time would hold).

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use littlefs_rust::{Error as LfsError, FileType, OpenFlags, SeekFrom};
use lp_nor_sim::NorFlashSim;

use crate::candidates::littlefs_package::{read_exact, write_all};
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
const NEW_TMP: &str = ".f3-new.tmp";
/// F3's `block_cycles` when the dial is unset.
pub const F3_BLOCK_CYCLES: &str = "100";

pub struct LittlefsPatternPackage;

impl Candidate for LittlefsPatternPackage {
    fn name(&self) -> &str {
        "f3"
    }

    fn format(&self, flash: &mut NorFlashSim, cfg: &CandidateConfig) -> Result<(), StoreError> {
        LfsVolume::format(flash, &f3_config(cfg))
    }

    fn mount(
        &self,
        flash: NorFlashSim,
        cfg: &CandidateConfig,
    ) -> Result<Box<dyn CandidateStore>, (StoreError, NorFlashSim)> {
        Ok(Box::new(PatternPackageStore {
            vol: LfsVolume::mount(flash, &f3_config(cfg))?,
            pending: BTreeMap::new(),
            swept_tmp: false,
            buffer_peak: Cell::new(0),
            plain_deletes: Vec::new(),
            plain_rewritten: BTreeSet::new(),
        }))
    }
}

/// `cfg` with F3's default `block_cycles` filled in when the dial is unset.
pub fn f3_config(cfg: &CandidateConfig) -> CandidateConfig {
    let mut c = cfg.clone();
    c.dials
        .entry("block_cycles".into())
        .or_insert_with(|| F3_BLOCK_CYCLES.into());
    c
}

/// A member as it will be written.
struct EncodedMember {
    codec: MemberCodec,
    stored: Vec<u8>,
    raw_len: u32,
    raw_crc: u32,
}

impl EncodedMember {
    fn index_entry(&self, rel: &str) -> PkgMember {
        PkgMember {
            path: rel.into(),
            offset: 0,
            stored_len: self.stored.len() as u32,
            raw_len: self.raw_len,
            raw_crc: self.raw_crc,
            codec: self.codec,
        }
    }
}

/// One pattern's edits since the last commit.
#[derive(Default)]
struct PatternEdit {
    /// Every member of the committed package is gone (a prefix covered it).
    cleared: bool,
    /// rel → new member, or `None` = deleted.
    members: BTreeMap<String, Option<EncodedMember>>,
}

struct PatternPackageStore {
    vol: LfsVolume,
    /// Keyed by the pattern directory, `/projects/<slot>/modules/<p>`.
    pending: BTreeMap<String, PatternEdit>,
    swept_tmp: bool,
    buffer_peak: Cell<u64>,
    /// Prefixes deleted this step over plain files, applied at `commit`.
    plain_deletes: Vec<String>,
    /// Plain files put this step (they survive this step's deletes).
    plain_rewritten: BTreeSet<String>,
}

/// `/projects/<slot>/modules/<p>/<rel>` → (pattern dir, rel).
fn member_of(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/projects/")?;
    let (slot, rest) = rest.split_once('/')?;
    let rest = rest.strip_prefix("modules/")?;
    let (pattern, rel) = rest.split_once('/')?;
    if slot.is_empty() || pattern.is_empty() || rel.is_empty() {
        return None;
    }
    let key_len = "/projects/".len() + slot.len() + "/modules/".len() + pattern.len();
    Some((&path[..key_len], rel))
}

fn pkg_path(key: &str) -> String {
    format!("{key}.pkg")
}

fn tmp_path(key: &str) -> String {
    format!("{key}.pkg.tmp")
}

/// A package or its temporary, directly in a project's `modules/`.
fn is_package_file(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/projects/") else {
        return false;
    };
    let Some((slot, rest)) = rest.split_once('/') else {
        return false;
    };
    let Some(name) = rest.strip_prefix("modules/") else {
        return false;
    };
    !slot.is_empty()
        && !name.contains('/')
        && (name.ends_with(".pkg") || name.ends_with(".pkg.tmp"))
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

/// Can a path under directory `dir` (no trailing slash) start with `prefix`?
fn reachable(dir: &str, prefix: &str) -> bool {
    let dir = format!("{dir}/");
    dir.starts_with(prefix) || prefix.starts_with(&dir)
}

impl PatternPackageStore {
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

    /// Patterns with a committed package or with edits, reachable by `prefix`.
    fn patterns(&self, prefix: &str) -> Result<BTreeSet<String>, StoreError> {
        let mut out: BTreeSet<String> = self
            .pending
            .keys()
            .filter(|k| reachable(k, prefix))
            .cloned()
            .collect();
        out.extend(
            self.committed_patterns(prefix)?
                .into_iter()
                .filter(|k| reachable(k, prefix)),
        );
        Ok(out)
    }

    /// Pattern directories with a committed package, in the slots `prefix`
    /// can reach.
    fn committed_patterns(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let slots = match self.vol.list_dir(PROJECTS) {
            Ok(e) => e,
            Err(LfsError::NoEntry) => return Ok(Vec::new()),
            Err(e) => return Err(self.vol.err(e)),
        };
        let mut out = Vec::new();
        for s in slots.into_iter().filter(|e| e.file_type == FileType::Dir) {
            let modules = format!("{PROJECTS}/{}/modules", s.name);
            if !reachable(&modules, prefix) {
                continue;
            }
            let entries = match self.vol.list_dir(&modules) {
                Ok(e) => e,
                Err(LfsError::NoEntry | LfsError::NotDir) => continue,
                Err(e) => return Err(self.vol.err(e)),
            };
            out.extend(
                entries
                    .into_iter()
                    .filter(|e| e.file_type == FileType::File)
                    .filter_map(|e| {
                        e.name
                            .strip_suffix(".pkg")
                            .map(|p| format!("{modules}/{p}"))
                    }),
            );
        }
        Ok(out)
    }

    /// The committed package's index, or `None` when it has none.
    fn read_index(&self, key: &str) -> Result<Option<Vec<PkgMember>>, StoreError> {
        let f = match self.vol.open(&pkg_path(key), OpenFlags::READ) {
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
    fn read_member(&self, key: &str, rel: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let f = match self.vol.open(&pkg_path(key), OpenFlags::READ) {
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

    /// The paths a pattern holds now: committed (unless cleared) with edits
    /// laid on.
    fn pattern_paths(&self, key: &str) -> Result<Vec<String>, StoreError> {
        let edit = self.pending.get(key);
        let mut rels: BTreeSet<String> = BTreeSet::new();
        if !edit.is_some_and(|e| e.cleared)
            && let Some(idx) = self.read_index(key)?
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
        Ok(rels.into_iter().map(|r| format!("{key}/{r}")).collect())
    }

    /// Write the pattern's next package (or remove it when empty).
    fn commit_pattern(&self, key: &str, edit: &PatternEdit) -> Result<(), StoreError> {
        let old = if edit.cleared {
            None
        } else {
            self.read_index(key)?
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
                members.push((m.index_entry(rel), Src::New(m)));
            }
        }
        let (pkg, tmp) = (pkg_path(key), tmp_path(key));
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

    /// Remove every package temporary and new-file temporary a cut left.
    fn sweep_tmp(&self) -> Result<(), StoreError> {
        for p in self.vol.files_under("/")? {
            if p.rsplit('/').next() == Some(NEW_TMP) || (is_package_file(&p) && p.ends_with(".tmp"))
            {
                self.vol.remove(&p)?;
            }
        }
        Ok(())
    }

    /// Every committed package's index.
    fn all_indexes(&self) -> Vec<Vec<PkgMember>> {
        self.committed_patterns("/")
            .unwrap_or_default()
            .iter()
            .filter_map(|k| self.read_index(k).ok().flatten())
            .collect()
    }
}

impl CandidateStore for PatternPackageStore {
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        if let Some((key, rel)) = member_of(path) {
            let (codec, stored) = encode_member(bytes);
            let m = EncodedMember {
                codec,
                stored,
                raw_len: bytes.len() as u32,
                raw_crc: lp_crc32::crc32(bytes),
            };
            self.pending
                .entry(key.into())
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
        if let Some((key, rel)) = member_of(path) {
            if let Some(edit) = self.pending.get(key) {
                match edit.members.get(rel) {
                    Some(Some(m)) => {
                        return decode_member(&m.index_entry(rel), &m.stored)
                            .map(Some)
                            .map_err(StoreError::Corrupt);
                    }
                    Some(None) => return Ok(None),
                    None if edit.cleared => return Ok(None),
                    None => {}
                }
            }
            return self.read_member(key, rel);
        }
        if is_reserved(path) || self.plain_doomed(path) {
            return Ok(None);
        }
        self.vol.read_file(path)
    }

    fn delete_prefix(&mut self, prefix: &str) -> Result<(), StoreError> {
        self.plain_deletes.push(prefix.into());
        self.plain_rewritten.retain(|p| !p.starts_with(prefix));
        for key in self.patterns(prefix)? {
            if format!("{key}/").starts_with(prefix) {
                let e = self.pending.entry(key).or_default();
                e.cleared = true;
                e.members.clear();
                continue;
            }
            let doomed: Vec<String> = self
                .pattern_paths(&key)?
                .into_iter()
                .filter(|p| p.starts_with(prefix))
                .collect();
            let e = self.pending.entry(key.clone()).or_default();
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
        for key in self.patterns(prefix)? {
            out.extend(
                self.pattern_paths(&key)?
                    .into_iter()
                    .filter(|p| p.starts_with(prefix)),
            );
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
        for (key, edit) in &pending {
            if let Err(e) = self.commit_pattern(key, edit) {
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
        let indexes = self.all_indexes();
        // The read path's transient: the largest index plus the largest
        // stored member (the raw output is the caller's document buffer).
        let read_path = indexes
            .iter()
            .map(|idx| {
                head_len(idx) as u64
                    + (idx.len() * std::mem::size_of::<PkgMember>()) as u64
                    + idx.iter().map(|m| m.path.len() as u64).sum::<u64>()
                    + idx.iter().map(|m| m.stored_len as u64).max().unwrap_or(0)
            })
            .max()
            .unwrap_or(0);
        let largest_package = indexes
            .iter()
            .map(|idx| idx.iter().map(|m| m.stored_len as u64).sum::<u64>())
            .max()
            .unwrap_or(0);
        let mut extra = BTreeMap::new();
        extra.insert("open_files_max".into(), 2.0);
        extra.insert("packages".into(), indexes.len() as f64);
        extra.insert(
            "step_buffer_peak_bytes".into(),
            self.buffer_peak.get() as f64,
        );
        extra.insert("step_buffer_push_bytes".into(), largest_package as f64);
        CandidateReport {
            ram_bytes: lfs_ram_bytes(2) + COPY_BUF as u64 + read_path,
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
    use crate::candidates::littlefs_volume::lfs_config;

    #[test]
    fn members_and_plain_files_split_where_the_design_says() {
        assert_eq!(
            member_of("/projects/a/modules/x/shader.glsl"),
            Some(("/projects/a/modules/x", "shader.glsl"))
        );
        assert_eq!(
            member_of("/projects/a/modules/x/sub/deep.json"),
            Some(("/projects/a/modules/x", "sub/deep.json"))
        );
        assert_eq!(member_of("/projects/a/modules/x.pkg"), None);
        assert_eq!(member_of("/projects/a/project.json"), None);
        assert_eq!(member_of("/projects/a/.lp/panel.json"), None);
        assert_eq!(member_of("/hardware.json"), None);
        assert!(is_package_file("/projects/a/modules/x.pkg"));
        assert!(is_package_file("/projects/a/modules/x.pkg.tmp"));
        assert!(!is_package_file("/projects/a/modules/x/y.pkg"));
        assert!(!is_package_file("/projects/a.pkg"));
        assert!(is_reserved("/projects/a/.lp/.f3-new.tmp"));
        assert_eq!(new_tmp_for("/hardware.json"), "/.f3-new.tmp");
        assert!(reachable("/projects/a/modules/x", "/projects/a/"));
        assert!(reachable(
            "/projects/a/modules/x",
            "/projects/a/modules/x/s"
        ));
        assert!(!reachable("/projects/a/modules/x", "/projects/b/"));
    }

    #[test]
    fn block_cycles_defaults_to_one_hundred_and_the_dial_overrides_it() {
        let cfg = CandidateConfig::new(128);
        assert_eq!(lfs_config(&f3_config(&cfg)).block_cycles, 100);
        let off = cfg.clone().with_dial("block_cycles", "-1");
        assert_eq!(lfs_config(&f3_config(&off)).block_cycles, -1);
        let five = cfg.with_dial("block_cycles", "500");
        assert_eq!(lfs_config(&f3_config(&five)).block_cycles, 500);
    }

    #[test]
    fn round_trips_fault_free() {
        round_trip(&LittlefsPatternPackage);
    }

    #[test]
    fn a_save_rewrites_only_its_own_pattern_package() {
        let cfg = CandidateConfig::new(64);
        let mut flash = NorFlashSim::new(cfg.geometry());
        LittlefsPatternPackage.format(&mut flash, &cfg).unwrap();
        let mut s = LittlefsPatternPackage
            .mount(flash, &cfg)
            .map_err(|(e, _)| e)
            .unwrap();
        // Shaders that do not deflate much, so each package spans blocks.
        let mut rng = lp_nor_sim::SimRng::new(5);
        let shader = |rng: &mut lp_nor_sim::SimRng| -> Vec<u8> {
            (0..6000).map(|_| b'a' + rng.below(26) as u8).collect()
        };
        let path = |m: u32| format!("/projects/a/modules/m{m}/shader.glsl");
        for m in 0..3 {
            s.put(&path(m), &shader(&mut rng)).unwrap();
        }
        s.commit().unwrap();
        let programmed = |s: &dyn CandidateStore| s.flash_snapshot().stats().program_bytes;

        let before = programmed(s.as_ref());
        let one = shader(&mut rng);
        s.put(&path(1), &one).unwrap();
        s.commit().unwrap();
        let wrote_one = programmed(s.as_ref()) - before;

        let before = programmed(s.as_ref());
        for m in 0..3 {
            s.put(&path(m), &shader(&mut rng)).unwrap();
        }
        s.commit().unwrap();
        let wrote_three = programmed(s.as_ref()) - before;

        assert_eq!(
            s.list("/projects/a/").unwrap(),
            (0..3).map(path).collect::<Vec<_>>()
        );
        assert!(
            wrote_one * 2 < wrote_three,
            "one pattern {wrote_one} B, three {wrote_three} B"
        );
    }

    #[test]
    fn small_exhaustive_sweep_runs() {
        let s = small_sweep(&LittlefsPatternPackage);
        eprintln!("f3 small sweep: {s:?}");
        assert!(s.cases > 0);
    }
}
