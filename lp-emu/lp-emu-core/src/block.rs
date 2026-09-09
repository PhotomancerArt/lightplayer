//! A pre-decoded basic-block cache: the table, the arena, and invalidation.
//!
//! # What it is for
//!
//! An interpreter that re-decides what every instruction *is* on every
//! execution spends most of its host time on that decision rather than on the
//! guest's arithmetic. Measured on the ESP32-C6 machine running the product's
//! own render loop (M5 P1): **~4 % of host time executes the guest's
//! arithmetic and ~74 % works out what the next instruction is** — fetching
//! it, decoding it, dispatching to a handler, and the per-instruction
//! bookkeeping around all three.
//!
//! A block cache decides once for a *run* of instructions and remembers the
//! answer. The same measurement says the memory for that is trivial and the
//! re-use is essentially perfect: the render loop enters **~37,500 distinct
//! block starts** across 428 executed 4 KiB pages, and **99.7 %** of block
//! entries go to a start that has been entered a hundred times or more. There
//! is no warm-up problem and no capacity problem, so a flat direct-mapped
//! table with a tag is enough — no hash map, no LRU.
//!
//! # What this module knows, and what it must never know
//!
//! It owns block identity, the direct-mapped table, the slot arena,
//! invalidation and the block's cost bound. It **names no `Bus` and no ISA
//! type** (M5 MD1/F4): the architecture supplies the slot type and the decoder
//! as its own code, and `lp-xt-emu` — which has no bus at all — must be able
//! to join the same layer. The one thing the core does know about a slot is
//! its width and an upper bound on what it can charge, because that is what
//! the whole-block budget test needs.
//!
//! # The invariant everything else follows from
//!
//! **The cache changes dispatch, never accounting.** Nothing observable may
//! depend on a hit or a miss: the same guest, the same input and the same
//! grade produce the same cycle count, the same instruction count, the same
//! transcript and the same waveform with the cache on or off. It is therefore
//! **not architectural state** — it is absent from a snapshot, and a restore
//! invalidates all of it.
//!
//! # Per-package `opt-level = 3` reaches only what it codegens
//!
//! Every generic here is instantiated from inside an emulator crate that the
//! root `Cargo.toml`'s D2 list names (`lp-riscv-emu`, `lp-emu-esp32c6`, and
//! later `lp-xt-emu`), behind **non-generic entry points**, following the
//! pattern `lp_xt_emu::Emulator::run_loop` documents. A per-package
//! `opt-level` override only reaches code *codegen'd in that package*: a
//! generic function is codegen'd in whichever crate instantiates it, at that
//! crate's opt-level, so a generic public entry point silently opts the hot
//! loop out of the override. M6 lost 25 % on the Xtensa probe to exactly that
//! (plan DD6). Do not expose a generic entry point from here that a crate
//! outside the list would instantiate.
//!
//! # Invalidation
//!
//! Correctness comes from `fence.i` (M5 MD12). The firmware emits one after
//! publishing JIT'd code — `lpvm_native::rt_jit::buffer::JitBuffer::from_code`
//! — and the architecture's fence handler calls [`BlockCache::invalidate_all`].
//! Code the *emulator itself* writes (a flash-cache refill, a ROM-hook
//! `ebreak` patch, a snapshot restore, a reboot) never emits a guest fence, so
//! each of those funnels calls [`BlockCache::invalidate_range`] or
//! [`BlockCache::invalidate_all`] explicitly.

extern crate alloc;

use alloc::{boxed::Box, vec, vec::Vec};

use crate::cycle_model::{CycleModel, InstClass};

/// One pre-decoded instruction, as the architecture chose to represent it.
///
/// The core needs exactly two facts about it.
pub trait Slot: Copy {
    /// Bytes the instruction occupies — 2 for a compressed encoding, 4
    /// otherwise. The block's byte span is the sum, and that span is what
    /// [`BlockCache::invalidate_range`] tests against.
    fn width(&self) -> u8;

    /// An **upper bound** on the cost class this slot can charge.
    ///
    /// Upper bound, not the exact class, and deliberately so. It is used only
    /// by the whole-block budget test ([`Block::max_cycles`]); the cycles
    /// actually charged always come from the class the executor *returns*, so
    /// a bound that is too generous costs a little fast-path coverage near a
    /// slice deadline and can never miscount a cycle. A bound that were too
    /// *small* would be a correctness bug, which is why the decoder is
    /// allowed to answer coarsely (`DivRem` for any `OP` with the M-extension
    /// funct7, `Load` for any compressed body slot) rather than re-deriving
    /// the executor's exact classification and risking a disagreement with it.
    fn cost_bound(&self) -> InstClass;
}

/// A run of pre-decoded instructions ending at a control transfer.
///
/// Blocks deliberately do **not** end at a plain store (M5 MD2): P1 measured
/// that terminating there costs 28–31 % of the mean block length and produces
/// 40–45 % more blocks, and the self-modifying-code pressure it would insure
/// against is 0.02 % of stores across five pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    /// Guest address of the first instruction.
    pub pc: u32,
    /// Bytes the block's instructions occupy, so `[pc, pc + bytes)` is the
    /// span an invalidation compares against.
    pub bytes: u32,
    /// The most cycles this block can charge, summed from every slot's
    /// [`Slot::cost_bound`].
    ///
    /// The budget rule (M5 MD3): a block may be run whole, with no per-slot
    /// deadline compare, exactly when `cycle_count + max_cycles <= end`.
    /// Every interior instruction boundary is then strictly below `end` — the
    /// prefix is strictly less than the whole — so the slice stops at
    /// precisely the instruction a single-stepping loop would have stopped
    /// at. Otherwise the block runs slot by slot with the compare the
    /// single-stepping loop uses today. Both branches are exact.
    pub max_cycles: u32,
    /// First slot in the arena.
    pub start: u32,
    /// Slot count.
    pub len: u32,
}

impl Block {
    /// The half-open guest byte span the block's instructions occupy.
    #[inline]
    #[must_use]
    pub const fn end(&self) -> u32 {
        self.pc.wrapping_add(self.bytes)
    }
}

/// One direct-mapped table entry: the tag, and where the block lives.
#[derive(Clone, Copy)]
struct Entry {
    /// Full guest `pc` of the block this entry names — the tag. A partial tag
    /// would alias two block starts onto one entry and run the wrong code.
    pc: u32,
    /// Index into `blocks`, or [`NO_BLOCK`] when the entry is empty.
    block: u32,
}

const NO_BLOCK: u32 = u32::MAX;
/// "No arena range to recycle" — an arena index this large cannot occur.
const NO_SLOT: u32 = u32::MAX;

const EMPTY_ENTRY: Entry = Entry {
    pc: 0,
    block: NO_BLOCK,
};

/// What the cache did, for the run report and for the phase's own check
/// against P1's predictions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockStats {
    /// Blocks built by the architecture's decoder.
    pub decodes: u64,
    /// Block entries served from the table.
    pub hits: u64,
    /// Slots executed — the retired-instruction count *through the cache*,
    /// which with `decodes + hits` gives the mean realised block length.
    pub slots_run: u64,
    /// Whole-cache flushes: `fence.i`, a snapshot restore, a reboot, a cycle
    /// model change, or an arena that filled.
    pub flushes: u64,
    /// Flushes caused by the arena or the block list filling. **A non-zero
    /// count on the render images is a finding**, not a number to accept: P1's
    /// working set says it should be rare to never.
    pub capacity_flushes: u64,
    /// [`BlockCache::invalidate_range`] calls — the emulator-side code-writer
    /// funnels.
    pub range_invalidations: u64,
    /// Entries dropped by a range invalidation.
    pub range_entries_dropped: u64,
    /// Decodes that landed on a table entry already holding a *different*
    /// block start. The direct-mapped collision rate; MD sizing says start at
    /// 2^16 entries and report this.
    pub collisions: u64,
    /// Slots the arena has ever held at once.
    pub arena_high_water: u32,
    /// Blocks the block list has ever held at once.
    pub blocks_high_water: u32,
}

impl BlockStats {
    /// Mean realised block length: slots executed per block entered.
    #[must_use]
    pub fn mean_block_len(&self) -> f64 {
        let entries = self.decodes + self.hits;
        if entries == 0 {
            0.0
        } else {
            self.slots_run as f64 / entries as f64
        }
    }

    /// Share of block entries served without a decode.
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let entries = self.decodes + self.hits;
        if entries == 0 {
            0.0
        } else {
            self.hits as f64 / entries as f64
        }
    }
}

/// Default table size: 2^16 entries.
///
/// P1 measured **37,517 distinct block starts** on `render-basic` and 34,906
/// on `render-rocaille`; 2^12 would thrash and 2^16 leaves the table a little
/// over half occupied. 512 KiB of table at 8 bytes an entry.
pub const DEFAULT_TABLE_BITS: u32 = 16;

/// Default slot-arena cap.
///
/// P1's working set is ~37.5 k blocks at a mean of 4.69 slots — about 176 k
/// slots. 2^19 leaves room for the re-decodes a direct-mapped table's
/// collisions cause without ever reaching the capacity flush.
pub const DEFAULT_ARENA_SLOTS: usize = 1 << 19;

/// Default cap on distinct live blocks, for the same reason.
pub const DEFAULT_BLOCK_CAP: usize = 1 << 17;

/// A direct-mapped cache of pre-decoded blocks over one flat slot arena.
///
/// Overflow **flushes the whole cache** (M5 MD7): deterministic, a pure
/// function of the instruction stream, trivially correct, and with no LRU to
/// get wrong. `rv32emu`'s `code_cache_flush` makes the same choice.
pub struct BlockCache<S: Slot> {
    table: Box<[Entry]>,
    index_mask: u32,
    blocks: Vec<Block>,
    block_cap: usize,
    arena: Vec<S>,
    arena_cap: usize,
    /// The decoder's landing pad, kept here so a block build allocates
    /// nothing on the hot path.
    scratch: Vec<S>,
    stats: BlockStats,
}

impl<S: Slot> BlockCache<S> {
    /// A cache with `1 << table_bits` table entries and room for `arena_cap`
    /// slots across `block_cap` blocks.
    ///
    /// # Panics
    /// If `table_bits` is 0 or above 24 — a table outside that range is a
    /// configuration mistake rather than a tuning choice.
    #[must_use]
    pub fn new(table_bits: u32, arena_cap: usize, block_cap: usize) -> Self {
        assert!(
            (1..=24).contains(&table_bits),
            "BlockCache: table_bits {table_bits} is outside 1..=24"
        );
        let entries = 1usize << table_bits;
        Self {
            table: vec![EMPTY_ENTRY; entries].into_boxed_slice(),
            index_mask: (entries as u32) - 1,
            blocks: Vec::new(),
            block_cap,
            arena: Vec::new(),
            arena_cap,
            scratch: Vec::new(),
            stats: BlockStats::default(),
        }
    }

    /// The sizes M5 P2 measured against.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_TABLE_BITS, DEFAULT_ARENA_SLOTS, DEFAULT_BLOCK_CAP)
    }

    #[inline]
    #[must_use]
    pub const fn stats(&self) -> BlockStats {
        self.stats
    }

    /// Table entries, for the run report.
    #[inline]
    #[must_use]
    pub fn table_entries(&self) -> usize {
        self.table.len()
    }

    #[inline]
    fn index_of(&self, pc: u32) -> usize {
        // RVC makes every `pc` 2-aligned, so bit 0 carries no information.
        ((pc >> 1) & self.index_mask) as usize
    }

    /// The block starting at `pc`, if one is cached.
    #[inline]
    #[must_use]
    pub fn lookup(&mut self, pc: u32) -> Option<Block> {
        let entry = self.table[self.index_of(pc)];
        if entry.block == NO_BLOCK || entry.pc != pc {
            return None;
        }
        self.stats.hits += 1;
        Some(self.blocks[entry.block as usize])
    }

    /// One slot of a block, by absolute arena index.
    ///
    /// Bounds-checked on purpose: the milestone forbids `unsafe` for
    /// dispatch, and a mis-derived index must be a panic rather than a wild
    /// read.
    #[inline]
    #[must_use]
    pub fn slot(&self, index: u32) -> S {
        self.arena[index as usize]
    }

    /// Count a slot as executed, for [`BlockStats::mean_block_len`].
    #[inline]
    pub fn note_slots_run(&mut self, slots: u32) {
        self.stats.slots_run += u64::from(slots);
    }

    /// Build the block at `pc`: `fill` appends its slots to the scratch
    /// buffer, and what it appended is installed in the table.
    ///
    /// Returns `None` when `fill` produced no slots — the architecture's way
    /// of saying "this address is not cacheable", which is always safe: the
    /// caller falls back to single-stepping and behaviour is unchanged.
    ///
    /// `model` is the cycle model the block's [`Block::max_cycles`] is
    /// computed against. A hart that changes its cycle model must invalidate
    /// (`MachineHart::set_cycle_model` does), or a stale bound would let a
    /// block run whole past a deadline it should have stopped inside.
    pub fn build<F>(&mut self, pc: u32, model: CycleModel, fill: F) -> Option<Block>
    where
        F: FnOnce(&mut Vec<S>),
    {
        self.scratch.clear();
        fill(&mut self.scratch);
        let len = self.scratch.len();
        if len == 0 {
            return None;
        }

        let mut bytes = 0u32;
        let mut max_cycles = 0u32;
        for slot in &self.scratch {
            bytes += u32::from(slot.width());
            max_cycles += u32::from(model.cycles_for(slot.cost_bound()));
        }

        let index = self.index_of(pc);
        let existing = self.table[index];

        // Overwriting an entry recycles its block record, and its arena range
        // when the new block fits in the old one. Without that, two hot
        // blocks aliasing one table index would append to the arena on every
        // alternation and drive a capacity flush that P1's working set says
        // should never happen.
        let mut block_index = NO_BLOCK;
        let mut start = NO_SLOT;
        if existing.block != NO_BLOCK {
            if existing.pc != pc {
                self.stats.collisions += 1;
            }
            block_index = existing.block;
            let old = self.blocks[block_index as usize];
            if old.len >= len as u32 {
                start = old.start;
            }
        }

        if start == NO_SLOT {
            let needs_block_record = block_index == NO_BLOCK;
            if self.arena.len() + len > self.arena_cap
                || (needs_block_record && self.blocks.len() >= self.block_cap)
            {
                // Everything is gone, including the record we were about to
                // recycle and the arena offset we were about to take.
                self.flush_for_capacity();
                block_index = NO_BLOCK;
            }
            start = self.arena.len() as u32;
            self.arena.extend_from_slice(&self.scratch);
        } else {
            let at = start as usize;
            self.arena[at..at + len].copy_from_slice(&self.scratch);
        }

        let block = Block {
            pc,
            bytes,
            max_cycles,
            start,
            len: len as u32,
        };
        if block_index == NO_BLOCK {
            self.blocks.push(block);
            block_index = (self.blocks.len() - 1) as u32;
        } else {
            self.blocks[block_index as usize] = block;
        }
        self.finish_install(index, pc, block_index);
        Some(block)
    }

    fn finish_install(&mut self, index: usize, pc: u32, block: u32) {
        self.table[index] = Entry { pc, block };
        self.stats.decodes += 1;
        self.stats.arena_high_water = self.stats.arena_high_water.max(self.arena.len() as u32);
        self.stats.blocks_high_water = self.stats.blocks_high_water.max(self.blocks.len() as u32);
    }

    fn flush_for_capacity(&mut self) {
        self.stats.capacity_flushes += 1;
        self.clear();
    }

    /// Forget every block.
    ///
    /// The `fence.i` answer (M5 MD12), and the answer for a snapshot restore,
    /// a reboot and a cycle-model change. `fence.i` fires a handful of times
    /// in a render run — once per shader compile — so range precision would
    /// buy nothing and cost correctness surface.
    pub fn invalidate_all(&mut self) {
        self.stats.flushes += 1;
        self.clear();
    }

    fn clear(&mut self) {
        self.table.fill(EMPTY_ENTRY);
        self.blocks.clear();
        self.arena.clear();
    }

    /// Forget every block whose instructions overlap `[lo, hi)`.
    ///
    /// The emulator-side funnels: a flash-cache MMU refill and a ROM-hook
    /// `ebreak` patch both write guest *code* without the guest ever
    /// executing a `fence.i`, so each one names the window it wrote. A block
    /// that merely *starts* before `lo` still counts — it may reach into the
    /// window.
    ///
    /// The arena space the dropped blocks held is not reclaimed; it is
    /// recovered by the next capacity flush. Range invalidations are rare
    /// (P1: 98 cache refills across a whole render run) and a compacting
    /// arena would be a moving-target bug for no measured gain.
    pub fn invalidate_range(&mut self, lo: u32, hi: u32) {
        self.stats.range_invalidations += 1;
        if lo >= hi {
            return;
        }
        let mut dropped = 0u64;
        for entry in self.table.iter_mut() {
            if entry.block == NO_BLOCK {
                continue;
            }
            let block = self.blocks[entry.block as usize];
            if block.pc < hi && block.end() > lo {
                *entry = EMPTY_ENTRY;
                dropped += 1;
            }
        }
        self.stats.range_entries_dropped += dropped;
    }

    /// True when nothing is cached — what the build-time placement paths
    /// assert rather than invalidate.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

impl<S: Slot> core::fmt::Debug for BlockCache<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BlockCache")
            .field("table_entries", &self.table.len())
            .field("blocks", &self.blocks.len())
            .field("arena_slots", &self.arena.len())
            .field("stats", &self.stats)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A toy slot: arch-free and bus-free, which is the point of this layer.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Toy {
        width: u8,
        class: InstClass,
        tag: u32,
    }

    impl Slot for Toy {
        fn width(&self) -> u8 {
            self.width
        }
        fn cost_bound(&self) -> InstClass {
            self.class
        }
    }

    fn toy(tag: u32) -> Toy {
        Toy {
            width: 4,
            class: InstClass::Alu,
            tag,
        }
    }

    fn build(cache: &mut BlockCache<Toy>, pc: u32, n: u32) -> Block {
        cache
            .build(pc, CycleModel::Esp32C6, |out| {
                for i in 0..n {
                    out.push(toy(pc + i));
                }
            })
            .expect("a non-empty block installs")
    }

    #[test]
    fn a_built_block_is_found_again_and_its_slots_come_back() {
        let mut cache = BlockCache::<Toy>::new(8, 64, 16);
        let block = build(&mut cache, 0x4080_0000, 3);
        assert_eq!(block.pc, 0x4080_0000);
        assert_eq!(block.bytes, 12);
        assert_eq!(block.len, 3);
        // Three `Alu` slots at one cycle each under the C6 model.
        assert_eq!(block.max_cycles, 3);

        let found = cache.lookup(0x4080_0000).expect("cached");
        assert_eq!(found, block);
        for i in 0..3 {
            assert_eq!(cache.slot(found.start + i).tag, 0x4080_0000 + i);
        }
        assert_eq!(cache.stats().decodes, 1);
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn an_empty_decode_installs_nothing_and_the_address_stays_uncached() {
        let mut cache = BlockCache::<Toy>::new(8, 64, 16);
        assert!(cache.build(0x4080_0000, CycleModel::Esp32C6, |_| {}).is_none());
        assert!(cache.lookup(0x4080_0000).is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn max_cycles_sums_the_slots_cost_bounds() {
        let mut cache = BlockCache::<Toy>::new(8, 64, 16);
        let block = cache
            .build(0x4080_0000, CycleModel::Esp32C6, |out| {
                out.push(Toy {
                    width: 4,
                    class: InstClass::Load,
                    tag: 0,
                }); // 2
                out.push(Toy {
                    width: 2,
                    class: InstClass::DivRem,
                    tag: 1,
                }); // 32
                out.push(Toy {
                    width: 4,
                    class: InstClass::BranchTaken,
                    tag: 2,
                }); // 2
            })
            .unwrap();
        assert_eq!(block.max_cycles, 36);
        assert_eq!(block.bytes, 10);
    }

    #[test]
    fn a_tag_mismatch_on_the_same_index_is_a_miss_not_the_wrong_block() {
        // 2^3 entries: `pc >> 1` mod 8. 0x1000 and 0x1010 collide.
        let mut cache = BlockCache::<Toy>::new(3, 64, 16);
        build(&mut cache, 0x1000, 2);
        assert!(cache.lookup(0x1000).is_some());
        build(&mut cache, 0x1010, 2);
        assert!(cache.lookup(0x1010).is_some());
        assert!(
            cache.lookup(0x1000).is_none(),
            "the evicted start must miss, never answer with its evictor's block"
        );
        assert_eq!(cache.stats().collisions, 1);
    }

    #[test]
    fn an_alternating_pair_on_one_index_reuses_the_arena_rather_than_growing_it() {
        let mut cache = BlockCache::<Toy>::new(3, 64, 16);
        build(&mut cache, 0x1000, 4);
        let high = cache.stats().arena_high_water;
        for _ in 0..20 {
            build(&mut cache, 0x1010, 4);
            build(&mut cache, 0x1000, 4);
        }
        assert_eq!(
            cache.stats().arena_high_water,
            high,
            "a ping-pong at one index must not append to the arena forever"
        );
        assert_eq!(cache.stats().capacity_flushes, 0);
    }

    #[test]
    fn invalidate_all_forgets_everything_and_counts_the_flush() {
        let mut cache = BlockCache::<Toy>::new(8, 64, 16);
        build(&mut cache, 0x4080_0000, 3);
        build(&mut cache, 0x4080_1000, 3);
        cache.invalidate_all();
        assert!(cache.lookup(0x4080_0000).is_none());
        assert!(cache.lookup(0x4080_1000).is_none());
        assert!(cache.is_empty());
        assert_eq!(cache.stats().flushes, 1);
    }

    #[test]
    fn invalidate_range_drops_a_block_that_reaches_into_the_window() {
        let mut cache = BlockCache::<Toy>::new(12, 64, 16);
        // 0x1000..0x1010, 0x1010..0x1020, 0x1100..0x1110 — three distinct
        // table indices at 12 bits.
        build(&mut cache, 0x1000, 4);
        build(&mut cache, 0x1010, 4);
        build(&mut cache, 0x1100, 4);

        // A window inside the FIRST block's span, starting after its pc.
        cache.invalidate_range(0x1008, 0x100c);
        assert!(
            cache.lookup(0x1000).is_none(),
            "a block that starts before the window but reaches into it must go"
        );
        assert!(cache.lookup(0x1010).is_some());
        assert!(cache.lookup(0x1100).is_some());
        assert_eq!(cache.stats().range_entries_dropped, 1);
    }

    #[test]
    fn invalidate_range_touching_only_the_boundary_keeps_both_neighbours() {
        let mut cache = BlockCache::<Toy>::new(8, 64, 16);
        build(&mut cache, 0x1000, 4); // 0x1000..0x1010
        build(&mut cache, 0x1010, 4); // 0x1010..0x1020
        // Empty window at the seam.
        cache.invalidate_range(0x1010, 0x1010);
        assert!(cache.lookup(0x1000).is_some());
        assert!(cache.lookup(0x1010).is_some());
        assert_eq!(cache.stats().range_entries_dropped, 0);
    }

    #[test]
    fn an_arena_that_fills_flushes_the_whole_cache_and_still_installs() {
        let mut cache = BlockCache::<Toy>::new(12, 8, 16);
        build(&mut cache, 0x1000, 4);
        build(&mut cache, 0x1100, 4);
        assert!(cache.lookup(0x1000).is_some());
        // The arena is full; the next block flushes and starts over.
        let block = build(&mut cache, 0x1200, 4);
        assert_eq!(cache.stats().capacity_flushes, 1);
        assert!(cache.lookup(0x1000).is_none());
        assert!(cache.lookup(0x1100).is_none());
        assert_eq!(cache.lookup(0x1200), Some(block));
        assert_eq!(cache.slot(block.start).tag, 0x1200);
    }

    #[test]
    fn mean_block_length_and_hit_rate_read_off_the_counters() {
        let mut cache = BlockCache::<Toy>::new(8, 64, 16);
        build(&mut cache, 0x1000, 4);
        cache.note_slots_run(4);
        for _ in 0..9 {
            cache.lookup(0x1000).unwrap();
            cache.note_slots_run(4);
        }
        let stats = cache.stats();
        assert_eq!(stats.decodes, 1);
        assert_eq!(stats.hits, 9);
        assert!((stats.mean_block_len() - 4.0).abs() < 1e-9);
        assert!((stats.hit_rate() - 0.9).abs() < 1e-9);
    }
}
