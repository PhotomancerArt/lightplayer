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

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use lp_emu_core::bus::{Bus, Watchpoint};
use lp_emu_core::memory::{MemoryAccessKind, MemoryError};
use lp_emu_core::sched::{Cycles, EventId, Scheduler};

use crate::host::HostSinks;
use crate::periph::{BoxedPeripheral, BusCx, IrqLines, Width};
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
    /// "Last region hit" cache. The common case — the same stack or heap
    /// region twice in a row — is then one compare instead of a search.
    last_region: usize,
    /// Insertion order, so a peripheral's index — which [`event_id`] packs
    /// into the scheduler's event tags — never moves under it.
    mmio: Vec<MmioRange>,
    /// Indices into `mmio`, sorted by base. The decode's binary search.
    mmio_by_base: Vec<usize>,
    /// Address ranges that belong to MMIO even where no peripheral claims
    /// them. The chip crate registers these; the common crate has no
    /// addresses of its own.
    mmio_windows: Vec<(u32, u32)>,

    strict: bool,
    sideband: bool,
    watchpoints: [Option<Watchpoint>; WATCHPOINT_SLOTS],
    /// Bit per armed slot; `0` short-circuits the per-access check.
    armed: u32,

    unmapped_sites: BTreeSet<(u32, u32)>,
    unmapped_reads: u64,
    unmapped_writes: u64,

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
            last_region: 0,
            mmio: Vec::new(),
            mmio_by_base: Vec::new(),
            mmio_windows: Vec::new(),
            strict: false,
            sideband: false,
            watchpoints: [None; WATCHPOINT_SLOTS],
            armed: 0,
            unmapped_sites: BTreeSet::new(),
            unmapped_reads: 0,
            unmapped_writes: 0,
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
        self.last_region = 0;
    }

    /// Add a peripheral at `base` covering `len` bytes. Returns its index —
    /// what [`event_id`] packs into an event tag, and what the machine keeps
    /// to reach it later.
    ///
    /// The index is the insertion order and never moves: the decode's sorted
    /// order lives in a side table, because an index that shifted when a
    /// lower-based peripheral was registered later would silently re-point
    /// every already-scheduled event.
    pub fn add_peripheral(&mut self, base: u32, len: u32, periph: BoxedPeripheral) -> usize {
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
            };
            range.periph.on_event(id, &mut cx);
        }
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

    /// `true` if `address` falls in a declared MMIO window.
    pub fn in_mmio_window(&self, address: u32) -> bool {
        self.mmio_windows
            .iter()
            .any(|(b, l)| address >= *b && address < b.wrapping_add(*l))
    }

    // ---- decode -------------------------------------------------------

    fn region_index(&self, address: u32) -> Option<usize> {
        // The "last hit" cache: one compare for the common case.
        if let Some(r) = self.regions.get(self.last_region)
            && r.contains(address)
        {
            return Some(self.last_region);
        }
        let i = self.regions.partition_point(|r| r.base <= address);
        let i = i.checked_sub(1)?;
        self.regions[i].contains(address).then_some(i)
    }

    fn mmio_index(&self, address: u32) -> Option<usize> {
        let k = self
            .mmio_by_base
            .partition_point(|&i| self.mmio[i].base <= address);
        let i = self.mmio_by_base[k.checked_sub(1)?];
        let r = &self.mmio[i];
        (address < r.base.wrapping_add(r.len)).then_some(i)
    }

    // ---- watchpoints --------------------------------------------------

    fn check_watchpoints(
        &self,
        address: u32,
        len: u32,
        kind: MemoryAccessKind,
    ) -> Result<(), MemoryError> {
        for slot in 0..WATCHPOINT_SLOTS {
            if self.armed & (1 << slot) == 0 {
                continue;
            }
            let Some(wp) = self.watchpoints[slot] else {
                continue;
            };
            let wanted = match kind {
                MemoryAccessKind::Read => wp.on_load,
                MemoryAccessKind::Write => wp.on_store,
                MemoryAccessKind::InstructionFetch => wp.on_execute,
            };
            if !wanted {
                continue;
            }
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

    // ---- the access paths ---------------------------------------------

    fn read(&mut self, address: u32, width: Width) -> Result<u32, MemoryError> {
        let len = width.bytes();
        self.check_watchpoints(address, len, MemoryAccessKind::Read)?;

        if let Some(i) = self.region_index(address) {
            if !self.regions[i].span_fits(address, len) {
                return Err(MemoryError::InvalidAccess {
                    address,
                    size: len as usize,
                    kind: MemoryAccessKind::Read,
                });
            }
            self.last_region = i;
            let off = (address - self.regions[i].base) as usize;
            let data = &self.regions[i].data;
            let mut v = 0u32;
            for k in 0..len as usize {
                v |= u32::from(data[off + k]) << (8 * k);
            }
            return Ok(v);
        }

        if let Some(i) = self.mmio_index(address) {
            require_mmio_alignment(address, width)?;
            let base = self.mmio[i].base;
            let off = address - base;
            let (block, name) = {
                let p = &self.mmio[i].periph;
                (p.name(), p.reg_name(off))
            };
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

    fn write(&mut self, address: u32, width: Width, value: u32) -> Result<(), MemoryError> {
        let len = width.bytes();
        self.check_watchpoints(address, len, MemoryAccessKind::Write)?;

        if let Some(i) = self.region_index(address) {
            if !self.regions[i].writable || !self.regions[i].span_fits(address, len) {
                return Err(MemoryError::InvalidAccess {
                    address,
                    size: len as usize,
                    kind: MemoryAccessKind::Write,
                });
            }
            self.last_region = i;
            let off = (address - self.regions[i].base) as usize;
            let data = &mut self.regions[i].data;
            for k in 0..len as usize {
                data[off + k] = (value >> (8 * k)) as u8;
            }
            return Ok(());
        }

        if let Some(i) = self.mmio_index(address) {
            require_mmio_alignment(address, width)?;
            let base = self.mmio[i].base;
            let off = address - base;
            let (block, name) = {
                let p = &self.mmio[i].periph;
                (p.name(), p.reg_name(off))
            };
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

fn require_mmio_alignment(address: u32, width: Width) -> Result<(), MemoryError> {
    let alignment = width.bytes();
    if address % alignment == 0 {
        return Ok(());
    }
    Err(MemoryError::Unaligned {
        address,
        alignment: alignment as usize,
    })
}

/// Does the watchpoint cover any byte of `[address, address + len)`?
///
/// NAPOT is the RISC-V debug spec's `tdata2` encoding: the trailing ones of
/// the written value, plus the first zero above them, are the bits the
/// compare ignores — so `mask = value ^ (value + 1)` and the region is
/// `value & !mask` of size `mask + 1`.
fn watchpoint_overlaps(wp: &Watchpoint, address: u32, len: u32) -> bool {
    let (base, size) = if wp.napot {
        let mask = wp.address ^ wp.address.wrapping_add(1);
        (wp.address & !mask, u64::from(mask) + 1)
    } else {
        (wp.address, 1)
    };
    let (a0, a1) = (u64::from(address), u64::from(address) + u64::from(len));
    let (b0, b1) = (u64::from(base), u64::from(base) + size);
    a0 < b1 && b0 < a1
}

impl Bus for SocBus {
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
        let i = self.region_index(address).ok_or_else(fault)?;
        if !self.regions[i].exec {
            return Err(fault());
        }
        self.last_region = i;
        let region = &self.regions[i];
        let off = (address - region.base) as usize;
        // Two bytes are enough: a compressed instruction at the very end of
        // a region is legal, and the decoder asks for no more than it needs.
        if off + 2 > region.data.len() {
            return Err(fault());
        }
        let d = &region.data;
        let lo = u32::from(d[off]) | (u32::from(d[off + 1]) << 8);
        if off + 4 <= d.len() {
            Ok(lo | (u32::from(d[off + 2]) << 16) | (u32::from(d[off + 3]) << 24))
        } else {
            Ok(lo)
        }
    }

    fn read_word(&mut self, address: u32) -> Result<i32, MemoryError> {
        self.read(address, Width::Word).map(|v| v as i32)
    }

    fn read_halfword(&mut self, address: u32) -> Result<i16, MemoryError> {
        self.read(address, Width::Half).map(|v| v as u16 as i16)
    }

    fn read_byte(&mut self, address: u32) -> Result<i8, MemoryError> {
        self.read(address, Width::Byte).map(|v| v as u8 as i8)
    }

    fn read_u8(&mut self, address: u32) -> Result<u8, MemoryError> {
        self.read(address, Width::Byte).map(|v| v as u8)
    }

    fn write_word(&mut self, address: u32, value: i32) -> Result<(), MemoryError> {
        self.write(address, Width::Word, value as u32)
    }

    fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), MemoryError> {
        self.write(address, Width::Half, value as u16 as u32)
    }

    fn write_byte(&mut self, address: u32, value: i8) -> Result<(), MemoryError> {
        self.write(address, Width::Byte, value as u8 as u32)
    }

    fn set_watchpoint(&mut self, slot: usize, wp: Option<Watchpoint>) {
        if slot >= WATCHPOINT_SLOTS {
            log::warn!("SocBus: watchpoint slot {slot} is out of range, ignored");
            return;
        }
        self.watchpoints[slot] = wp;
        if wp.is_some() {
            self.armed |= 1 << slot;
        } else {
            self.armed &= !(1 << slot);
        }
    }

    fn take_sideband(&mut self) -> bool {
        core::mem::replace(&mut self.sideband, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::periph::Peripheral;
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
        bus.add_region(RamRegion::new("rom", 0x4000_0000, 0x100).executable().read_only());
        assert!(bus.write_word(0x4000_0000, 1).is_err());
        bus.load_image(0x4000_0000, &[0x13, 0x00, 0x00, 0x00]).unwrap();
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
