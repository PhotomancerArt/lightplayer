//! `TreeStore`: the path API over the record log — format, mount, get, put,
//! delete_prefix, list, commit.
//!
//! Writes are buffered in RAM (`WorkEntry::Staged`) and nothing touches flash
//! until `commit`, which lays out every new record, marks, makes room (GC)
//! or refuses with `NoSpace`, appends the records, and writes the root last.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;

use crate::blob_codec::{chunk_dict_ref, decode_chunk};
use crate::dir_node::{DirEntry, EntryKind, decode_dir, encode_dir};
use crate::file_tree::{FileEntry, WorkEntry, build_dirs, is_hot, tree_ram_bytes, valid_path};
use crate::flash::Flash;
use crate::gc_copy::collect_sector;
use crate::gc_mark::{mark, recount_live_bytes};
use crate::gc_victim::choose_victim;
use crate::multi_node::MultiNode;
use crate::node_layout::{LaidRecord, LayoutCtx, NodeTag, layout_node};
use crate::object_id::ObjectId;
use crate::record_header::encode_record;
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::record_log::RecordLog;
use crate::record_plan::RecordPlan;
use crate::record_scan::{scan_sector, tail_is_erased};
use crate::root_record::RootRecord;
use crate::root_select::select_root;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN, SectorHeader};
use crate::sector_table::SectorUse;
use crate::small_sort::sort_small_by;
use crate::space_estimate::{fits_after_compaction, sectors_needed};
use crate::store_config::{Codec, StoreConfig};
use crate::store_error::StoreError;
use crate::store_stats::TreeStoreStats;

type Res<T, F> = Result<T, StoreError<<F as Flash>::Error>>;

/// Deepest directory nesting mount will follow.
const MAX_DIR_DEPTH: u32 = 64;

/// The T1 store.
pub struct TreeStore<F: Flash> {
    log: RecordLog<F>,
    cfg: StoreConfig,
    root: Option<(ObjectId, RootRecord)>,
    max_root_seq: u64,
    working: BTreeMap<String, WorkEntry>,
    dirty: bool,
    dict_cache: Option<(ObjectId, Vec<u8>)>,
    stats: TreeStoreStats,
}

impl<F: Flash> TreeStore<F> {
    /// Kill and erase every sector, then commit an empty tree. Works on any
    /// flash content.
    pub fn format(flash: &mut F, cfg: &StoreConfig) -> Res<(), F> {
        check_config(flash.sector_count(), flash.sector_size(), cfg)?;
        let mut store = TreeStore::<&mut F>::empty(RecordLog::new(flash), cfg.clone());
        for s in 0..store.log.sector_count {
            store.log.kill_and_erase(s)?;
        }
        store.dirty = true;
        store.commit()
    }

    /// Scan the flash and adopt the newest complete root (I1). Never panics
    /// on any content; a flash with no complete root is an error.
    pub fn mount(flash: F, cfg: StoreConfig) -> Result<Self, (StoreError<F::Error>, F)> {
        if let Err(e) = check_config(flash.sector_count(), flash.sector_size(), &cfg) {
            return Err((e, flash));
        }
        let mut store = Self::empty(RecordLog::new(flash), cfg);
        match store.load() {
            Ok(()) => Ok(store),
            Err(e) => Err((e, store.into_flash())),
        }
    }

    /// The file at `path`: staged bytes if put since the last commit.
    pub fn get(&mut self, path: &str) -> Res<Option<Vec<u8>>, F> {
        let fe = match self.working.get(path) {
            None => return Ok(None),
            Some(WorkEntry::Staged(b)) => return Ok(Some(b.clone())),
            Some(WorkEntry::Committed(fe)) => *fe,
        };
        let bytes = self.read_node(fe.id)?;
        if bytes.len() != fe.size as usize {
            return Err(StoreError::Corrupt("file size"));
        }
        Ok(Some(bytes))
    }

    /// Stage `bytes` at `path` (RAM only until `commit`).
    pub fn put(&mut self, path: &str, bytes: &[u8]) -> Res<(), F> {
        if !valid_path(path) {
            return Err(StoreError::InvalidPath);
        }
        if path.len() > usize::from(u16::MAX) || bytes.len() > u32::MAX as usize {
            return Err(StoreError::TooLarge);
        }
        self.working
            .insert(String::from(path), WorkEntry::Staged(bytes.to_vec()));
        self.dirty = true;
        let staged = self.staged_bytes();
        self.note_buffer(staged);
        Ok(())
    }

    /// Remove every path starting with `prefix` (a plain string prefix).
    pub fn delete_prefix(&mut self, prefix: &str) -> Res<(), F> {
        let doomed = self.matching(prefix);
        self.dirty |= !doomed.is_empty();
        for p in doomed {
            self.working.remove(&p);
        }
        Ok(())
    }

    /// Every file path starting with `prefix`, sorted.
    pub fn list(&mut self, prefix: &str) -> Res<Vec<String>, F> {
        Ok(self.matching(prefix))
    }

    /// Make everything since the last commit durable at once: new records,
    /// changed directories, then a root with seq + 1. On `NoSpace` nothing of
    /// the commit was written and the staged changes remain.
    pub fn commit(&mut self) -> Res<(), F> {
        if !self.dirty {
            return Ok(());
        }
        let working = core::mem::take(&mut self.working);
        match self.commit_with(&working) {
            Ok(committed) => {
                self.working = committed;
                self.dirty = false;
                Ok(())
            }
            Err(e) => {
                self.working = working;
                Err(e)
            }
        }
    }

    /// Drop staged changes: back to the committed tree.
    pub fn discard_uncommitted(&mut self) -> Res<(), F> {
        let Some((_, root)) = self.root else {
            return Err(StoreError::Corrupt("no root"));
        };
        self.working = self.load_tree(&root)?;
        self.dirty = false;
        Ok(())
    }

    pub fn flash(&self) -> &F {
        &self.log.flash
    }

    /// For a harness installing fault plans. Changing cells under the store
    /// is not supported.
    pub fn flash_mut(&mut self) -> &mut F {
        &mut self.log.flash
    }

    pub fn into_flash(self) -> F {
        self.log.into_flash()
    }

    pub fn config(&self) -> &StoreConfig {
        &self.cfg
    }

    /// Sectors free right now (no live record, not a head).
    pub fn free_sectors(&self) -> u32 {
        self.log.free_count()
    }

    pub fn stats(&self) -> TreeStoreStats {
        let mut s = self.stats.clone();
        s.index_entries = self.log.index.len();
        s.index_ram_bytes = self.log.index.ram_bytes();
        s.tree_ram_bytes = tree_ram_bytes(&self.working);
        s.sector_table_ram_bytes = self.log.sectors.ram_bytes();
        s.records_written = self.log.counters.records_written;
        s.record_bytes_written = self.log.counters.record_bytes_written;
        s.sectors_opened = self.log.counters.sectors_opened;
        s.erases = self.log.counters.erases;
        s
    }

    // ---- mount ------------------------------------------------------------

    fn empty(log: RecordLog<F>, cfg: StoreConfig) -> Self {
        Self {
            log,
            cfg,
            root: None,
            max_root_seq: 0,
            working: BTreeMap::new(),
            dirty: false,
            dict_cache: None,
            stats: TreeStoreStats::default(),
        }
    }

    fn load(&mut self) -> Res<(), F> {
        let n = self.log.sector_count;
        let mut valid: Vec<(u32, u32, SectorHeader)> = Vec::new();
        for s in 0..n {
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            let addr = self.log.addr(s, 0);
            self.log.read(addr, &mut h)?;
            if let Some(hd) = SectorHeader::decode(&h) {
                valid.push((hd.seq, s, hd));
                self.log.sectors.erase_counts[s as usize] = hd.erase_count;
            }
        }
        sort_small_by(&mut valid, |a, b| (a.0, a.1) < (b.0, b.1));
        let mut roots = Vec::new();
        let mut closed = alloc::vec![false; n as usize];
        for &(_, s, header) in &valid {
            let scan = scan_sector(&mut self.log, s)?;
            self.log.sectors.uses[s as usize] = SectorUse::Written {
                header,
                end: scan.end,
            };
            closed[s as usize] = scan.closed;
            // Ascending seq: a later sector's copy wins.
            for (id, loc) in scan.records {
                self.log.index.insert(id, loc);
            }
            roots.extend(scan.roots);
        }
        self.max_root_seq = roots.iter().map(|r| r.0).max().unwrap_or(0);
        self.log.next_sector_seq = valid.last().map_or(1, |v| v.0.saturating_add(1));
        let Some(sel) = select_root(&mut self.log, roots)? else {
            return Err(StoreError::Corrupt("no complete root"));
        };
        recount_live_bytes(&mut self.log, &sel.live);
        self.note_buffer(sel.live.len() * core::mem::size_of::<ObjectId>());
        self.working = self.load_tree(&sel.root)?;
        self.root = Some((sel.id, sel.root));
        for kind in HeadKind::ALL {
            let cand = valid
                .iter()
                .rev()
                .find(|v| v.2.kind == kind)
                .map(|v| v.1)
                .filter(|&s| !closed[s as usize]);
            if let Some(s) = cand
                && let SectorUse::Written { end, .. } = self.log.sectors.uses[s as usize]
                && tail_is_erased(&mut self.log, s, end)?
            {
                self.log.heads[kind.index()] = Some(s);
            }
        }
        self.stats.mount_bytes_read = self.log.counters.bytes_read;
        Ok(())
    }

    fn load_tree(&mut self, root: &RootRecord) -> Res<BTreeMap<String, WorkEntry>, F> {
        let mut files = BTreeMap::new();
        self.load_dir(root.cold_dir, String::new(), 0, &mut files)?;
        let hot = self.read_node(root.hot_dir)?;
        for e in decode_dir(&hot).ok_or(StoreError::Corrupt("hot dir"))? {
            if e.kind != EntryKind::File || !valid_path(&e.name) {
                return Err(StoreError::Corrupt("hot dir entry"));
            }
            let fe = FileEntry {
                id: e.id,
                size: e.size,
            };
            files.insert(e.name, WorkEntry::Committed(fe));
        }
        Ok(files)
    }

    fn load_dir(
        &mut self,
        id: ObjectId,
        prefix: String,
        depth: u32,
        files: &mut BTreeMap<String, WorkEntry>,
    ) -> Res<(), F> {
        if depth > MAX_DIR_DEPTH {
            return Err(StoreError::Corrupt("dir depth"));
        }
        let bytes = self.read_node(id)?;
        for e in decode_dir(&bytes).ok_or(StoreError::Corrupt("dir"))? {
            if e.name.is_empty() || e.name.contains('/') {
                return Err(StoreError::Corrupt("dir entry name"));
            }
            let mut path = prefix.clone();
            path.push('/');
            path.push_str(&e.name);
            match e.kind {
                EntryKind::File => {
                    let fe = FileEntry {
                        id: e.id,
                        size: e.size,
                    };
                    files.insert(path, WorkEntry::Committed(fe));
                }
                EntryKind::Dir => self.load_dir(e.id, path, depth + 1, files)?,
            }
        }
        Ok(())
    }

    // ---- reading nodes ----------------------------------------------------

    fn read_node(&mut self, id: ObjectId) -> Res<Vec<u8>, F> {
        let (loc, payload) = self.log.read_payload(id)?;
        let out = match loc.kind {
            RecordKind::Blob => self.decode_blob(loc.codec, &payload)?,
            RecordKind::Dir | RecordKind::Dict => payload,
            RecordKind::Multi => {
                let m = MultiNode::decode(&payload).ok_or(StoreError::Corrupt("multi"))?;
                let mut out = Vec::with_capacity((m.total_len as usize).min(1 << 20));
                self.read_multi(&m, &mut out)?;
                if out.len() != m.total_len as usize {
                    return Err(StoreError::Corrupt("multi length"));
                }
                out
            }
            RecordKind::Root => return Err(StoreError::Corrupt("root is not a node")),
        };
        self.note_buffer(out.len());
        Ok(out)
    }

    fn read_multi(&mut self, m: &MultiNode, out: &mut Vec<u8>) -> Res<(), F> {
        for &c in &m.children {
            let (loc, payload) = self.log.read_payload(c)?;
            match (m.level, loc.kind) {
                (0, RecordKind::Blob) => {
                    let b = self.decode_blob(loc.codec, &payload)?;
                    out.extend_from_slice(&b);
                }
                (l, RecordKind::Multi) if l > 0 => {
                    let cm = MultiNode::decode(&payload).ok_or(StoreError::Corrupt("multi"))?;
                    if cm.level != l - 1 {
                        return Err(StoreError::Corrupt("multi level"));
                    }
                    self.read_multi(&cm, out)?;
                }
                _ => return Err(StoreError::Corrupt("multi child")),
            }
        }
        Ok(())
    }

    fn decode_blob(&mut self, codec: ChunkCodec, payload: &[u8]) -> Res<Vec<u8>, F> {
        let dict_id = chunk_dict_ref(codec, payload);
        if let Some(d) = dict_id {
            self.load_dict(d)?;
        }
        let dict = dict_id.and_then(|_| self.dict_cache.as_ref().map(|c| c.1.as_slice()));
        if let Some(d) = dict {
            self.stats.largest_buffer = self.stats.largest_buffer.max(d.len() + 4096);
        }
        decode_chunk(codec, payload, dict).ok_or(StoreError::Corrupt("chunk does not decode"))
    }

    fn load_dict(&mut self, id: ObjectId) -> Res<(), F> {
        if self.dict_cache.as_ref().is_some_and(|c| c.0 == id) {
            return Ok(());
        }
        let bytes = self.read_node(id)?;
        self.dict_cache = Some((id, bytes));
        Ok(())
    }

    // ---- commit -----------------------------------------------------------

    fn commit_with(
        &mut self,
        working: &BTreeMap<String, WorkEntry>,
    ) -> Res<BTreeMap<String, WorkEntry>, F> {
        let empty = RecordPlan::default();
        let old_roots: Vec<ObjectId> = self.root.iter().map(|r| r.0).collect();
        let old_live = mark(&mut self.log, &old_roots, &empty)?;
        let mut plan = RecordPlan::default();
        let (dict_id, dict_bytes) = self.choose_dict(working, &old_live, &mut plan)?;
        let ctx = LayoutCtx {
            record_max: self.cfg.record_max,
            codec: self.cfg.codec,
            dict: dict_bytes.as_deref().map(|b| (dict_id, b)),
        };

        // In `working`'s (sorted) order.
        let mut files: Vec<(String, FileEntry)> = Vec::with_capacity(working.len());
        for (path, e) in working {
            let fe = match e {
                WorkEntry::Committed(fe) => *fe,
                WorkEntry::Staged(b) => {
                    let head = if is_hot(path) {
                        HeadKind::Hot
                    } else {
                        HeadKind::Cold
                    };
                    let id = self.plan_node(NodeTag::File, b, head, &ctx, &mut plan, &old_live)?;
                    FileEntry {
                        id,
                        size: b.len() as u32,
                    }
                }
            };
            files.push((path.clone(), fe));
        }

        let mut hot_entries: Vec<DirEntry> = files
            .iter()
            .filter(|(p, _)| is_hot(p))
            .map(|(p, fe)| DirEntry {
                name: p.clone(),
                kind: EntryKind::File,
                size: fe.size,
                id: fe.id,
            })
            .collect();
        let hot_bytes = encode_dir(&mut hot_entries);
        let hot_id = self.plan_node(
            NodeTag::Dir,
            &hot_bytes,
            HeadKind::Hot,
            &ctx,
            &mut plan,
            &old_live,
        )?;
        let cold: Vec<(&str, FileEntry)> = files
            .iter()
            .filter(|(p, _)| !is_hot(p))
            .map(|(p, fe)| (&p[1..], *fe))
            .collect();
        let cold_id = build_dirs(&cold, &mut |mut entries| {
            let bytes = encode_dir(&mut entries);
            self.plan_node(
                NodeTag::Dir,
                &bytes,
                HeadKind::Cold,
                &ctx,
                &mut plan,
                &old_live,
            )
        })?;

        // Inserts, not `collect`: a collected BTreeMap sorts, and that sort
        // is kilobytes of RV32 code.
        let mut committed: BTreeMap<String, WorkEntry> = BTreeMap::new();
        for (p, fe) in files {
            committed.insert(p, WorkEntry::Committed(fe));
        }
        if let Some((_, r)) = self.root
            && r.cold_dir == cold_id
            && r.hot_dir == hot_id
            && r.dict == dict_id
        {
            return Ok(committed);
        }

        let seq = self.max_root_seq.saturating_add(1);
        let root = RootRecord {
            seq,
            cold_dir: cold_id,
            hot_dir: hot_id,
            dict: dict_id,
            next_key_id: 0,
        };
        let payload = root.encode();
        let root_id = RootRecord::id_of(&payload);
        let root_rec = LaidRecord {
            id: root_id,
            kind: RecordKind::Root,
            codec: ChunkCodec::Stored,
            payload,
        };
        plan.push(root_rec, HeadKind::Hot);

        let mut live = old_live;
        live.extend(mark(&mut self.log, &[root_id], &plan)?);
        recount_live_bytes(&mut self.log, &live);
        self.note_buffer(plan.total_bytes() as usize);
        self.note_buffer(live.len() * core::mem::size_of::<ObjectId>());

        self.ensure_space(&plan, &live)?;
        for p in &plan.records {
            let raw = encode_record(p.rec.kind, p.rec.codec, p.rec.id, &p.rec.payload);
            self.log
                .append(p.head, &raw, p.rec.id, p.rec.kind, p.rec.codec)?;
        }
        self.root = Some((root_id, root));
        self.max_root_seq = seq;
        self.stats.commits += 1;
        Ok(committed)
    }

    /// Lay a node out into the plan, skipping every record already present
    /// with a complete closure (dedup by id, no byte compare). Returns its id.
    fn plan_node(
        &mut self,
        tag: NodeTag,
        bytes: &[u8],
        head: HeadKind,
        ctx: &LayoutCtx<'_>,
        plan: &mut RecordPlan,
        old_live: &BTreeSet<ObjectId>,
    ) -> Res<ObjectId, F> {
        let id = ObjectId::of(tag.id_tag(), bytes);
        if self.is_known(id, plan, old_live)? {
            self.stats.dedup_hits += 1;
            return Ok(id);
        }
        let (_, recs) = layout_node(tag, bytes, ctx);
        for r in recs {
            if r.id != id && self.is_known(r.id, plan, old_live)? {
                self.stats.dedup_hits += 1;
                continue;
            }
            plan.push(r, head);
        }
        Ok(id)
    }

    /// Present with everything it names: planned, live under the current
    /// root, or indexed with a complete closure (a garbage record whose
    /// children were collected must be written again).
    fn is_known(
        &mut self,
        id: ObjectId,
        plan: &RecordPlan,
        old_live: &BTreeSet<ObjectId>,
    ) -> Res<bool, F> {
        if plan.contains(id) || old_live.contains(&id) {
            return Ok(true);
        }
        if !self.log.index.contains(id) {
            return Ok(false);
        }
        match mark(&mut self.log, &[id], &RecordPlan::default()) {
            Ok(_) => Ok(true),
            Err(StoreError::Corrupt(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// The dictionary this commit codes against: a fresh one trained from
    /// the commit's new content when it is a push (≥ `dict_train_min`), else
    /// the root's.
    fn choose_dict(
        &mut self,
        working: &BTreeMap<String, WorkEntry>,
        old_live: &BTreeSet<ObjectId>,
        plan: &mut RecordPlan,
    ) -> Res<(ObjectId, Option<Vec<u8>>), F> {
        if self.cfg.codec != Codec::DeflateDict {
            return Ok((ObjectId::NONE, None));
        }
        if let Some(fresh) = self.train_dict(working, old_live, plan)? {
            return Ok(fresh);
        }
        let current = self.root.map_or(ObjectId::NONE, |r| r.1.dict);
        if current.is_none() || !cfg!(feature = "encode") {
            return Ok((current, None));
        }
        self.load_dict(current)?;
        Ok((current, self.dict_cache.as_ref().map(|c| c.1.clone())))
    }

    #[cfg(feature = "encode")]
    fn train_dict(
        &mut self,
        working: &BTreeMap<String, WorkEntry>,
        old_live: &BTreeSet<ObjectId>,
        plan: &mut RecordPlan,
    ) -> Res<Option<(ObjectId, Option<Vec<u8>>)>, F> {
        use crate::object_id::IdTag;
        let samples: Vec<&[u8]> = working
            .iter()
            .filter_map(|(p, e)| match e {
                WorkEntry::Staged(b) if !is_hot(p) => Some(b.as_slice()),
                _ => None,
            })
            .filter(|b| {
                let id = ObjectId::of(IdTag::File, b);
                !old_live.contains(&id) && !self.log.index.contains(id)
            })
            .collect();
        let total: usize = samples.iter().map(|b| b.len()).sum();
        if total < self.cfg.dict_train_min as usize {
            return Ok(None);
        }
        let dict = crate::store_dictionary::train_dictionary(&samples, self.cfg.dict_size as usize);
        if dict.is_empty() {
            return Ok(None);
        }
        let stored = LayoutCtx {
            record_max: self.cfg.record_max,
            codec: Codec::Stored,
            dict: None,
        };
        let id = self.plan_node(
            NodeTag::Dict,
            &dict,
            HeadKind::Cold,
            &stored,
            plan,
            old_live,
        )?;
        self.note_buffer(dict.len());
        self.dict_cache = Some((id, dict.clone()));
        Ok(Some((id, Some(dict))))
    }

    #[cfg(not(feature = "encode"))]
    fn train_dict(
        &mut self,
        _working: &BTreeMap<String, WorkEntry>,
        _old_live: &BTreeSet<ObjectId>,
        _plan: &mut RecordPlan,
    ) -> Res<Option<(ObjectId, Option<Vec<u8>>)>, F> {
        Ok(None)
    }

    /// Make the free sectors cover the plan plus the reserve, collecting
    /// victims as needed; `NoSpace` when the conservative bound says the
    /// commit cannot fit, before any record of it is written.
    fn ensure_space(&mut self, plan: &RecordPlan, live: &BTreeSet<ObjectId>) -> Res<(), F> {
        let cap = self.log.sector_capacity();
        let lens: Vec<(HeadKind, u32)> = plan
            .records
            .iter()
            .map(|r| (r.head, r.total_len()))
            .collect();
        let reserve = self.cfg.reserve;
        let enough = |log: &RecordLog<F>| {
            let remaining = [
                log.head_remaining(HeadKind::Cold),
                log.head_remaining(HeadKind::Hot),
            ];
            log.free_count() >= sectors_needed(remaining, lens.iter().copied(), cap) + reserve
        };
        if enough(&self.log) {
            return Ok(());
        }
        let sizes: Vec<u32> = self
            .log
            .index
            .iter()
            .filter(|(id, _)| live.contains(id))
            .map(|(_, loc)| loc.total_len())
            .chain(lens.iter().map(|l| l.1))
            .collect();
        if !fits_after_compaction(sizes, cap, self.log.sector_count, reserve) {
            return Err(StoreError::NoSpace);
        }
        for _ in 0..self.log.sector_count * 4 {
            if self.log.free_count() == 0 {
                break;
            }
            let Some(victim) = choose_victim(&self.log, self.cfg.gc_policy) else {
                break;
            };
            let copied = collect_sector(&mut self.log, victim, live)?;
            self.stats.gc_runs += 1;
            self.stats.gc_copies += copied.records;
            self.stats.gc_copy_bytes += copied.bytes;
            if enough(&self.log) {
                return Ok(());
            }
        }
        Err(StoreError::NoSpace)
    }

    // ---- helpers ----------------------------------------------------------

    fn matching(&self, prefix: &str) -> Vec<String> {
        self.working
            .range::<str, _>((
                core::ops::Bound::Included(prefix),
                core::ops::Bound::Unbounded,
            ))
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, _)| k.clone())
            .collect()
    }

    fn staged_bytes(&self) -> usize {
        self.working
            .values()
            .map(|e| match e {
                WorkEntry::Staged(b) => b.len(),
                WorkEntry::Committed(_) => 0,
            })
            .sum()
    }

    fn note_buffer(&mut self, n: usize) {
        self.stats.largest_buffer = self.stats.largest_buffer.max(n);
    }
}

fn check_config<E>(
    sector_count: u32,
    sector_size: u32,
    cfg: &StoreConfig,
) -> Result<(), StoreError<E>> {
    if cfg.json_tree {
        return Err(StoreError::Unsupported(
            "json_tree: JSON-tree mode is not implemented",
        ));
    }
    if !(512..=32768).contains(&sector_size) {
        return Err(StoreError::BadConfig("sector size must be 512..=32768"));
    }
    if sector_count < 4 || sector_count > u32::from(u16::MAX) {
        return Err(StoreError::BadConfig("sector count must be 4..=65535"));
    }
    if cfg.record_max < 128 || cfg.record_max > sector_size - SECTOR_HEADER_LEN {
        return Err(StoreError::BadConfig(
            "record_max must be 128..=sector size - 20",
        ));
    }
    if cfg.reserve + 2 >= sector_count {
        return Err(StoreError::BadConfig("reserve leaves no room"));
    }
    Ok(())
}
