//! Discovery: finding everything the classic can run, including the code it
//! writes itself (XD9, M7 P05).
//!
//! The RV32 walk's rules (`lp-emu-jit/README.md` §Discovery) transfer as
//! *shape* and not as rules, because three things are true of Xtensa that are
//! not true of RISC-V: an instruction's length is decided by its opcode and
//! not by low bits the decoder can read without understanding the word;
//! literal pools sit **inside** `.text`, immediately before the functions
//! that `l32r` them (10.5 % of the image is `l32r`); and a zero-overhead loop
//! has a back-edge that no decoder sees. The rules, each numbered so the
//! README and the tests can name them:
//!
//! 1. **Symbols are the seeds, biggest first, one at a time.** The ELF entry,
//!    the vectors, and every function symbol in an executable region of the
//!    app *and of the mask ROM*, explored to exhaustion before the next one
//!    is added (the RV32 rule, for the RV32 reason: a budget spent on seeds
//!    follows no edge at all).
//! 2. **Widths come from the decoder — 2 or 3 — never from a stride.** All
//!    four `pc mod 4` residues are live in equal measure (study §2.3), so a
//!    fixed scan is meaningless here rather than merely lossy.
//! 3. **A block never crosses its symbol's extent.** `[st_value, st_value +
//!    st_size)` is the primary bound; a fall-through past it is a `Fall` into
//!    the next symbol's start if one begins there and `Undecodable` otherwise.
//!    Where no symbol covers a start — the guest-written region — the bound
//!    is the executable span the start is in.
//! 4. **Every word an `l32r` names is data and is never a block start.** The
//!    targets are collected as the sweep decodes; a start that lands on one is
//!    dropped and counted ([`DiscoverStats::literal_starts_dropped`]), and a
//!    block that walks onto one ends before it. `.literal` sections would be
//!    honoured too, but the link keeps none (P05 verified: the app has
//!    `.rwtext`, `.text` and `.vectors`; the ROM's fifteen executable
//!    sections are all `.text`-shaped) — the `l32r` set is the pool rule.
//! 5. **Edges:** `j` names its target; a conditional branch names its target
//!    and its fall-through; `call0/4/8/12` and `callx0/4/8/12` name their
//!    **return address** (the single largest rule on RV32, and the same one
//!    here); `jx` and `callx*` name no target; `entry`, `rotw`, `movsp`,
//!    `rsil` and `waiti` end a block and name the next instruction.
//! 6. **A `loop` names `LEND` as a start and marks the instruction ending
//!    exactly at `LEND` a terminator with a static back-edge to `LBEG`**
//!    ([`Decoded::lbeg`]) — two blocks: the loop head ends at the `loop`, the
//!    body ends at `LEND`. Without this a translated body would run once and
//!    fall through, silently (study §2.2).
//! 7. **An instruction the translator refuses ends the block before it and
//!    the walk steps over it by the decoder's own width** — a `wsr`, an
//!    `isync`, a `break` in the middle of a function does not hide the rest of
//!    the function. The width is exact because the decoder decoded the
//!    instruction ([`Decode::Undecodable`]); there is **no bounded skip** over
//!    bytes the decoder does not decode at all ([`Decode::Refused`]), because
//!    the length of an unknown Xtensa encoding is not knowable from its first
//!    byte. Those end the walk and the next seed carries on (JD7).
//! 8. **The third path.** The guest's own code — the shader the firmware JITs
//!    into SRAM0, 11.86 % of the render loop — has no symbol and no extent.
//!    Its seeds are every **word-aligned** address in the spans the guest
//!    stored into executable memory that decodes ([`word_seeds`]; the region
//!    is written by aligned word stores only, so word granularity is exact),
//!    walked with [`discover_from`] and the installed starts as the stop set,
//!    bounded by the span. `exec_of` maps a write address to the address the
//!    code executes at — identity on the classic, the D-bus/I-bus alias on
//!    the S3 (P09).
//! 9. **Nothing here can be wrong, only short (JD7).** A fetch that cannot be
//!    served, a refused word, a literal, an extent, a start that was already
//!    claimed: every one ends a block and hands the pc to the interpreter. The
//!    counters in [`DiscoverStats`] are the walk's own account of what it gave
//!    up on, so a coverage shortfall names its cause rather than hiding it.
//!
//! [`build`] is the door P04 left: a block per **supplied** start, following
//! widths but no edges. The seam tests and `--jit-seeds` use it.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use lp_xt_inst::Inst;

use crate::blocks::{Block, BlockEnd, BlockSet, MAX_BLOCK_INSTS};
use crate::decode::{Decode, Decoded, Edges, decode, edges};

/// A symbol's extent, `[start, end)` — rule 3's bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Extent {
    pub start: u32,
    pub end: u32,
}

/// What bounds a block (rules 3 and 8).
#[derive(Clone, Copy, Debug, Default)]
pub struct Bounds<'a> {
    /// Symbol extents, sorted by `start`, each with `end > start`. A symbol
    /// whose `st_size` is zero is not an extent (it bounds nothing) and is
    /// left out by the caller.
    pub extents: &'a [Extent],
    /// Executable spans `[lo, hi)`, sorted and non-overlapping. A block never
    /// leaves the span its start is in, and a start in no span holds nothing.
    /// Empty means every fetchable address is executable — the tests' shape.
    pub spans: &'a [(u32, u32)],
}

impl Bounds<'_> {
    /// No bounds at all: the fetch is the only thing that ends a block.
    pub const NONE: Bounds<'static> = Bounds {
        extents: &[],
        spans: &[],
    };

    /// The extent covering `pc`, if one does.
    ///
    /// The last extent starting at or before `pc`, plus the few before it in
    /// case several symbols share a start (an alias is sorted beside the
    /// function it names). Deeper nesting — a symbol inside another symbol's
    /// extent that ends before it — is not looked for; `symbol_at` in the
    /// machine has the same limit.
    fn extent_of(&self, pc: u32) -> Option<Extent> {
        let i = self.extents.partition_point(|e| e.start <= pc);
        self.extents[..i]
            .iter()
            .rev()
            .take(4)
            .copied()
            .find(|e| pc < e.end)
    }

    fn next_extent_start_after(&self, pc: u32) -> u32 {
        let i = self.extents.partition_point(|e| e.start <= pc);
        self.extents.get(i).map_or(u32::MAX, |e| e.start)
    }

    fn span_of(&self, pc: u32) -> Option<(u32, u32)> {
        let i = self.spans.partition_point(|&(lo, _)| lo <= pc);
        self.spans[..i].last().copied().filter(|&(_, hi)| pc < hi)
    }

    /// The first address a block starting at `pc` may not reach.
    ///
    /// The covering extent's end (rule 3); else the covering span's end,
    /// capped at the next symbol's start so a walk out of a hole never runs
    /// into a function's middle unbounded (rule 8); `pc` itself when the
    /// address is in no span, so the start holds nothing.
    #[must_use]
    pub fn limit(&self, pc: u32) -> u32 {
        if let Some(e) = self.extent_of(pc) {
            return e.end;
        }
        let next = self.next_extent_start_after(pc);
        if self.spans.is_empty() {
            return next;
        }
        match self.span_of(pc) {
            Some((_, hi)) => hi.min(next),
            None => pc,
        }
    }

    /// Does a symbol's extent begin exactly at `pc`?
    #[must_use]
    pub fn is_symbol_start(&self, pc: u32) -> bool {
        self.extents.binary_search_by_key(&pc, |e| e.start).is_ok()
    }
}

/// What one walk found, and what it had to give up on.
///
/// A coverage shortfall has to be explainable, so every way a block can end
/// short of a terminator has a counter here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiscoverStats {
    /// Seeds the caller supplied.
    pub seeds: usize,
    /// Distinct block starts the walk reached, after literal starts were
    /// dropped and before empty ones were.
    pub starts: usize,
    /// Blocks with at least one instruction — what was translated.
    pub blocks: usize,
    /// Instructions across those blocks.
    pub insts: usize,
    /// Blocks that ended **before** an instruction the translator refuses
    /// (rule 7). The walk stepped over it; the interpreter runs it.
    pub undecodable: usize,
    /// Blocks that ended at bytes the decoder does not decode, or that could
    /// not be fetched. The walk stopped there (rule 7's second half).
    pub refused: usize,
    /// Blocks that ended at their symbol's extent (rule 3).
    pub extent_ends: usize,
    /// Blocks that walked onto a word an `l32r` names and ended before it
    /// (rule 4).
    pub data_ends: usize,
    /// Distinct words the image's `l32r`s name.
    pub literals: usize,
    /// Starts that landed on one of those words and were dropped (rule 4).
    pub literal_starts_dropped: usize,
    /// `LEND` addresses the walk marked as terminators (rule 6).
    pub loop_ends: usize,
    /// Starts whose first instruction did not decode, so no block was built.
    /// A symbol that names data lands here.
    pub empty_starts: usize,
    /// Blocks cut at [`MAX_BLOCK_INSTS`] rather than at a boundary.
    pub capped: usize,
    /// The budget stopped the walk before the image did.
    pub truncated: bool,
}

/// A walk's result.
#[derive(Clone, Debug, Default)]
pub struct Discovered {
    pub set: BlockSet,
    pub stats: DiscoverStats,
}

/// Walk the image from `seeds`, following widths and static edges, bounded by
/// `bounds`.
///
/// `fetch` reads one guest byte, purely: this runs before the guest reaches
/// any of these addresses, so a fetch that charged a cycle or fired a
/// watchpoint would be a change the guest can see. `None` ends the block
/// exactly as bytes the decoder refuses do.
///
/// `max_blocks` is a *host* bound (every engine refuses a large enough
/// function, at wildly different sizes); [`DiscoverStats::truncated`] says
/// whether it bound.
#[must_use]
pub fn discover(
    seeds: &[u32],
    bounds: Bounds,
    max_blocks: usize,
    fetch: &mut dyn FnMut(u32) -> Option<u8>,
) -> Discovered {
    discover_from(seeds, bounds, max_blocks, &BTreeSet::new(), fetch)
}

/// [`discover`], stopping wherever `known` already holds a block start.
///
/// The incremental walk (XD10; P07 wires the event, this supplies the walk).
/// An installed module owns the blocks it was emitted from, so a second walk
/// over newly published code must neither emit them again nor follow edges
/// into them: `known` is a **stop set** and a **do-not-claim set**, exactly as
/// on the RV32 side. A seed in `known` is skipped; an edge into `known` is not
/// followed, so the block ends there and the stay exits; a block being built
/// stops at the first `known` start it runs into. The hart re-enters through
/// its entry index into whichever module holds the pc.
#[must_use]
pub fn discover_from(
    seeds: &[u32],
    bounds: Bounds,
    max_blocks: usize,
    known: &BTreeSet<u32>,
    fetch: &mut dyn FnMut(u32) -> Option<u8>,
) -> Discovered {
    let mut sweep = Sweep::new(bounds, known, true, fetch);
    sweep.find_starts(seeds, max_blocks);
    sweep.build_blocks()
}

/// Build one block per **supplied** start: widths, extents and literals are
/// honoured, edges are not followed.
///
/// P04's door, kept: the seam tests want "one start, one block", and
/// `--jit-seeds` wants a walk from exactly the addresses it was given. A block
/// ends after a terminator, before an instruction the translator does not
/// decode, at [`MAX_BLOCK_INSTS`], at the next supplied start, and at an
/// address that would wrap.
#[must_use]
pub fn build(starts: &[u32], fetch: &mut dyn FnMut(u32) -> Option<u8>) -> Discovered {
    let mut sweep = Sweep::new(Bounds::NONE, &EMPTY, false, fetch);
    sweep.find_starts(starts, usize::MAX);
    sweep.build_blocks()
}

static EMPTY: BTreeSet<u32> = BTreeSet::new();

/// The third path's seeds (rule 8): every word-aligned address in `spans`
/// whose bytes decode to an instruction the translator will take.
///
/// `spans` are **write** addresses `[lo, hi)`; `exec_of` maps each to the
/// address the code executes at (identity on the classic, the alias offset on
/// the S3), and the seeds are execute addresses. Sorted and unique.
#[must_use]
pub fn word_seeds(
    spans: &[(u32, u32)],
    exec_of: &dyn Fn(u32) -> u32,
    fetch: &mut dyn FnMut(u32) -> Option<u8>,
) -> Vec<u32> {
    let mut out = Vec::new();
    for &(lo, hi) in spans {
        let mut at = lo.wrapping_add(3) & !3;
        while at < hi {
            let x = exec_of(at);
            let mut bytes = [0u8; 3];
            let mut got = 0;
            for (i, slot) in bytes.iter_mut().enumerate() {
                match x.checked_add(i as u32).and_then(&mut *fetch) {
                    Some(b) => {
                        *slot = b;
                        got = i + 1;
                    }
                    None => break,
                }
            }
            if got > 0 && matches!(decode(&bytes[..got]), Decode::Ok(_)) {
                out.push(x);
            }
            let Some(next) = at.checked_add(4) else { break };
            at = next;
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Why a block ended, for the counters; a terminator, a start it fell into
/// and the length cap are not "short".
#[derive(Clone, Copy)]
enum Why {
    Terminator,
    Undecodable,
    Refused,
    Extent,
    Data,
}

/// One decode attempt inside a bound.
enum Step {
    Ok(Decoded),
    /// A decoded instruction the translator refuses; the walk steps over it
    /// by this many bytes (rule 7).
    Undecodable(u8),
    /// Bytes the decoder does not decode, or none fetched: the walk stops.
    Refused,
    /// The instruction (or its first byte) lies at or past the bound.
    OverLimit,
    /// A word an `l32r` names: data (rule 4).
    Literal,
}

struct Sweep<'a, 'f> {
    bounds: Bounds<'a>,
    known: &'a BTreeSet<u32>,
    /// Follow static edges (the sweep) or not ([`build`]).
    follow: bool,
    fetch: &'f mut dyn FnMut(u32) -> Option<u8>,
    starts: BTreeSet<u32>,
    literals: BTreeSet<u32>,
    /// `LEND` → `LBEG`, from every `loop` the walk decoded.
    lends: BTreeMap<u32, u32>,
    stats: DiscoverStats,
}

impl<'a, 'f> Sweep<'a, 'f> {
    fn new(
        bounds: Bounds<'a>,
        known: &'a BTreeSet<u32>,
        follow: bool,
        fetch: &'f mut dyn FnMut(u32) -> Option<u8>,
    ) -> Self {
        Self {
            bounds,
            known,
            follow,
            fetch,
            starts: BTreeSet::new(),
            literals: BTreeSet::new(),
            lends: BTreeMap::new(),
            stats: DiscoverStats::default(),
        }
    }

    /// Decode the instruction at `at`, which may not reach `limit`.
    fn step(&mut self, at: u32, limit: u32) -> Step {
        if at >= limit {
            return Step::OverLimit;
        }
        if self.literals.contains(&at) {
            return Step::Literal;
        }
        let room = (limit - at).min(3) as usize;
        let mut bytes = [0u8; 3];
        let mut got = 0usize;
        for (i, slot) in bytes.iter_mut().enumerate().take(room) {
            match at.checked_add(i as u32).and_then(&mut *self.fetch) {
                Some(b) => {
                    *slot = b;
                    got = i + 1;
                }
                None => break,
            }
        }
        if got == 0 {
            return Step::Refused;
        }
        match decode(&bytes[..got]) {
            Decode::Ok(d) => {
                // An instruction that would cross the bound, or wrap the
                // address space, is not this block's to run.
                match at.checked_add(u32::from(d.width)) {
                    Some(next) if next <= limit => {
                        if let Inst::L32r(_, imm) = d.inst {
                            self.literals
                                .insert(lp_xt_inst::disasm::l32r_target(at, imm));
                        }
                        if let Edges::Loop { body, end } = edges(at, &d) {
                            self.lends.insert(end, body);
                        }
                        Step::Ok(d)
                    }
                    _ => Step::OverLimit,
                }
            }
            // `ill` ends the walk like bytes that do not decode: nothing
            // follows it by fall-through, and zeroed memory is made of it.
            Decode::Undecodable { inst, .. } if crate::decode::ends_the_walk(&inst) => {
                Step::Refused
            }
            Decode::Undecodable { width, .. } => match at.checked_add(u32::from(width)) {
                Some(next) if next <= limit => Step::Undecodable(width),
                _ => Step::OverLimit,
            },
            Decode::Refused { .. } => Step::Refused,
        }
    }

    /// Pass one: where do blocks start?
    fn find_starts(&mut self, seeds: &[u32], max_blocks: usize) {
        self.stats.seeds = seeds.len();
        let mut queue: Vec<u32> = Vec::new();
        let mut walked: BTreeSet<u32> = BTreeSet::new();
        let mut truncated = false;
        let mut seeds = seeds.iter().copied();

        // A seed is a place to start looking, not a block to have: each is
        // explored to exhaustion before the next is even added (rule 1).
        loop {
            let Some(pc) = queue.pop().or_else(|| {
                seeds.find(|&pc| {
                    if self.starts.len() >= max_blocks {
                        truncated = true;
                        return false;
                    }
                    !self.known.contains(&pc) && self.starts.insert(pc)
                })
            }) else {
                break;
            };
            if !walked.insert(pc) {
                continue;
            }
            let limit = self.bounds.limit(pc);
            let mut at = pc;
            let mut room = MAX_BLOCK_INSTS;
            loop {
                let step = self.step(at, limit);
                let mut edge = |target: u32, this: &mut Self| {
                    if this.starts.len() >= max_blocks {
                        truncated = true;
                        return;
                    }
                    // A pc an installed module already holds is not this
                    // walk's to claim: the block ends there and the stay exits.
                    if this.known.contains(&target) {
                        return;
                    }
                    if this.starts.insert(target) {
                        queue.push(target);
                    }
                };
                match step {
                    Step::Ok(d) => {
                        let next = at.wrapping_add(u32::from(d.width));
                        if self.follow {
                            match edges(at, &d) {
                                Edges::Jump(t) => {
                                    edge(t, self);
                                    break;
                                }
                                Edges::Branch { target, next } => {
                                    edge(target, self);
                                    edge(next, self);
                                    break;
                                }
                                Edges::Call { target, ret } => {
                                    if let Some(t) = target {
                                        edge(t, self);
                                    }
                                    // A call names its return address (rule
                                    // 5): the rest of the calling function is
                                    // invisible without it.
                                    edge(ret, self);
                                    break;
                                }
                                Edges::Loop { body, end } => {
                                    edge(body, self);
                                    edge(end, self);
                                    break;
                                }
                                Edges::Next(n) => {
                                    edge(n, self);
                                    break;
                                }
                                Edges::None => {
                                    if d.control {
                                        // `jx`, `ret`, `retw`, `rfi`…: the
                                        // block ends and names nothing.
                                        break;
                                    }
                                }
                            }
                        } else if d.control {
                            break;
                        }
                        room -= 1;
                        if room == 0 {
                            // Cut by length rather than by a transfer, so the
                            // cut point is a start too — otherwise pass two
                            // ends the block falling into an address the
                            // module has no label for.
                            if self.follow {
                                edge(next, self);
                            }
                            break;
                        }
                        if next >= limit {
                            // The extent ends here (rule 3): a `Fall` into the
                            // next symbol if one starts exactly here.
                            if self.follow && self.bounds.is_symbol_start(next) {
                                edge(next, self);
                            }
                            break;
                        }
                        // Running into a start that is, or will be, walked in
                        // its own right: the rest is that start's.
                        if self.known.contains(&next) || self.starts.contains(&next) {
                            break;
                        }
                        at = next;
                    }
                    Step::Undecodable(width) => {
                        // Rule 7: the block ends before it, and the address
                        // after it is a real start because the decoder — not
                        // a guess — gave the width.
                        if self.follow {
                            let next = at.wrapping_add(u32::from(width));
                            if next < limit {
                                edge(next, self);
                            }
                        }
                        break;
                    }
                    Step::Refused | Step::OverLimit | Step::Literal => break,
                }
            }
        }

        // Rule 4: a start on a word an `l32r` names is data, whatever named it.
        let dropped: Vec<u32> = self
            .starts
            .iter()
            .copied()
            .filter(|pc| self.literals.contains(pc))
            .collect();
        for pc in &dropped {
            self.starts.remove(pc);
        }
        self.stats.literal_starts_dropped = dropped.len();
        self.stats.literals = self.literals.len();
        self.stats.loop_ends = self.lends.len();
        self.stats.starts = self.starts.len();
        self.stats.truncated = truncated;
    }

    /// Pass two: build each block, stopping at the next start.
    fn build_blocks(mut self) -> Discovered {
        let starts: Vec<u32> = self.starts.iter().copied().collect();
        let mut blocks: Vec<Block> = Vec::with_capacity(starts.len());
        let mut index = BTreeMap::new();
        for &pc in &starts {
            let limit = self.bounds.limit(pc);
            let mut insts: Vec<(u32, Decoded)> = Vec::new();
            let mut at = pc;
            // Why the block ended short of a terminator, counted only once
            // the block is known to hold something: a start whose *first*
            // instruction is refused is an empty start and nothing else.
            let mut why = Why::Terminator;
            let end = loop {
                if insts.len() >= MAX_BLOCK_INSTS {
                    self.stats.capped += 1;
                    break BlockEnd::Fall(at);
                }
                // Another block starts here, so this one falls into it rather
                // than decoding the same instruction twice.
                if at != pc && (self.starts.contains(&at) || self.known.contains(&at)) {
                    break BlockEnd::Fall(at);
                }
                match self.step(at, limit) {
                    Step::Ok(mut d) => {
                        let next = at.wrapping_add(u32::from(d.width));
                        // Rule 6: the instruction ending exactly at a `LEND`
                        // is a terminator with a static back-edge to `LBEG`.
                        if let Some(&lbeg) = self.lends.get(&next) {
                            d.lbeg = Some(lbeg);
                            d.control = true;
                        }
                        let control = d.control;
                        insts.push((at, d));
                        if control {
                            break BlockEnd::Term;
                        }
                        if next >= limit {
                            why = Why::Extent;
                            break if self.bounds.is_symbol_start(next) {
                                BlockEnd::Fall(next)
                            } else {
                                BlockEnd::Undecodable(next)
                            };
                        }
                        at = next;
                    }
                    Step::Undecodable(_) => {
                        why = Why::Undecodable;
                        break BlockEnd::Undecodable(at);
                    }
                    Step::Refused => {
                        why = Why::Refused;
                        break BlockEnd::Undecodable(at);
                    }
                    Step::OverLimit => {
                        why = Why::Extent;
                        break BlockEnd::Undecodable(at);
                    }
                    Step::Literal => {
                        why = Why::Data;
                        break BlockEnd::Undecodable(at);
                    }
                }
            };
            // A start whose very first instruction could not be decoded has
            // no block: an empty one would be an entry the module cannot run,
            // and a symbol that names data is exactly this.
            if insts.is_empty() {
                self.stats.empty_starts += 1;
                continue;
            }
            match why {
                Why::Terminator => {}
                Why::Undecodable => self.stats.undecodable += 1,
                Why::Refused => self.stats.refused += 1,
                Why::Extent => self.stats.extent_ends += 1,
                Why::Data => self.stats.data_ends += 1,
            }
            self.stats.insts += insts.len();
            index.insert(pc, blocks.len());
            blocks.push(Block { pc, insts, end });
        }
        self.stats.blocks = blocks.len();
        Discovered {
            set: BlockSet::from_blocks(blocks, index),
            stats: self.stats,
        }
    }
}
