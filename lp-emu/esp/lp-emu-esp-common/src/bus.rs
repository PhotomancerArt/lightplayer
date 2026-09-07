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
use lp_emu_core::memory::{MemoryAccessKind, MemoryError};
use lp_emu_core::sched::{Cycles, EventId, Scheduler};

use crate::host::HostSinks;
use crate::periph::{
    BoxedPeripheral, BusCx, CpuIntMatrix, IrqLines, MachineRequest, NoCpuInterrupts, RegGrade,
    Width,
};
use crate::trace::{Access, MmioEvent, Trace};

/// Hardware trigger slots, matching the RISC-V debug spec's count on the
/// ESP32-C6 (four `mcontrol` triggers).
pub const WATCHPOINT_SLOTS: usize = 4;

/// How many distinct `(pc, address)` unmapped sites are remembered before
/// the bus stops recording new ones. Counting continues; only the
/// log-it-once set is capped, so a runaway pointer cannot eat the host's
/// memory.
const UNMAPPED_SITES_CAP: usize = 4096;

/// Guard against a peripheral that reschedules itself at the current cycle
/// forever. One `run_due_events` call will not dispatch more than this.
const MAX_EVENTS_PER_TICK: u32 = 100_000;

/// A span of guest RAM at a chip-specific base.
#[derive(Clone, Debug)]
pub struct RamRegion {
    pub name: &'static str,
    pub base: u32,
    /// Instruction fetch is allowed from this region.
    pub exec: bool,
    /// Guest stores are allowed. `false` models ROM and a read-only flash
    /// cache window.
    pub writable: bool,
    pub data: Vec<u8>,
}

impl RamRegion {
    /// A zeroed, readable, writable, non-executable region.
    pub fn new(name: &'static str, base: u32, len: u32) -> Self {
        Self {
            name,
            base,
            exec: false,
            writable: true,
            data: alloc::vec![0; len as usize],
        }
    }

    /// A region initialised from bytes (a ROM image, a flash window).
    pub fn from_bytes(name: &'static str, base: u32, data: Vec<u8>) -> Self {
        Self {
            name,
            base,
            exec: false,
            writable: true,
            data,
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

    pub fn len(&self) -> u32 {
        self.data.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
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
    pub irq: IrqLines,
    pub unmapped_sites: BTreeSet<(u32, u32)>,
    pub unmapped_reads: u64,
    pub unmapped_writes: u64,
    pub first_strict_violation: Option<StrictViolation>,
    pub request: Option<MachineRequest>,
}

/// One entry in the MMIO decode table.
struct MmioRange {
    base: u32,
    len: u32,
    periph: BoxedPeripheral,
}

/// The SoC bus.
pub struct SocBus {
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
    sideband: bool,
    /// The chip's misaligned-access policy, mirrored from the hart. The C6
    /// core performs misaligned data accesses in hardware, so its machine
    /// sets both permissive; the flag lives here because the bus is the
    /// component that actually decides.
    allow_unaligned: bool,
    watchpoints: [Option<Watchpoint>; WATCHPOINT_SLOTS],
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

    /// Source levels → "the CPU interrupt this hart should take". Installed
    /// by the chip crate; [`NoCpuInterrupts`] until then.
    matrix: Box<dyn CpuIntMatrix>,
    /// A peripheral's request to the machine. See [`MachineRequest`].
    request: Option<MachineRequest>,

    now: Cycles,
    pc: u32,
    hart: usize,

    pub sched: Scheduler,
    pub irq: IrqLines,
    pub trace: Trace,
    pub host: HostSinks,
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
            regions: Vec::new(),
            last_regions: [0, 0],
            last_fetch_region: 0,
            mmio: Vec::new(),
            mmio_by_base: Vec::new(),
            last_mmio: None,
            mmio_windows: Vec::new(),
            strict: false,
            strict_grade: None,
            sideband: false,
            allow_unaligned: false,
            watchpoints: [None; WATCHPOINT_SLOTS],
            armed_for: [0; 3],
            single_store_watch: None,
            unmapped_sites: BTreeSet::new(),
            unmapped_reads: 0,
            unmapped_writes: 0,
            first_strict_violation: None,
            matrix: Box::new(NoCpuInterrupts),
            request: None,
            now: 0,
            pc: 0,
            hart: 0,
            sched: Scheduler::new(),
            irq: IrqLines::new(),
            trace: Trace::disabled(),
            host: HostSinks::new(),
        }
    }

    // ---- construction ------------------------------------------------

    /// Add a RAM region. Panics on an overlap with an existing one: a
    /// machine whose memory map contradicts itself is a build-time bug, and
    /// discovering it as a mysterious aliasing read at cycle 400,000 is
    /// strictly worse than discovering it here.
    pub fn add_region(&mut self, region: RamRegion) {
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
        self.regions.push(region);
        self.regions.sort_by_key(|r| r.base);
        self.last_regions = [0, 0];
        self.last_fetch_region = 0;
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
    /// `Some(Documented)` stops on the first `Modeled` register any block
    /// answers — which on a chip whose accept tables are all `Modeled` is
    /// the first MMIO access of the boot. That is the point: the level says
    /// what the run is allowed to trust, and a register outside it is a
    /// stop with its name in the report, not a silent guess.
    pub fn set_strict_grade(&mut self, level: Option<RegGrade>) {
        self.strict_grade = level;
    }

    pub fn strict_grade(&self) -> Option<RegGrade> {
        self.strict_grade
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

    /// Install the chip's interrupt matrix. See [`CpuIntMatrix`].
    pub fn set_matrix(&mut self, matrix: Box<dyn CpuIntMatrix>) {
        self.matrix = matrix;
    }

    pub fn matrix(&self) -> &dyn CpuIntMatrix {
        &*self.matrix
    }

    pub fn matrix_mut(&mut self) -> &mut dyn CpuIntMatrix {
        &mut *self.matrix
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

    pub fn regions(&self) -> &[RamRegion] {
        &self.regions
    }

    pub fn region_mut(&mut self, name: &str) -> Option<&mut RamRegion> {
        self.regions.iter_mut().find(|r| r.name == name)
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
        let off = (address - self.regions[i].base) as usize;
        self.regions[i].data[off..off + bytes.len()].copy_from_slice(bytes);
        Ok(())
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
        self.regions.iter().map(|r| r.data.clone()).collect()
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
        for (region, bytes) in self.regions.iter_mut().zip(data) {
            assert_eq!(
                bytes.len(),
                region.data.len(),
                "SocBus::restore_regions: region `{}` is {} bytes, snapshot has {}",
                region.name,
                region.data.len(),
                bytes.len()
            );
            region.data.copy_from_slice(bytes);
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
            irq: self.irq.clone(),
            unmapped_sites: self.unmapped_sites.clone(),
            unmapped_reads: self.unmapped_reads,
            unmapped_writes: self.unmapped_writes,
            first_strict_violation: self.first_strict_violation,
            request: self.request,
        }
    }

    pub fn restore_scalars(&mut self, s: &BusScalars) {
        self.now = s.now;
        self.pc = s.pc;
        self.hart = s.hart;
        self.sideband = s.sideband;
        self.irq = s.irq.clone();
        self.unmapped_sites = s.unmapped_sites.clone();
        self.unmapped_reads = s.unmapped_reads;
        self.unmapped_writes = s.unmapped_writes;
        self.first_strict_violation = s.first_strict_violation;
        self.request = s.request;
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
        for slot in 0..WATCHPOINT_SLOTS {
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

        if let Some(i) = self.region_index(address) {
            // `region_index` guarantees `address` is inside the region, so
            // "the span fits" is exactly "the slice exists"; it also updated
            // its own cache, so there is nothing left to record here.
            let off = (address - self.regions[i].base) as usize;
            let data = &self.regions[i].data;
            let v = match width {
                Width::Word => data
                    .get(off..off + 4)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                Width::Half => data
                    .get(off..off + 2)
                    .map(|b| u32::from(u16::from_le_bytes([b[0], b[1]]))),
                Width::Byte => data.get(off).map(|b| u32::from(*b)),
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

        if let Some(i) = self.region_index(address) {
            let fault = MemoryError::InvalidAccess {
                address,
                size: len as usize,
                kind: MemoryAccessKind::Write,
            };
            if !self.regions[i].writable {
                return Err(fault);
            }
            let off = (address - self.regions[i].base) as usize;
            let data = &mut self.regions[i].data;
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
        let grade = self.mmio[index].periph.reg_grade(off);
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
        let region = &self.regions[i];
        let off = (address - region.base) as usize;
        let d = &region.data;
        if let Some(b) = d.get(off..off + 4) {
            return Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        }
        // Two bytes are enough: a compressed instruction at the very end of
        // a region is legal, and the decoder asks for no more than it needs.
        match d.get(off..off + 2) {
            Some(b) => Ok(u32::from(u16::from_le_bytes([b[0], b[1]]))),
            None => Err(fault()),
        }
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
        if slot >= WATCHPOINT_SLOTS {
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

    fn pending_cpu_interrupt(&self) -> Option<u8> {
        self.matrix.cpu_interrupt(self.hart, &self.irq)
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
        bus.set_watchpoint(WATCHPOINT_SLOTS, Some(wp));
        assert!(bus.write_word(0x4080_0100, 0).is_ok());
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

        fn reg_grade(&self, off: u32) -> RegGrade {
            self.grades.grade(off)
        }

        fn save_state(&self) -> Vec<u8> {
            Vec::new()
        }

        fn load_state(&mut self, _bytes: &[u8]) {}
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
}
