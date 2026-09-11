//! Recording a whole-image module's real work, so another engine can run it.
//!
//! # Why this exists
//!
//! JD26 asks for **ns per guest instruction at steady state, in `bun` and in
//! `node`**, for a module holding the whole image at several
//! blocks-per-function. Neither engine has a bus, a peripheral model or an
//! interpreter, and building one in JavaScript would be a second
//! implementation of the machine — the exact thing
//! `scripts/emu/jit-engine-check.mjs` was written to avoid.
//!
//! So the numbers come from a **recording and a replay**, the same shape
//! [`lp_emu_jit::replay`] describes: a native `--jit` run records what each
//! entry into translated code was handed and what every import answered, in
//! call order, and the engines replay that against the same module bytes.
//! Nothing has to be reimplemented, and because the recording carries what the
//! run produced, a replay is an **identity check in that engine** as well as a
//! stopwatch.
//!
//! # One recording, every module size
//!
//! A global block index does not depend on how the module is split — the block
//! set is the same, the order is the same, and the exchange protocol is the
//! same — so **one** recording drives the module emitted at 1k, 2k, 4k, 8k and
//! anything else. That is what makes the sizing table affordable: only the
//! recording pays cranelift, and it pays it once.
//!
//! # The memory delta, and why a replay without it lies
//!
//! Translated code does not cover the whole run: the interpreter runs whatever
//! the module refuses, *between* entries, and those instructions write guest
//! memory. A replay that loaded one snapshot and then ran only the entries
//! would be running them against memory the real run never had — the spike's
//! entry-21 divergence, and a harness bug that reads exactly like a translator
//! bug. So every entry carries the granules that changed since the previous
//! one ended, and the replay applies them before running it.
//!
//! The shadow is refreshed **after** each entry, not before the next, so the
//! delta holds the interpreter's writes and never the module's own — those the
//! replay is supposed to reproduce for itself.
//!
//! # What it refuses to record
//!
//! A `step_one` call. The escape hatch hands an arbitrary guest instruction to
//! the interpreter, and reproducing one in a replay means recording the whole
//! register file and everything it wrote. With the whole image installed and
//! `Emit::EVERYTHING` the static escape rate is **0.0 %** and no such call
//! happens, so this recorder says so and stops rather than carrying a format
//! for a case it has never seen.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// The unit the between-entries memory diff is taken in, matching
/// [`lp_emu_jit::replay::GRANULE_BYTES`] so the two formats can be read by one
/// reader.
pub const GRANULE: usize = 64;

/// The coarse pass's unit: one `memcmp` decides whether 4 KiB of guest memory
/// needs looking at granule by granule.
const CHUNK: usize = 4096;

/// One import call, as the replay hands it back.
#[derive(Clone, Copy, Debug)]
pub struct CallRec {
    /// 0 `mmio_load`, 1 `mmio_store`, 2 `poll` (M7b P2).
    pub kind: u8,
    pub pc: u32,
    pub cycle: u64,
    pub address: u32,
    pub access: u32,
    /// The value a store carried; zero for a load.
    pub value: u32,
    /// `(status << 32) | value` for a load; `(status << 32) | pc` for a store
    /// or a poll, because both of those answer a polling point.
    pub result: u64,
}

impl CallRec {
    fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        out.write_all(&[self.kind, 0, 0, 0])?;
        out.write_all(&self.pc.to_le_bytes())?;
        out.write_all(&self.cycle.to_le_bytes())?;
        out.write_all(&self.address.to_le_bytes())?;
        out.write_all(&self.access.to_le_bytes())?;
        out.write_all(&self.value.to_le_bytes())?;
        out.write_all(&0u32.to_le_bytes())?;
        out.write_all(&self.result.to_le_bytes())
    }
}

/// What one entry into translated code was handed and what it produced.
#[derive(Clone, Debug)]
pub struct EntryRec {
    pub entry: u32,
    pub cycle_in: u64,
    pub instret_in: u64,
    pub end: u64,
    pub watch_lo: u64,
    pub watch_hi: u64,
    /// `x1`..`x31` as the entry was handed them. The interpreter runs between
    /// entries and writes registers, and registers are not memory, so a delta
    /// cannot carry them.
    pub regs_in: [i32; 31],
    /// Guest memory the interpreter changed since the previous entry ended.
    pub delta: Vec<(u32, Vec<u8>)>,
    pub calls: Vec<CallRec>,
    pub exit_pc: u32,
    pub flags: i32,
    pub cycle_out: u64,
    pub instret_out: u64,
    /// `x1`..`x31` as the entry left them, so a replay in another engine is an
    /// identity check and not only a stopwatch.
    pub regs_out: [i32; 31],
}

/// Where a recording is being written, and what it has seen.
pub struct Recorder {
    dir: PathBuf,
    /// The byte ranges of the imported memory a replay has to reproduce: the
    /// guest's own regions plus the translator's tables. Never the ~200 MiB
    /// gap between them, which nothing reads and touching would commit.
    ///
    /// The flag says whether a range can change *between* entries and so has
    /// to be diffed at every one. The translator's own tables cannot — only a
    /// translation event rewrites them, and that builds a new core — and the
    /// mask ROM cannot either, but it is cheap to leave in.
    live: Vec<(u32, u32, bool)>,
    shadow: Vec<u8>,
    /// The live ranges as they stood at the **first** recorded entry, which is
    /// what a replay starts every iteration from. Separate from
    /// [`Self::shadow`], which the diffs move forward.
    initial: Vec<u8>,
    entries: Vec<EntryRec>,
    /// Entries still to record before the recording is complete.
    want: usize,
    started: bool,
    pub done: bool,
    /// Set when the escape hatch fired inside a recorded entry; the recording
    /// is then abandoned rather than written short.
    pub escaped: bool,
}

impl Recorder {
    #[must_use]
    pub fn new(dir: PathBuf, live: Vec<(u32, u32, bool)>, want: usize) -> Self {
        let bytes = live.iter().map(|&(_, l, _)| l as usize).sum();
        Self {
            dir,
            live,
            shadow: vec![0; bytes],
            initial: Vec::new(),
            entries: Vec::with_capacity(want),
            want,
            started: false,
            done: false,
            escaped: false,
        }
    }

    #[must_use]
    pub fn wants_more(&self) -> bool {
        !self.done && self.entries.len() < self.want
    }

    /// Take the granules that changed since the last refresh, and refresh.
    ///
    /// A coarse pass first: 16 MiB of guest regions have to be looked at at
    /// every recorded entry, and almost none of it ever changes, so a
    /// [`CHUNK`]-sized `!=` — which is one `memcmp` — decides whether the
    /// granule loop runs at all. Without it the per-granule loop is the
    /// recorder's whole cost.
    fn diff(&mut self, mem: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::new();
        let mut at = 0usize;
        for &(base, len, volatile) in &self.live {
            let len = len as usize;
            if !volatile {
                at += len;
                continue;
            }
            let now = &mem[base as usize..][..len];
            let was = &mut self.shadow[at..at + len];
            let mut c = 0usize;
            while c < len {
                let chunk_end = (c + CHUNK).min(len);
                if now[c..chunk_end] != was[c..chunk_end] {
                    let mut g = c;
                    while g < chunk_end {
                        let end = (g + GRANULE).min(chunk_end);
                        if now[g..end] != was[g..end] {
                            was[g..end].copy_from_slice(&now[g..end]);
                            out.push((base + g as u32, now[g..end].to_vec()));
                        }
                        g += GRANULE;
                    }
                }
                c = chunk_end;
            }
            at += len;
        }
        out
    }

    /// The first recorded entry takes the whole live image; every one after
    /// takes only what changed.
    pub fn before_entry(&mut self, mem: &[u8]) -> Vec<(u32, Vec<u8>)> {
        if !self.started {
            self.started = true;
            let mut at = 0usize;
            for &(base, len, _) in &self.live {
                self.shadow[at..at + len as usize]
                    .copy_from_slice(&mem[base as usize..][..len as usize]);
                at += len as usize;
            }
            self.initial = self.shadow.clone();
            return Vec::new();
        }
        self.diff(mem)
    }

    /// Refresh the shadow past the module's own writes, which a replay makes
    /// for itself.
    pub fn after_entry(&mut self, mem: &[u8], rec: EntryRec) {
        let _ = self.diff(mem);
        self.entries.push(rec);
        if self.entries.len() >= self.want {
            self.done = true;
        }
    }

    /// Write the recording out beside the module it is of.
    ///
    /// # Errors
    ///
    /// Anything the filesystem refuses.
    pub fn finish(
        &self,
        wasm: &[u8],
        mem: &[u8],
        pages: u64,
        exchange_offset: u32,
        fn_blocks: usize,
        blocks: usize,
    ) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        fs::write(self.dir.join("module.wasm"), wasm)?;

        // The live ranges as the first recorded entry found them, whole rather
        // than sparse. Whole because a replay has to be able to put the memory
        // *back* between iterations, and a sparse image cannot un-write a
        // granule that started at zero. The ~200 MiB the arena spans between
        // regions is still not here, which is what keeps this ~24 MB.
        let mut image = BufWriter::new(fs::File::create(self.dir.join("memory.bin"))?);
        let mut at = 0usize;
        for &(base, len, _) in &self.live {
            let _ = mem;
            image.write_all(&base.to_le_bytes())?;
            image.write_all(&len.to_le_bytes())?;
            image.write_all(&self.initial[at..at + len as usize])?;
            at += len as usize;
        }
        image.flush()?;

        let mut entries = BufWriter::new(fs::File::create(self.dir.join("entries.bin"))?);
        for e in &self.entries {
            entries.write_all(&e.entry.to_le_bytes())?;
            for v in [e.cycle_in, e.instret_in, e.end, e.watch_lo, e.watch_hi] {
                entries.write_all(&v.to_le_bytes())?;
            }
            for r in e.regs_in {
                entries.write_all(&r.to_le_bytes())?;
            }
            entries.write_all(&(e.delta.len() as u32).to_le_bytes())?;
            for (off, bytes) in &e.delta {
                entries.write_all(&off.to_le_bytes())?;
                entries.write_all(&(bytes.len() as u32).to_le_bytes())?;
                entries.write_all(bytes)?;
            }
            entries.write_all(&(e.calls.len() as u32).to_le_bytes())?;
            for c in &e.calls {
                c.write(&mut entries)?;
            }
            entries.write_all(&e.exit_pc.to_le_bytes())?;
            entries.write_all(&e.flags.to_le_bytes())?;
            entries.write_all(&e.cycle_out.to_le_bytes())?;
            entries.write_all(&e.instret_out.to_le_bytes())?;
            for r in e.regs_out {
                entries.write_all(&r.to_le_bytes())?;
            }
        }
        entries.flush()?;

        let retired: u64 = self
            .entries
            .iter()
            .map(|e| e.instret_out.saturating_sub(e.instret_in))
            .sum();
        let calls: usize = self.entries.iter().map(|e| e.calls.len()).sum();
        let delta_bytes: usize = self
            .entries
            .iter()
            .flat_map(|e| e.delta.iter())
            .map(|(_, b)| b.len())
            .sum();
        let meta = format!(
            "{{\n  \"pages\": {pages},\n  \"exchange\": {exchange_offset},\n  \
             \"entries\": {},\n  \"retired\": {retired},\n  \"calls\": {calls},\n  \
             \"deltaBytes\": {delta_bytes},\n  \"fnBlocks\": {fn_blocks},\n  \
             \"blocks\": {blocks},\n  \"moduleBytes\": {}\n}}\n",
            self.entries.len(),
            wasm.len(),
        );
        fs::write(self.dir.join("meta.json"), meta)?;
        Ok(())
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
