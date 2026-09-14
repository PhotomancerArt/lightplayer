//! The round-trip harness: an emitted module under wasmtime against a **real
//! `XtHart`** behind the escape hatch.
//!
//! The host here owns a machine-mode hart over a RAM-only bus. An escape
//! runs `XtHart::step_one` on it — the interpreter, not a script — and the
//! fused poll runs `XtHart::poll_after_store`. So a program run under
//! [`Emit::NOTHING`] is the interpreter's own run, and the same program under
//! [`Emit::EVERYTHING`] must leave the same file, the same window, the same
//! counters and the same memory. That is the instruction-scale identity the
//! machine-scale oracle (`scripts/emu/v3-oracle.sh`) repeats on the classic.
//!
//! The harness re-enters translated code the way the hart's slice loop does:
//! a pc the block set holds is entered, any other pc is stepped on the hart,
//! until the pc leaves the program (a return to [`STOP`], or a trap to the
//! vectors).
//!
//! # The memory
//!
//! One `GuestArena`, laid out as the classic's driver lays the real one out:
//! the exchange area, the permission table, the indirect-target tables, and
//! the guest's own RAM at [`GUEST_BASE`]. The guest RAM is three regions,
//! with the same rules the classic's bus gives SRAM0, DRAM and an MMIO
//! window:
//!
//! | region | guest | rule | permission byte |
//! |---|---|---|---|
//! | code | `+0x0_0000..+0x1_0000` | executable, **word-only**, publishes | [`PERM_READ_WORD`] |
//! | data | `+0x1_0000..+0x3_0000` | any width, any alignment | `PERM_READ_WRITE` |
//! | mmio | `+0x3_0000..+0x3_4000` | one device word, scripted side-band/yield | `PERM_NONE` |
#![allow(
    dead_code,
    reason = "one harness for four suites; each uses a different subset of it"
)]

use std::collections::BTreeSet;

use lp_emu_core::arena::GuestArena;
use lp_emu_core::bus::{Bus, Watchpoint};
use lp_emu_core::memory::{MemoryAccessKind, MemoryError};
use lp_emu_core::{CycleModel, StoreWatch};
use lp_emu_jit::dispatch::{target_table_bytes, write_target_tables};
use lp_emu_jit::host::{
    FLAG_SLICE_ENDED, HostOps, MMIO_LEAVE_AFTER, MMIO_OK, MMIO_PENDING, MMIO_REFUSED,
    MMIO_SLICE_ENDED, MmioLoad, MmioStore, PERM_ENTRIES, PERM_NONE, PERM_READ_WRITE, PERM_SHIFT,
    Polled, STEP_CONTINUE, STEP_SLICE_ENDED, StepOne,
};
use lp_emu_jit::host_wasmtime::WasmtimeCore;
use lp_emu_jit::translate::Layout;
use lp_xt_emu::mach::interrupt::IntLine;
use lp_xt_emu::mach::sr::{PS_BOOT, PS_EXCM, PS_WOE};
use lp_xt_emu::mach::trap::NUM_INTERRUPTS;
use lp_xt_emu::mach::{CoreConfig, XtHart};
use lp_xt_inst::{Inst, Reg};
use lp_xt_jit::blocks::BlockSet;
use lp_xt_jit::discover::{Bounds, Extent, discover_from};
use lp_xt_jit::replay::case::{self, Call};
use lp_xt_jit::replay::{Window, marshal};
use lp_xt_jit::translate::mem::PERM_READ_WORD;
use lp_xt_jit::translate::{Emit, emit_module, known_lends};

pub const EXCHANGE_AT: u32 = 0;
pub const PERM_AT: u32 = 0x1000;
pub const IND_AT: u32 = PERM_AT + PERM_ENTRIES;
pub const ARENA_AT: u32 = 0x20_0000;
pub const RAM_LEN: u32 = 0x4_0000;
pub const PAGES: u64 = 36;

pub const GUEST_BASE: u32 = 0x4000_0000;
pub const CODE_LEN: u32 = 0x1_0000;
pub const DATA_BASE: u32 = GUEST_BASE + CODE_LEN;
pub const DATA_LEN: u32 = 0x2_0000;
pub const MMIO_BASE: u32 = DATA_BASE + DATA_LEN;
pub const MMIO_LEN: u32 = 0x4000;
/// The device word every MMIO access reads and writes.
pub const DEVICE: u32 = MMIO_BASE + 0x40;
/// `VECBASE`: inside the code region, past the program.
pub const VECBASE: u32 = GUEST_BASE + 0x8000;
/// Where a program is assembled.
pub const PROGRAM_AT: u32 = GUEST_BASE + 0x100;
/// Where a program's literals may be placed (`l32r` is backward-only).
pub const LITERALS_AT: u32 = GUEST_BASE + 0x80;
/// A return address outside the program: the harness stops there.
pub const STOP: u32 = GUEST_BASE + 0xF000;
/// A data area programs may use.
pub const DATA: u32 = DATA_BASE + 0x1000;
/// The stack pointer programs start with.
pub const SP: u32 = DATA_BASE + 0x1_0000;

pub fn a(n: u8) -> Reg {
    Reg::new(n)
}

/// The `l32r` field that names `target` from an instruction at `pc`.
pub fn l32r_field(pc: u32, target: u32) -> u16 {
    let base = pc.wrapping_add(3) & !3;
    let words = (base - target) / 4;
    (0x1_0000 - words) as u16
}

// --- the bus ---------------------------------------------------------------

/// RAM over the arena's guest region, with the three regions' rules and a
/// scripted device.
pub struct RamBus {
    ram: *mut u8,
    /// The device word.
    pub device: u32,
    /// An MMIO load leaves a yield on the bus (`MMIO_PENDING`).
    pub load_leaves_yield: bool,
    /// An MMIO store raises the side-band.
    pub store_raises_sideband: bool,
    yield_pending: bool,
    sideband: bool,
    watch_code_stores: bool,
    code_dirty: Vec<(u32, u32)>,
    watchpoints: [Option<Watchpoint>; 2],
    /// Every MMIO access, `(is_store, address)`.
    pub mmio: Vec<(bool, u32)>,
}

impl RamBus {
    fn offset(&self, address: u32, len: u32, kind: MemoryAccessKind) -> Result<usize, MemoryError> {
        let end = u64::from(address) + u64::from(len);
        if address < GUEST_BASE || end > u64::from(GUEST_BASE) + u64::from(RAM_LEN) {
            return Err(MemoryError::InvalidAccess {
                address,
                size: len as usize,
                kind,
            });
        }
        // The word-only rule of the code region, for **data** accesses.
        if kind != MemoryAccessKind::InstructionFetch
            && address < DATA_BASE
            && (len != 4 || address % 4 != 0)
        {
            return Err(MemoryError::InvalidAccess {
                address,
                size: len as usize,
                kind,
            });
        }
        Ok((address - GUEST_BASE) as usize)
    }

    fn bytes(&self) -> &mut [u8] {
        // SAFETY: the arena outlives the bus and nothing else holds a
        // reference into this span while the bus is used.
        unsafe { std::slice::from_raw_parts_mut(self.ram, RAM_LEN as usize) }
    }

    fn is_mmio(address: u32) -> bool {
        (MMIO_BASE..MMIO_BASE + MMIO_LEN).contains(&address)
    }

    fn read(&mut self, address: u32, len: u32) -> Result<u32, MemoryError> {
        if Self::is_mmio(address) {
            self.mmio.push((false, address));
            if self.load_leaves_yield {
                self.yield_pending = true;
            }
            return Ok(if address == DEVICE { self.device } else { 0 });
        }
        let i = self.offset(address, len, MemoryAccessKind::Read)?;
        let b = self.bytes();
        let mut v = 0u32;
        for k in 0..len as usize {
            v |= u32::from(b[i + k]) << (8 * k);
        }
        Ok(v)
    }

    fn write(&mut self, address: u32, len: u32, v: u32) -> Result<(), MemoryError> {
        if Self::is_mmio(address) {
            self.mmio.push((true, address));
            if address == DEVICE {
                self.device = v;
            }
            if self.store_raises_sideband {
                self.sideband = true;
            }
            return Ok(());
        }
        let i = self.offset(address, len, MemoryAccessKind::Write)?;
        if self.watch_code_stores && address < DATA_BASE {
            self.code_dirty.push((address, address + len));
        }
        let b = self.bytes();
        for k in 0..len as usize {
            b[i + k] = (v >> (8 * k)) as u8;
        }
        Ok(())
    }

    pub fn write_u32(&mut self, address: u32, v: u32) {
        let i = (address - GUEST_BASE) as usize;
        self.bytes()[i..i + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn read_u32(&self, address: u32) -> u32 {
        let i = (address - GUEST_BASE) as usize;
        u32::from_le_bytes(self.bytes()[i..i + 4].try_into().unwrap())
    }
}

impl Bus for RamBus {
    fn fetch_instruction(&mut self, address: u32) -> Result<u32, MemoryError> {
        let i = self.offset(address, 4, MemoryAccessKind::InstructionFetch)?;
        Ok(u32::from_le_bytes(
            self.bytes()[i..i + 4].try_into().unwrap(),
        ))
    }
    fn fetch_bytes(&mut self, pc: u32, out: &mut [u8; 3]) -> Result<usize, MemoryError> {
        let i = self.offset(pc, 1, MemoryAccessKind::InstructionFetch)?;
        let n = (RAM_LEN as usize - i).min(3);
        out[..n].copy_from_slice(&self.bytes()[i..i + n]);
        Ok(n)
    }
    fn read_word(&mut self, address: u32) -> Result<i32, MemoryError> {
        self.read(address, 4).map(|v| v as i32)
    }
    fn read_halfword(&mut self, address: u32) -> Result<i16, MemoryError> {
        self.read(address, 2).map(|v| v as i16)
    }
    fn read_byte(&mut self, address: u32) -> Result<i8, MemoryError> {
        self.read(address, 1).map(|v| v as i8)
    }
    fn read_u8(&mut self, address: u32) -> Result<u8, MemoryError> {
        self.read(address, 1).map(|v| v as u8)
    }
    fn write_word(&mut self, address: u32, value: i32) -> Result<(), MemoryError> {
        self.write(address, 4, value as u32)
    }
    fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), MemoryError> {
        self.write(address, 2, value as u16 as u32)
    }
    fn write_byte(&mut self, address: u32, value: i8) -> Result<(), MemoryError> {
        self.write(address, 1, value as u8 as u32)
    }
    fn set_watchpoint(&mut self, slot: usize, wp: Option<Watchpoint>) {
        self.watchpoints[slot] = wp;
    }
    fn take_sideband(&mut self) -> bool {
        std::mem::take(&mut self.sideband)
    }
    fn take_yield(&mut self) -> bool {
        std::mem::take(&mut self.yield_pending)
    }
    fn sideband_or_yield_pending(&self) -> bool {
        self.sideband || self.yield_pending
    }
    fn fetch_is_pure(&self) -> bool {
        true
    }
    fn watch_code_stores(&mut self, on: bool) {
        self.watch_code_stores |= on;
    }
    fn code_dirty(&self) -> bool {
        !self.code_dirty.is_empty()
    }
    fn take_code_dirty(&mut self) -> Vec<(u32, u32)> {
        std::mem::take(&mut self.code_dirty)
    }
    fn store_watch(&self) -> StoreWatch {
        StoreWatch::None
    }
}

// --- the host --------------------------------------------------------------

pub struct Host {
    pub hart: XtHart<RamBus>,
    pub bus: RamBus,
    mem: *mut u8,
    /// The pcs handed to `step_one`, in order.
    pub escapes: Vec<u32>,
    pub slice_end: Option<lp_xt_emu::mach::SliceEnd>,
    /// Every import call's answer, for the engine case.
    pub calls: Vec<Call>,
}

// SAFETY: the pointer is only dereferenced from inside a `WasmtimeCore::enter`
// call on the thread that set it, and the arena outlives the core.
unsafe impl Send for Host {}

/// The exchange area, as a slice with no borrow of the host: the hart and
/// the exchange are two disjoint places and a marshal touches both.
fn exchange_slice<'a>(mem: *mut u8) -> &'a mut [u8] {
    // SAFETY: the exchange area is inside an allocation that outlives the
    // host and never moves, and nothing else aliases it while a marshal runs.
    unsafe {
        std::slice::from_raw_parts_mut(
            mem.add(EXCHANGE_AT as usize),
            lp_xt_jit::LAYOUT.len() as usize,
        )
    }
}

impl Host {
    fn exchange_bytes(&self) -> &mut [u8] {
        exchange_slice(self.mem)
    }

    fn ram_bytes(&self) -> &[u8] {
        // SAFETY: as above.
        unsafe { std::slice::from_raw_parts(self.mem.add(ARENA_AT as usize), RAM_LEN as usize) }
    }

    /// Exchange → hart, all of it.
    pub fn take_state(&mut self) {
        let x = exchange_slice(self.mem);
        let (lbeg, lend, lcount) = marshal::read_state(x, self.hart.cpu_mut());
        let sr = self.hart.sr_mut();
        sr.lbeg = lbeg;
        sr.lend = lend;
        sr.lcount = lcount;
    }

    /// Hart → exchange, all of it.
    pub fn give_state(&mut self) {
        let sr = self.hart.sr();
        let (lbeg, lend, lcount) = (sr.lbeg, sr.lend, sr.lcount);
        marshal::write_state(self.exchange_bytes(), self.hart.cpu(), lbeg, lend, lcount);
    }

    /// Polling point (c), as the hart runs it, with the state a stay keeps
    /// current in the exchange area marshalled in first.
    fn polling_point(&mut self, post_pc: u32, post_cycle: u64, post_instret: u64) -> Polled {
        let x = exchange_slice(self.mem);
        let (lbeg, lend, lcount) = marshal::read_polled_state(x, self.hart.cpu_mut());
        let sr = self.hart.sr_mut();
        sr.lbeg = lbeg;
        sr.lend = lend;
        sr.lcount = lcount;
        self.hart.set_pc(post_pc);
        self.hart.set_counters(post_cycle, post_instret);
        let polled = self.hart.poll_after_store(&mut self.bus);
        if polled.yielded {
            self.slice_end = Some(lp_xt_emu::mach::SliceEnd::BusYield);
            Polled {
                status: MMIO_SLICE_ENDED,
                pc: self.hart.pc(),
            }
        } else if polled.code_dirty || self.hart.pc() != post_pc || !self.bus.fetch_is_pure() {
            Polled {
                status: MMIO_LEAVE_AFTER,
                pc: self.hart.pc(),
            }
        } else {
            Polled {
                status: MMIO_OK,
                pc: post_pc,
            }
        }
    }
}

impl HostOps for Host {
    fn layout(&self) -> lp_emu_jit::host::ExchangeLayout {
        lp_xt_jit::LAYOUT
    }

    fn step_one_wide(&mut self, pc: u32, cycle: u64, instret: u64) -> StepOne {
        self.escapes.push(pc);
        let before = self.ram_bytes().to_vec();
        self.take_state();
        self.hart.set_pc(pc);
        self.hart.set_counters(cycle, instret);
        let ended = self.hart.step_one(&mut self.bus);
        self.give_state();
        let out = StepOne {
            pc: self.hart.pc(),
            cycle: self.hart.cycle_count(),
            instret: self.hart.instruction_count(),
            status: if ended.is_some() {
                STEP_SLICE_ENDED
            } else {
                STEP_CONTINUE
            },
        };
        self.slice_end = ended;
        let mem = case::granules(&before, self.ram_bytes(), ARENA_AT);
        self.calls.push(Call::Step {
            pc: out.pc,
            cycle: out.cycle,
            instret: out.instret,
            status: out.status,
            words: marshal::words(self.exchange_bytes()),
            mem,
        });
        out
    }

    fn step_one(&mut self, _pc: u32, _cycle: u64, _instret: u64, _regs: &mut [i32; 32]) -> StepOne {
        unreachable!("an Xtensa host escapes through `step_one_wide`")
    }

    fn mmio_load(&mut self, _pc: u32, _cycle: u64, address: u32, kind: u32) -> MmioLoad {
        use lp_emu_jit::host::load_kind;
        let read = match kind {
            load_kind::W => self.bus.read_word(address).map(|v| v as u32),
            load_kind::H => self.bus.read_halfword(address).map(|v| i32::from(v) as u32),
            load_kind::HU => self.bus.read_halfword(address).map(|v| u32::from(v as u16)),
            load_kind::B => self.bus.read_byte(address).map(|v| i32::from(v) as u32),
            load_kind::BU => self.bus.read_u8(address).map(u32::from),
            other => unreachable!("{other} is not a load kind"),
        };
        let out = match read {
            Ok(value) => MmioLoad {
                value,
                status: if self.bus.sideband_or_yield_pending() {
                    MMIO_PENDING
                } else {
                    MMIO_OK
                },
            },
            Err(_) => MmioLoad {
                value: 0,
                status: MMIO_REFUSED,
            },
        };
        self.calls.push(Call::Load(
            (i64::from(out.status) << 32) | i64::from(out.value),
        ));
        out
    }

    fn mmio_store(
        &mut self,
        _pc: u32,
        _cycle: u64,
        address: u32,
        kind: u32,
        value: u32,
        post_pc: u32,
        post_cycle: u64,
        post_instret: u64,
    ) -> MmioStore {
        use lp_emu_jit::host::store_kind;
        let written = match kind {
            store_kind::B => self.bus.write_byte(address, value as i8),
            store_kind::H => self.bus.write_halfword(address, value as i16),
            store_kind::W => self.bus.write_word(address, value as i32),
            other => unreachable!("{other} is not a store kind"),
        };
        let out = match written {
            Ok(()) => self.polling_point(post_pc, post_cycle, post_instret),
            Err(_) => Polled {
                status: MMIO_REFUSED,
                pc: post_pc,
            },
        };
        self.calls.push(Call::Store(
            (i64::from(out.status) << 32) | i64::from(out.pc),
        ));
        out
    }

    fn poll(&mut self, pc: u32, cycle: u64, instret: u64) -> Polled {
        let out = self.polling_point(pc, cycle, instret);
        self.calls.push(Call::Poll(
            (i64::from(out.status) << 32) | i64::from(out.pc),
        ));
        out
    }

    fn exchange(&mut self) -> &mut [u8] {
        self.exchange_bytes()
    }
}

// --- the program and the run -----------------------------------------------

/// A program: instructions at [`PROGRAM_AT`], words at [`LITERALS_AT`], and
/// a setup closure that seeds the hart and the bus.
pub struct Program {
    pub insts: Vec<Inst>,
    pub literals: Vec<u32>,
    pub setup: Box<dyn Fn(&mut XtHart<RamBus>, &mut RamBus)>,
    pub model: CycleModel,
    /// Extra block starts the walk should seed from, past the program's
    /// own entry (a `jx` target the edges cannot see).
    pub extra_seeds: Vec<u32>,
    /// The most hart steps or entries before the harness gives up.
    pub budget: usize,
}

impl Program {
    pub fn new(insts: Vec<Inst>) -> Self {
        Self {
            insts,
            literals: Vec::new(),
            setup: Box::new(|_, _| {}),
            model: CycleModel::InstructionCount,
            extra_seeds: Vec::new(),
            budget: 20_000,
        }
    }

    pub fn literals(mut self, words: Vec<u32>) -> Self {
        self.literals = words;
        self
    }

    pub fn setup(mut self, f: impl Fn(&mut XtHart<RamBus>, &mut RamBus) + 'static) -> Self {
        self.setup = Box::new(f);
        self
    }

    pub fn model(mut self, model: CycleModel) -> Self {
        self.model = model;
        self
    }

    pub fn seeds(mut self, seeds: Vec<u32>) -> Self {
        self.extra_seeds = seeds;
        self
    }

    /// The pc after each instruction, so a test can name a target.
    pub fn pcs(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut pc = PROGRAM_AT;
        for inst in &self.insts {
            out.push(pc);
            pc += lp_xt_inst::encode(inst).len() as u32;
        }
        out.push(pc);
        out
    }
}

/// What a run left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub pc: u32,
    pub ar: [u32; 64],
    pub window: Window,
    pub cycle: u64,
    pub instret: u64,
    pub epc1: u32,
    pub exccause: u32,
    pub excvaddr: u32,
    pub memory_fnv: u64,
    pub device: u32,
}

/// Everything a run produced beyond its outcome.
pub struct Run {
    pub outcome: Outcome,
    /// `(exit pc, why)` per entry into translated code.
    pub exits: Vec<(u32, i32)>,
    pub escapes: Vec<u32>,
    pub native_insts: usize,
    pub escaped_insts: usize,
    pub entries: usize,
    pub hart_steps: usize,
    pub mmio: Vec<(bool, u32)>,
}

fn config() -> CoreConfig {
    CoreConfig {
        reset_pc: PROGRAM_AT,
        reset_vecbase: VECBASE,
        prid: 0xCDCD,
        interrupts: [IntLine::UNUSED; NUM_INTERRUPTS],
    }
}

fn write_permissions(mem: &mut [u8]) {
    let table = &mut mem[PERM_AT as usize..][..PERM_ENTRIES as usize];
    table.fill(PERM_NONE);
    let mut set = |lo: u32, len: u32, perm: u8| {
        let mut page = lo >> PERM_SHIFT;
        while page < (lo + len) >> PERM_SHIFT {
            table[page as usize] = perm;
            page += 1;
        }
    };
    set(GUEST_BASE, CODE_LEN, PERM_READ_WORD);
    set(DATA_BASE, DATA_LEN, PERM_READ_WRITE);
    set(MMIO_BASE, MMIO_LEN, PERM_NONE);
}

/// Run `program` under `policy`, `fn_blocks` blocks a function, until it
/// leaves the program.
///
/// `case_name` names the engine case to write when
/// `LP_EMU_XT_JIT_ENGINE_CASE` is set.
pub fn run(program: &Program, policy: Emit, fn_blocks: usize, case_name: &str) -> Run {
    let mut image = Vec::new();
    for inst in &program.insts {
        image.extend_from_slice(&lp_xt_inst::encode(inst));
    }
    let end = PROGRAM_AT + image.len() as u32;

    let mut mem = GuestArena::zeroed((PAGES as usize) * 65536);
    let guard = mem.guard();
    {
        let m = &mut mem[..];
        let at = (ARENA_AT + PROGRAM_AT - GUEST_BASE) as usize;
        m[at..at + image.len()].copy_from_slice(&image);
        for (i, w) in program.literals.iter().enumerate() {
            let at = (ARENA_AT + LITERALS_AT - GUEST_BASE) as usize + 4 * i;
            m[at..at + 4].copy_from_slice(&w.to_le_bytes());
        }
        write_permissions(m);
    }

    // The walk: the real sweep, bounded by the program's own extent.
    let arena: &[u8] = &mem[..];
    let mut fetch = |pc: u32| {
        pc.checked_sub(GUEST_BASE)
            .and_then(|o| arena.get((ARENA_AT + o) as usize))
            .copied()
    };
    let extents = [Extent {
        start: PROGRAM_AT,
        end,
    }];
    let mut seeds = vec![PROGRAM_AT];
    seeds.extend_from_slice(&program.extra_seeds);
    let found = discover_from(
        &seeds,
        Bounds {
            extents: &extents,
            spans: &[(GUEST_BASE, GUEST_BASE + CODE_LEN)],
        },
        usize::MAX,
        &BTreeSet::new(),
        &mut fetch,
    );
    let set: &BlockSet = &found.set;
    assert!(!set.is_empty(), "the program walked to nothing");
    let lends = known_lends(set);

    let ind_len = target_table_bytes(set);
    assert!(
        u64::from(IND_AT) + ind_len <= u64::from(ARENA_AT),
        "the indirect tables ({ind_len} B) overrun the arena's guest region"
    );
    write_target_tables(&mut mem[..], 0, IND_AT, set);

    let layout = Layout {
        memory_pages: PAGES,
        guest_base: GUEST_BASE,
        arena_offset: ARENA_AT,
        perm_offset: PERM_AT,
        exchange_offset: EXCHANGE_AT,
        indirect: Some(IND_AT),
        fast_reads: None,
    };
    let emitted = emit_module(set, program.model, layout, policy, fn_blocks);

    let base = mem.as_mut_ptr();
    let mut hart = XtHart::new(0, config());
    hart.set_cycle_model(program.model);
    hart.set_ps_raw(PS_BOOT);
    hart.set_block_cache(false);
    let mut bus = RamBus {
        // SAFETY: the guest region is inside the arena, which outlives the
        // bus.
        ram: unsafe { base.add(ARENA_AT as usize) },
        device: 0,
        load_leaves_yield: false,
        store_raises_sideband: false,
        yield_pending: false,
        sideband: false,
        watch_code_stores: false,
        code_dirty: Vec::new(),
        watchpoints: [None, None],
        mmio: Vec::new(),
    };
    bus.watch_code_stores(true);
    (program.setup)(&mut hart, &mut bus);
    hart.set_pc(PROGRAM_AT);

    let host = Host {
        hart,
        bus,
        mem: base,
        escapes: Vec::new(),
        slice_end: None,
        calls: Vec::new(),
    };
    // SAFETY: `mem` outlives `core`, and nothing else holds a reference into
    // it while `enter` is running.
    let mut core = unsafe { WasmtimeCore::new(&emitted.wasm, host, base, mem.len(), guard) }
        .expect("the emitted module compiles and instantiates");

    let case_dir = std::env::var("LP_EMU_XT_JIT_ENGINE_CASE").ok();
    let live_ranges = |core: &mut WasmtimeCore<Host>| -> Vec<(u32, Vec<u8>)> {
        let x = core.ops_mut().exchange_bytes().to_vec();
        let m = core.ops_mut().mem;
        // SAFETY: the arena's own bytes, at the offsets the layout names.
        let slice = |at: u32, len: u32| unsafe {
            std::slice::from_raw_parts(m.add(at as usize), len as usize).to_vec()
        };
        vec![
            (EXCHANGE_AT, x),
            (PERM_AT, slice(PERM_AT, PERM_ENTRIES)),
            (IND_AT, slice(IND_AT, ind_len as u32)),
            (ARENA_AT, slice(ARENA_AT, RAM_LEN)),
        ]
    };
    let mut initial: Option<Vec<(u32, Vec<u8>)>> = None;
    let mut shadow: Vec<u8> = Vec::new();
    let mut entries: Vec<case::Entry> = Vec::new();
    let mut final_ranges: Vec<(u32, Vec<u8>)> = Vec::new();

    let mut exits = Vec::new();
    let mut hart_steps = 0usize;
    let mut entered = 0usize;
    let mut budget = program.budget;
    loop {
        budget -= 1;
        assert!(
            budget > 0,
            "the program did not leave in {} steps",
            program.budget
        );
        let h = core.ops_mut();
        let pc = h.hart.pc();
        if !(PROGRAM_AT..end).contains(&pc) {
            break;
        }
        let ps = h.hart.ps();
        let can_enter = ps & PS_WOE != 0
            && ps & PS_EXCM == 0
            && (h.hart.sr().lcount == 0 || lends.contains(&h.hart.sr().lend));
        let entry = set.index.get(&pc).copied().filter(|_| can_enter);
        let Some(entry) = entry else {
            h.hart.step_one(&mut h.bus);
            hart_steps += 1;
            continue;
        };
        entered += 1;
        let (cycle, instret) = (h.hart.cycle_count(), h.hart.instruction_count());
        h.give_state();
        h.calls.clear();
        h.slice_end = None;
        let words_in = marshal::words(h.exchange_bytes());
        let delta = if case_dir.is_some() {
            let now = h.ram_bytes().to_vec();
            let d = if initial.is_none() {
                initial = Some(live_ranges(&mut core));
                Vec::new()
            } else {
                case::granules(&shadow, &now, ARENA_AT)
            };
            shadow = now;
            d
        } else {
            Vec::new()
        };
        let end_cycle = u64::MAX;
        let exit = core
            .enter(entry as u32, cycle, instret, end_cycle, (0, 0))
            .expect("translated code does not trap");
        let h = core.ops_mut();
        let x = h.exchange_bytes();
        let read_i64 =
            |x: &[u8], at: u64| u64::from_le_bytes(x[at as usize..][..8].try_into().unwrap());
        let read_i32 =
            |x: &[u8], at: u64| i32::from_le_bytes(x[at as usize..][..4].try_into().unwrap());
        let cycle_out = read_i64(x, lp_xt_jit::LAYOUT.cycle());
        let instret_out = read_i64(x, lp_xt_jit::LAYOUT.instret());
        let flags = read_i32(x, lp_xt_jit::LAYOUT.flags());
        let why = read_i32(x, lp_xt_jit::LAYOUT.exit_why());
        let words_out = marshal::words(x);
        h.take_state();
        h.hart.set_pc(exit.pc);
        h.hart.set_counters(cycle_out, instret_out);
        exits.push((exit.pc, why));
        if std::env::var_os("LP_EMU_XT_JIT_TRACE").is_some() {
            eprintln!(
                "  entry {entry} @ {pc:#010x} -> {:#010x} why {why} cycle {cycle}->{cycle_out} instret {instret}->{instret_out}",
                exit.pc
            );
        }
        if case_dir.is_some() {
            entries.push(case::Entry {
                entry: entry as u32,
                cycle,
                instret,
                end: end_cycle,
                watch_lo: 0,
                watch_hi: 0,
                words_in,
                delta,
                calls: std::mem::take(&mut h.calls),
                exit_pc: exit.pc,
                flags,
                cycle_out,
                instret_out,
                words_out,
            });
            shadow = h.ram_bytes().to_vec();
            final_ranges = live_ranges(&mut core);
        }
        if flags & FLAG_SLICE_ENDED != 0 {
            // The interpreter's own slice end — a bus yield, a break. The
            // harness carries on as the machine would at the next slice.
            let _ = core.ops_mut().slice_end.take();
        }
        // The hart's no-progress rule (`XtHart::run_blocks`): a stay that
        // left where it entered with nothing retired — a refused block, a
        // budget that does not fit — hands that instruction to the
        // interpreter rather than being asked again.
        if exit.pc == pc && instret_out == instret {
            let h = core.ops_mut();
            h.hart.step_one(&mut h.bus);
            hart_steps += 1;
        }
    }

    if let (Some(dir), Some(initial)) = (case_dir, initial) {
        std::fs::create_dir_all(&dir).expect("the case directory");
        let module = format!("{case_name}.wasm");
        std::fs::write(format!("{dir}/{module}"), &emitted.wasm).expect("the module");
        let ranges: Vec<(u32, &[u8])> = initial.iter().map(|(a, b)| (*a, b.as_slice())).collect();
        let finals: Vec<(u32, &[u8])> = final_ranges
            .iter()
            .map(|(a, b)| (*a, b.as_slice()))
            .collect();
        let json = case::json(
            case_name,
            PAGES,
            EXCHANGE_AT,
            &module,
            &ranges,
            &entries,
            &finals,
        );
        std::fs::write(format!("{dir}/{case_name}.json"), json).expect("the case");
    }

    let h = core.ops_mut();
    let cpu = h.hart.cpu();
    let sr = h.hart.sr();
    let mut fnv = case::Fnv::default();
    fnv.update(h.ram_bytes());
    let outcome = Outcome {
        pc: h.hart.pc(),
        ar: cpu.ar,
        window: Window {
            window_base: cpu.window_base,
            window_start: cpu.window_start,
            sar: cpu.sar,
            lbeg: sr.lbeg,
            lend: sr.lend,
            lcount: sr.lcount,
            ps: h.hart.ps(),
        },
        cycle: h.hart.cycle_count(),
        instret: h.hart.instruction_count(),
        epc1: sr.epc[1],
        exccause: sr.exccause,
        excvaddr: sr.excvaddr,
        memory_fnv: fnv.finish(),
        device: h.bus.device,
    };
    Run {
        outcome,
        exits,
        escapes: h.escapes.clone(),
        native_insts: emitted.native_insts,
        escaped_insts: emitted.escaped_insts,
        entries: entered,
        hart_steps,
        mmio: h.bus.mmio.clone(),
    }
}

/// The program on a bare hart — no module, no marshalling, the interpreter
/// stepping from [`PROGRAM_AT`] until the pc leaves the program.
///
/// The third reading: the escape-everything run marshals the head through
/// the exchange area exactly as the emitted one does, so a marshalling or
/// layout mistake would agree with itself. This one cannot.
pub fn pure(program: &Program) -> Outcome {
    let mut image = Vec::new();
    for inst in &program.insts {
        image.extend_from_slice(&lp_xt_inst::encode(inst));
    }
    let end = PROGRAM_AT + image.len() as u32;
    let mut mem = GuestArena::zeroed((PAGES as usize) * 65536);
    {
        let m = &mut mem[..];
        let at = (ARENA_AT + PROGRAM_AT - GUEST_BASE) as usize;
        m[at..at + image.len()].copy_from_slice(&image);
        for (i, w) in program.literals.iter().enumerate() {
            let at = (ARENA_AT + LITERALS_AT - GUEST_BASE) as usize + 4 * i;
            m[at..at + 4].copy_from_slice(&w.to_le_bytes());
        }
    }
    let base = mem.as_mut_ptr();
    let mut hart = XtHart::new(0, config());
    hart.set_cycle_model(program.model);
    hart.set_ps_raw(PS_BOOT);
    hart.set_block_cache(false);
    let mut bus = RamBus {
        // SAFETY: the guest region is inside the arena, which outlives the
        // bus.
        ram: unsafe { base.add(ARENA_AT as usize) },
        device: 0,
        load_leaves_yield: false,
        store_raises_sideband: false,
        yield_pending: false,
        sideband: false,
        watch_code_stores: false,
        code_dirty: Vec::new(),
        watchpoints: [None, None],
        mmio: Vec::new(),
    };
    bus.watch_code_stores(true);
    (program.setup)(&mut hart, &mut bus);
    hart.set_pc(PROGRAM_AT);
    let mut budget = program.budget;
    while (PROGRAM_AT..end).contains(&hart.pc()) {
        budget -= 1;
        assert!(budget > 0, "the bare hart did not leave in {} steps", program.budget);
        hart.step_one(&mut bus);
    }
    let cpu = hart.cpu();
    let sr = hart.sr();
    let mut fnv = case::Fnv::default();
    // SAFETY: the arena's own guest bytes.
    fnv.update(unsafe { std::slice::from_raw_parts(base.add(ARENA_AT as usize), RAM_LEN as usize) });
    Outcome {
        pc: hart.pc(),
        ar: cpu.ar,
        window: Window {
            window_base: cpu.window_base,
            window_start: cpu.window_start,
            sar: cpu.sar,
            lbeg: sr.lbeg,
            lend: sr.lend,
            lcount: sr.lcount,
            ps: hart.ps(),
        },
        cycle: hart.cycle_count(),
        instret: hart.instruction_count(),
        epc1: sr.epc[1],
        exccause: sr.exccause,
        excvaddr: sr.excvaddr,
        memory_fnv: fnv.finish(),
        device: bus.device,
    }
}

/// The round trip: the emitted module and the escape-everything module
/// agree on everything, at 1, 8 and 64 blocks a function — and both agree
/// with the bare hart. Returns the emitted run at 64 for the test's own
/// assertions.
pub fn agree(name: &str, program: &Program) -> Run {
    let bare = pure(program);
    let mut last = None;
    for fn_blocks in [1usize, 8, 64] {
        let nothing = run(
            program,
            Emit::NOTHING,
            fn_blocks,
            &format!("{name}-nothing-{fn_blocks}"),
        );
        let everything = run(
            program,
            Emit::EVERYTHING,
            fn_blocks,
            &format!("{name}-emitted-{fn_blocks}"),
        );
        assert_eq!(nothing.native_insts, 0, "nothing is emitted under NOTHING");
        assert!(
            everything.native_insts > 0,
            "{name}: the emitted module emitted nothing"
        );
        assert_eq!(
            nothing.outcome, bare,
            "{name} at {fn_blocks} blocks/fn: the escape-everything run diverged from the bare \
             hart — the marshalling, not an arm; exits {:?}",
            nothing.exits
        );
        assert_eq!(
            everything.outcome, nothing.outcome,
            "{name} at {fn_blocks} blocks/fn: the emitted run diverged from the interpreter's; \
             exits {:?} against {:?}",
            everything.exits, nothing.exits
        );
        last = Some(everything);
    }
    last.expect("three sizes ran")
}

/// A `why` an emitted run reported at some exit.
pub fn exited_with(run: &Run, code: i32) -> bool {
    run.exits.iter().any(|&(_, w)| w == code)
}
