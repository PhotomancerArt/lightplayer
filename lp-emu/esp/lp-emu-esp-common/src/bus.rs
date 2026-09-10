//! `SocBus` — RAM regions, an MMIO decode table, and an honest policy for
//! everything else.
//!
//! The user-mode [`lp_emu_core::Memory`] is a flat address space with fixed
//! bases. An SoC is not: it is a handful of RAM windows at chip-specific
//! addresses plus a decode table that routes some ranges to peripherals.
//! `SocBus` is that shape, and it holds **no chip numbers** — the chip crate
//! registers its regions, its MMIO windows and its peripherals, and this
//! crate never learns what a C6 is.
//!
//! Three policies are worth stating out loud, because they are what makes
//! bring-up on this bus different from bring-up on a vendor emulator:
//!
//! 1. **Unmapped is visible.** A read of an address nothing claims returns
//!    0 and a write is dropped — the same as the vendor emulator — but each
//!    distinct `(pc, address)` is logged once and every one is counted.
//!    Silence is what makes a wrong memory map cost a day.
//! 2. **Strict mode makes it fatal.** [`SocBus::set_strict`] turns the same
//!    access into [`MemoryError::InvalidAccess`]. This is the vision's
//!    honest-peripheral policy: a run that must not guess can say so.
//! 3. **Watchpoints fire before the access.** esp-rtos's stack guard is a
//!    trigger on the guard word; a bus that performed the write and then
//!    trapped would have already destroyed the evidence.

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lp_emu_core::bus::{Bus, Watchpoint};
use lp_emu_core::cycle_model::MemoryCost;
use lp_emu_core::memory::{MemoryAccessKind, MemoryError};
use lp_emu_core::sched::{Cycles, EventId, Scheduler};

use crate::host::HostSinks;
use crate::periph::{
    BoxedPeripheral, BusCx, CpuIntMatrix, IrqLines, MachineRequest, NoCpuInterrupts, RegGrade,
    Width,
};
use crate::pins::Fabric;
use crate::trace::{Access, MmioEvent, Trace};

/// The most watchpoint slots any machine on this bus can arm — a **capacity**,
/// not a claim about any chip.
///
/// How many a machine actually has is a chip fact and lives in the chip crate:
/// the RISC-V debug spec's `mcontrol` triggers number four on the ESP32-C6,
/// and Xtensa LX6/LX7 have two `DBREAK` slots. Each machine declares its own
/// with [`SocBus::set_watchpoint_slots`]; this is only the size of the array
/// that holds them, and it grows when some machine needs more.
pub const MAX_WATCHPOINT_SLOTS: usize = 4;

/// How many distinct `(pc, address)` unmapped sites are remembered before
/// the bus stops recording new ones. Counting continues; only the
/// log-it-once set is capped, so a runaway pointer cannot eat the host's
/// memory.
const UNMAPPED_SITES_CAP: usize = 4096;

/// Guard against a peripheral that reschedules itself at the current cycle
/// forever. One `run_due_events` call will not dispatch more than this.
const MAX_EVENTS_PER_TICK: u32 = 100_000;

/// A span of guest RAM at a chip-specific base.
///
/// A region **describes** a span; it does not own its bytes. Those live in
/// the bus's [guest arena](SocBus::guest_arena), one contiguous host
/// allocation in which a guest address sits at `address - arena_base`, flat
/// across the whole mapped span. Read them with
/// [`SocBus::region_bytes`].
///
/// The arena is what lets a translated core address guest memory with a
/// constant folded into each emitted access instead of a per-access region
/// lookup (M7 JD4). It also means a region cannot be read without the bus it
/// belongs to, which is why the byte accessors are on `SocBus`.
#[derive(Clone, Debug)]
pub struct RamRegion {
    pub name: &'static str,
    pub base: u32,
    /// Instruction fetch is allowed from this region.
    pub exec: bool,
    /// Guest stores are allowed. `false` models ROM and a read-only flash
    /// cache window.
    pub writable: bool,
    len: u32,
    /// Initial contents, moved into the bus's arena by
    /// [`SocBus::add_region`]. Empty for every region that is on a bus — it
    /// exists only to carry [`RamRegion::from_bytes`]'s bytes across the
    /// hand-off.
    init: Vec<u8>,
}

impl RamRegion {
    /// A zeroed, readable, writable, non-executable region.
    pub fn new(name: &'static str, base: u32, len: u32) -> Self {
        Self {
            name,
            base,
            exec: false,
            writable: true,
            len,
            init: Vec::new(),
        }
    }

    /// A region initialised from bytes (a ROM image, a flash window).
    pub fn from_bytes(name: &'static str, base: u32, data: Vec<u8>) -> Self {
        Self {
            name,
            base,
            exec: false,
            writable: true,
            len: data.len() as u32,
            init: data,
        }
    }

    pub fn executable(mut self) -> Self {
        self.exec = true;
        self
    }

    pub fn read_only(mut self) -> Self {
        self.writable = false;
        self
    }

    /// May the guest fetch instructions from here? A translated core asks,
    /// because a symbol that is not in an executable region is not a place to
    /// start decoding from.
    pub fn is_executable(&self) -> bool {
        self.exec
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn end(&self) -> u32 {
        self.base.wrapping_add(self.len())
    }

    pub fn contains(&self, address: u32) -> bool {
        address >= self.base && address < self.end()
    }

    fn span_fits(&self, address: u32, len: u32) -> bool {
        self.contains(address) && u64::from(address) + u64::from(len) <= u64::from(self.end())
    }
}

/// The first access strict mode refused, kept so the machine can report the
/// access that actually caused a run to stop.
///
/// The hart turns the refusal into an architectural access fault and jumps to
/// `mtvec` like any other, so by the time a machine notices, the PC has moved
/// on; this is the record of where it really happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrictViolation {
    pub cycle: Cycles,
    pub pc: u32,
    pub address: u32,
    pub width: Width,
    pub access: Access,
    /// The address was inside a declared MMIO window — an unmodelled block,
    /// not a wild pointer.
    pub in_mmio_window: bool,
    /// `Some(grade)` when the refusal was a strict-**grade** one: the
    /// register exists and is modelled, but its [`RegGrade`] is below the
    /// level [`SocBus::set_strict_grade`] asked for. `None` for an unmapped
    /// access.
    pub grade: Option<RegGrade>,
}

/// The bus's scalar state, for a machine snapshot.
///
/// Region bytes and peripheral blobs are saved separately (they are the big
/// halves); this is everything else the bus carries that a restored run has
/// to agree with, including the diagnostic counters — a restored run that
/// reported different unmapped totals would not be the same run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BusScalars {
    pub now: Cycles,
    pub pc: u32,
    pub hart: usize,
    pub sideband: bool,
    /// A peripheral asked the machine to take over before the next
    /// instruction ([`BusCx::yield_to_machine`]).
    pub yield_now: bool,
    pub irq: IrqLines,
    pub unmapped_sites: BTreeSet<(u32, u32)>,
    pub unmapped_reads: u64,
    pub unmapped_writes: u64,
    pub first_strict_violation: Option<StrictViolation>,
    pub request: Option<MachineRequest>,
    /// The signal fabric: routing and pad levels (plan DD34 e). Part of the
    /// bus's state, so a snapshot that forgot it would restore a machine
    /// whose pads had lost their routing.
    pub pins: Fabric,
}

/// One entry in the MMIO decode table.
struct MmioRange {
    base: u32,
    len: u32,
    periph: BoxedPeripheral,
}

/// The SoC bus.
/// Granularity of the `--strict-bus` code-page checker: the chip's own MMU
/// page, and the granularity P1 measured the executed-page set at (428 pages
/// on a whole render run).
const CODE_PAGE_LEN: u32 = 4096;

/// The page granularity of [`SocBus::permission_table`]. 16 KiB is the
/// spike's, and the size the 0.849 ns/instruction phone reading was taken
/// with: small enough that a region boundary costs at most one page, large
/// enough that the whole table is 16 KiB for a 256 MiB span.
pub const PERMISSION_PAGE_LEN: u32 = 16 * 1024;

/// [`SocBus::permission_table`]: not plain RAM. Every access on this page
/// must go through the bus.
pub const PERM_NONE: u8 = 0;
/// [`SocBus::permission_table`]: plain RAM, loads may be performed inline;
/// stores must go through the bus.
pub const PERM_READ: u8 = 1;
/// [`SocBus::permission_table`]: plain RAM, loads and stores may both be
/// performed inline.
pub const PERM_READ_WRITE: u8 = 2;

pub struct SocBus {
    /// Every region's bytes, in one contiguous allocation: the guest address
    /// `a` is at `a - arena_base`, flat across the whole mapped span,
    /// including the gaps between regions.
    ///
    /// The flat map is the point (M7 JD4). It costs address space — the C6's
    /// regions span 0x4000_0000..0x5000_4000, so the arena is ~256 MiB of
    /// *reservation* against ~17 MiB of real regions — and it buys a
    /// translated core that reaches guest memory with one constant fold and
    /// no per-access region lookup. The allocation is zero-filled by the
    /// allocator, so the pages in the gaps are never touched and never
    /// resident.
    ///
    /// It must not move once the machine is running: [`SocBus::add_region`]
    /// is the only thing that can reallocate it, and every region is added
    /// during construction.
    arena: Vec<u8>,
    /// The guest address of `arena[0]`.
    arena_base: u32,
    /// Sorted by base, non-overlapping.
    regions: Vec<RamRegion>,
    /// Two-entry "last hit" data-region cache, checked before the binary
    /// search. One slot missed on every other access: the harness
    /// alternates between HP SRAM (data) and the flash-cache rodata window,
    /// so a single cache thrashed between the two on every load. Slot 0 is
    /// the most recent hit; a hit in slot 1 promotes it to slot 0 (a cheap
    /// two-way LRU, not a full one).
    last_regions: [usize; 2],
    /// The same cache for instruction fetch, kept apart from `last_regions`:
    /// code runs from the flash window while data lives in HP SRAM, so a
    /// shared cache missed on nearly every instruction (fetch, load, fetch…)
    /// and each miss was a binary search.
    last_fetch_region: usize,
    /// Insertion order, so a peripheral's index — which [`event_id`] packs
    /// into the scheduler's event tags — never moves under it.
    mmio: Vec<MmioRange>,
    /// Indices into `mmio`, sorted by base. The decode's binary search.
    mmio_by_base: Vec<usize>,
    /// "Last hit" cache for the MMIO decode: the peripheral index plus its
    /// `(base, len)`, checked with one subtract-compare before
    /// `mmio_by_base`'s binary search. 86% of a boot's MMIO traffic is
    /// UART0's TX-FIFO status register polled at baud, so the same
    /// peripheral answers back-to-back almost always.
    last_mmio: Option<(usize, u32, u32)>,
    /// Address ranges that belong to MMIO even where no peripheral claims
    /// them. The chip crate registers these; the common crate has no
    /// addresses of its own.
    mmio_windows: Vec<(u32, u32)>,

    strict: bool,
    /// `Some(level)`: an access to a register graded below `level` is
    /// refused like an unmapped one. See [`SocBus::set_strict_grade`].
    strict_grade: Option<RegGrade>,
    /// The blocks the level applies to, or `None` for every graded block.
    /// See [`SocBus::set_strict_grade_blocks`].
    strict_grade_blocks: Option<Vec<&'static str>>,
    sideband: bool,
    /// See [`BusCx::yield_to_machine`].
    yield_now: bool,
    /// The chip's misaligned-access policy, mirrored from the hart. The C6
    /// core performs misaligned data accesses in hardware, so its machine
    /// sets both permissive; the flag lives here because the bus is the
    /// component that actually decides.
    allow_unaligned: bool,
    watchpoints: [Option<Watchpoint>; MAX_WATCHPOINT_SLOTS],
    /// How many of [`MAX_WATCHPOINT_SLOTS`] this machine actually has. See
    /// [`SocBus::set_watchpoint_slots`]; the default is the maximum.
    slots: usize,
    /// Bit per armed slot that wants each access kind, indexed by
    /// [`kind_index`]; `0` short-circuits the per-access check. esp-hal keeps
    /// a store trigger on the stack guard for the whole run, so *something*
    /// is armed almost always; a fetch or a load still has to cost one test,
    /// not a four-slot walk.
    armed_for: [u32; 3],
    /// `Some((lo, hi, slot))` when **exactly one** store-kind watchpoint slot
    /// is armed — esp-hal's stack guard holds this for the whole run.
    /// [`SocBus::write`] then costs one range compare instead of the general
    /// slot walk; recomputed in [`set_watchpoint`](Bus::set_watchpoint)
    /// whenever the armed set changes, so more than one store watchpoint
    /// (rare, and the general path still handles it) falls back to `None`.
    single_store_watch: Option<(u64, u64, u8)>,

    unmapped_sites: BTreeSet<(u32, u32)>,
    unmapped_reads: u64,
    unmapped_writes: u64,
    first_strict_violation: Option<StrictViolation>,

    /// Windows of guest **code** this bus has written from the host side
    /// since the machine last drained them ([`SocBus::take_code_writes`]).
    ///
    /// A flash-cache MMU refill and a ROM-hook `ebreak` patch both write
    /// instructions the guest will execute, and neither is followed by a
    /// guest `fence.i` — we are not the guest. Every such write goes through
    /// [`SocBus::load_image`], which is the funnel, so recording it here is
    /// the whole of the emulator side of the block cache's invalidation.
    code_writes: Vec<(u32, u32)>,

    /// `--strict-bus` only: the 4 KiB pages the guest has fetched
    /// instructions from.
    ///
    /// P1 measured 428 of these on a whole render run, so a set is small and
    /// nothing is gained by a bitmap over the address space.
    code_pages: BTreeSet<u32>,
    /// `--strict-bus` only: word addresses on a code page whose **bytes the
    /// guest changed** and has not published with a `fence.i`.
    ///
    /// Word-granular rather than page-granular, and only when the store
    /// actually changed something. Both refinements remove a false positive
    /// the page model has: firmware routinely writes code that lives on the
    /// same 4 KiB page as the writer, and P1 measured that **36 %** of the
    /// render loop's stores onto executed pages write bytes that were already
    /// there.
    unpublished_code_words: BTreeSet<u32>,
    /// `--strict-bus` only: how many times a word in
    /// [`unpublished_code_words`](Self::unpublished_code_words) was then
    /// fetched or run out of a cached block. See
    /// [`SocBus::missing_fence_reports`].
    missing_fence_reports: u64,

    /// Source levels → "the CPU interrupt this hart should take". Installed
    /// by the chip crate; [`NoCpuInterrupts`] until then.
    matrix: Box<dyn CpuIntMatrix>,
    /// A peripheral's request to the machine. See [`MachineRequest`].
    request: Option<MachineRequest>,

    /// What an access's *address* costs, installed by the chip crate, or
    /// `None` for a machine whose time grade charges nothing for one.
    ///
    /// A trait object rather than a generic parameter or an enum, and the
    /// reason is that [`SocBus`] is one type the whole machine names: making
    /// it `SocBus<M>` would make the machine, its builder, every peripheral
    /// view and every test generic over a parameter that only the cycle
    /// counter reads, and would monomorphize the entire run loop twice. An
    /// enum cannot be it either — the variants would have to be the chips'
    /// models, and this crate is below the chips. The cost is one virtual
    /// call per access at a grade that has a model, and one null test at a
    /// grade that does not; [`SocBus::set_matrix`] pays the same price for
    /// the same reason.
    memory_cost: Option<Box<dyn MemoryCost + Send>>,
    /// Cycles [`memory_cost`](Self::memory_cost) has charged since the
    /// privileged stepper last drained it.
    memory_cycles: u32,

    now: Cycles,
    pc: u32,
    hart: usize,

    pub sched: Scheduler,
    pub irq: IrqLines,
    pub trace: Trace,
    pub host: HostSinks,
    /// Where a peripheral's output signal goes: the routing the chip's GPIO
    /// view writes and the levels its output blocks drive. See
    /// [`crate::pins`].
    pub pins: Fabric,
}

impl Default for SocBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Pack a peripheral index and a peripheral-local event number into the
/// scheduler's opaque [`EventId`].
///
/// The scheduler is arch-neutral and does not know what an event *is*; this
/// is the bus's convention for routing one back to its owner. 16 bits of
/// peripheral index, 16 bits of local event number.
pub const fn event_id(peripheral: usize, local: u16) -> EventId {
    EventId(((peripheral as u32) << 16) | local as u32)
}

/// The peripheral index encoded in an [`EventId`] by [`event_id`].
pub const fn event_peripheral(id: EventId) -> usize {
    (id.0 >> 16) as usize
}

/// The peripheral-local event number encoded in an [`EventId`].
pub const fn event_local(id: EventId) -> u16 {
    (id.0 & 0xffff) as u16
}

impl SocBus {
    pub fn new() -> Self {
        Self {
            arena: Vec::new(),
            arena_base: 0,
            regions: Vec::new(),
            last_regions: [0, 0],
            last_fetch_region: 0,
            mmio: Vec::new(),
            mmio_by_base: Vec::new(),
            last_mmio: None,
            mmio_windows: Vec::new(),
            strict: false,
            strict_grade: None,
            strict_grade_blocks: None,
            sideband: false,
            yield_now: false,
            allow_unaligned: false,
            watchpoints: [None; MAX_WATCHPOINT_SLOTS],
            slots: MAX_WATCHPOINT_SLOTS,
            armed_for: [0; 3],
            single_store_watch: None,
            unmapped_sites: BTreeSet::new(),
            unmapped_reads: 0,
            unmapped_writes: 0,
            first_strict_violation: None,
            code_writes: Vec::new(),
            code_pages: BTreeSet::new(),
            unpublished_code_words: BTreeSet::new(),
            missing_fence_reports: 0,
            matrix: Box::new(NoCpuInterrupts),
            request: None,
            memory_cost: None,
            memory_cycles: 0,
            now: 0,
            pc: 0,
            hart: 0,
            sched: Scheduler::new(),
            irq: IrqLines::new(),
            trace: Trace::disabled(),
            host: HostSinks::new(),
            pins: Fabric::new(),
        }
    }

    // ---- construction ------------------------------------------------

    /// Add a RAM region. Panics on an overlap with an existing one: a
    /// machine whose memory map contradicts itself is a build-time bug, and
    /// discovering it as a mysterious aliasing read at cycle 400,000 is
    /// strictly worse than discovering it here.
    pub fn add_region(&mut self, mut region: RamRegion) {
        for r in &self.regions {
            let overlaps = region.base < r.end() && r.base < region.end();
            assert!(
                !overlaps,
                "SocBus: region `{}` (0x{:08x}..0x{:08x}) overlaps `{}` (0x{:08x}..0x{:08x})",
                region.name,
                region.base,
                region.end(),
                r.name,
                r.base,
                r.end()
            );
        }
        self.cover_in_arena(region.base, region.len);
        let init = core::mem::take(&mut region.init);
        if !init.is_empty() {
            let off = (region.base - self.arena_base) as usize;
            self.arena[off..off + init.len()].copy_from_slice(&init);
        }
        self.regions.push(region);
        self.regions.sort_by_key(|r| r.base);
        self.last_regions = [0, 0];
        self.last_fetch_region = 0;
    }

    /// Reserve the guest span the arena covers, before any region is added.
    ///
    /// A chip declares its whole memory map here so the arena is allocated
    /// **once** and never moves. Without it the arena still grows to cover
    /// each region as it arrives, copying the regions already placed — fine
    /// for a handful of small test regions, wasteful for a real map, and it
    /// would move an allocation a translated core may already be holding.
    ///
    /// Calling it after regions exist only widens the span; it never shrinks
    /// it and never drops bytes.
    pub fn reserve_guest_span(&mut self, base: u32, len: u32) {
        self.cover_in_arena(base, len);
    }

    /// Grow the arena so `[base, base + len)` is inside it, moving the bytes
    /// of every region already placed to their new offsets.
    fn cover_in_arena(&mut self, base: u32, len: u32) {
        let want_hi = u64::from(base) + u64::from(len);
        if self.arena.is_empty() {
            self.arena_base = base;
            self.arena = alloc::vec![0u8; len as usize];
            return;
        }
        let have_lo = u64::from(self.arena_base);
        let have_hi = have_lo + self.arena.len() as u64;
        let lo = have_lo.min(u64::from(base));
        let hi = have_hi.max(want_hi);
        if lo == have_lo && hi == have_hi {
            return;
        }
        let mut grown = alloc::vec![0u8; (hi - lo) as usize];
        // Only the regions carry meaningful bytes; the gaps are zero in both
        // the old arena and the new one, so copying region by region is both
        // cheaper than copying the whole span and exactly equivalent.
        for r in &self.regions {
            let from = (u64::from(r.base) - have_lo) as usize;
            let to = (u64::from(r.base) - lo) as usize;
            let n = r.len as usize;
            grown[to..to + n].copy_from_slice(&self.arena[from..from + n]);
        }
        self.arena = grown;
        self.arena_base = lo as u32;
    }

    /// Add a peripheral at `base` covering `len` bytes. Returns its index —
    /// what [`event_id`] packs into an event tag, and what the machine keeps
    /// to reach it later.
    ///
    /// The index is the insertion order and never moves: the decode's sorted
    /// order lives in a side table, because an index that shifted when a
    /// lower-based peripheral was registered later would silently re-point
    /// every already-scheduled event.
    pub fn add_peripheral(&mut self, base: u32, len: u32, mut periph: BoxedPeripheral) -> usize {
        periph.attached(self.mmio.len());
        for r in &self.mmio {
            let overlaps = base < r.base + r.len && r.base < base + len;
            assert!(
                !overlaps,
                "SocBus: peripheral `{}` at 0x{:08x}..0x{:08x} overlaps `{}` at \
                 0x{:08x}..0x{:08x}",
                periph.name(),
                base,
                base + len,
                r.periph.name(),
                r.base,
                r.base + r.len
            );
        }
        self.mmio.push(MmioRange { base, len, periph });
        let index = self.mmio.len() - 1;
        self.mmio_by_base.push(index);
        self.mmio_by_base.sort_by_key(|&i| self.mmio[i].base);
        index
    }

    /// Declare an address range as MMIO. Accesses inside a window that no
    /// peripheral claims are still unmapped, but the machine knows they
    /// were meant to be peripheral space — which is what makes "the ROM
    /// touched a block we have not modelled" readable in the log.
    pub fn add_mmio_window(&mut self, base: u32, len: u32) {
        self.mmio_windows.push((base, len));
        self.mmio_windows.sort_by_key(|(b, _)| *b);
    }

    /// Every access to an address nothing claims becomes a fault instead of
    /// a silent zero. The vision's honest-peripheral policy.
    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
    }

    pub fn strict(&self) -> bool {
        self.strict
    }

    /// Refuse every access to a register whose [`RegGrade`] is **below**
    /// `level` — the honest-peripheral policy one rung up from
    /// [`set_strict`](Self::set_strict): not "is this block modelled" but
    /// "is what the model says about this register backed by evidence".
    /// The refusal is a [`StrictViolation`] with `grade` set and
    /// `in_mmio_window` true, reported the same way as an unmapped access
    /// and, like it, before the peripheral sees the access.
    ///
    /// `Some(RegGrade::Modeled)` refuses nothing (no grade is below it);
    /// `Some(Documented)` refuses a register a graded block calls `Modeled`,
    /// and `Some(Measured)` refuses everything a transcript has not proved.
    ///
    /// # Scope: the blocks that published a table (M6 P4)
    ///
    /// The level applies to a block that **answers** `reg_grade` — one whose
    /// registers somebody read against the PAC, the drivers and a transcript
    /// and then wrote down. A block that publishes no table is passed over,
    /// because "nobody graded this" is a different statement from "this is
    /// modelled", and conflating them made the level useless: with every
    /// accept table ungraded, `documented` stopped at the first MMIO access
    /// of the boot (M6 P2, deviation 4) and the flag measured how much of
    /// the chip had been graded rather than what the run was allowed to
    /// trust.
    ///
    /// [`blocks_in_strict_grade_scope`](Self::blocks_in_strict_grade_scope)
    /// is what keeps that honest: the run report names the blocks that were
    /// actually checked, so an ungraded one reads as an unanswered question
    /// and never as a pass.
    ///
    /// Since 2026-09-08 the accept blocks publish tables too, so "the blocks
    /// that published one" is most of the chip — and a run that wants the
    /// narrow claim says which blocks it means with
    /// [`set_strict_grade_blocks`](Self::set_strict_grade_blocks).
    pub fn set_strict_grade(&mut self, level: Option<RegGrade>) {
        self.strict_grade = level;
    }

    pub fn strict_grade(&self) -> Option<RegGrade> {
        self.strict_grade
    }

    /// Narrow the level to a named set of blocks.
    ///
    /// `None` — the default — means every block that publishes a table,
    /// which is the survey: *what does this boot read that we only
    /// modelled?* A named set is the gate: *this driver, on this block,
    /// crosses nothing below the level*, which is the claim G4-4 makes about
    /// `USB_DEVICE` and which stopped being expressible the day the accept
    /// blocks were graded.
    ///
    /// A name that matches no mapped block is kept rather than rejected, and
    /// shows up as its absence from
    /// [`blocks_in_strict_grade_scope`](Self::blocks_in_strict_grade_scope)
    /// — the caller can then say so, which is better than this layer
    /// guessing whether a block is missing on purpose.
    pub fn set_strict_grade_blocks(&mut self, blocks: Option<Vec<&'static str>>) {
        self.strict_grade_blocks = blocks;
    }

    /// The names of the blocks a `--strict-grade` run actually checks: those
    /// that publish a per-register grade table, intersected with
    /// [`set_strict_grade_blocks`](Self::set_strict_grade_blocks) when one
    /// was named.
    pub fn blocks_in_strict_grade_scope(&self) -> Vec<&'static str> {
        self.mmio
            .iter()
            .filter(|w| w.periph.reg_grade(0).is_some())
            .map(|w| w.periph.name())
            .filter(|name| self.block_in_strict_grade_scope(name))
            .collect()
    }

    fn block_in_strict_grade_scope(&self, name: &str) -> bool {
        match &self.strict_grade_blocks {
            None => true,
            Some(names) => names.iter().any(|n| *n == name),
        }
    }

    /// Mirror the hart's misaligned-access policy onto the bus.
    ///
    /// The hart's own `allow_unaligned` only *warns* when the bus contradicts
    /// it; the bus is what enforces. RAM regions were never alignment-checked
    /// (the C6 core does misaligned RAM accesses in hardware), so this flag
    /// changes only the MMIO path — see
    /// [`SocBus::require_mmio_alignment`].
    pub fn set_allow_unaligned(&mut self, allow: bool) {
        self.allow_unaligned = allow;
    }

    pub fn allow_unaligned(&self) -> bool {
        self.allow_unaligned
    }

    /// Declare how many watchpoint slots this machine's hart has, of the
    /// [`MAX_WATCHPOINT_SLOTS`] the array holds.
    ///
    /// A fresh [`SocBus`] has the maximum, so a machine that never calls this
    /// behaves exactly as it always did. Arming a slot at or above the count
    /// is ignored with a warning — exactly what arming one at or above the
    /// array's size always did.
    ///
    /// Called once at machine construction. Narrowing it after a slot above
    /// the new count is armed would leave that slot armed and unreachable, so
    /// this clears every slot from `count` up as it narrows — through the
    /// ordinary [`set_watchpoint`](Bus::set_watchpoint) path, so the armed
    /// bitmask and the single-store fast path are recomputed with it.
    pub fn set_watchpoint_slots(&mut self, count: usize) {
        assert!(
            count <= MAX_WATCHPOINT_SLOTS,
            "SocBus: {count} watchpoint slots asked for, {MAX_WATCHPOINT_SLOTS} is the array's size"
        );
        // Widen first, so the disarm below is not itself rejected by the
        // bounds guard when the count is being narrowed.
        self.slots = MAX_WATCHPOINT_SLOTS;
        for slot in count..MAX_WATCHPOINT_SLOTS {
            Bus::set_watchpoint(self, slot, None);
        }
        self.slots = count;
    }

    /// How many watchpoint slots this machine has. See
    /// [`set_watchpoint_slots`](Self::set_watchpoint_slots).
    pub fn watchpoint_slots(&self) -> usize {
        self.slots
    }

    /// Install the chip's interrupt matrix. See [`CpuIntMatrix`].
    pub fn set_matrix(&mut self, matrix: Box<dyn CpuIntMatrix>) {
        self.matrix = matrix;
    }

    /// Install what a memory access's address costs, or `None` for free.
    ///
    /// The chip crate owns the model; this crate holds it and calls it. See
    /// [`SocBus::memory_cost`] for why it is a trait object.
    pub fn set_memory_cost(&mut self, cost: Option<Box<dyn MemoryCost + Send>>) {
        self.memory_cost = cost;
        self.memory_cycles = 0;
    }

    /// Whether a memory-cost model is installed. For tests and for the
    /// machine's own reporting; the hot path never asks.
    pub fn has_memory_cost(&self) -> bool {
        self.memory_cost.is_some()
    }

    pub fn matrix(&self) -> &dyn CpuIntMatrix {
        &*self.matrix
    }

    pub fn matrix_mut(&mut self) -> &mut dyn CpuIntMatrix {
        &mut *self.matrix
    }

    /// Every CPU interrupt the chip's matrix currently asserts at the issuing
    /// hart, as a bitmask.
    ///
    /// The Xtensa form of [`Bus::pending_cpu_interrupt`]: the hart resolves it
    /// against `INTENABLE` and `PS.INTLEVEL`, CPU registers this bus cannot
    /// see. An RV32 hart keeps asking `pending_cpu_interrupt`, because its
    /// enables and priorities are the matrix's own MMIO registers. See
    /// [`CpuIntMatrix::asserted`].
    ///
    /// Same side-band contract as `pending_cpu_interrupt`: a bus that raises
    /// [`Bus::take_sideband`] must be prepared for this to be read before the
    /// next instruction retires.
    ///
    /// M2 P2 — this is the honest feed for `XtHart::set_external_mask` (M1
    /// R5); the wiring itself lands with the Xtensa machine in M3.
    pub fn pending_cpu_interrupt_mask(&self) -> u32 {
        self.matrix.asserted(self.hart, &self.irq)
    }

    // ---- machine plumbing --------------------------------------------

    /// Tell the bus what cycle it is. The machine sets this before stepping;
    /// peripherals read it through [`BusCx::now`].
    pub fn set_time(&mut self, now: Cycles) {
        self.now = now;
    }

    pub fn now(&self) -> Cycles {
        self.now
    }

    /// Tell the bus which PC is issuing accesses. This is what makes the
    /// trace and the spin detector worth having.
    ///
    /// The privileged hart calls it per instruction through
    /// [`Bus::set_issuing`]; the machine calls it directly only when it
    /// touches memory on the guest's behalf (a `--probe` read, a snapshot).
    pub fn set_pc(&mut self, pc: u32) {
        self.pc = pc;
    }

    pub fn pc(&self) -> u32 {
        self.pc
    }

    /// Which hart is issuing accesses (PD6). Always 0 until a second one
    /// exists.
    pub fn set_hart(&mut self, hart: usize) {
        self.hart = hart;
    }

    pub fn hart(&self) -> usize {
        self.hart
    }

    pub fn peripheral_count(&self) -> usize {
        self.mmio.len()
    }

    pub fn peripheral(&self, index: usize) -> Option<&dyn crate::periph::Peripheral> {
        self.mmio.get(index).map(|r| &*r.periph)
    }

    pub fn peripheral_mut(&mut self, index: usize) -> Option<&mut BoxedPeripheral> {
        self.mmio.get_mut(index).map(|r| &mut r.periph)
    }

    /// Find a peripheral's index by its instance name.
    pub fn peripheral_index(&self, name: &str) -> Option<usize> {
        self.mmio.iter().position(|r| r.periph.name() == name)
    }

    /// Drive one peripheral's own API from the machine, with a full
    /// [`BusCx`] — the seam a host-side control channel reaches a block
    /// through.
    ///
    /// `None` when the index is out of range or the block there is not a
    /// `T` (it declined the downcast: see [`Peripheral::as_any_mut`]). The
    /// closure sees the bus's current time and pc, so whatever it schedules,
    /// traces or requests is stamped with the guest cycle the machine drained
    /// the command at — never a wall-clock one.
    ///
    /// [`Peripheral::as_any_mut`]: crate::periph::Peripheral::as_any_mut
    pub fn with_peripheral<T: core::any::Any, R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut T, &mut BusCx<'_>) -> R,
    ) -> Option<R> {
        let periph = self
            .mmio
            .get_mut(index)?
            .periph
            .as_any_mut()?
            .downcast_mut::<T>()?;
        let mut cx = BusCx {
            now: self.now,
            pc: self.pc,
            hart: self.hart,
            sched: &mut self.sched,
            irq: &mut self.irq,
            trace: &mut self.trace,
            host: &mut self.host,
            matrix: &mut *self.matrix,
            request: &mut self.request,
            yield_now: &mut self.yield_now,
            pins: &mut self.pins,
        };
        Some(f(periph, &mut cx))
    }

    pub fn regions(&self) -> &[RamRegion] {
        &self.regions
    }

    pub fn region_mut(&mut self, name: &str) -> Option<&mut RamRegion> {
        self.regions.iter_mut().find(|r| r.name == name)
    }

    // ---- the guest arena ------------------------------------------------

    /// `region`'s bytes, as a view into the guest arena.
    ///
    /// Panics if `region` is not one of this bus's regions — the arena is
    /// where the bytes are, so a region from another bus has none here.
    pub fn region_bytes(&self, region: &RamRegion) -> &[u8] {
        let off = self.arena_offset(region.base).expect("region on this bus");
        &self.arena[off..off + region.len as usize]
    }

    /// The whole arena: guest address `a` is at `a - guest_arena_base()`.
    ///
    /// This is what a translated core is handed. Everything outside a region
    /// is zero and stays zero — the bus never serves it.
    pub fn guest_arena(&self) -> &[u8] {
        &self.arena
    }

    /// The whole arena, writable.
    ///
    /// Two callers, both outside the guest's own execution:
    ///
    /// - a **translated core**, which runs guest stores against these bytes
    ///   directly on the pages the [permission table](Self::permission_table)
    ///   marks as plain RAM — that is the point of the arena;
    /// - the same core's own **tables**. A translated module needs its
    ///   permission table and its host exchange area to be at fixed offsets in
    ///   the *same* linear memory as guest RAM, and the arena is flat across
    ///   the gaps between regions — ~7.7 MiB of them on the C6, which the bus
    ///   never serves and never will. Putting the tables in a gap keeps the
    ///   arena one allocation with no copy, and a guest access to a gap page
    ///   goes out through the bus and faults there exactly as it always has,
    ///   because a gap has no region and so has permission zero.
    ///
    /// It does not fire watchpoints, does not charge and is not a store: this
    /// is the host reaching into memory, not the guest.
    pub fn guest_arena_mut(&mut self) -> &mut [u8] {
        &mut self.arena
    }

    /// The guest address of `guest_arena()[0]`.
    pub fn guest_arena_base(&self) -> u32 {
        self.arena_base
    }

    /// The largest `[base, base + len)` inside the arena that **no region
    /// covers**, or `None` when the arena is fully covered.
    ///
    /// A translated core's tables go here — see [`Self::guest_arena_mut`].
    /// Reported as a guest address so the caller can check it against the
    /// permission table it builds.
    pub fn largest_arena_gap(&self) -> Option<(u32, u32)> {
        let lo = u64::from(self.arena_base);
        let hi = lo + self.arena.len() as u64;
        let mut best: Option<(u32, u32)> = None;
        let mut at = lo;
        let consider = |from: u64, to: u64, best: &mut Option<(u32, u32)>| {
            if to > from && best.is_none_or(|(_, len)| u64::from(len) < to - from) {
                *best = Some((from as u32, (to - from) as u32));
            }
        };
        // `regions` is base-sorted and non-overlapping, so one pass with a
        // watermark finds every gap, and the tail after the last region is one
        // more.
        for r in &self.regions {
            consider(at, u64::from(r.base), &mut best);
            at = at.max(u64::from(r.end()));
        }
        consider(at, hi, &mut best);
        best
    }

    /// The arena offset of a guest address, or `None` when it is outside the
    /// arena's span.
    #[inline(always)]
    fn arena_offset(&self, address: u32) -> Option<usize> {
        let off = u64::from(address).checked_sub(u64::from(self.arena_base))?;
        (off < self.arena.len() as u64).then_some(off as usize)
    }

    /// `(base, len, writable)` per region, base-sorted.
    pub fn region_spans(&self) -> Vec<(u32, u32, bool)> {
        self.regions
            .iter()
            .map(|r| (r.base, r.len(), r.writable))
            .collect()
    }

    /// One byte per [`PERMISSION_PAGE_LEN`] page of the arena, saying what a
    /// translated core may do on that page without going through the bus:
    /// [`PERM_NONE`], [`PERM_READ`] or [`PERM_READ_WRITE`].
    ///
    /// A page is plain RAM only when the regions cover **all** of it with a
    /// single writability. A page that is partly covered, or that straddles a
    /// read-only and a writable region, is [`PERM_NONE`]: the alternative is
    /// a per-access region lookup, which is the cost the flat arena exists to
    /// delete.
    ///
    /// Three things take the whole table to [`PERM_NONE`], because each one
    /// makes an access the bus never sees observably different from one it
    /// does:
    ///
    /// - a **memory-cost model** ([`SocBus::set_memory_cost`]) charges cycles
    ///   per access, and an inline access charges none. This is the same
    ///   condition [`Bus::fetch_is_pure`] already refuses on, for the same
    ///   reason.
    /// - **`--strict-bus`** ([`SocBus::set_strict`]) watches guest stores for
    ///   the missing-fence checker, and an inline store is not watched.
    /// - an **MMIO window** overlapping the page, which would make an inline
    ///   access read RAM where the bus would have reached a peripheral. The
    ///   C6's windows do not overlap its regions, so this costs nothing
    ///   there; it is checked so a chip whose map does overlap cannot get a
    ///   wrong answer.
    ///
    /// Computed on demand rather than cached: a run translates twice (the
    /// image at boot and the shader at its `fence.i`), so a 16 KiB table is
    /// far cheaper to rebuild than to keep correct.
    pub fn permission_table(&self) -> Vec<u8> {
        let pages = self.arena.len().div_ceil(PERMISSION_PAGE_LEN as usize);
        let mut table = alloc::vec![PERM_NONE; pages];
        if self.memory_cost.is_some() || self.strict {
            return table;
        }
        for (page, slot) in table.iter_mut().enumerate() {
            let lo = u64::from(self.arena_base) + (page as u64) * u64::from(PERMISSION_PAGE_LEN);
            let hi = (lo + u64::from(PERMISSION_PAGE_LEN))
                .min(u64::from(self.arena_base) + self.arena.len() as u64);
            let covering = self
                .regions
                .iter()
                .find(|r| u64::from(r.base) <= lo && u64::from(r.end()) >= hi && r.len() != 0);
            let Some(r) = covering else { continue };
            let mmio_overlap = self
                .mmio_windows
                .iter()
                .any(|&(base, len)| lo < u64::from(base) + u64::from(len) && u64::from(base) < hi);
            if mmio_overlap {
                continue;
            }
            *slot = if r.writable {
                PERM_READ_WRITE
            } else {
                PERM_READ
            };
        }
        table
    }

    /// Place bytes into RAM from the host side — ELF segments, a ROM image,
    /// the bootloader's leftovers. Ignores `writable` (this is not the guest
    /// storing) and fires no watchpoints.
    pub fn load_image(&mut self, address: u32, bytes: &[u8]) -> Result<(), MemoryError> {
        let Some(i) = self.region_index(address) else {
            return Err(MemoryError::InvalidAccess {
                address,
                size: bytes.len(),
                kind: MemoryAccessKind::Write,
            });
        };
        if !self.regions[i].span_fits(address, bytes.len() as u32) {
            return Err(MemoryError::InvalidAccess {
                address,
                size: bytes.len(),
                kind: MemoryAccessKind::Write,
            });
        }
        let off = (address - self.arena_base) as usize;
        self.arena[off..off + bytes.len()].copy_from_slice(bytes);
        if self.regions[i].exec {
            // The block cache's emulator-side funnel. Every host-side write
            // of guest code comes through here — a flash-cache MMU refill
            // (`cache::fill`), a ROM-hook `ebreak` patch (`rom::install_at`
            // and `uninstall_all`), an ELF segment at build time, a snapshot
            // restore's region reload — and none of them is followed by a
            // guest `fence.i`, because none of them is the guest. The machine
            // drains this at the slice boundary and invalidates.
            self.code_writes
                .push((address, address + bytes.len() as u32));
        }
        Ok(())
    }

    /// Windows of guest code written from the host side since the last call.
    ///
    /// The machine drains this once per slice, beside `refill_cache`, and
    /// hands each window to the hart's block cache. Empty on almost every
    /// slice — P1 counted 98 cache refills across a whole render run.
    pub fn take_code_writes(&mut self) -> Vec<(u32, u32)> {
        core::mem::take(&mut self.code_writes)
    }

    /// True when nothing is waiting to be invalidated — the one-`bool` check
    /// the slice boundary makes before doing anything.
    #[inline]
    pub fn code_writes_pending(&self) -> bool {
        !self.code_writes.is_empty()
    }

    // ---- the `--strict-bus` missing-fence checker ----------------------

    /// How many times `--strict-bus` has caught a code page being executed
    /// after the guest wrote it with no `fence.i` in between.
    ///
    /// Zero on the product's own firmware is the claim the fence contract
    /// makes; a `--boot rom-up` run is expected to report a handful, because
    /// the mask ROM and the ESP-IDF second-stage bootloader copy code into
    /// RAM and jump into it and we own neither (M5 MD13). That is what makes
    /// this a working checker rather than an untested one.
    #[inline]
    pub fn missing_fence_reports(&self) -> u64 {
        self.missing_fence_reports
    }

    /// 4 KiB pages the guest has fetched instructions from, under
    /// `--strict-bus`. P1 measured 428 on a whole render run.
    #[inline]
    pub fn code_pages_seen(&self) -> usize {
        self.code_pages.len()
    }

    /// A guest store landed in region `i` at byte offset `off`. Under
    /// `--strict-bus`, mark the words it **changed** when they are on a page
    /// the guest has executed from.
    #[inline(never)]
    fn note_guest_code_write(&mut self, off: usize, address: u32, len: u32, value: u32) {
        let page = address & !(CODE_PAGE_LEN - 1);
        if !self.code_pages.contains(&page) {
            return;
        }
        let old = &self.arena[off..off + len as usize];
        if old == &value.to_le_bytes()[..len as usize] {
            // A store that writes what was already there publishes nothing
            // and owes no fence. P1 measured 36 % of the render loop's stores
            // onto executed pages doing exactly this.
            return;
        }
        // Every word the store touches, so a byte or halfword store still
        // names the instruction it lands in.
        let mut at = address & !3;
        while at < address + len {
            self.unpublished_code_words.insert(at);
            at += 4;
        }
    }

    /// A guest instruction at `pc` is about to run. Under `--strict-bus`,
    /// remember its page, and report it when the guest changed the bytes it
    /// is made of and never published the change.
    #[inline(never)]
    fn check_code_word(&mut self, pc: u32, fetching: bool) {
        if fetching {
            self.code_pages.insert(pc & !(CODE_PAGE_LEN - 1));
        }
        if self.unpublished_code_words.is_empty() {
            return;
        }
        // An instruction is 2 or 4 bytes and may straddle a word boundary.
        for word in [pc & !3, (pc + 2) & !3] {
            if self.unpublished_code_words.remove(&word) {
                self.missing_fence_reports += 1;
                log::error!(
                    "strict-bus: the instruction at {pc:#010x} was written by the guest and then \
                     executed with no `fence.i` between. That is a firmware bug: instructions \
                     published without a fence are not guaranteed to be visible to the fetch \
                     path on real silicon, and the emulator\u{27}s block cache will serve the \
                     old ones"
                );
            }
        }
    }

    /// Dispatch every event due at or before `now` to the peripheral that
    /// scheduled it.
    pub fn run_due_events(&mut self, now: Cycles) {
        self.now = now;
        let mut dispatched = 0u32;
        while let Some(id) = self.sched.pop_due(now) {
            dispatched += 1;
            if dispatched > MAX_EVENTS_PER_TICK {
                log::error!(
                    "SocBus: {MAX_EVENTS_PER_TICK} events dispatched at cycle {now} without \
                     time advancing — a peripheral is rescheduling itself at `now`. Dropping \
                     the rest of this tick."
                );
                break;
            }
            let index = event_peripheral(id);
            let Some(range) = self.mmio.get_mut(index) else {
                log::warn!(
                    "SocBus: event {:#010x} names peripheral {index}, which does not exist",
                    id.0
                );
                continue;
            };
            let mut cx = BusCx {
                now,
                pc: self.pc,
                hart: self.hart,
                sched: &mut self.sched,
                irq: &mut self.irq,
                trace: &mut self.trace,
                host: &mut self.host,
                matrix: &mut *self.matrix,
                request: &mut self.request,
                yield_now: &mut self.yield_now,
                pins: &mut self.pins,
            };
            range.periph.on_event(id, &mut cx);
        }
    }

    /// Give every peripheral its [`Peripheral::started`] call, in
    /// registration order, at the current bus time. The machine calls this
    /// once, after the ROM and the app are placed and before the first
    /// slice; see the trait method for what it is for.
    ///
    /// [`Peripheral::started`]: crate::periph::Peripheral::started
    pub fn start_peripherals(&mut self) {
        for range in &mut self.mmio {
            let mut cx = BusCx {
                now: self.now,
                pc: self.pc,
                hart: self.hart,
                sched: &mut self.sched,
                irq: &mut self.irq,
                trace: &mut self.trace,
                host: &mut self.host,
                matrix: &mut *self.matrix,
                request: &mut self.request,
                yield_now: &mut self.yield_now,
                pins: &mut self.pins,
            };
            range.periph.started(&mut cx);
        }
    }

    /// The request a peripheral left for the machine, if any, clearing it.
    pub fn take_request(&mut self) -> Option<MachineRequest> {
        self.request.take()
    }

    // ---- diagnostics --------------------------------------------------

    /// Distinct `(pc, address)` sites that hit nothing, capped at
    /// [`UNMAPPED_SITES_CAP`].
    pub fn unmapped_sites(&self) -> usize {
        self.unmapped_sites.len()
    }

    pub fn unmapped_reads(&self) -> u64 {
        self.unmapped_reads
    }

    pub fn unmapped_writes(&self) -> u64 {
        self.unmapped_writes
    }

    /// The first access strict mode refused, if any. The machine reports it:
    /// by the time a slice ends the hart has already taken the architectural
    /// access fault and moved the PC.
    pub fn first_strict_violation(&self) -> Option<StrictViolation> {
        self.first_strict_violation
    }

    pub fn clear_strict_violation(&mut self) {
        self.first_strict_violation = None;
    }

    // ---- snapshot -----------------------------------------------------

    /// Every RAM region's bytes, in the bus's own (base-sorted) order.
    pub fn save_regions(&self) -> Vec<Vec<u8>> {
        self.regions
            .iter()
            .map(|r| self.region_bytes(r).to_vec())
            .collect()
    }

    /// Put region bytes back. A length mismatch is a snapshot taken from a
    /// differently built machine and is refused loudly rather than half
    /// applied.
    pub fn restore_regions(&mut self, data: &[Vec<u8>]) {
        assert_eq!(
            data.len(),
            self.regions.len(),
            "SocBus::restore_regions: snapshot has {} regions, this bus has {}",
            data.len(),
            self.regions.len()
        );
        for (i, bytes) in data.iter().enumerate() {
            let (name, base, len) = {
                let r = &self.regions[i];
                (r.name, r.base, r.len as usize)
            };
            assert_eq!(
                bytes.len(),
                len,
                "SocBus::restore_regions: region `{name}` is {len} bytes, snapshot has {}",
                bytes.len()
            );
            let off = (base - self.arena_base) as usize;
            self.arena[off..off + len].copy_from_slice(bytes);
        }
        self.last_regions = [0, 0];
        self.last_fetch_region = 0;
    }

    /// Each peripheral's own blob, named, in registration order.
    pub fn save_peripherals(&self) -> Vec<(String, Vec<u8>)> {
        self.mmio
            .iter()
            .map(|r| (r.periph.name().to_string(), r.periph.save_state()))
            .collect()
    }

    pub fn restore_peripherals(&mut self, states: &[(String, Vec<u8>)]) {
        assert_eq!(
            states.len(),
            self.mmio.len(),
            "SocBus::restore_peripherals: snapshot has {} peripherals, this bus has {}",
            states.len(),
            self.mmio.len()
        );
        for (range, (name, bytes)) in self.mmio.iter_mut().zip(states) {
            assert_eq!(
                range.periph.name(),
                name,
                "SocBus::restore_peripherals: peripheral {} is `{}`, snapshot has `{}` — \
                 registration order is part of the snapshot (event ids pack the index)",
                range.base,
                range.periph.name(),
                name
            );
            range.periph.load_state(bytes);
        }
    }

    /// Everything else the bus carries. See [`BusScalars`].
    pub fn save_scalars(&self) -> BusScalars {
        BusScalars {
            now: self.now,
            pc: self.pc,
            hart: self.hart,
            sideband: self.sideband,
            yield_now: self.yield_now,
            irq: self.irq.clone(),
            unmapped_sites: self.unmapped_sites.clone(),
            unmapped_reads: self.unmapped_reads,
            unmapped_writes: self.unmapped_writes,
            first_strict_violation: self.first_strict_violation,
            request: self.request,
            pins: self.pins.clone(),
        }
    }

    pub fn restore_scalars(&mut self, s: &BusScalars) {
        self.now = s.now;
        self.pc = s.pc;
        self.hart = s.hart;
        self.sideband = s.sideband;
        self.yield_now = s.yield_now;
        self.irq = s.irq.clone();
        self.unmapped_sites = s.unmapped_sites.clone();
        self.unmapped_reads = s.unmapped_reads;
        self.unmapped_writes = s.unmapped_writes;
        self.first_strict_violation = s.first_strict_violation;
        self.request = s.request;
        self.pins = s.pins.clone();
    }

    /// `true` if `address` falls in a declared MMIO window.
    pub fn in_mmio_window(&self, address: u32) -> bool {
        self.mmio_windows
            .iter()
            .any(|(b, l)| address >= *b && address < b.wrapping_add(*l))
    }

    // ---- decode -------------------------------------------------------

    #[inline(always)]
    fn region_index(&mut self, address: u32) -> Option<usize> {
        // The two-entry "last hit" cache: one or two compares for the
        // common case, instead of thrashing a single slot between the two
        // working sets a run alternates over.
        if let Some(r) = self.regions.get(self.last_regions[0])
            && r.contains(address)
        {
            return Some(self.last_regions[0]);
        }
        if let Some(r) = self.regions.get(self.last_regions[1])
            && r.contains(address)
        {
            self.last_regions.swap(0, 1);
            return Some(self.last_regions[0]);
        }
        let i = self.region_index_slow(address)?;
        self.last_regions[1] = self.last_regions[0];
        self.last_regions[0] = i;
        Some(i)
    }

    /// [`region_index`](Self::region_index) for instruction fetch, with its
    /// own last-hit cache.
    #[inline(always)]
    fn fetch_region_index(&self, address: u32) -> Option<usize> {
        if let Some(r) = self.regions.get(self.last_fetch_region)
            && r.contains(address)
        {
            return Some(self.last_fetch_region);
        }
        self.region_index_slow(address)
    }

    #[inline(never)]
    fn region_index_slow(&self, address: u32) -> Option<usize> {
        let i = self.regions.partition_point(|r| r.base <= address);
        let i = i.checked_sub(1)?;
        self.regions[i].contains(address).then_some(i)
    }

    #[inline(always)]
    fn mmio_index(&mut self, address: u32) -> Option<usize> {
        // The last-hit cache: one subtract-compare, checked before the
        // sorted-by-base binary search.
        if let Some((i, base, len)) = self.last_mmio
            && address.wrapping_sub(base) < len
        {
            return Some(i);
        }
        self.mmio_index_slow(address)
    }

    #[inline(never)]
    fn mmio_index_slow(&mut self, address: u32) -> Option<usize> {
        let k = self
            .mmio_by_base
            .partition_point(|&i| self.mmio[i].base <= address);
        let i = self.mmio_by_base[k.checked_sub(1)?];
        let r = &self.mmio[i];
        if address.wrapping_sub(r.base) < r.len {
            self.last_mmio = Some((i, r.base, r.len));
            Some(i)
        } else {
            None
        }
    }

    /// MMIO accesses are register-aligned unless
    /// [`set_allow_unaligned`](Self::set_allow_unaligned) says otherwise —
    /// and even then, only when the access stays inside **one** 32-bit
    /// register.
    ///
    /// A byte at `+0x001` or a half-word at `+0x006` reaches its register
    /// through the peripheral's byte-lane path exactly; a word at `+0x002`
    /// would have to be split into two register accesses, running two
    /// registers' side effects for one instruction. That is a different
    /// machine, so the bus refuses it whatever the flag says.
    fn require_mmio_alignment(&self, address: u32, width: Width) -> Result<(), MemoryError> {
        let alignment = width.bytes();
        if address % alignment == 0 {
            return Ok(());
        }
        if self.allow_unaligned && (address % 4) + alignment <= 4 {
            return Ok(());
        }
        Err(MemoryError::Unaligned {
            address,
            alignment: alignment as usize,
        })
    }

    // ---- watchpoints --------------------------------------------------

    /// One test for the common case: no armed slot wants this access kind.
    #[inline(always)]
    fn check_watchpoints(
        &self,
        address: u32,
        len: u32,
        kind: MemoryAccessKind,
    ) -> Result<(), MemoryError> {
        let mask = self.armed_for[kind_index(kind)];
        if mask == 0 {
            return Ok(());
        }
        self.check_watchpoints_slow(address, len, kind, mask)
    }

    /// The slot walk, in slot order, over the slots `mask` names — the same
    /// slots the four-way filter used to select one at a time.
    #[inline(never)]
    fn check_watchpoints_slow(
        &self,
        address: u32,
        len: u32,
        kind: MemoryAccessKind,
        mask: u32,
    ) -> Result<(), MemoryError> {
        for slot in 0..self.slots {
            if mask & (1 << slot) == 0 {
                continue;
            }
            let Some(wp) = self.watchpoints[slot] else {
                continue;
            };
            if watchpoint_overlaps(&wp, address, len) {
                return Err(MemoryError::Watchpoint {
                    address,
                    kind,
                    slot: slot as u8,
                });
            }
        }
        Ok(())
    }

    /// [`check_watchpoints`](Self::check_watchpoints) specialised for
    /// stores: when [`single_store_watch`](Self::single_store_watch) is
    /// armed — the common case, esp-hal's stack guard — this is one range
    /// compare instead of the slot walk. Zero or more-than-one store
    /// watchpoints fall back to the general path, which still gives the
    /// zero case its one-test short-circuit.
    #[inline(always)]
    fn check_store_watchpoint(&self, address: u32, len: u32) -> Result<(), MemoryError> {
        if let Some((lo, hi, slot)) = self.single_store_watch {
            let a0 = u64::from(address);
            let a1 = a0 + u64::from(len);
            return if a0 < hi && lo < a1 {
                Err(MemoryError::Watchpoint {
                    address,
                    kind: MemoryAccessKind::Write,
                    slot,
                })
            } else {
                Ok(())
            };
        }
        self.check_watchpoints(address, len, MemoryAccessKind::Write)
    }

    /// Recompute [`single_store_watch`](Self::single_store_watch) from
    /// `armed_for`/`watchpoints`. Called whenever either changes
    /// ([`set_watchpoint`](Bus::set_watchpoint)); `None` when zero or more
    /// than one store-kind slot is armed, so [`check_store_watchpoint`]
    /// falls back to the general walk in both of those cases.
    fn recompute_single_store_watch(&mut self) {
        let mask = self.armed_for[kind_index(MemoryAccessKind::Write)];
        self.single_store_watch = (mask.count_ones() == 1)
            .then(|| mask.trailing_zeros() as usize)
            .and_then(|slot| self.watchpoints[slot].map(|wp| (slot, wp)))
            .map(|(slot, wp)| {
                let (lo, hi) = watchpoint_span(&wp);
                (lo, hi, slot as u8)
            });
    }

    // ---- the access paths ---------------------------------------------

    #[inline]
    fn read(&mut self, address: u32, width: Width) -> Result<u32, MemoryError> {
        let len = width.bytes();
        self.check_watchpoints(address, len, MemoryAccessKind::Read)?;
        if let Some(cost) = self.memory_cost.as_mut() {
            self.memory_cycles += cost.load(address, len as u8);
        }

        if let Some(i) = self.region_index(address) {
            // `region_index` guarantees `address` is inside the region and
            // updated its own cache, so all that is left is "does the *span*
            // fit". The arena is flat across region boundaries, so an access
            // running off the end of a region would otherwise read the next
            // one's bytes instead of faulting — this compare is what the
            // region-owned `Vec`'s bounds check used to do.
            // `region_index` guarantees `base <= address < end`, so the
            // wrapping subtract lands in `1..=region.len()` and one u32
            // compare is exact — the same shape `fetch_instruction` uses, and
            // no widening to u64.
            let room = self.regions[i].end().wrapping_sub(address);
            let off = (address - self.arena_base) as usize;
            let data = &self.arena;
            let v = if room < len {
                None
            } else {
                match width {
                    Width::Word => data
                        .get(off..off + 4)
                        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                    Width::Half => data
                        .get(off..off + 2)
                        .map(|b| u32::from(u16::from_le_bytes([b[0], b[1]]))),
                    Width::Byte => data.get(off).map(|b| u32::from(*b)),
                }
            };
            return v.ok_or(MemoryError::InvalidAccess {
                address,
                size: len as usize,
                kind: MemoryAccessKind::Read,
            });
        }

        self.read_mmio(address, width)
    }

    /// The MMIO and unmapped halves of [`read`](Self::read), out of line so
    /// the RAM path stays small enough to inline into the executors.
    #[inline(never)]
    fn read_mmio(&mut self, address: u32, width: Width) -> Result<u32, MemoryError> {
        if let Some(i) = self.mmio_index(address) {
            self.require_mmio_alignment(address, width)?;
            let base = self.mmio[i].base;
            let off = address - base;
            // Names are for the trace line only; `reg_name` is a table walk
            // and this runs on every MMIO access, traced or not.
            let traced = self.trace.is_enabled();
            let (block, name) = if traced {
                let p = &self.mmio[i].periph;
                (p.name(), p.reg_name(off))
            } else {
                ("", None)
            };
            self.check_grade(i, off, address, width, Access::Read)?;
            let (now, pc, hart) = (self.now, self.pc, self.hart);
            let value = {
                let range = &mut self.mmio[i];
                let mut cx = BusCx {
                    now,
                    pc,
                    hart,
                    sched: &mut self.sched,
                    irq: &mut self.irq,
                    trace: &mut self.trace,
                    host: &mut self.host,
                    matrix: &mut *self.matrix,
                    request: &mut self.request,
                    yield_now: &mut self.yield_now,
                    pins: &mut self.pins,
                };
                range.periph.read(off, width, &mut cx)
            };
            if self.trace.is_enabled() {
                self.trace.mmio(
                    now,
                    pc,
                    &MmioEvent {
                        access: Access::Read,
                        width,
                        block,
                        off,
                        name,
                        value,
                        flags: "",
                    },
                );
            }
            return Ok(value);
        }

        self.unmapped(address, width, Access::Read, 0)?;
        Ok(0)
    }

    #[inline]
    fn write(&mut self, address: u32, width: Width, value: u32) -> Result<(), MemoryError> {
        let len = width.bytes();
        self.check_store_watchpoint(address, len)?;
        if let Some(cost) = self.memory_cost.as_mut() {
            self.memory_cycles += cost.store(address, len as u8);
        }

        if let Some(i) = self.region_index(address) {
            let fault = MemoryError::InvalidAccess {
                address,
                size: len as usize,
                kind: MemoryAccessKind::Write,
            };
            if !self.regions[i].writable {
                return Err(fault);
            }
            // See `read`: the arena is flat, so the region's own end is what
            // bounds the access, and one u32 compare says so exactly.
            if self.regions[i].end().wrapping_sub(address) < len {
                return Err(fault);
            }
            let off = (address - self.arena_base) as usize;
            if self.strict {
                self.note_guest_code_write(off, address, len, value);
            }
            let data = &mut self.arena;
            let stored = match width {
                Width::Word => data
                    .get_mut(off..off + 4)
                    .map(|b| b.copy_from_slice(&value.to_le_bytes())),
                Width::Half => data
                    .get_mut(off..off + 2)
                    .map(|b| b.copy_from_slice(&(value as u16).to_le_bytes())),
                Width::Byte => data.get_mut(off).map(|b| *b = value as u8),
            };
            return stored.ok_or(fault);
        }

        self.write_mmio(address, width, value)
    }

    /// The MMIO and unmapped halves of [`write`](Self::write); see
    /// [`read_mmio`](Self::read_mmio).
    #[inline(never)]
    fn write_mmio(&mut self, address: u32, width: Width, value: u32) -> Result<(), MemoryError> {
        if let Some(i) = self.mmio_index(address) {
            self.require_mmio_alignment(address, width)?;
            let base = self.mmio[i].base;
            let off = address - base;
            let traced = self.trace.is_enabled();
            let (block, name) = if traced {
                let p = &self.mmio[i].periph;
                (p.name(), p.reg_name(off))
            } else {
                ("", None)
            };
            self.check_grade(i, off, address, width, Access::Write)?;
            let (now, pc, hart) = (self.now, self.pc, self.hart);
            {
                let range = &mut self.mmio[i];
                let mut cx = BusCx {
                    now,
                    pc,
                    hart,
                    sched: &mut self.sched,
                    irq: &mut self.irq,
                    trace: &mut self.trace,
                    host: &mut self.host,
                    matrix: &mut *self.matrix,
                    request: &mut self.request,
                    yield_now: &mut self.yield_now,
                    pins: &mut self.pins,
                };
                range.periph.write(off, width, value, &mut cx);
            }
            // An MMIO store is the only thing that can have changed the
            // interrupt state under the stepper's feet.
            self.sideband = true;
            if self.trace.is_enabled() {
                self.trace.mmio(
                    now,
                    pc,
                    &MmioEvent {
                        access: Access::Write,
                        width,
                        block,
                        off,
                        name,
                        value,
                        flags: "",
                    },
                );
            }
            return Ok(());
        }

        self.unmapped(address, width, Access::Write, value)
    }

    /// The strict-grade policy ([`set_strict_grade`](Self::set_strict_grade)):
    /// a register graded below the level is refused before the peripheral
    /// sees the access, recorded as the first violation if none is yet, and
    /// noted in the trace with its name and grade.
    fn check_grade(
        &mut self,
        index: usize,
        off: u32,
        address: u32,
        width: Width,
        access: Access,
    ) -> Result<(), MemoryError> {
        let Some(level) = self.strict_grade else {
            return Ok(());
        };
        // A block that publishes no grade table is out of scope, not
        // `Modeled`: see `Peripheral::reg_grade`. So is one the run did not
        // name, when it named any.
        if !self.block_in_strict_grade_scope(self.mmio[index].periph.name()) {
            return Ok(());
        }
        let Some(grade) = self.mmio[index].periph.reg_grade(off) else {
            return Ok(());
        };
        if grade >= level {
            return Ok(());
        }
        if self.first_strict_violation.is_none() {
            self.first_strict_violation = Some(StrictViolation {
                cycle: self.now,
                pc: self.pc,
                address,
                width,
                access,
                in_mmio_window: true,
                grade: Some(grade),
            });
            let p = &self.mmio[index].periph;
            let line = alloc::format!(
                "cyc={} pc=0x{:08x} STRICT-GRADE {} of {}+0x{off:03x} {}: graded {grade}, the \
                 run trusts {level} and above",
                self.now,
                self.pc,
                match access {
                    Access::Read => "read",
                    Access::Write => "write",
                },
                p.name(),
                p.reg_name(off).unwrap_or("?"),
            );
            self.trace.note(&line);
            log::warn!("{line}");
        }
        Err(MemoryError::InvalidAccess {
            address,
            size: width.bytes() as usize,
            kind: match access {
                Access::Read => MemoryAccessKind::Read,
                Access::Write => MemoryAccessKind::Write,
            },
        })
    }

    /// The unmapped-access policy: count always, log the first time this
    /// exact `(pc, address)` appears, fault in strict mode.
    fn unmapped(
        &mut self,
        address: u32,
        width: Width,
        access: Access,
        value: u32,
    ) -> Result<(), MemoryError> {
        match access {
            Access::Read => self.unmapped_reads += 1,
            Access::Write => {
                self.unmapped_writes += 1;
                self.trace.note_write_anywhere();
            }
        }

        if self.strict && self.first_strict_violation.is_none() {
            self.first_strict_violation = Some(StrictViolation {
                cycle: self.now,
                pc: self.pc,
                address,
                width,
                access,
                in_mmio_window: self.in_mmio_window(address),
                grade: None,
            });
        }

        let site = (self.pc, address);
        let first_time = if self.unmapped_sites.len() < UNMAPPED_SITES_CAP {
            self.unmapped_sites.insert(site)
        } else {
            !self.unmapped_sites.contains(&site)
        };

        if first_time {
            let window = if self.in_mmio_window(address) {
                " (inside a declared MMIO window: an unmodelled block)"
            } else {
                ""
            };
            log::warn!(
                "UNMAPPED {}{} at 0x{address:08x} from pc=0x{:08x}{window}",
                match access {
                    Access::Read => "read",
                    Access::Write => "write",
                },
                width.bytes(),
                self.pc,
            );
            self.trace
                .unmapped(self.now, self.pc, access, width, address, value);
        }

        if self.strict {
            return Err(MemoryError::InvalidAccess {
                address,
                size: width.bytes() as usize,
                kind: match access {
                    Access::Read => MemoryAccessKind::Read,
                    Access::Write => MemoryAccessKind::Write,
                },
            });
        }
        Ok(())
    }
}

/// Index into [`SocBus::armed_for`] for an access kind.
#[inline(always)]
const fn kind_index(kind: MemoryAccessKind) -> usize {
    match kind {
        MemoryAccessKind::Read => 0,
        MemoryAccessKind::Write => 1,
        MemoryAccessKind::InstructionFetch => 2,
    }
}

/// Does the watchpoint cover any byte of `[address, address + len)`?
///
/// NAPOT is the RISC-V debug spec's `tdata2` encoding: the trailing ones of
/// the written value, plus the first zero above them, are the bits the
/// compare ignores — so `mask = value ^ (value + 1)` and the region is
/// `value & !mask` of size `mask + 1`.
fn watchpoint_overlaps(wp: &Watchpoint, address: u32, len: u32) -> bool {
    let (b0, b1) = watchpoint_span(wp);
    let (a0, a1) = (u64::from(address), u64::from(address) + u64::from(len));
    a0 < b1 && b0 < a1
}

/// The `[lo, hi)` byte range a watchpoint covers — the NAPOT decode shared by
/// [`watchpoint_overlaps`] and [`SocBus::single_store_watch`]'s precompute.
fn watchpoint_span(wp: &Watchpoint) -> (u64, u64) {
    let (base, size) = if wp.napot {
        let mask = wp.address ^ wp.address.wrapping_add(1);
        (wp.address & !mask, u64::from(mask) + 1)
    } else {
        (wp.address, 1)
    };
    (u64::from(base), u64::from(base) + size)
}

impl Bus for SocBus {
    #[inline]
    fn fetch_instruction(&mut self, address: u32) -> Result<u32, MemoryError> {
        if address % 2 != 0 {
            return Err(MemoryError::Unaligned {
                address,
                alignment: 2,
            });
        }
        self.check_watchpoints(address, 2, MemoryAccessKind::InstructionFetch)?;

        let fault = || MemoryError::InvalidAccess {
            address,
            size: 2,
            kind: MemoryAccessKind::InstructionFetch,
        };

        // Fetch never routes to MMIO: a jump into peripheral space is a
        // wild branch, and returning a register's value as an instruction
        // would turn it into a puzzle.
        let i = self.fetch_region_index(address).ok_or_else(fault)?;
        if !self.regions[i].exec {
            return Err(fault());
        }
        self.last_fetch_region = i;
        if self.strict {
            self.check_code_word(address, true);
        }
        if let Some(cost) = self.memory_cost.as_mut() {
            self.memory_cycles += cost.fetch(address);
        }
        // The arena is flat across region boundaries, so the region's own end
        // — not the arena's — is what says how much of this fetch is real.
        let room = self.regions[i].end().wrapping_sub(address);
        let off = (address - self.arena_base) as usize;
        let d = &self.arena;
        if room >= 4
            && let Some(b) = d.get(off..off + 4)
        {
            return Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        }
        // Two bytes are enough: a compressed instruction at the very end of
        // a region is legal, and the decoder asks for no more than it needs.
        match (room >= 2).then(|| d.get(off..off + 2)).flatten() {
            Some(b) => Ok(u32::from(u16::from_le_bytes([b[0], b[1]]))),
            None => Err(fault()),
        }
    }

    /// The variable-length fetch: up to three bytes at `pc`, for a decoder
    /// whose instructions are 2 or 3 bytes long and start at any alignment.
    ///
    /// This is [`fetch_instruction`](Bus::fetch_instruction)'s sibling, not
    /// its replacement: the two serve two instruction sets and neither is
    /// written in terms of the other, because the answers differ. The word
    /// form rejects an odd address (an RV32 fact) and assembles a `u32`; this
    /// one has no alignment rule at all, and returns the *count* of bytes it
    /// could really read.
    ///
    /// The count is the honest answer, not a courtesy: a decoder that needs
    /// more bytes than it got has run off the end of the region, and that is a
    /// fault for the decoder to raise, not something to paper over by reading
    /// the next region's first byte or by wrapping to the arena's start.
    ///
    /// **`SocBus` must not inherit the default.** `Bus::fetch_bytes`'s default
    /// is three [`read_u8`](Bus::read_u8) calls, and `read_u8` on this bus
    /// reaches the MMIO decode table, where a read has side effects: reading
    /// UART0's FIFO pops a byte. A fetch that walked off the end of an exec
    /// region into peripheral space would therefore clock a FIFO, silently, on
    /// the fetch path. Same rule as `fetch_instruction`'s, and for the same
    /// reason: fetch never routes to MMIO.
    #[inline]
    fn fetch_bytes(&mut self, pc: u32, out: &mut [u8; 3]) -> Result<usize, MemoryError> {
        // Two, matching `fetch_instruction`, and deliberately not the byte
        // count: the watchpoint models a hardware fetch trigger, which watches
        // the instruction's *address*. Sizing the check by the bytes returned
        // would make a 3-byte instruction trip a trigger that a 2-byte one at
        // the same address does not.
        self.check_watchpoints(pc, 2, MemoryAccessKind::InstructionFetch)?;

        let fault = || MemoryError::InvalidAccess {
            address: pc,
            size: 1,
            kind: MemoryAccessKind::InstructionFetch,
        };

        // Fetch never routes to MMIO: a jump into peripheral space is a
        // wild branch, and returning a register's value as an instruction
        // would turn it into a puzzle.
        let i = self.fetch_region_index(pc).ok_or_else(fault)?;
        if !self.regions[i].exec {
            return Err(fault());
        }
        self.last_fetch_region = i;
        if self.strict {
            self.check_code_word(pc, true);
        }
        if let Some(cost) = self.memory_cost.as_mut() {
            self.memory_cycles += cost.fetch(pc);
        }
        // The arena is flat across region boundaries, so the region's own end
        // — not the arena's — is what says how much of this fetch is real.
        // Two regions that happen to sit next to each other in the arena are
        // still two regions, and an instruction does not straddle them. The
        // subtraction cannot wrap: `fetch_region_index` already placed `pc`
        // inside the region, so `pc < end()`.
        let room = self.regions[i].end().wrapping_sub(pc).min(3) as usize;
        let off = (pc - self.arena_base) as usize;
        let d = &self.arena;
        // One bounds check, one copy — no per-byte loop, because the count is
        // the whole answer and a partial copy would be the same slice anyway.
        let got = match d.get(off..off + room) {
            Some(b) => {
                out[..b.len()].copy_from_slice(b);
                b.len()
            }
            None => return Err(fault()),
        };
        // Unreachable while `fetch_region_index` succeeded on a non-empty
        // region, but making `Ok(0)` impossible by construction beats making
        // it impossible by argument: a decoder handed zero bytes would see an
        // empty instruction rather than a fault.
        if got == 0 {
            return Err(fault());
        }
        Ok(got)
    }

    #[inline]
    fn read_word(&mut self, address: u32) -> Result<i32, MemoryError> {
        self.read(address, Width::Word).map(|v| v as i32)
    }

    #[inline]
    fn read_halfword(&mut self, address: u32) -> Result<i16, MemoryError> {
        self.read(address, Width::Half).map(|v| v as u16 as i16)
    }

    #[inline]
    fn read_byte(&mut self, address: u32) -> Result<i8, MemoryError> {
        self.read(address, Width::Byte).map(|v| v as u8 as i8)
    }

    #[inline]
    fn read_u8(&mut self, address: u32) -> Result<u8, MemoryError> {
        self.read(address, Width::Byte).map(|v| v as u8)
    }

    #[inline]
    fn write_word(&mut self, address: u32, value: i32) -> Result<(), MemoryError> {
        self.write(address, Width::Word, value as u32)
    }

    #[inline]
    fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), MemoryError> {
        self.write(address, Width::Half, value as u16 as u32)
    }

    #[inline]
    fn write_byte(&mut self, address: u32, value: i8) -> Result<(), MemoryError> {
        self.write(address, Width::Byte, value as u8 as u32)
    }

    fn set_watchpoint(&mut self, slot: usize, wp: Option<Watchpoint>) {
        if slot >= self.slots {
            log::warn!("SocBus: watchpoint slot {slot} is out of range, ignored");
            return;
        }
        // A trigger CSR write is not an MMIO access, so it has no line of
        // its own; the arming it produces is worth one, because "the stack
        // guard moved to X" is exactly what a boot trace is read for. Only
        // when the slot's effective watchpoint actually changes — esp-hal
        // rewrites all four trigger CSRs on every context switch, and four
        // lines per switch would bury the ones that matter.
        if self.trace.is_enabled() && self.watchpoints[slot] != wp {
            let line = match wp {
                Some(w) => alloc::format!(
                    "cyc={} pc=0x{:08x} WATCHPOINT slot={slot} armed at 0x{:08x}{}{}{}{}",
                    self.now,
                    self.pc,
                    w.address,
                    if w.napot { " napot" } else { "" },
                    if w.on_store { " store" } else { "" },
                    if w.on_load { " load" } else { "" },
                    if w.on_execute { " exec" } else { "" },
                ),
                None => alloc::format!(
                    "cyc={} pc=0x{:08x} WATCHPOINT slot={slot} disarmed",
                    self.now,
                    self.pc
                ),
            };
            self.trace.note(&line);
        }
        self.watchpoints[slot] = wp;
        let bit = 1u32 << slot;
        for (kind, wanted) in [
            (MemoryAccessKind::Read, wp.is_some_and(|w| w.on_load)),
            (MemoryAccessKind::Write, wp.is_some_and(|w| w.on_store)),
            (
                MemoryAccessKind::InstructionFetch,
                wp.is_some_and(|w| w.on_execute),
            ),
        ] {
            let mask = &mut self.armed_for[kind_index(kind)];
            if wanted {
                *mask |= bit;
            } else {
                *mask &= !bit;
            }
        }
        self.recompute_single_store_watch();
    }

    #[inline(always)]
    fn set_issuing(&mut self, pc: u32, cycle: u64) {
        self.pc = pc;
        self.now = cycle;
    }

    fn take_sideband(&mut self) -> bool {
        core::mem::replace(&mut self.sideband, false)
    }

    fn take_yield(&mut self) -> bool {
        core::mem::replace(&mut self.yield_now, false)
    }

    #[inline]
    fn take_memory_cost(&mut self) -> u32 {
        core::mem::take(&mut self.memory_cycles)
    }

    fn pending_cpu_interrupt(&self) -> Option<u8> {
        self.matrix.cpu_interrupt(self.hart, &self.irq)
    }

    /// May the hart's block cache decode ahead of the guest?
    ///
    /// Two things on this bus make a fetch more than a read, and either one
    /// makes reading an instruction the guest has not reached yet a change to
    /// what the run counts or does:
    ///
    /// - a **memory-cost model** (`t3`) charges cycles for the fetch itself,
    ///   so a decode-ahead fetch and the later execution would charge it
    ///   twice or not at all;
    /// - an **execute watchpoint** turns a fetch into a trap, and a
    ///   decode-ahead would take it one or more instructions early.
    ///
    /// Both can appear and disappear mid-run — the grade is fixed at build
    /// time, but a guest CSR write arms a trigger whenever it likes — so the
    /// hart re-reads this after every instruction the cache did not run.
    fn fetch_is_pure(&self) -> bool {
        self.memory_cost.is_none()
            && self.armed_for[kind_index(MemoryAccessKind::InstructionFetch)] == 0
    }

    /// Instructions in `[pc, pc + bytes)` are about to run from a cached
    /// block, so this bus will see no fetch for them.
    ///
    /// Under `--strict-bus` the missing-fence checker lives on the fetch
    /// path, and a cached block has no fetch path — this is where it is told
    /// instead. Off by default and inlined away.
    fn note_cached_execute(&mut self, pc: u32, bytes: u32) {
        if !self.strict {
            return;
        }
        if self.unpublished_code_words.is_empty() {
            return;
        }
        let mut at = pc & !3;
        let end = pc.saturating_add(bytes);
        while at < end {
            self.check_code_word(at, false);
            at = at.saturating_add(4);
        }
    }

    /// The guest retired a `fence.i`: everything it has written is published.
    fn note_fence_i(&mut self) {
        self.unpublished_code_words.clear();
    }

    fn sideband_or_yield_pending(&self) -> bool {
        self.sideband || self.yield_now
    }

    fn load_watchpoints_armed(&self) -> bool {
        self.armed_for[kind_index(MemoryAccessKind::Read)] != 0
    }

    fn store_watch(&self) -> lp_emu_core::StoreWatch {
        if self.armed_for[kind_index(MemoryAccessKind::Write)] == 0 {
            return lp_emu_core::StoreWatch::None;
        }
        match self.single_store_watch {
            Some((lo, hi, _)) => lp_emu_core::StoreWatch::One { lo, hi },
            None => lp_emu_core::StoreWatch::Many,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::periph::{Peripheral, RegGrades, Strap};
    use crate::regfile::RegFile;
    use crate::trace::SharedBuffer;

    /// A peripheral that records what it was asked and answers predictably.
    struct Probe {
        name: &'static str,
        last: Option<(u32, Width, u32)>,
        reads: u32,
        answer: u32,
        /// Set a level and schedule an event on the next write.
        raise_on_write: Option<(u16, EventId)>,
        events: Vec<EventId>,
        index: usize,
    }

    impl Probe {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                last: None,
                reads: 0,
                answer: 0,
                raise_on_write: None,
                events: Vec::new(),
                index: 0,
            }
        }
    }

    impl Peripheral for Probe {
        fn name(&self) -> &'static str {
            self.name
        }

        fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
            self.reads += 1;
            self.last = Some((off, width, 0));
            self.answer
        }

        fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
            self.last = Some((off, width, value));
            if let Some((source, id)) = self.raise_on_write {
                cx.irq.set_level(source, true);
                cx.sched.schedule_in(cx.now, 10, id);
            }
        }

        fn on_event(&mut self, id: EventId, _cx: &mut BusCx<'_>) {
            self.events.push(id);
        }

        fn reg_name(&self, off: u32) -> Option<&'static str> {
            (off == 0x1c).then_some("status")
        }

        fn save_state(&self) -> Vec<u8> {
            alloc::vec![self.reads as u8]
        }

        fn load_state(&mut self, bytes: &[u8]) {
            self.reads = u32::from(bytes.first().copied().unwrap_or(0));
        }
    }

    fn bus_with_ram() -> SocBus {
        let mut bus = SocBus::new();
        bus.add_region(RamRegion::new("hp-ram", 0x4080_0000, 0x1000).executable());
        bus.add_region(RamRegion::new("lp-ram", 0x5000_0000, 0x100));
        bus
    }

    #[test]
    fn ram_regions_route_by_address() {
        let mut bus = bus_with_ram();
        bus.write_word(0x4080_0010, 0x1234_5678).unwrap();
        bus.write_word(0x5000_0010, 0x0bad_c0de).unwrap();
        assert_eq!(bus.read_word(0x4080_0010).unwrap(), 0x1234_5678);
        assert_eq!(bus.read_word(0x5000_0010).unwrap(), 0x0bad_c0de);
        // Byte order is little-endian, like the guest's.
        assert_eq!(bus.read_u8(0x4080_0010).unwrap(), 0x78);
        assert_eq!(bus.read_u8(0x4080_0013).unwrap(), 0x12);
    }

    #[test]
    fn the_last_hit_cache_does_not_change_the_answer() {
        let mut bus = bus_with_ram();
        bus.write_word(0x4080_0000, 1).unwrap();
        bus.write_word(0x5000_0000, 2).unwrap();
        for _ in 0..4 {
            assert_eq!(bus.read_word(0x4080_0000).unwrap(), 1);
            assert_eq!(bus.read_word(0x5000_0000).unwrap(), 2);
        }
    }

    #[test]
    fn an_access_that_runs_off_the_end_of_a_region_faults() {
        let mut bus = bus_with_ram();
        let err = bus.read_word(0x5000_00fe).unwrap_err();
        assert!(matches!(err, MemoryError::InvalidAccess { .. }));
    }

    #[test]
    #[should_panic(expected = "overlaps")]
    fn overlapping_regions_are_a_build_time_panic() {
        let mut bus = SocBus::new();
        bus.add_region(RamRegion::new("a", 0x1000, 0x100));
        bus.add_region(RamRegion::new("b", 0x1080, 0x100));
    }

    #[test]
    fn mmio_routes_to_the_peripheral_with_the_register_offset() {
        let mut bus = bus_with_ram();
        let idx = bus.add_peripheral(0x6000_0000, 0x1000, Box::new(Probe::new("UART0")));
        bus.write_word(0x6000_001c, 0xdead_beefu32 as i32).unwrap();
        let p = bus.peripheral_mut(idx).unwrap();
        // Downcasting is not the point; ask the bus what it saw instead.
        assert_eq!(p.name(), "UART0");
        assert_eq!(p.save_state(), alloc::vec![0]);
        assert_eq!(bus.read_word(0x6000_001c).unwrap(), 0);
    }

    /// `mmio_index`'s last-hit cache must never disagree with the sorted
    /// binary search it shortcuts: every mapped block's first and last byte,
    /// the first unmapped byte past it, and the gap before the next block —
    /// with the cache pre-warmed on the *other* block each time, so a stale
    /// neighbour would show up as a wrong answer instead of a lucky hit.
    #[test]
    fn mmio_last_hit_cache_agrees_with_the_slow_path_at_every_boundary() {
        let mut bus = SocBus::new();
        bus.add_peripheral(0x6000_0000, 0x100, Box::new(Probe::new("A")));
        bus.add_peripheral(0x6000_1000, 0x100, Box::new(Probe::new("B")));

        let probes: &[u32] = &[
            0x6000_0000, // A's first byte
            0x6000_00ff, // A's last byte
            0x6000_0100, // first unmapped byte after A
            0x6000_0fff, // last unmapped byte before B
            0x6000_1000, // B's first byte
            0x6000_10ff, // B's last byte
            0x6000_1100, // first unmapped byte after B
        ];

        for &addr in probes {
            // Warm the cache on the block the probe is *not* in, so a hit
            // here can only come from the real decode, never a leftover.
            let _ = bus.mmio_index(0x6000_0000);
            let _ = bus.mmio_index(0x6000_1000);
            let cached = bus.mmio_index(addr);

            bus.last_mmio = None;
            let slow = bus.mmio_index_slow(addr);

            assert_eq!(
                cached, slow,
                "0x{addr:08x}: cache said {cached:?}, the slow path said {slow:?}"
            );
        }
    }

    #[test]
    fn byte_and_halfword_lanes_reach_the_peripheral_intact() {
        // esp-println writes a word to USB_DEVICE; the ROM writes UART's
        // FIFO as a byte. Both must arrive as what they were.
        let mut bus = SocBus::new();
        bus.add_peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)));
        bus.write_byte(0x6000_0001, 0x41u8 as i8).unwrap();
        bus.write_halfword(0x6000_0006, 0x1234u16 as i16).unwrap();
        assert_eq!(bus.read_u8(0x6000_0001).unwrap(), 0x41);
        assert_eq!(bus.read_word(0x6000_0000).unwrap() as u32, 0x0000_4100);
        assert_eq!(bus.read_word(0x6000_0004).unwrap() as u32, 0x1234_0000);
    }

    #[test]
    fn unaligned_mmio_word_access_faults() {
        let mut bus = SocBus::new();
        bus.add_peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)));
        assert!(matches!(
            bus.read_word(0x6000_0002).unwrap_err(),
            MemoryError::Unaligned { alignment: 4, .. }
        ));
        assert!(matches!(
            bus.write_halfword(0x6000_0001, 0).unwrap_err(),
            MemoryError::Unaligned { alignment: 2, .. }
        ));
    }

    #[test]
    fn unmapped_reads_return_zero_writes_are_dropped_and_both_are_counted() {
        let mut bus = bus_with_ram();
        bus.set_pc(0x4200_0000);
        assert_eq!(bus.read_word(0x7000_0000).unwrap(), 0);
        bus.write_word(0x7000_0000, 0xffff_ffffu32 as i32).unwrap();
        assert_eq!(bus.read_word(0x7000_0000).unwrap(), 0);
        assert_eq!(bus.unmapped_reads(), 2);
        assert_eq!(bus.unmapped_writes(), 1);
    }

    #[test]
    fn an_unmapped_site_is_logged_once_per_pc_and_address() {
        let buf = SharedBuffer::new();
        let mut bus = bus_with_ram();
        bus.trace = Trace::to_sink(Box::new(buf.clone()));
        bus.set_pc(0x4200_0000);
        for _ in 0..5 {
            bus.read_word(0x7000_0000).unwrap();
        }
        // Same address, different PC: a second site.
        bus.set_pc(0x4200_0004);
        bus.read_word(0x7000_0000).unwrap();
        assert_eq!(bus.unmapped_sites(), 2);
        assert_eq!(bus.unmapped_reads(), 6);
        assert_eq!(buf.lines().len(), 2);
        assert!(buf.lines()[0].contains("UNMAPPED+0x70000000"));
    }

    #[test]
    fn strict_mode_turns_an_unmapped_access_into_a_fault() {
        let mut bus = bus_with_ram();
        bus.set_strict(true);
        assert!(matches!(
            bus.read_word(0x7000_0000).unwrap_err(),
            MemoryError::InvalidAccess {
                kind: MemoryAccessKind::Read,
                ..
            }
        ));
        assert!(matches!(
            bus.write_word(0x7000_0000, 1).unwrap_err(),
            MemoryError::InvalidAccess {
                kind: MemoryAccessKind::Write,
                ..
            }
        ));
        // Still counted, so a strict run reports the same numbers.
        assert_eq!(bus.unmapped_reads(), 1);
        assert_eq!(bus.unmapped_writes(), 1);
    }

    #[test]
    fn an_mmio_window_with_no_peripheral_is_still_unmapped_but_says_so() {
        let mut bus = bus_with_ram();
        bus.add_mmio_window(0x6000_0000, 0x0010_0000);
        assert!(bus.in_mmio_window(0x6000_5000));
        assert!(!bus.in_mmio_window(0x7000_0000));
        assert_eq!(bus.read_word(0x6000_5000).unwrap(), 0);
        assert_eq!(bus.unmapped_reads(), 1);
    }

    #[test]
    fn a_watchpoint_fires_before_the_write_and_the_bytes_are_unchanged() {
        let mut bus = bus_with_ram();
        bus.write_word(0x4080_0100, 0xa5a5_a5a5u32 as i32).unwrap();
        bus.set_watchpoint(
            1,
            Some(Watchpoint {
                address: 0x4080_0100,
                napot: false,
                on_store: true,
                on_load: false,
                on_execute: false,
            }),
        );
        let err = bus.write_word(0x4080_0100, 0).unwrap_err();
        assert!(matches!(
            err,
            MemoryError::Watchpoint {
                slot: 1,
                kind: MemoryAccessKind::Write,
                ..
            }
        ));
        // The guard word survived, which is the whole point.
        bus.set_watchpoint(1, None);
        assert_eq!(bus.read_word(0x4080_0100).unwrap() as u32, 0xa5a5_a5a5);
    }

    #[test]
    fn a_store_watchpoint_ignores_loads_and_vice_versa() {
        let mut bus = bus_with_ram();
        bus.set_watchpoint(
            0,
            Some(Watchpoint {
                address: 0x4080_0100,
                napot: false,
                on_store: true,
                on_load: false,
                on_execute: false,
            }),
        );
        assert!(bus.read_word(0x4080_0100).is_ok());
        assert!(bus.write_word(0x4080_0100, 0).is_err());
    }

    #[test]
    fn a_napot_watchpoint_covers_its_whole_region() {
        // tdata2 = 0x40800103 -> trailing ones `11`, so mask = 0b111 and the
        // region is 0x40800100..0x40800108 (8 bytes).
        let wp = Watchpoint {
            address: 0x4080_0103,
            napot: true,
            on_store: true,
            on_load: true,
            on_execute: false,
        };
        assert!(watchpoint_overlaps(&wp, 0x4080_0100, 4));
        assert!(watchpoint_overlaps(&wp, 0x4080_0104, 4));
        assert!(watchpoint_overlaps(&wp, 0x4080_00fe, 4)); // straddles the start
        assert!(!watchpoint_overlaps(&wp, 0x4080_0108, 4));
        assert!(!watchpoint_overlaps(&wp, 0x4080_00f8, 4));

        let mut bus = bus_with_ram();
        bus.set_watchpoint(2, Some(wp));
        assert!(bus.write_byte(0x4080_0107, 0).is_err());
        assert!(bus.write_byte(0x4080_0108, 0).is_ok());
    }

    #[test]
    fn an_unarmed_slot_costs_nothing_and_clearing_disarms() {
        let mut bus = bus_with_ram();
        let wp = Watchpoint {
            address: 0x4080_0100,
            napot: false,
            on_store: true,
            on_load: true,
            on_execute: false,
        };
        assert!(bus.write_word(0x4080_0100, 0).is_ok());
        bus.set_watchpoint(0, Some(wp));
        assert!(bus.write_word(0x4080_0100, 0).is_err());
        bus.set_watchpoint(0, None);
        assert!(bus.write_word(0x4080_0100, 0).is_ok());
        // Out-of-range slots are ignored, not fatal.
        bus.set_watchpoint(MAX_WATCHPOINT_SLOTS, Some(wp));
        assert!(bus.write_word(0x4080_0100, 0).is_ok());
    }

    /// A machine with fewer slots than the array holds: the slots it has
    /// work, and the ones above the count are not arm-able.
    #[test]
    fn a_narrowed_slot_count_ignores_the_slots_above_it() {
        let mut bus = bus_with_ram();
        bus.set_watchpoint_slots(2);
        assert_eq!(bus.watchpoint_slots(), 2);

        let in_range = Watchpoint {
            address: 0x4080_0100,
            napot: false,
            on_store: true,
            on_load: false,
            on_execute: false,
        };
        bus.set_watchpoint(1, Some(in_range));
        assert!(bus.write_word(0x4080_0100, 0).is_err(), "slot 1 fires");

        let above = Watchpoint {
            address: 0x4080_0200,
            napot: false,
            on_store: true,
            on_load: false,
            on_execute: false,
        };
        bus.set_watchpoint(2, Some(above));
        assert!(
            bus.write_word(0x4080_0200, 0).is_ok(),
            "slot 2 is above the count: the arm is ignored and its range is clear"
        );
    }

    /// The ordering the setter's disarm loop exists to prevent: a slot armed
    /// while the count was wide must not survive the narrowing, armed and
    /// unreachable.
    #[test]
    fn narrowing_disarms_a_slot_that_was_already_armed() {
        let mut bus = bus_with_ram();
        let wp = Watchpoint {
            address: 0x4080_0300,
            napot: false,
            on_store: true,
            on_load: false,
            on_execute: false,
        };
        bus.set_watchpoint(3, Some(wp));
        assert!(bus.write_word(0x4080_0300, 0).is_err(), "armed while wide");

        bus.set_watchpoint_slots(2);
        assert!(
            bus.write_word(0x4080_0300, 0).is_ok(),
            "narrowing cleared slot 3"
        );
    }

    /// Nothing changed for a caller that never declares a count.
    #[test]
    fn the_default_slot_count_is_the_maximum() {
        let mut bus = bus_with_ram();
        assert_eq!(bus.watchpoint_slots(), MAX_WATCHPOINT_SLOTS);
        let wp = Watchpoint {
            address: 0x4080_0400,
            napot: false,
            on_store: true,
            on_load: false,
            on_execute: false,
        };
        bus.set_watchpoint(MAX_WATCHPOINT_SLOTS - 1, Some(wp));
        assert!(bus.write_word(0x4080_0400, 0).is_err());
    }

    #[test]
    fn sideband_is_set_by_mmio_writes_only() {
        let mut bus = bus_with_ram();
        bus.add_peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)));
        assert!(!bus.take_sideband());

        bus.write_word(0x4080_0000, 1).unwrap();
        assert!(!bus.take_sideband(), "a RAM store is not a bus event");

        bus.read_word(0x6000_0000).unwrap();
        assert!(!bus.take_sideband(), "an MMIO read is not a bus event");

        bus.write_word(0x6000_0000, 1).unwrap();
        assert!(bus.take_sideband());
        assert!(!bus.take_sideband(), "take clears");
    }

    #[test]
    fn fetch_comes_from_exec_regions_only() {
        let mut bus = bus_with_ram();
        bus.add_peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)));
        bus.write_word(0x4080_0000, 0x0000_0013).unwrap(); // nop
        assert_eq!(bus.fetch_instruction(0x4080_0000).unwrap(), 0x0000_0013);

        // Non-exec RAM.
        assert!(matches!(
            bus.fetch_instruction(0x5000_0000).unwrap_err(),
            MemoryError::InvalidAccess {
                kind: MemoryAccessKind::InstructionFetch,
                ..
            }
        ));
        // MMIO.
        assert!(matches!(
            bus.fetch_instruction(0x6000_0000).unwrap_err(),
            MemoryError::InvalidAccess {
                kind: MemoryAccessKind::InstructionFetch,
                ..
            }
        ));
        // Unmapped.
        assert!(bus.fetch_instruction(0x7000_0000).is_err());
        // Odd address.
        assert!(matches!(
            bus.fetch_instruction(0x4080_0001).unwrap_err(),
            MemoryError::Unaligned { alignment: 2, .. }
        ));
    }

    #[test]
    fn a_compressed_instruction_at_the_end_of_a_region_still_fetches() {
        let mut bus = SocBus::new();
        bus.add_region(RamRegion::new("tiny", 0x4080_0000, 4).executable());
        bus.write_word(0x4080_0000, 0x0000_4501).unwrap();
        assert_eq!(bus.fetch_instruction(0x4080_0002).unwrap(), 0x0000_0000);
        assert!(bus.fetch_instruction(0x4080_0004).is_err());
    }

    // ---- byte-granular fetch (`Bus::fetch_bytes`) ----------------------
    //
    // The fixture is the shape that makes the interesting cases reachable:
    // two **adjacent** exec regions, an MMIO window that abuts the second
    // one's end, and a non-exec region above the window. The arena is one
    // flat allocation covering 0x4080_0000..0x4080_0130, so it physically
    // holds bytes underneath the MMIO hole — which is exactly what a fetch
    // must refuse to hand back.

    const FETCH_A: u32 = 0x4080_0000; // exec, 0x10 bytes
    const FETCH_B: u32 = 0x4080_0010; // exec, 0x10 bytes, adjacent to A
    const FETCH_MMIO: u32 = 0x4080_0020; // peripheral window, abuts B's end
    const FETCH_DATA: u32 = 0x4080_0120; // non-exec RAM above the window

    /// The fixture, plus the peripheral's index so a test can ask how many
    /// reads it saw.
    fn bus_for_byte_fetch() -> (SocBus, usize) {
        let mut bus = SocBus::new();
        bus.add_region(RamRegion::new("code-a", FETCH_A, 0x10).executable());
        bus.add_region(RamRegion::new("code-b", FETCH_B, 0x10).executable());
        bus.add_region(RamRegion::new("data", FETCH_DATA, 0x10));
        let probe = bus.add_peripheral(FETCH_MMIO, 0x100, Box::new(Probe::new("UART0")));
        // Distinct, recognisable bytes per region: A is 0xa0.., B is 0xb0..,
        // the non-exec region is 0xd0.., so a byte that leaked across a
        // boundary names the region it came from.
        let a: Vec<u8> = (0..0x10u8).map(|i| 0xa0 | i).collect();
        let b: Vec<u8> = (0..0x10u8).map(|i| 0xb0 | i).collect();
        let d: Vec<u8> = (0..0x10u8).map(|i| 0xd0 | i).collect();
        bus.load_image(FETCH_A, &a).unwrap();
        bus.load_image(FETCH_B, &b).unwrap();
        bus.load_image(FETCH_DATA, &d).unwrap();
        (bus, probe)
    }

    /// How many MMIO reads the fixture's peripheral has served. `Probe`'s
    /// `save_state` is its read count, which beats downcasting.
    fn probe_reads(bus: &mut SocBus, probe: usize) -> u8 {
        bus.peripheral_mut(probe).unwrap().save_state()[0]
    }

    /// A cost model that charges a different, recognisable amount per access
    /// kind, so "charged once, as a fetch" is distinguishable from "charged
    /// three times, as loads" — which is what inheriting the default
    /// `Bus::fetch_bytes` would look like.
    struct CountingCost;

    impl lp_emu_core::cycle_model::MemoryCost for CountingCost {
        fn fetch(&mut self, _addr: u32) -> u32 {
            7
        }
        fn load(&mut self, _addr: u32, _width: u8) -> u32 {
            100
        }
        fn store(&mut self, _addr: u32, _width: u8) -> u32 {
            1000
        }
    }

    #[test]
    fn fetch_bytes_reads_three_at_any_alignment() {
        let (mut bus, _) = bus_for_byte_fetch();
        // Xtensa instructions start wherever the previous one ended, so every
        // alignment is a real fetch address. No `Unaligned` here, ever.
        for k in 0..4u32 {
            let mut out = [0u8; 3];
            let got = bus.fetch_bytes(FETCH_A + k, &mut out).unwrap();
            assert_eq!(got, 3, "three bytes of room at +{k}");
            assert_eq!(
                out,
                [0xa0 | k as u8, 0xa0 | (k + 1) as u8, 0xa0 | (k + 2) as u8],
                "the arena's own bytes at +{k}"
            );
        }
    }

    #[test]
    fn fetch_bytes_at_a_region_edge_returns_what_is_there() {
        let (mut bus, _) = bus_for_byte_fetch();
        // Two bytes before the end of `code-b`, the last exec region, whose
        // end abuts the MMIO window: the count is what is really there.
        let mut out = [0u8; 3];
        let got = bus.fetch_bytes(FETCH_B + 0xe, &mut out).unwrap();
        assert_eq!(got, 2);
        assert_eq!(&out[..2], &[0xbe, 0xbf]);

        let mut out = [0u8; 3];
        let got = bus.fetch_bytes(FETCH_B + 0xf, &mut out).unwrap();
        assert_eq!(got, 1);
        assert_eq!(out[0], 0xbf);

        // One past the end is not an edge, it is a fault.
        let mut out = [0u8; 3];
        assert!(bus.fetch_bytes(FETCH_B + 0x10, &mut out).is_err());
    }

    #[test]
    fn fetch_bytes_does_not_straddle_into_the_next_region() {
        let (mut bus, _) = bus_for_byte_fetch();
        // `code-a` and `code-b` are adjacent in the arena and both executable,
        // so a fetch that used the ARENA's bounds instead of the REGION's
        // would happily return `code-b`'s first byte as the third byte of an
        // instruction in `code-a`. This is the test the phase exists for.
        let mut out = [0xffu8; 3];
        let got = bus.fetch_bytes(FETCH_A + 0xe, &mut out).unwrap();
        assert_eq!(got, 2, "two bytes are what is really there");
        assert_eq!(&out[..2], &[0xae, 0xaf]);
        assert_ne!(
            out[2], 0xb0,
            "the third byte must not be `code-b`'s first byte"
        );
        assert_eq!(out[2], 0xff, "and nothing was written past the count");

        // The region really is adjacent — `code-b`'s own first byte is there
        // when it is asked for by its own address.
        let mut out = [0u8; 3];
        assert_eq!(bus.fetch_bytes(FETCH_B, &mut out).unwrap(), 3);
        assert_eq!(out[0], 0xb0);
    }

    #[test]
    fn fetch_bytes_never_routes_to_mmio() {
        let (mut bus, probe) = bus_for_byte_fetch();
        // Straight at the peripheral's base: a wild branch into peripheral
        // space is a fault, not a register read dressed up as an instruction.
        let mut out = [0u8; 3];
        let err = bus.fetch_bytes(FETCH_MMIO, &mut out).unwrap_err();
        assert!(matches!(
            err,
            MemoryError::InvalidAccess {
                address: FETCH_MMIO,
                kind: MemoryAccessKind::InstructionFetch,
                ..
            }
        ));
        assert_eq!(probe_reads(&mut bus, probe), 0, "no MMIO read happened");

        // And at the last bytes of the exec region whose end abuts that
        // window: the third byte would be the peripheral's first register.
        // The default `Bus::fetch_bytes` (three `read_u8` calls) would read
        // it, and on a real UART that pops a FIFO byte.
        let mut out = [0u8; 3];
        assert_eq!(bus.fetch_bytes(FETCH_B + 0xe, &mut out).unwrap(), 2);
        assert_eq!(probe_reads(&mut bus, probe), 0, "still no MMIO read");
    }

    #[test]
    fn fetch_bytes_refuses_a_non_exec_region() {
        let (mut bus, probe) = bus_for_byte_fetch();
        let mut out = [0u8; 3];
        let err = bus.fetch_bytes(FETCH_DATA, &mut out).unwrap_err();
        assert!(matches!(
            err,
            MemoryError::InvalidAccess {
                address: FETCH_DATA,
                kind: MemoryAccessKind::InstructionFetch,
                ..
            }
        ));
        assert_eq!(out, [0u8; 3], "and no bytes were handed back");
        assert_eq!(probe_reads(&mut bus, probe), 0);
    }

    #[test]
    fn fetch_bytes_agrees_with_fetch_instruction_where_both_are_defined() {
        let (mut bus, _) = bus_for_byte_fetch();
        // The two methods are separate bodies with separate contracts; this
        // is the check that they read the same memory. Word-aligned, four
        // bytes of room: the only place `fetch_instruction` returns a whole
        // word, and therefore the only place the two are comparable.
        for base in [FETCH_A, FETCH_B] {
            for off in (0..0x10u32 - 4).step_by(4) {
                let at = base + off;
                let word = bus.fetch_instruction(at).unwrap();
                let mut out = [0u8; 3];
                assert_eq!(bus.fetch_bytes(at, &mut out).unwrap(), 3);
                let fourth = bus.read_u8(at + 3).unwrap();
                assert_eq!(
                    word,
                    u32::from_le_bytes([out[0], out[1], out[2], fourth]),
                    "the two fetch paths disagree at {at:#010x}"
                );
            }
        }
    }

    #[test]
    fn fetch_bytes_honours_a_watchpoint() {
        let (mut bus, _) = bus_for_byte_fetch();
        bus.set_watchpoint(
            1,
            Some(Watchpoint {
                address: FETCH_A + 4,
                napot: false,
                on_store: false,
                on_load: false,
                on_execute: true,
            }),
        );
        let mut out = [0xffu8; 3];
        let err = bus.fetch_bytes(FETCH_A + 4, &mut out).unwrap_err();
        assert!(matches!(
            err,
            MemoryError::Watchpoint {
                slot: 1,
                kind: MemoryAccessKind::InstructionFetch,
                ..
            }
        ));
        assert_eq!(out, [0xffu8; 3], "the trigger fires before any byte moves");

        // Disarmed, the same address fetches normally.
        bus.set_watchpoint(1, None);
        let mut out = [0u8; 3];
        assert_eq!(bus.fetch_bytes(FETCH_A + 4, &mut out).unwrap(), 3);
        assert_eq!(out[0], 0xa4);
    }

    #[test]
    fn fetch_bytes_unmapped_is_a_fault_with_the_address() {
        let (mut bus, _) = bus_for_byte_fetch();
        let mut out = [0u8; 3];
        let err = bus.fetch_bytes(0x7000_0000, &mut out).unwrap_err();
        assert!(matches!(
            err,
            MemoryError::InvalidAccess {
                address: 0x7000_0000,
                kind: MemoryAccessKind::InstructionFetch,
                ..
            }
        ));
    }

    #[test]
    fn fetch_bytes_charges_the_memory_cost_model_once() {
        let (mut bus, _) = bus_for_byte_fetch();
        bus.set_memory_cost(Some(Box::new(CountingCost)));
        assert_eq!(bus.take_memory_cost(), 0);

        let mut out = [0u8; 3];
        bus.fetch_bytes(FETCH_A, &mut out).unwrap();
        // Exactly one fetch. Three loads (the default's three `read_u8`
        // calls) would be 300, and a fetch plus loads would be 307.
        assert_eq!(bus.take_memory_cost(), 7);

        // A two-byte fetch at a region edge is still one fetch, not two.
        let mut out = [0u8; 3];
        bus.fetch_bytes(FETCH_B + 0xe, &mut out).unwrap();
        assert_eq!(bus.take_memory_cost(), 7);
    }

    #[test]
    fn a_read_only_region_refuses_guest_stores_but_load_image_still_places_bytes() {
        let mut bus = SocBus::new();
        bus.add_region(
            RamRegion::new("rom", 0x4000_0000, 0x100)
                .executable()
                .read_only(),
        );
        assert!(bus.write_word(0x4000_0000, 1).is_err());
        bus.load_image(0x4000_0000, &[0x13, 0x00, 0x00, 0x00])
            .unwrap();
        assert_eq!(bus.fetch_instruction(0x4000_0000).unwrap(), 0x13);
    }

    #[test]
    fn load_image_refuses_an_address_outside_every_region() {
        let mut bus = bus_with_ram();
        assert!(bus.load_image(0x7000_0000, &[1, 2, 3]).is_err());
        assert!(bus.load_image(0x5000_00ff, &[1, 2, 3]).is_err());
    }

    #[test]
    fn a_peripheral_raises_a_level_and_schedules_an_event_that_comes_back() {
        let mut bus = SocBus::new();
        let mut probe = Probe::new("TIMG0");
        probe.index = 0;
        probe.raise_on_write = Some((17, event_id(0, 3)));
        let idx = bus.add_peripheral(0x6000_8000, 0x100, Box::new(probe));
        assert_eq!(idx, 0);

        bus.set_time(1_000);
        bus.write_word(0x6000_8000, 1).unwrap();
        assert!(bus.irq.level(17));
        assert!(bus.irq.take_changed());
        assert_eq!(bus.sched.next_deadline(), Some(1_010));

        bus.run_due_events(1_005);
        assert_eq!(bus.sched.next_deadline(), Some(1_010));
        bus.run_due_events(1_010);
        assert_eq!(bus.sched.next_deadline(), None);
    }

    #[test]
    fn event_ids_round_trip_through_the_peripheral_index() {
        let id = event_id(5, 0x1234);
        assert_eq!(event_peripheral(id), 5);
        assert_eq!(event_local(id), 0x1234);
        assert_eq!(event_peripheral(event_id(0, 0)), 0);
    }

    #[test]
    fn an_event_for_a_peripheral_that_does_not_exist_is_survivable() {
        let mut bus = SocBus::new();
        bus.sched.schedule_at(10, event_id(9, 0));
        bus.run_due_events(10);
        assert_eq!(bus.sched.next_deadline(), None);
    }

    #[test]
    fn peripheral_indices_are_stable_when_a_lower_base_is_added_later() {
        let mut bus = SocBus::new();
        let timg = bus.add_peripheral(0x6000_8000, 0x100, Box::new(RegFile::new("TIMG0", 0x100)));
        // An event scheduled against TIMG0's index before UART0 exists.
        bus.sched.schedule_at(10, event_id(timg, 1));

        let uart = bus.add_peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)));
        assert_eq!((timg, uart), (0, 1));
        assert_eq!(bus.peripheral(timg).map(|p| p.name()), Some("TIMG0"));
        assert_eq!(bus.peripheral(uart).map(|p| p.name()), Some("UART0"));
        assert_eq!(bus.peripheral_index("UART0"), Some(uart));
        // ...and the decode still routes by address, not by insertion order.
        bus.write_word(0x6000_0000, 0x1234).unwrap();
        bus.write_word(0x6000_8000, 0x5678).unwrap();
        assert_eq!(bus.read_word(0x6000_0000).unwrap(), 0x1234);
        assert_eq!(bus.read_word(0x6000_8000).unwrap(), 0x5678);
    }

    #[test]
    fn permissive_unaligned_reaches_a_register_but_never_straddles_two() {
        let mut bus = SocBus::new();
        bus.add_peripheral(0x6000_0000, 0x100, Box::new(RegFile::new("UART0", 0x100)));
        bus.set_allow_unaligned(true);
        assert!(bus.allow_unaligned());

        // Inside one 32-bit register: exact through the byte-lane path.
        bus.write_halfword(0x6000_0001, 0x1234u16 as i16).unwrap();
        assert_eq!(bus.read_word(0x6000_0000).unwrap() as u32, 0x0012_3400);
        assert_eq!(bus.read_halfword(0x6000_0001).unwrap() as u16, 0x1234);

        // Straddling two registers: still refused, permissive or not.
        assert!(matches!(
            bus.read_word(0x6000_0002).unwrap_err(),
            MemoryError::Unaligned { alignment: 4, .. }
        ));
        assert!(matches!(
            bus.write_halfword(0x6000_0003, 0).unwrap_err(),
            MemoryError::Unaligned { alignment: 2, .. }
        ));
    }

    #[test]
    fn strict_mode_records_the_first_refused_access_only() {
        let mut bus = bus_with_ram();
        bus.add_mmio_window(0x6000_0000, 0x0010_0000);
        bus.set_strict(true);
        bus.set_time(1234);
        bus.set_pc(0x4200_1000);

        assert!(bus.first_strict_violation().is_none());
        assert!(bus.read_word(0x6000_5000).is_err());
        assert!(bus.write_word(0x7000_0000, 1).is_err());

        let v = bus.first_strict_violation().expect("recorded");
        assert_eq!(
            (v.cycle, v.pc, v.address, v.access, v.in_mmio_window),
            (1234, 0x4200_1000, 0x6000_5000, Access::Read, true),
            "the FIRST one, with the window flag that says `unmodelled block`"
        );
        bus.clear_strict_violation();
        assert!(bus.first_strict_violation().is_none());
    }

    /// A matrix that asserts CPU interrupt `n` while source `n + 40` is high.
    struct TestMatrix;

    impl CpuIntMatrix for TestMatrix {
        fn asserted(&self, _hart: usize, irq: &IrqLines) -> u32 {
            (0u8..16)
                .filter(|n| irq.level(u16::from(*n) + 40))
                .fold(0u32, |acc, n| acc | 1 << n)
        }

        fn cpu_interrupt(&self, _hart: usize, irq: &IrqLines) -> Option<u8> {
            (0u8..16).find(|n| irq.level(u16::from(*n) + 40))
        }

        fn as_any(&self) -> &dyn core::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
            self
        }
    }

    /// A peripheral whose only behaviour is to ask for a reset when written.
    struct Resetter;

    impl Peripheral for Resetter {
        fn name(&self) -> &'static str {
            "WDT"
        }

        fn read(&mut self, _off: u32, _width: Width, _cx: &mut BusCx<'_>) -> u32 {
            0
        }

        fn write(&mut self, _off: u32, _width: Width, _value: u32, cx: &mut BusCx<'_>) {
            let at = cx.now;
            cx.request(MachineRequest::Reset {
                source: "WDT stage 0",
                at,
                strap: Strap::App,
            });
        }

        fn save_state(&self) -> Vec<u8> {
            Vec::new()
        }

        fn load_state(&mut self, _bytes: &[u8]) {}
    }

    #[test]
    fn a_peripheral_can_ask_the_machine_for_a_reset_and_the_first_request_wins() {
        let mut bus = SocBus::new();
        bus.add_peripheral(0x6000_8000, 0x100, Box::new(Resetter));
        assert_eq!(bus.take_request(), None);

        bus.set_time(77);
        bus.write_word(0x6000_8000, 1).unwrap();
        bus.set_time(78);
        bus.write_word(0x6000_8000, 1).unwrap();
        assert_eq!(
            bus.take_request(),
            Some(MachineRequest::Reset {
                source: "WDT stage 0",
                at: 77,
                strap: Strap::App,
            }),
            "the first request is the one the machine sees"
        );
        assert_eq!(bus.take_request(), None, "take clears");
    }

    /// A block with one `documented` register at `+0x24` and everything
    /// else `modeled`.
    struct Graded {
        grades: RegGrades,
        reads: u32,
    }

    impl Peripheral for Graded {
        fn name(&self) -> &'static str {
            "USB_DEVICE"
        }

        fn read(&mut self, _off: u32, _width: Width, _cx: &mut BusCx<'_>) -> u32 {
            self.reads += 1;
            0x55
        }

        fn write(&mut self, _off: u32, _width: Width, _value: u32, _cx: &mut BusCx<'_>) {}

        fn reg_name(&self, off: u32) -> Option<&'static str> {
            match off & !3 {
                0x00 => Some("ep1"),
                0x24 => Some("fram_num"),
                _ => None,
            }
        }

        fn reg_grade(&self, off: u32) -> Option<RegGrade> {
            Some(self.grades.grade(off))
        }

        fn save_state(&self) -> Vec<u8> {
            Vec::new()
        }

        fn load_state(&mut self, _bytes: &[u8]) {}
    }

    /// A block that publishes no grade table — every accept table on the
    /// chip today.
    struct Ungraded;

    impl Peripheral for Ungraded {
        fn name(&self) -> &'static str {
            "PCR"
        }

        fn read(&mut self, _off: u32, _width: Width, _cx: &mut BusCx<'_>) -> u32 {
            0x77
        }

        fn write(&mut self, _off: u32, _width: Width, _value: u32, _cx: &mut BusCx<'_>) {}

        fn save_state(&self) -> Vec<u8> {
            Vec::new()
        }

        fn load_state(&mut self, _bytes: &[u8]) {}
    }

    /// M6 P4's scoping rule. "Nobody graded this block" is not "this block
    /// is modelled": a run under `--strict-grade documented` crosses an
    /// accept table without stopping, and the report says which blocks were
    /// actually checked so that the pass cannot be mistaken for a verdict on
    /// the whole chip.
    #[test]
    fn strict_grade_passes_over_a_block_that_publishes_no_table_and_says_which_it_checked() {
        let mut bus = bus_with_ram();
        bus.add_mmio_window(0x6000_0000, 0x0010_0000);
        bus.add_peripheral(
            0x6000_f000,
            0x100,
            Box::new(Graded {
                grades: RegGrades::new().with_grade(0x24, RegGrade::Documented),
                reads: 0,
            }),
        );
        bus.add_peripheral(0x6009_6000, 0x100, Box::new(Ungraded));

        bus.set_strict_grade(Some(RegGrade::Documented));
        // The ungraded block answers, at any level.
        assert_eq!(bus.read_word(0x6009_6000).unwrap(), 0x77);
        bus.set_strict_grade(Some(RegGrade::Measured));
        assert_eq!(bus.read_word(0x6009_6004).unwrap(), 0x77);
        assert!(bus.first_strict_violation().is_none());
        // And the graded one is still checked.
        assert!(bus.read_word(0x6000_f000).is_err());

        assert_eq!(bus.blocks_in_strict_grade_scope(), vec!["USB_DEVICE"]);
    }

    #[test]
    fn strict_grade_refuses_a_register_below_the_level_before_the_peripheral_sees_it() {
        let buf = SharedBuffer::new();
        let mut bus = bus_with_ram();
        bus.trace = Trace::to_sink(Box::new(buf.clone()));
        bus.add_mmio_window(0x6000_0000, 0x0010_0000);
        bus.add_peripheral(
            0x6000_f000,
            0x100,
            Box::new(Graded {
                grades: RegGrades::new().with_grade(0x24, RegGrade::Documented),
                reads: 0,
            }),
        );
        bus.set_time(9);
        bus.set_pc(0x4200_0000);

        // No level: everything is answered.
        assert_eq!(bus.read_word(0x6000_f000).unwrap(), 0x55);
        // `modeled` is the floor, so it refuses nothing.
        bus.set_strict_grade(Some(RegGrade::Modeled));
        assert_eq!(bus.read_word(0x6000_f000).unwrap(), 0x55);
        assert!(bus.first_strict_violation().is_none());

        bus.set_strict_grade(Some(RegGrade::Documented));
        assert_eq!(bus.strict_grade(), Some(RegGrade::Documented));
        // The documented register still answers …
        assert_eq!(bus.read_word(0x6000_f024).unwrap(), 0x55);
        // … the modeled one is refused, and the peripheral never saw it.
        let err = bus.read_word(0x6000_f000).unwrap_err();
        assert!(matches!(
            err,
            MemoryError::InvalidAccess {
                kind: MemoryAccessKind::Read,
                ..
            }
        ));
        let v = bus.first_strict_violation().expect("recorded");
        assert_eq!(
            (
                v.cycle,
                v.pc,
                v.address,
                v.access,
                v.in_mmio_window,
                v.grade
            ),
            (
                9,
                0x4200_0000,
                0x6000_f000,
                Access::Read,
                true,
                Some(RegGrade::Modeled)
            )
        );
        assert!(bus.write_word(0x6000_f000, 1).is_err());
        assert_eq!(
            bus.unmapped_reads() + bus.unmapped_writes(),
            0,
            "a grade refusal is not an unmapped access"
        );
        let lines = buf.lines();
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert_eq!(
            lines[3],
            "cyc=9 pc=0x42000000 STRICT-GRADE read of USB_DEVICE+0x000 ep1: graded modeled, \
             the run trusts documented and above"
        );
        // `measured` refuses the documented one too.
        bus.clear_strict_violation();
        bus.set_strict_grade(Some(RegGrade::Measured));
        assert!(bus.read_word(0x6000_f024).is_err());
        assert_eq!(
            bus.first_strict_violation().unwrap().grade,
            Some(RegGrade::Documented)
        );
    }

    #[test]
    fn reg_grades_default_to_modeled_and_replace_by_offset() {
        let g = RegGrades::new()
            .with_grade(0x24, RegGrade::Documented)
            .with_grade(0x18, RegGrade::Documented)
            .with_grade(0x25, RegGrade::Measured);
        assert_eq!(g.grade(0x00), RegGrade::Modeled);
        assert_eq!(g.grade(0x18), RegGrade::Documented);
        assert_eq!(g.grade(0x24), RegGrade::Measured, "0x25 aliases 0x24");
        assert_eq!(g.grade(0x26), RegGrade::Measured);
        assert_eq!(
            g.entries(),
            [(0x18, RegGrade::Documented), (0x24, RegGrade::Measured)]
        );
        assert!(RegGrade::Modeled < RegGrade::Documented);
        assert!(RegGrade::Documented < RegGrade::Measured);
        assert_eq!(RegGrade::parse("documented"), Some(RegGrade::Documented));
        assert_eq!(RegGrade::parse("strict"), None);
        assert_eq!(RegGrade::Measured.to_string(), "measured");
    }

    #[test]
    fn arming_a_watchpoint_is_one_trace_line_and_rewriting_the_same_one_is_none() {
        let buf = SharedBuffer::new();
        let mut bus = bus_with_ram();
        bus.trace = Trace::to_sink(Box::new(buf.clone()));
        bus.set_time(5);
        bus.set_pc(0x4200_0010);
        let wp = Watchpoint {
            address: 0x4080_0101,
            napot: true,
            on_store: true,
            on_load: false,
            on_execute: false,
        };
        bus.set_watchpoint(0, Some(wp));
        bus.set_watchpoint(0, Some(wp));
        bus.set_watchpoint(0, None);
        assert_eq!(
            buf.lines(),
            [
                "cyc=5 pc=0x42000010 WATCHPOINT slot=0 armed at 0x40800101 napot store",
                "cyc=5 pc=0x42000010 WATCHPOINT slot=0 disarmed",
            ]
        );
    }

    #[test]
    fn the_matrix_answers_pending_cpu_interrupt_from_the_source_levels() {
        let mut bus = SocBus::new();
        // Without a matrix installed, nothing is ever asserted.
        assert_eq!(bus.pending_cpu_interrupt(), None);

        let mut probe = Probe::new("TIMG0");
        probe.raise_on_write = Some((43, event_id(0, 0)));
        bus.add_peripheral(0x6000_8000, 0x100, Box::new(probe));
        bus.set_matrix(Box::new(TestMatrix));

        assert_eq!(bus.pending_cpu_interrupt(), None);
        bus.write_word(0x6000_8000, 1).unwrap();
        assert!(bus.take_sideband(), "the store raised the side-band");
        assert_eq!(
            bus.pending_cpu_interrupt(),
            Some(3),
            "source 43 routes to CPU interrupt 3"
        );

        bus.irq.set_level(43, false);
        assert_eq!(bus.pending_cpu_interrupt(), None);
    }

    #[test]
    fn the_mask_and_the_option_agree_on_the_test_matrix() {
        let mut bus = SocBus::new();
        bus.set_matrix(Box::new(TestMatrix));
        assert_eq!(bus.pending_cpu_interrupt_mask(), 0, "no source is high");

        // Sources 40, 43 and 51 route to CPU interrupts 0, 3 and 11.
        bus.irq.set_level(40, true);
        bus.irq.set_level(43, true);
        bus.irq.set_level(51, true);
        assert_eq!(
            bus.pending_cpu_interrupt_mask(),
            (1 << 0) | (1 << 3) | (1 << 11),
            "exactly the bits for the raised sources, and nothing else"
        );

        // The `Option` answer is one of the bits the mask has set: the mask is
        // asserted, the option is takeable, and takeable implies asserted.
        let taken = bus.pending_cpu_interrupt().expect("something is asserted");
        assert!(
            bus.pending_cpu_interrupt_mask() & (1 << taken) != 0,
            "cpu_interrupt returned {taken}, which the mask does not assert"
        );

        // A source going low clears its bit and nothing else's.
        bus.irq.set_level(43, false);
        assert_eq!(
            bus.pending_cpu_interrupt_mask(),
            (1 << 0) | (1 << 11),
            "only CPU interrupt 3 dropped"
        );
    }

    #[test]
    fn a_bus_with_no_matrix_asserts_nothing() {
        let mut bus = SocBus::new();
        assert_eq!(bus.pending_cpu_interrupt_mask(), 0);
        assert_eq!(bus.pending_cpu_interrupt(), None);

        // Raising a source changes nothing: `NoCpuInterrupts` has no routing.
        bus.irq.set_level(43, true);
        assert_eq!(bus.pending_cpu_interrupt_mask(), 0);
        assert_eq!(bus.pending_cpu_interrupt(), None);
    }

    #[test]
    fn a_snapshot_of_regions_peripherals_and_scalars_round_trips() {
        let mut bus = bus_with_ram();
        bus.add_peripheral(0x6000_0000, 0x100, Box::new(Probe::new("UART0")));
        bus.set_time(99);
        bus.set_pc(0x4200_0004);
        bus.write_word(0x4080_0010, 0x1234_5678).unwrap();
        bus.read_word(0x6000_0000).unwrap(); // Probe counts reads
        bus.read_word(0x7000_0000).unwrap(); // unmapped

        let regions = bus.save_regions();
        let periph = bus.save_peripherals();
        let scalars = bus.save_scalars();
        assert_eq!(periph[0].0, "UART0");

        // Move on, then go back.
        bus.write_word(0x4080_0010, 0).unwrap();
        bus.read_word(0x6000_0000).unwrap();
        bus.read_word(0x7000_0004).unwrap();
        bus.set_time(500);

        bus.restore_regions(&regions);
        bus.restore_peripherals(&periph);
        bus.restore_scalars(&scalars);

        assert_eq!(bus.read_word(0x4080_0010).unwrap(), 0x1234_5678);
        assert_eq!(bus.now(), 99);
        assert_eq!(bus.unmapped_reads(), 1);
        assert_eq!(bus.unmapped_sites(), 1);
        assert_eq!(bus.peripheral(0).unwrap().save_state(), alloc::vec![1]);
    }

    #[test]
    #[should_panic(expected = "this bus has")]
    fn restoring_a_snapshot_from_a_different_machine_is_refused() {
        let mut bus = bus_with_ram();
        bus.restore_regions(&[alloc::vec![0; 8]]);
    }

    #[test]
    fn traced_mmio_names_the_register_when_the_peripheral_knows_it() {
        let buf = SharedBuffer::new();
        let mut bus = SocBus::new();
        bus.trace = Trace::to_sink(Box::new(buf.clone()));
        bus.add_peripheral(0x6000_0000, 0x1000, Box::new(Probe::new("UART0")));
        bus.set_time(99);
        bus.set_pc(0x4200_1000);
        bus.read_word(0x6000_001c).unwrap();
        bus.read_word(0x6000_0020).unwrap();
        assert_eq!(
            buf.lines(),
            [
                "cyc=99 pc=0x42001000 R4 UART0+0x01c status = 0x00000000",
                "cyc=99 pc=0x42001000 R4 UART0+0x020 = 0x00000000",
            ]
        );
    }

    // ---- the guest arena (M7 JD4) ---------------------------------------

    #[test]
    fn regions_are_views_into_one_flat_arena() {
        let bus = bus_with_ram();
        // The arena spans from the lowest base to the highest end, gaps and
        // all: that flatness is the point — a guest address is at
        // `address - base` with no region lookup.
        assert_eq!(bus.guest_arena_base(), 0x4080_0000);
        assert_eq!(
            bus.guest_arena().len(),
            (0x5000_0100u64 - 0x4080_0000) as usize
        );
        for r in bus.regions() {
            let off = (r.base - bus.guest_arena_base()) as usize;
            assert_eq!(
                bus.region_bytes(r).as_ptr(),
                bus.guest_arena()[off..].as_ptr(),
                "`{}` is a view into the arena at its own offset",
                r.name
            );
        }
    }

    #[test]
    fn a_guest_store_lands_in_the_arena_at_the_flat_offset() {
        let mut bus = bus_with_ram();
        bus.write_word(0x4080_0010, 0x1234_5678).unwrap();
        let off = (0x4080_0010u32 - bus.guest_arena_base()) as usize;
        assert_eq!(
            &bus.guest_arena()[off..off + 4],
            &0x1234_5678u32.to_le_bytes()
        );
    }

    #[test]
    fn an_access_running_off_the_end_of_a_region_still_faults() {
        // The regression the flat arena creates and this compare closes: the
        // last word of `hp-ram` is followed in the *arena* by the gap before
        // `lp-ram`, so an access that straddles the end would read zeros
        // instead of faulting if the region's own end were not checked.
        let mut bus = bus_with_ram();
        let last = 0x4080_0000 + 0x1000 - 2;
        assert!(
            bus.read_word(last).is_err(),
            "a word straddling the end faults"
        );
        assert!(bus.write_word(last, 1).is_err(), "so does a store");
        assert!(
            bus.read_halfword(last).is_ok(),
            "a halfword that fits does not"
        );
    }

    #[test]
    fn a_fetch_off_the_end_of_a_region_reads_no_further_than_the_region() {
        let mut bus = SocBus::new();
        bus.add_region(RamRegion::new("code", 0x4080_0000, 4).executable());
        bus.add_region(RamRegion::new("data", 0x4080_1000, 4));
        bus.load_image(0x4080_0000, &[0x01, 0x02, 0x03, 0x04])
            .unwrap();
        bus.load_image(0x4080_1000, &[0xaa, 0xbb, 0xcc, 0xdd])
            .unwrap();
        // Two bytes left in the region: a compressed instruction is legal
        // there and a 32-bit fetch must not reach past the region's end.
        assert_eq!(bus.fetch_instruction(0x4080_0002).unwrap(), 0x0403);
        assert!(bus.fetch_instruction(0x4080_0004).is_err());
    }

    #[test]
    fn growing_the_arena_keeps_every_region_s_bytes() {
        let mut bus = SocBus::new();
        bus.add_region(RamRegion::new("high", 0x4080_0000, 0x100));
        bus.write_word(0x4080_0000, 0xdead_beefu32 as i32).unwrap();
        // A region below the current base moves every existing region's
        // offset; the bytes must move with them.
        bus.add_region(RamRegion::from_bytes(
            "low",
            0x4000_0000,
            alloc::vec![7u8; 8],
        ));
        assert_eq!(bus.guest_arena_base(), 0x4000_0000);
        assert_eq!(bus.read_word(0x4080_0000).unwrap(), 0xdead_beefu32 as i32);
        assert_eq!(bus.read_byte(0x4000_0003).unwrap(), 7);
    }

    #[test]
    fn reserving_the_span_first_leaves_the_arena_where_it_is() {
        let mut bus = SocBus::new();
        bus.reserve_guest_span(0x4000_0000, 0x1000_0100);
        let base = bus.guest_arena().as_ptr();
        bus.add_region(RamRegion::new("rom", 0x4000_0000, 0x100));
        bus.add_region(RamRegion::new("ram", 0x5000_0000, 0x100));
        assert_eq!(bus.guest_arena().as_ptr(), base, "the arena did not move");
        assert_eq!(bus.guest_arena_base(), 0x4000_0000);
    }

    #[test]
    fn a_snapshot_round_trips_through_the_arena() {
        let mut bus = bus_with_ram();
        bus.write_word(0x4080_0010, 0x1234_5678).unwrap();
        let saved = bus.save_regions();
        bus.write_word(0x4080_0010, 0).unwrap();
        bus.restore_regions(&saved);
        assert_eq!(bus.read_word(0x4080_0010).unwrap(), 0x1234_5678);
    }

    // ---- the permission table -------------------------------------------

    #[test]
    fn a_fully_covered_page_reports_its_writability() {
        let mut bus = SocBus::new();
        bus.reserve_guest_span(0x4000_0000, 3 * PERMISSION_PAGE_LEN);
        bus.add_region(RamRegion::new("rw", 0x4000_0000, PERMISSION_PAGE_LEN));
        bus.add_region(
            RamRegion::new("ro", 0x4000_0000 + PERMISSION_PAGE_LEN, PERMISSION_PAGE_LEN)
                .read_only(),
        );
        let table = bus.permission_table();
        assert_eq!(table.len(), 3);
        assert_eq!(table[0], PERM_READ_WRITE);
        assert_eq!(table[1], PERM_READ);
        // Covered by nothing.
        assert_eq!(table[2], PERM_NONE);
    }

    #[test]
    fn a_partly_covered_page_is_not_plain_ram() {
        let mut bus = SocBus::new();
        bus.reserve_guest_span(0x4000_0000, 2 * PERMISSION_PAGE_LEN);
        // Half a page: an inline access to the other half would read a byte
        // the bus would have refused.
        bus.add_region(RamRegion::new("half", 0x4000_0000, PERMISSION_PAGE_LEN / 2));
        assert_eq!(bus.permission_table()[0], PERM_NONE);
    }

    #[test]
    fn a_cost_model_or_strict_bus_takes_the_whole_table_to_none() {
        let mut bus = SocBus::new();
        bus.reserve_guest_span(0x4000_0000, PERMISSION_PAGE_LEN);
        bus.add_region(RamRegion::new("rw", 0x4000_0000, PERMISSION_PAGE_LEN));
        assert_eq!(bus.permission_table()[0], PERM_READ_WRITE);

        bus.set_strict(true);
        assert_eq!(
            bus.permission_table()[0],
            PERM_NONE,
            "--strict-bus watches guest stores, and an inline store is not watched"
        );
        bus.set_strict(false);

        bus.set_memory_cost(Some(Box::new(lp_emu_core::cycle_model::NoMemoryCost)));
        assert_eq!(
            bus.permission_table()[0],
            PERM_NONE,
            "a cost model charges per access, and an inline access charges none"
        );
    }

    #[test]
    fn an_mmio_window_over_a_page_takes_it_to_none() {
        let mut bus = SocBus::new();
        bus.reserve_guest_span(0x4000_0000, PERMISSION_PAGE_LEN);
        bus.add_region(RamRegion::new("rw", 0x4000_0000, PERMISSION_PAGE_LEN));
        bus.add_mmio_window(0x4000_0000 + 0x100, 0x10);
        assert_eq!(bus.permission_table()[0], PERM_NONE);
    }

    // ---- the peek methods a translated core reads ------------------------

    #[test]
    fn store_watch_reports_one_range_and_refuses_two() {
        let mut bus = bus_with_ram();
        assert_eq!(bus.store_watch(), lp_emu_core::StoreWatch::None);
        assert!(!bus.load_watchpoints_armed());

        bus.set_watchpoint(
            0,
            Some(lp_emu_core::Watchpoint {
                address: 0x4080_0100,
                napot: false,
                on_store: true,
                on_load: false,
                on_execute: false,
            }),
        );
        assert_eq!(
            bus.store_watch(),
            lp_emu_core::StoreWatch::One {
                lo: 0x4080_0100,
                hi: 0x4080_0101
            },
            "esp-hal's stack guard is one range and has to stay honourable"
        );
        assert!(!bus.load_watchpoints_armed());

        bus.set_watchpoint(
            1,
            Some(lp_emu_core::Watchpoint {
                address: 0x4080_0200,
                napot: false,
                on_store: true,
                on_load: false,
                on_execute: false,
            }),
        );
        assert_eq!(bus.store_watch(), lp_emu_core::StoreWatch::Many);

        bus.set_watchpoint(
            2,
            Some(lp_emu_core::Watchpoint {
                address: 0x4080_0300,
                napot: false,
                on_store: false,
                on_load: true,
                on_execute: false,
            }),
        );
        assert!(
            bus.load_watchpoints_armed(),
            "translated code does loads the bus never sees, so it refuses"
        );
    }
}
