//! The privileged hart's conformance claim.
//!
//! Every program here is hand-encoded through [`lp_xt_inst::encode`] and run
//! on a `Bus` written in this file — a flat RAM with two real watchpoint
//! slots, a side-band flag and a stand-in interrupt line — so the tests
//! exercise the same trait boundary the machine's bus will implement. The
//! window handlers used by the deep-recursion test are the ISA RM's own
//! listing (§4.7.1.6), which is also what `xtensa-lx-rt` ships.

use lp_emu_core::{Bus, CycleModel, MemoryAccessKind, MemoryError, Watchpoint};
use lp_xt_inst::{
    AluRrr, AluRs, AtomicLsOp, BrZ, CallOp, Inst, LoadOp, LoopOp, NullaryNarrowOp, NullaryOp, Reg,
    RfOp, SpecialReg, SrOp, StoreOp, UrOp, UserReg, WindowLsOp, encode,
};

use super::extreg::{DCR_ENABLEOCD, XDM_OCD_DCR_CLR, XDM_OCD_DCR_SET, XDM_OCD_DSR};
use super::interrupt::{IntKind, IntLine};
use super::sr::{PS_BOOT, PS_CALLINC_SHIFT, PS_EXCM, PS_INTLEVEL_MASK, PS_RESET, PS_UM, PS_WOE};
use super::trap::{
    NUM_INTERRUPTS, VECOFS_DOUBLE, VECOFS_KERNEL, VECOFS_LEVEL3, VECOFS_LEVEL6_DEBUG, VECOFS_USER,
    VECOFS_WINDOW_OF4, VECOFS_WINDOW_OF8, VECOFS_WINDOW_OF12, VECOFS_WINDOW_UF4, VECOFS_WINDOW_UF8,
    VECOFS_WINDOW_UF12, cause, debugcause,
};
use super::{CoreConfig, HartFault, SliceEnd, XtHart};
use crate::cpu::CPENABLE_FPU;

const RAM_BASE: u32 = 0x4000_0000;
const RAM_LEN: usize = 0x4000;
/// The vector table sits at the bottom of RAM, 1 KiB aligned.
const VEC: u32 = RAM_BASE;
/// Where programs start.
const CODE: u32 = RAM_BASE + 0x1000;
/// A data word the tests load, store and watch.
const DATA: u32 = RAM_BASE + 0x2000;
/// The address whose accesses raise the bus's side-band.
const MMIO: u32 = RAM_BASE + 0x2100;
/// Stack top, 16-aligned, with room for a few hundred bytes of frames.
const STACK_TOP: u32 = RAM_BASE + RAM_LEN as u32 - 16;
/// Unmapped, well away from RAM.
const NOWHERE: u32 = 0x6000_0000;

// The lines the tests configure — the classic's shape (esp-hal's
// `CpuInterrupt` table), which is core configuration and therefore a
// constructor argument, never a constant in the hart.
const IRQ_LEVEL_L1: u8 = 0;
const IRQ_TIMER0_L1: u8 = 6;
const IRQ_SOFTWARE_L1: u8 = 7;
const IRQ_TIMER1_L3: u8 = 15;
const IRQ_LEVEL_L2: u8 = 19;
const IRQ_EDGE_L3: u8 = 22;
const IRQ_LEVEL_L3: u8 = 23;
const IRQ_SOFTWARE_L3: u8 = 29;
const IRQ_LEVEL_L5: u8 = 31;

fn config() -> CoreConfig {
    let mut interrupts = [IntLine::UNUSED; NUM_INTERRUPTS];
    interrupts[usize::from(IRQ_LEVEL_L1)] = IntLine::new(1, IntKind::Level);
    interrupts[usize::from(IRQ_TIMER0_L1)] = IntLine::new(1, IntKind::Timer(0));
    interrupts[usize::from(IRQ_SOFTWARE_L1)] = IntLine::new(1, IntKind::Software);
    interrupts[usize::from(IRQ_TIMER1_L3)] = IntLine::new(3, IntKind::Timer(1));
    interrupts[usize::from(IRQ_LEVEL_L2)] = IntLine::new(2, IntKind::Level);
    interrupts[usize::from(IRQ_EDGE_L3)] = IntLine::new(3, IntKind::Edge);
    interrupts[usize::from(IRQ_LEVEL_L3)] = IntLine::new(3, IntKind::Level);
    interrupts[usize::from(IRQ_SOFTWARE_L3)] = IntLine::new(3, IntKind::Software);
    interrupts[usize::from(IRQ_LEVEL_L5)] = IntLine::new(5, IntKind::Level);
    CoreConfig {
        reset_pc: CODE,
        reset_vecbase: VEC,
        prid: 0xCDCD,
        interrupts,
    }
}

// --- the bus double ---------------------------------------------------------

/// Flat RAM at [`RAM_BASE`] with two real watchpoint slots and a side-band
/// flag raised by any access to [`MMIO`]. Deliberately **not** `Clone`: the
/// hart must be snapshottable without it.
struct TestBus {
    ram: Vec<u8>,
    watchpoints: [Option<Watchpoint>; 2],
    sideband: bool,
    /// The stand-in for an interrupt matrix: what
    /// [`Bus::pending_cpu_interrupt`] answers.
    pending: Option<u8>,
    /// When set, what [`Bus::pending_cpu_interrupt_mask`] answers instead
    /// of the trait default (`pending` widened to a mask) — the shape of an
    /// Xtensa matrix, which answers the single-line form `None` and the mask
    /// form with the whole asserted set.
    pending_mask: Option<u32>,
    /// When set, an access to [`MMIO`] latches this into `pending`.
    raise_on_mmio: Option<u8>,
    /// When clear, an MMIO access raises no side-band even though it may
    /// have raised `pending` — the negative poll-point test.
    sideband_enabled: bool,
}

impl TestBus {
    fn new() -> Self {
        Self {
            ram: vec![0u8; RAM_LEN],
            watchpoints: [None; 2],
            sideband: false,
            pending: None,
            pending_mask: None,
            raise_on_mmio: None,
            sideband_enabled: true,
        }
    }

    fn offset(
        &self,
        address: u32,
        size: u32,
        kind: MemoryAccessKind,
    ) -> Result<usize, MemoryError> {
        let end = u64::from(address) + u64::from(size);
        if address < RAM_BASE || end > u64::from(RAM_BASE) + RAM_LEN as u64 {
            return Err(MemoryError::InvalidAccess {
                address,
                size: size as usize,
                kind,
            });
        }
        if address % size != 0 {
            return Err(MemoryError::Unaligned {
                address,
                alignment: size as usize,
            });
        }
        Ok((address - RAM_BASE) as usize)
    }

    fn region(wp: &Watchpoint) -> (u32, u32) {
        if !wp.napot {
            return (wp.address, 1);
        }
        let ones = wp.address.trailing_ones().min(30);
        let len = 1u32 << (ones + 1);
        (wp.address & !(len - 1), len)
    }

    fn check(&self, address: u32, size: u32, kind: MemoryAccessKind) -> Result<(), MemoryError> {
        for (slot, wp) in self.watchpoints.iter().enumerate() {
            let Some(wp) = wp else { continue };
            let wanted = match kind {
                MemoryAccessKind::Read => wp.on_load,
                MemoryAccessKind::Write => wp.on_store,
                MemoryAccessKind::InstructionFetch => wp.on_execute,
            };
            if !wanted {
                continue;
            }
            let (base, len) = Self::region(wp);
            if address < base.wrapping_add(len) && base < address.wrapping_add(size) {
                return Err(MemoryError::Watchpoint {
                    address,
                    kind,
                    slot: slot as u8,
                });
            }
        }
        Ok(())
    }

    fn note_access(&mut self, address: u32) {
        if address == MMIO {
            if self.sideband_enabled {
                self.sideband = true;
            }
            if let Some(n) = self.raise_on_mmio {
                self.pending = Some(n);
            }
        }
    }

    fn word_at(&self, address: u32) -> u32 {
        let i = (address - RAM_BASE) as usize;
        u32::from_le_bytes(self.ram[i..i + 4].try_into().unwrap())
    }

    fn set_word(&mut self, address: u32, v: u32) {
        let i = (address - RAM_BASE) as usize;
        self.ram[i..i + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn read(&mut self, address: u32, size: u32) -> Result<u32, MemoryError> {
        let i = self.offset(address, size, MemoryAccessKind::Read)?;
        self.check(address, size, MemoryAccessKind::Read)?;
        self.note_access(address);
        let mut v = 0u32;
        for k in 0..size as usize {
            v |= u32::from(self.ram[i + k]) << (8 * k);
        }
        Ok(v)
    }

    fn write(&mut self, address: u32, size: u32, v: u32) -> Result<(), MemoryError> {
        let i = self.offset(address, size, MemoryAccessKind::Write)?;
        self.check(address, size, MemoryAccessKind::Write)?;
        self.note_access(address);
        for k in 0..size as usize {
            self.ram[i + k] = (v >> (8 * k)) as u8;
        }
        Ok(())
    }
}

impl Bus for TestBus {
    fn fetch_instruction(&mut self, address: u32) -> Result<u32, MemoryError> {
        let i = self.offset(address, 4, MemoryAccessKind::InstructionFetch)?;
        Ok(u32::from_le_bytes(self.ram[i..i + 4].try_into().unwrap()))
    }
    fn fetch_bytes(&mut self, pc: u32, out: &mut [u8; 3]) -> Result<usize, MemoryError> {
        let i = self.offset(pc, 1, MemoryAccessKind::InstructionFetch)?;
        let n = (RAM_LEN - i).min(3);
        out[..n].copy_from_slice(&self.ram[i..i + n]);
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
        core::mem::take(&mut self.sideband)
    }
    fn pending_cpu_interrupt(&self) -> Option<u8> {
        self.pending
    }
    fn pending_cpu_interrupt_mask(&self) -> u32 {
        self.pending_mask
            .unwrap_or_else(|| self.pending.map_or(0, |n| 1 << (n & 31)))
    }
}

// --- the assembler ---------------------------------------------------------

fn a(n: u8) -> Reg {
    Reg::new(n)
}

/// Place instructions back to back from `at`; returns the address after the
/// last one.
fn asm(bus: &mut TestBus, at: u32, insts: &[Inst]) -> u32 {
    let mut pc = at;
    for inst in insts {
        let bytes = encode(inst);
        let i = (pc - RAM_BASE) as usize;
        bus.ram[i..i + bytes.len()].copy_from_slice(&bytes);
        pc += bytes.len() as u32;
    }
    pc
}

fn wsr(reg: SpecialReg, r: u8) -> Inst {
    Inst::Sr(SrOp::Wsr, reg, a(r))
}
fn rsr(reg: SpecialReg, r: u8) -> Inst {
    Inst::Sr(SrOp::Rsr, reg, a(r))
}
fn movi(r: u8, v: i32) -> Inst {
    Inst::Movi(a(r), v)
}
fn addi(rt: u8, rs: u8, v: i32) -> Inst {
    Inst::Addi(a(rt), a(rs), v)
}
fn l32i(rt: u8, rs: u8, off: u32) -> Inst {
    Inst::Load(LoadOp::L32i, a(rt), a(rs), off)
}
fn s32i(rt: u8, rs: u8, off: u32) -> Inst {
    Inst::Store(StoreOp::S32i, a(rt), a(rs), off)
}
fn nop() -> Inst {
    Inst::Nullary(NullaryOp::Nop)
}
fn ill() -> Inst {
    Inst::Nullary(NullaryOp::Ill)
}
fn brk() -> Inst {
    Inst::Break(0, 0)
}
fn retw() -> Inst {
    Inst::Nullary(NullaryOp::Retw)
}
fn s32e(at: u8, r#as: u8, off: i32) -> Inst {
    Inst::WindowLs(WindowLsOp::S32e, a(at), a(r#as), off)
}
fn l32e(at: u8, r#as: u8, off: i32) -> Inst {
    Inst::WindowLs(WindowLsOp::L32e, a(at), a(r#as), off)
}

/// Install the ISA RM's six window handlers (§4.7.1.6) at `VEC`.
fn install_window_handlers(bus: &mut TestBus) {
    asm(
        bus,
        VEC + VECOFS_WINDOW_OF4,
        &[
            s32e(0, 5, -16),
            s32e(1, 5, -12),
            s32e(2, 5, -8),
            s32e(3, 5, -4),
            Inst::Rf(RfOp::Rfwo),
        ],
    );
    asm(
        bus,
        VEC + VECOFS_WINDOW_UF4,
        &[
            l32e(0, 5, -16),
            l32e(1, 5, -12),
            l32e(2, 5, -8),
            l32e(3, 5, -4),
            Inst::Rf(RfOp::Rfwu),
        ],
    );
    asm(
        bus,
        VEC + VECOFS_WINDOW_OF8,
        &[
            s32e(0, 9, -16),
            l32e(0, 1, -12),
            s32e(1, 9, -12),
            s32e(2, 9, -8),
            s32e(3, 9, -4),
            s32e(4, 0, -32),
            s32e(5, 0, -28),
            s32e(6, 0, -24),
            s32e(7, 0, -20),
            Inst::Rf(RfOp::Rfwo),
        ],
    );
    asm(
        bus,
        VEC + VECOFS_WINDOW_UF8,
        &[
            l32e(0, 9, -16),
            l32e(1, 9, -12),
            l32e(2, 9, -8),
            l32e(7, 1, -12),
            l32e(3, 9, -4),
            l32e(4, 7, -32),
            l32e(5, 7, -28),
            l32e(6, 7, -24),
            l32e(7, 7, -20),
            Inst::Rf(RfOp::Rfwu),
        ],
    );
    asm(
        bus,
        VEC + VECOFS_WINDOW_OF12,
        &[
            s32e(0, 13, -16),
            l32e(0, 1, -12),
            s32e(1, 13, -12),
            s32e(2, 13, -8),
            s32e(3, 13, -4),
            s32e(4, 0, -48),
            s32e(5, 0, -44),
            s32e(6, 0, -40),
            s32e(7, 0, -36),
            s32e(8, 0, -32),
            s32e(9, 0, -28),
            s32e(10, 0, -24),
            s32e(11, 0, -20),
            Inst::Rf(RfOp::Rfwo),
        ],
    );
    asm(
        bus,
        VEC + VECOFS_WINDOW_UF12,
        &[
            l32e(0, 13, -16),
            l32e(1, 13, -12),
            l32e(2, 13, -8),
            l32e(11, 1, -12),
            l32e(3, 13, -4),
            l32e(4, 11, -48),
            l32e(5, 11, -44),
            l32e(6, 11, -40),
            l32e(7, 11, -36),
            l32e(8, 11, -32),
            l32e(9, 11, -28),
            l32e(10, 11, -24),
            l32e(11, 11, -20),
            Inst::Rf(RfOp::Rfwu),
        ],
    );
}

/// A fresh hart and bus.
fn fresh() -> (XtHart<TestBus>, TestBus) {
    (XtHart::new(0, config()), TestBus::new())
}

/// A hart as a direct-load machine leaves it: PS seeded, SP set, at `CODE`.
fn booted() -> (XtHart<TestBus>, TestBus) {
    let (mut hart, bus) = fresh();
    hart.set_ps_raw(PS_BOOT);
    hart.cpu_mut().set_a(1, STACK_TOP);
    (hart, bus)
}

fn ps_intlevel(hart: &XtHart<TestBus>) -> u32 {
    hart.ps() & PS_INTLEVEL_MASK
}

fn excm(hart: &XtHart<TestBus>) -> bool {
    hart.ps() & PS_EXCM != 0
}

// --- 1. reset state --------------------------------------------------------

#[test]
fn reset_state_is_the_architectural_one() {
    let (hart, _) = fresh();
    assert_eq!(hart.ps(), PS_RESET, "PS = 0x1F: INTLEVEL 15, EXCM 1, WOE 0");
    assert_eq!(hart.ps(), 0x0000_001F);
    assert_eq!(hart.sr().vecbase, VEC);
    assert_eq!(hart.pc(), CODE);
    assert_eq!(hart.cpu().window_base, 0);
    assert_eq!(hart.cpu().window_start, 1);
    assert_eq!(hart.cpu().cpenable, 0);
    assert_eq!(hart.timers().ccompare, [0; 3]);
    assert_eq!(hart.breakpoints().ibreakenable, 0);
    assert_eq!(hart.cycle_count(), 0);
    assert_eq!(hart.instruction_count(), 0);
}

// --- 2. set_ps_raw + entry, and ruling R3 ------------------------------------

#[test]
fn seeded_ps_lets_entry_rotate() {
    let (mut hart, mut bus) = booted();
    asm(&mut bus, CODE, &[Inst::Entry(a(1), 16), brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: CODE + 3 }
    );
    assert_eq!(hart.cpu().window_base, 2, "rotated by PS.CALLINC = 2");
    assert_eq!(hart.cpu().window_start, 0b101);
    assert_eq!(
        hart.cpu().a(1),
        STACK_TOP - 16,
        "the callee's SP, in the new window"
    );
    assert!(!excm(&hart), "no fault");
}

/// R3: the RM (ENTRY page) — "ENTRY is undefined if PS.WOE is 0 or if
/// PS.EXCM is 1. Some implementations raise an illegal instruction exception
/// in these cases, as a debugging aid." This hart is one of those
/// implementations: a hart left at the reset PS raises on the app's first
/// instruction instead of silently not rotating.
#[test]
fn entry_with_woe_clear_is_an_illegal_instruction() {
    let (mut hart, mut bus) = fresh();
    assert_eq!(hart.ps(), PS_RESET, "WOE clear, EXCM set");
    asm(&mut bus, CODE, &[Inst::Entry(a(1), 16)]);
    hart.run_slice(&mut bus, 1);
    // EXCM was already 1, so this lands on the double-exception vector.
    assert_eq!(hart.sr().exccause, cause::ILLEGAL_INSTRUCTION);
    assert_eq!(hart.sr().depc, CODE);
    assert_eq!(hart.pc(), VEC + VECOFS_DOUBLE);
    assert_eq!(hart.cpu().window_base, 0, "no rotation happened");

    // And with EXCM clear but WOE still clear: the general vector.
    let (mut hart, mut bus) = fresh();
    hart.set_ps_raw(PS_UM | (2 << PS_CALLINC_SHIFT));
    asm(&mut bus, CODE, &[Inst::Entry(a(1), 16)]);
    hart.run_slice(&mut bus, 1);
    assert_eq!(hart.sr().exccause, cause::ILLEGAL_INSTRUCTION);
    assert_eq!(hart.sr().epc[1], CODE);
    assert_eq!(hart.pc(), VEC + VECOFS_USER);

    // RETW under the same conditions, and with a0[31:30] = 0.
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().set_a(0, 0x0000_1234);
    asm(&mut bus, CODE, &[retw()]);
    hart.run_slice(&mut bus, 1);
    assert_eq!(hart.sr().exccause, cause::ILLEGAL_INSTRUCTION);
    assert_eq!(hart.sr().epc[1], CODE);
}

// --- 3. exception entry ---------------------------------------------------

/// For each cause: EPC1, EXCCAUSE, EXCVADDR only for the memory causes,
/// PS.EXCM set, PS.INTLEVEL unchanged, the vector by PS.UM, a0 untouched.
#[test]
fn exception_entry_writes_exactly_what_the_vectors_read() {
    struct Case {
        cause: u32,
        program: Vec<Inst>,
        vaddr: Option<u32>,
        setup: fn(&mut XtHart<TestBus>),
    }
    let cases = [
        Case {
            cause: cause::ILLEGAL_INSTRUCTION,
            program: vec![ill()],
            vaddr: None,
            setup: |_| {},
        },
        Case {
            cause: cause::SYSCALL,
            program: vec![Inst::Nullary(NullaryOp::Syscall)],
            vaddr: None,
            setup: |_| {},
        },
        Case {
            cause: cause::LOAD_STORE_ERROR,
            program: vec![l32i(3, 2, 0)],
            vaddr: Some(NOWHERE),
            setup: |h| {
                h.cpu_mut().set_a(2, NOWHERE);
            },
        },
        Case {
            cause: cause::ALLOCA,
            program: vec![Inst::Rs(AluRs::Movsp, a(1), a(2))],
            vaddr: None,
            // Frame at base 4 with no caller bits below it.
            setup: |h| {
                h.cpu_mut().window_base = 4;
                h.cpu_mut().window_start = 1 << 4;
            },
        },
        Case {
            cause: cause::LOAD_STORE_ALIGNMENT,
            program: vec![l32i(3, 2, 0)],
            vaddr: Some(DATA + 2),
            setup: |h| {
                h.cpu_mut().set_a(2, DATA + 2);
            },
        },
        Case {
            cause: cause::COPROCESSOR0_DISABLED,
            program: vec![Inst::Ur(UrOp::Rur, UserReg::Fcr, a(3))],
            vaddr: None,
            setup: |_| {},
        },
    ];
    for um in [true, false] {
        for case in &cases {
            let (mut hart, mut bus) = booted();
            let mut ps = hart.ps() | 0x2; // INTLEVEL 2, to see it survive
            if !um {
                ps &= !PS_UM;
            }
            hart.set_ps_raw(ps);
            hart.sr_mut().excvaddr = 0xDEAD_BEEF;
            (case.setup)(&mut hart);
            // After the setup: a case may move the window, and a0 is a
            // window register.
            hart.cpu_mut().set_a(0, 0xA0A0_0000);
            asm(&mut bus, CODE, &case.program);
            assert_eq!(hart.run_slice(&mut bus, 1), SliceEnd::BudgetExhausted);
            let label = format!("cause {} um={um}", case.cause);
            assert_eq!(hart.sr().epc[1], CODE, "{label}: EPC1 is the faulting pc");
            assert_eq!(hart.sr().exccause, case.cause, "{label}: EXCCAUSE");
            match case.vaddr {
                Some(v) => assert_eq!(hart.sr().excvaddr, v, "{label}: EXCVADDR"),
                None => assert_eq!(
                    hart.sr().excvaddr,
                    0xDEAD_BEEF,
                    "{label}: EXCVADDR untouched"
                ),
            }
            assert!(excm(&hart), "{label}: PS.EXCM set");
            assert_eq!(ps_intlevel(&hart), 2, "{label}: PS.INTLEVEL unchanged");
            let want = VEC + if um { VECOFS_USER } else { VECOFS_KERNEL };
            assert_eq!(hart.pc(), want, "{label}: vector by PS.UM");
            assert_eq!(hart.cpu().a(0), 0xA0A0_0000, "{label}: a0 untouched");
            assert_eq!(hart.instruction_count(), 0, "{label}: nothing retired");
        }
    }
}

// --- 4. double exception --------------------------------------------------

#[test]
fn a_fault_inside_a_handler_takes_the_double_vector_with_depc() {
    let (mut hart, mut bus) = booted();
    hart.set_ps_raw(hart.ps() | PS_EXCM);
    hart.sr_mut().epc[1] = 0x1111_1111;
    asm(&mut bus, CODE, &[ill()]);
    hart.run_slice(&mut bus, 1);
    assert_eq!(hart.pc(), VEC + VECOFS_DOUBLE);
    assert_eq!(hart.sr().depc, CODE, "DEPC, not EPC1");
    assert_eq!(hart.sr().epc[1], 0x1111_1111, "EPC1 untouched");
    assert_eq!(hart.sr().exccause, cause::ILLEGAL_INSTRUCTION);
    // rfde returns to DEPC and leaves EXCM set.
    asm(&mut bus, VEC + VECOFS_DOUBLE, &[Inst::Rf(RfOp::Rfde)]);
    hart.run_slice(&mut bus, 1);
    assert_eq!(hart.pc(), CODE);
    assert!(excm(&hart));
}

// --- 5. vector-fetch fault -------------------------------------------------

#[test]
fn a_vector_in_unmapped_memory_is_a_hart_fault() {
    let (mut hart, mut bus) = booted();
    hart.sr_mut().vecbase = NOWHERE;
    asm(&mut bus, CODE, &[ill()]);
    assert_eq!(
        hart.run_slice(&mut bus, 10),
        SliceEnd::Fault(HartFault::TrapVectorFetch {
            vector: NOWHERE + VECOFS_USER
        })
    );
}

// --- 6. level-1 interrupt is cause 4 -----------------------------------------

#[test]
fn a_level1_interrupt_enters_the_general_vector_with_cause_4() {
    let (mut hart, mut bus) = booted();
    hart.sr_mut().excvaddr = 0x5555_5555;
    asm(&mut bus, CODE, &[nop(), nop()]);
    asm(
        &mut bus,
        VEC + VECOFS_USER,
        &[rsr(SpecialReg::Exccause, 2), brk()],
    );
    hart.cpu_mut().set_a(0, 0x0000_A000);
    // INTENABLE is a CPU register: the hart, not a bus, admits the line.
    hart.set_external_mask(1 << IRQ_LEVEL_L1);
    assert!(!hart.poll_interrupts(), "not enabled yet");
    let intenable_write = [movi(2, 1 << IRQ_LEVEL_L1), wsr(SpecialReg::Intenable, 2)];
    let after = asm(&mut bus, CODE, &intenable_write);
    asm(&mut bus, after, &[nop(), nop()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + VECOFS_USER + 3
        }
    );
    assert_eq!(hart.sr().exccause, cause::LEVEL1_INTERRUPT);
    assert_eq!(
        hart.sr().epc[1],
        after,
        "EPC1 is the instruction that had not run"
    );
    assert_eq!(hart.cpu().a(2), 4, "the vector read EXCCAUSE = 4");
    assert!(excm(&hart));
    assert_eq!(
        ps_intlevel(&hart),
        0,
        "INTLEVEL untouched by a level-1 entry"
    );
    assert_eq!(hart.sr().excvaddr, 0x5555_5555);
    assert_eq!(hart.cpu().a(0), 0x0000_A000, "a0 untouched");
}

// --- 7. levels 2..7 --------------------------------------------------------

#[test]
fn a_level3_interrupt_uses_its_vector_and_rfi_restores_exactly() {
    let (mut hart, mut bus) = booted();
    let ps_before = hart.ps();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L3;
    asm(&mut bus, CODE, &[nop(), nop(), brk()]);
    asm(&mut bus, VEC + VECOFS_LEVEL3, &[Inst::Rfi(3)]);
    hart.set_external_mask(1 << IRQ_LEVEL_L3);
    // (a): taken on entry to run_slice, before the first nop.
    assert!(hart.poll_interrupts());
    assert_eq!(hart.sr().epc[3], CODE);
    assert_eq!(hart.sr().eps[3], ps_before);
    assert_eq!(ps_intlevel(&hart), 3);
    assert!(excm(&hart));
    assert_eq!(hart.pc(), VEC + 0x1C0);
    // The level line is still asserted, but level 3 <= INTLEVEL 3 masks it,
    // so rfi returns to the nops and the slice runs to the break.
    hart.set_external_mask(0);
    let r = hart.run_slice(&mut bus, 100);
    assert_eq!(r, SliceEnd::Ebreak { pc: CODE + 6 });
    assert_eq!(hart.ps(), ps_before, "rfi 3 restored PS exactly");
    assert_eq!(hart.instruction_count(), 3, "rfi + two nops");
}

// --- 8. masking -----------------------------------------------------------

#[test]
fn masking_by_intenable_intlevel_and_excm() {
    // (a) INTENABLE clear.
    let (mut hart, _bus) = booted();
    hart.set_external_mask(1 << IRQ_LEVEL_L3);
    assert!(!hart.poll_interrupts());

    // (b) level <= INTLEVEL.
    let (mut hart, _bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L3;
    hart.set_ps_raw(hart.ps() | 3);
    hart.set_external_mask(1 << IRQ_LEVEL_L3);
    assert!(!hart.poll_interrupts(), "level 3 at INTLEVEL 3");
    hart.set_ps_raw((hart.ps() & !PS_INTLEVEL_MASK) | 2);
    assert!(hart.poll_interrupts(), "level 3 at INTLEVEL 2");

    // (c) EXCM masks everything at or below EXCM_LEVEL = 3, and nothing
    // above.
    let (mut hart, _bus) = booted();
    hart.interrupts_mut().intenable = (1 << IRQ_LEVEL_L3) | (1 << IRQ_LEVEL_L5);
    hart.set_ps_raw(hart.ps() | PS_EXCM);
    hart.set_external_mask(1 << IRQ_LEVEL_L3);
    assert!(!hart.poll_interrupts(), "level 3 under EXCM");
    hart.set_external_mask((1 << IRQ_LEVEL_L3) | (1 << IRQ_LEVEL_L5));
    assert!(hart.poll_interrupts(), "level 5 is above EXCM_LEVEL");
    assert_eq!(ps_intlevel(&hart), 5);

    // And taken as soon as the mask clears — at poll point (b), a wsr.ps.
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L3;
    hart.set_ps_raw(hart.ps() | 3);
    hart.set_external_mask(1 << IRQ_LEVEL_L3);
    let after = asm(
        &mut bus,
        CODE,
        &[movi(2, (PS_BOOT & 0xFFF) as i32), wsr(SpecialReg::Ps, 2)],
    );
    asm(&mut bus, after, &[brk()]);
    asm(&mut bus, VEC + VECOFS_LEVEL3, &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + VECOFS_LEVEL3
        },
        "delivered before the break after the wsr.ps retired"
    );
    assert_eq!(hart.sr().epc[3], after);
}

// --- 9. the poll points ----------------------------------------------------

#[test]
fn poll_point_a_on_entry_to_run_slice() {
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L2;
    hart.set_external_mask(1 << IRQ_LEVEL_L2);
    asm(&mut bus, CODE, &[nop()]);
    asm(&mut bus, VEC + super::trap::level_vecofs(2), &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + super::trap::level_vecofs(2)
        }
    );
    assert_eq!(hart.sr().epc[2], CODE, "nothing ran first");
}

#[test]
fn poll_point_b_after_a_wsr_intenable() {
    let (mut hart, mut bus) = booted();
    hart.set_external_mask(1 << IRQ_LEVEL_L2);
    hart.cpu_mut().set_a(2, 1 << IRQ_LEVEL_L2);
    let after = asm(&mut bus, CODE, &[wsr(SpecialReg::Intenable, 2)]);
    asm(&mut bus, after, &[movi(3, 77), brk()]);
    asm(&mut bus, VEC + super::trap::level_vecofs(2), &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + super::trap::level_vecofs(2)
        }
    );
    assert_eq!(hart.sr().epc[2], after);
    assert_eq!(hart.cpu().a(3), 0, "the movi after the wsr had not retired");
    assert_eq!(hart.instruction_count(), 1);
}

#[test]
fn poll_point_c_a_store_side_band_replaces_the_mask() {
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L2;
    bus.raise_on_mmio = Some(IRQ_LEVEL_L2);
    hart.cpu_mut().set_a(2, MMIO);
    let after = asm(&mut bus, CODE, &[s32i(3, 2, 0)]);
    asm(&mut bus, after, &[movi(4, 1), brk()]);
    asm(&mut bus, VEC + super::trap::level_vecofs(2), &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + super::trap::level_vecofs(2)
        }
    );
    assert_eq!(hart.sr().epc[2], after, "delivered right after the store");
    assert_eq!(
        hart.external_mask(),
        1 << IRQ_LEVEL_L2,
        "the mask came from the bus"
    );
    assert_eq!(hart.cpu().a(4), 0);

    // A store that lowers the line stops it being pending in the same
    // breath: the bus answers None after the second store.
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L2;
    hart.set_ps_raw(hart.ps() | 2); // masked for now
    hart.set_external_mask(1 << IRQ_LEVEL_L2);
    bus.pending = None; // the matrix has dropped the line
    hart.cpu_mut().set_a(2, MMIO);
    let after = asm(&mut bus, CODE, &[s32i(3, 2, 0)]);
    let after = asm(
        &mut bus,
        after,
        &[movi(2, (PS_BOOT & 0xFFF) as i32), wsr(SpecialReg::Ps, 2)],
    );
    asm(&mut bus, after, &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: after }
    );
    assert_eq!(
        hart.external_mask(),
        0,
        "the side-band replaced the stale mask"
    );
}

/// **M4 P3b (R6).** Poll point (c) reads the matrix's **mask**, not its
/// single-line answer. An Xtensa matrix answers [`Bus::pending_cpu_interrupt`]
/// `None` — the enables live in this hart — so a store that read the
/// single-line form would zero the mask and drop every line the machine fed
/// at the slice boundary. The shape that found it: a line fed at the
/// boundary while `PS.INTLEVEL` masks it, a store to MMIO inside the
/// critical section (esp-rtos raising its own yield through DPORT), then
/// `wsr PS` lowering the level — the interrupt must arrive at that `wsr`,
/// not at the next slice boundary.
#[test]
fn poll_point_c_reads_the_mask_form_and_keeps_the_lines_the_machine_fed() {
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L2;
    // Inside a critical section: level 2 is masked.
    hart.set_ps_raw(hart.ps() | 2);
    // The machine's feed at the slice boundary.
    hart.set_external_mask(1 << IRQ_LEVEL_L2);
    // The Xtensa matrix's two answers: nothing to name, everything asserted.
    bus.pending = None;
    bus.pending_mask = Some(1 << IRQ_LEVEL_L2);
    hart.cpu_mut().set_a(2, MMIO);
    let after_store = asm(&mut bus, CODE, &[s32i(3, 2, 0)]);
    let after_wsr = asm(
        &mut bus,
        after_store,
        &[movi(2, (PS_BOOT & 0xFFF) as i32), wsr(SpecialReg::Ps, 2)],
    );
    asm(&mut bus, after_wsr, &[movi(4, 1), brk()]);
    asm(&mut bus, VEC + super::trap::level_vecofs(2), &[brk()]);

    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + super::trap::level_vecofs(2)
        },
        "the level-2 line the machine fed survived the store's side-band"
    );
    assert_eq!(
        hart.external_mask(),
        1 << IRQ_LEVEL_L2,
        "the mask came from the bus's mask form, not its single-line None"
    );
    assert_eq!(
        hart.sr().epc[2],
        after_wsr,
        "delivered at the `wsr PS` that lowered the level, poll point (b)"
    );
    assert_eq!(hart.cpu().a(4), 0, "and before the next instruction retired");

    // The same store on a bus that only implements the single-line form
    // keeps the trait default's widening: `Some(n)` is line `n`.
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L2;
    bus.raise_on_mmio = Some(IRQ_LEVEL_L2);
    hart.cpu_mut().set_a(2, MMIO);
    let after = asm(&mut bus, CODE, &[s32i(3, 2, 0)]);
    asm(&mut bus, after, &[brk()]);
    asm(&mut bus, VEC + super::trap::level_vecofs(2), &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + super::trap::level_vecofs(2)
        }
    );
    assert_eq!(hart.external_mask(), 1 << IRQ_LEVEL_L2);
}

#[test]
fn poll_point_d_and_the_negative_case() {
    // Asserted mid-slice by the bus, with no side-band: not taken until the
    // machine polls at (d).
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L2;
    bus.sideband_enabled = false;
    bus.raise_on_mmio = Some(IRQ_LEVEL_L2);
    hart.cpu_mut().set_a(2, MMIO);
    let after = asm(&mut bus, CODE, &[s32i(3, 2, 0), nop(), nop()]);
    asm(&mut bus, after, &[brk()]);
    asm(&mut bus, VEC + super::trap::level_vecofs(2), &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: after }
    );
    assert_eq!(bus.pending, Some(IRQ_LEVEL_L2), "the matrix has it");
    assert!(!hart.poll_interrupts(), "the hart was never told");
    // (d): the machine reads the matrix at its scheduler event.
    hart.set_external_mask(1 << bus.pending.unwrap());
    assert!(hart.poll_interrupts());
    assert_eq!(hart.sr().epc[2], after);
}

// --- 10. windows as exceptions -----------------------------------------------

#[test]
fn an_entry_that_wraps_the_ring_vectors_by_the_callees_increment() {
    for (callee_inc, vecofs) in [
        (1u8, VECOFS_WINDOW_OF4),
        (2, VECOFS_WINDOW_OF8),
        (3, VECOFS_WINDOW_OF12),
    ] {
        let (mut hart, mut bus) = booted();
        // Frames: an old one at base 0 whose callee sits at `callee_inc`,
        // and the current one at base 14. CALLINC = 2 makes the new frame's
        // group 14 + 2 = 0 (mod 16), which the old frame owns.
        hart.cpu_mut().window_base = 14;
        hart.cpu_mut().window_start = (1 << 14) | 1 | (1 << callee_inc);
        hart.cpu_mut().set_a(1, STACK_TOP);
        asm(&mut bus, CODE, &[Inst::Entry(a(1), 16)]);
        asm(&mut bus, VEC + vecofs, &[brk()]);
        assert_eq!(
            hart.run_slice(&mut bus, 10),
            SliceEnd::Ebreak { pc: VEC + vecofs }
        );
        assert_eq!(hart.cpu().window_base, 0, "rotated to the victim (m)");
        assert_eq!((hart.ps() >> 8) & 0xF, 14, "PS.OWB = the old base");
        assert_eq!(hart.sr().epc[1], CODE);
        assert!(excm(&hart));
        assert_eq!(hart.instruction_count(), 0, "the entry has not retired");

        // rfwo: the spilled frame's bit clears, WindowBase returns, and the
        // entry re-executes — now without conflict.
        asm(&mut bus, VEC + vecofs, &[Inst::Rf(RfOp::Rfwo)]);
        asm(&mut bus, CODE + 3, &[brk()]);
        hart.set_pc(VEC + vecofs);
        assert_eq!(
            hart.run_slice(&mut bus, 10),
            SliceEnd::Ebreak { pc: CODE + 3 }
        );
        assert!(!excm(&hart));
        assert_eq!(hart.cpu().window_base, 0, "14 + CALLINC 2 wraps to 0");
        assert_eq!(
            hart.cpu().window_start,
            (1 << 14) | (1 << callee_inc) | 1,
            "rfwo cleared the victim's bit; the entry set its own in the same place"
        );
        assert_eq!(hart.instruction_count(), 2, "rfwo and the entry");
    }
}

#[test]
fn a_reference_to_an_occupied_register_overflows_and_retries() {
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().window_base = 15;
    // Old frame at base 0 (its callee at 2, call8), current at 15. a4 is
    // group 15 + 1 = 0.
    hart.cpu_mut().window_start = (1 << 15) | 1 | (1 << 2);
    asm(&mut bus, CODE, &[movi(4, 9), brk()]);
    asm(&mut bus, VEC + VECOFS_WINDOW_OF8, &[Inst::Rf(RfOp::Rfwo)]);
    assert_eq!(
        hart.run_slice(&mut bus, 10),
        SliceEnd::Ebreak { pc: CODE + 3 }
    );
    assert_eq!(hart.cpu().a(4), 9);
    assert_eq!(hart.cpu().window_base, 15);
    assert_eq!(
        hart.cpu().window_start,
        (1 << 15) | (1 << 2),
        "frame 0 spilled"
    );
    // a movi to a3 in the same state needs no spill.
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().window_base = 15;
    hart.cpu_mut().window_start = (1 << 15) | 1 | (1 << 2);
    asm(&mut bus, CODE, &[movi(3, 9), brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 10),
        SliceEnd::Ebreak { pc: CODE + 3 }
    );
    assert_eq!(hart.cpu().window_start, (1 << 15) | 1 | (1 << 2));
}

#[test]
fn a_retw_into_a_spilled_frame_underflows_and_rfwu_reloads() {
    let (mut hart, mut bus) = booted();
    let ret = CODE + 0x100;
    hart.cpu_mut().window_base = 2;
    hart.cpu_mut().window_start = 1 << 2; // the caller at 0 is not resident
    hart.cpu_mut().set_a(0, (2 << 30) | (ret & 0x3FFF_FFFF));
    asm(&mut bus, CODE, &[retw()]);
    asm(&mut bus, VEC + VECOFS_WINDOW_UF8, &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 10),
        SliceEnd::Ebreak {
            pc: VEC + VECOFS_WINDOW_UF8
        }
    );
    assert_eq!(
        hart.cpu().window_base,
        0,
        "WindowBase left decremented (RM §4.7.1.4)"
    );
    assert_eq!((hart.ps() >> 8) & 0xF, 2, "PS.OWB = the returning frame");
    assert_eq!(hart.sr().epc[1], CODE);
    assert!(excm(&hart));

    asm(&mut bus, VEC + VECOFS_WINDOW_UF8, &[Inst::Rf(RfOp::Rfwu)]);
    asm(&mut bus, ret, &[brk()]);
    hart.set_pc(VEC + VECOFS_WINDOW_UF8);
    assert_eq!(hart.run_slice(&mut bus, 10), SliceEnd::Ebreak { pc: ret });
    assert_eq!(hart.cpu().window_base, 0);
    assert_eq!(
        hart.cpu().window_start,
        1,
        "caller resident, returning frame's bit cleared"
    );
    assert!(!excm(&hart));
}

/// The judge: the same recursive program under the direct model (the
/// user-mode runner, unchanged) and under the exception model with the RM's
/// own handlers gives the same answer, through many wraps of the ring.
#[test]
fn windows_as_exceptions_agree_with_the_direct_model() {
    // f(n) = n == 0 ? 0 : n + f(n - 1), windowed, call8 recursion.
    //   f:  entry a1, 32
    //       beqz a2, zero        ; -> f+18
    //       addi a10, a2, -1
    //       call8 f              ; word offset -3
    //       add a2, a2, a10
    //       retw
    //   zero: movi a2, 0
    //       retw
    let f = || -> Vec<Inst> {
        vec![
            Inst::Entry(a(1), 32),
            Inst::BranchZ(BrZ::Beqz, a(2), 11),
            addi(10, 2, -1),
            Inst::Call(CallOp::Call8, -3),
            Inst::Rrr(AluRrr::Add, a(2), a(2), a(10)),
            retw(),
            movi(2, 0),
            retw(),
        ]
    };
    let depth = 20;
    let want: u32 = (0..=depth).sum();

    // The direct model: the user-mode runner.
    let code: Vec<u8> = f().iter().flat_map(|i| encode(i)).collect();
    let mut emu = crate::Emulator::new();
    assert_eq!(emu.run(&code, 0, depth), crate::RunOutcome::Ok(want));

    // The exception model: main calls f via call4 (so the outermost frame's
    // spill is an OF4, as xtensa-lx-rt's Reset arranges), the RM's handlers
    // do the spilling.
    let (mut hart, mut bus) = booted();
    // The boot frame's callee (main) is entered by a call4, as xtensa-lx-rt's
    // Reset frame is: its spill is then an OF4, which needs no saved caller
    // SP. (An OF8 of the outermost frame would read one that nobody wrote.)
    hart.set_ps_raw(PS_WOE | PS_UM | (1 << PS_CALLINC_SHIFT));
    install_window_handlers(&mut bus);
    let fpc = CODE + 0x40;
    asm(&mut bus, fpc, &f());
    // main at CODE: entry a1, 32; movi a6, depth; call4 f; break
    let call_pc = CODE + 6;
    let off = (fpc as i32 - ((call_pc & !3) as i32 + 4)) >> 2;
    asm(
        &mut bus,
        CODE,
        &[
            Inst::Entry(a(1), 32),
            movi(6, depth as i32),
            Inst::Call(CallOp::Call4, off),
            brk(),
        ],
    );
    assert_eq!(
        hart.run_slice(&mut bus, 100_000),
        SliceEnd::Ebreak { pc: CODE + 9 }
    );
    assert_eq!(hart.cpu().a(6), want, "f({depth}) through the handlers");
    assert!(!excm(&hart));
    assert_eq!(hart.cpu().window_base, 1, "back in main's frame");
    assert_eq!(
        hart.cpu().window_start,
        0b10,
        "main resident; the boot frame was spilled by the ring wrapping and main never \
         returned into it"
    );
}

// --- 11. movsp ---------------------------------------------------------------

#[test]
fn movsp_is_a_move_when_the_caller_is_resident_and_alloca_when_not() {
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().window_base = 4;
    hart.cpu_mut().window_start = (1 << 4) | (1 << 2);
    hart.cpu_mut().set_a(2, 0x1234_5670);
    asm(&mut bus, CODE, &[Inst::Rs(AluRs::Movsp, a(1), a(2)), brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 10),
        SliceEnd::Ebreak { pc: CODE + 3 }
    );
    assert_eq!(hart.cpu().a(1), 0x1234_5670);

    let (mut hart, mut bus) = booted();
    hart.cpu_mut().window_base = 4;
    hart.cpu_mut().window_start = 1 << 4;
    asm(&mut bus, CODE, &[Inst::Rs(AluRs::Movsp, a(1), a(2))]);
    hart.run_slice(&mut bus, 1);
    assert_eq!(hart.sr().exccause, cause::ALLOCA);
    assert_eq!(hart.pc(), VEC + VECOFS_USER);
}

// --- 12. LOOP ----------------------------------------------------------------

#[test]
fn loopnez_runs_the_body_count_times_and_closes_at_lend() {
    let (mut hart, mut bus) = booted();
    // L: loopnez a3, L+6 ; addi a4, a4, 1 ; break
    let l = CODE + 3;
    asm(
        &mut bus,
        CODE,
        &[
            movi(3, 4),
            Inst::Loop(LoopOp::Loopnez, a(3), 2),
            addi(4, 4, 1),
            brk(),
        ],
    );
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: l + 6 }
    );
    assert_eq!(hart.cpu().a(4), 4, "the body ran four times");
    assert_eq!(hart.sr().lcount, 0);
    assert_eq!(hart.sr().lbeg, l + 3);
    assert_eq!(hart.sr().lend, l + 6);

    // loopnez with a zero count skips the body outright.
    let (mut hart, mut bus) = booted();
    asm(
        &mut bus,
        CODE,
        &[
            movi(3, 0),
            Inst::Loop(LoopOp::Loopnez, a(3), 2),
            addi(4, 4, 1),
            brk(),
        ],
    );
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: l + 6 }
    );
    assert_eq!(hart.cpu().a(4), 0);
}

#[test]
fn a_wsr_lcount_mid_loop_changes_the_count() {
    let (mut hart, mut bus) = booted();
    // a5 = 0. The wsr is the loop's last instruction, and the RM computes the
    // loop-back (decrement, next = LBEG) *before* the instruction runs
    // (§3.5.4.1), so the first pass still loops back; the zero it wrote then
    // ends the loop on the second pass. Two iterations, not four.
    let l = CODE + 3;
    asm(
        &mut bus,
        CODE,
        &[
            movi(3, 4),
            Inst::Loop(LoopOp::Loopnez, a(3), 5),
            addi(4, 4, 1),
            wsr(SpecialReg::Lcount, 5),
            brk(),
        ],
    );
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: l + 9 }
    );
    assert_eq!(hart.cpu().a(4), 2);
    assert_eq!(hart.sr().lcount, 0);
}

#[test]
fn a_wsr_lend_moves_where_the_loop_closes() {
    let (mut hart, mut bus) = booted();
    // L: loopnez a3, L+9 ; addi a4 ; wsr.lend a5 (= L+12) ; addi a7 ; break
    // The first pass closes at the original LEND (the loop-back is decided
    // before the wsr runs, RM §3.5.4.1), so the second addi is skipped once;
    // from then on the loop closes at L+12 and a7 counts. Under the original
    // LEND a7 would never run at all.
    let l = CODE + 3;
    hart.cpu_mut().set_a(5, l + 12);
    asm(
        &mut bus,
        CODE,
        &[
            movi(3, 3),
            Inst::Loop(LoopOp::Loopnez, a(3), 5),
            addi(4, 4, 1),
            wsr(SpecialReg::Lend, 5),
            addi(7, 7, 1),
            brk(),
        ],
    );
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: l + 12 }
    );
    assert_eq!(hart.cpu().a(4), 3);
    assert_eq!(
        hart.cpu().a(7),
        2,
        "the moved LEND closed the loop after the second addi"
    );
    assert_eq!(hart.sr().lcount, 0);
}

#[test]
fn the_loop_back_is_disabled_while_excm_is_set() {
    let (mut hart, mut bus) = booted();
    let l = CODE + 3;
    hart.set_ps_raw(hart.ps() | PS_EXCM);
    asm(
        &mut bus,
        CODE,
        &[
            movi(3, 4),
            Inst::Loop(LoopOp::Loopnez, a(3), 2),
            addi(4, 4, 1),
            brk(),
        ],
    );
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: l + 6 }
    );
    assert_eq!(hart.cpu().a(4), 1, "one pass, no loop-back");
    assert_eq!(hart.sr().lcount, 3, "and LCOUNT untouched");
}

// --- 13. s32c1i --------------------------------------------------------------

#[test]
fn s32c1i_stores_only_when_memory_equals_scompare1() {
    let (mut hart, mut bus) = booted();
    bus.set_word(DATA, 0x11);
    hart.cpu_mut().set_a(2, DATA);
    hart.cpu_mut().set_a(3, 0x22);
    asm(
        &mut bus,
        CODE,
        &[
            movi(4, 0x11),
            wsr(SpecialReg::Scompare1, 4),
            Inst::AtomicLs(AtomicLsOp::S32c1i, a(3), a(2), 0),
            brk(),
        ],
    );
    hart.run_slice(&mut bus, 100);
    assert_eq!(bus.word_at(DATA), 0x22, "success: stored");
    assert_eq!(hart.cpu().a(3), 0x11, "the register gets the old value");

    let (mut hart, mut bus) = booted();
    bus.set_word(DATA, 0x99);
    hart.cpu_mut().set_a(2, DATA);
    hart.cpu_mut().set_a(3, 0x22);
    asm(
        &mut bus,
        CODE,
        &[
            movi(4, 0x11),
            wsr(SpecialReg::Scompare1, 4),
            Inst::AtomicLs(AtomicLsOp::S32c1i, a(3), a(2), 0),
            brk(),
        ],
    );
    hart.run_slice(&mut bus, 100);
    assert_eq!(bus.word_at(DATA), 0x99, "failure: not stored");
    assert_eq!(hart.cpu().a(3), 0x99, "the register gets the current value");
}

// --- 14. waiti ---------------------------------------------------------------

#[test]
fn waiti_parks_with_pc_past_it_and_an_interrupt_above_the_level_unparks() {
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = (1 << IRQ_LEVEL_L2) | (1 << IRQ_LEVEL_L5);
    asm(&mut bus, CODE, &[Inst::Waiti(3), brk()]);
    assert_eq!(hart.run_slice(&mut bus, 100), SliceEnd::Wfi);
    assert!(hart.is_waiti());
    assert_eq!(hart.pc(), CODE + 3, "pc past the waiti");
    assert_eq!(ps_intlevel(&hart), 3);

    // A level-2 line does not wake a hart parked at INTLEVEL 3 ...
    hart.set_external_mask(1 << IRQ_LEVEL_L2);
    assert!(!hart.poll_interrupts());
    assert!(hart.is_waiti());
    // ... a level-5 one does, and is delivered in the same breath: on
    // Xtensa the wake condition and the delivery condition are one, because
    // PS.INTLEVEL is both the mask and what waiti sets (there is no
    // separate global enable for the RV32 wake rule to ignore).
    hart.set_external_mask((1 << IRQ_LEVEL_L2) | (1 << IRQ_LEVEL_L5));
    assert!(hart.poll_interrupts());
    assert!(!hart.is_waiti());
    assert_eq!(
        hart.sr().epc[5],
        CODE + 3,
        "EPC5 is the instruction after the waiti"
    );
    assert_eq!(ps_intlevel(&hart), 5);

    // An interrupt already pending when waiti retires un-parks at once.
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L5;
    hart.set_ps_raw(hart.ps() | 6); // masked until waiti lowers the level
    hart.set_external_mask(1 << IRQ_LEVEL_L5);
    asm(&mut bus, CODE, &[Inst::Waiti(0)]);
    asm(&mut bus, VEC + super::trap::level_vecofs(5), &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + super::trap::level_vecofs(5)
        }
    );
    assert!(!hart.is_waiti());
}

// --- 15. break ---------------------------------------------------------------

#[test]
fn break_hands_the_pc_to_the_machine_and_deliver_breakpoint_is_the_debug_exception() {
    let (mut hart, mut bus) = booted();
    asm(&mut bus, CODE, &[nop(), brk(), nop()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: CODE + 3 }
    );
    assert_eq!(hart.pc(), CODE + 3, "not advanced");
    assert_eq!(hart.instruction_count(), 1, "the break did not retire");
    let ps = hart.ps();
    hart.deliver_breakpoint(CODE + 3);
    assert_eq!(hart.pc(), VEC + VECOFS_LEVEL6_DEBUG);
    assert_eq!(hart.sr().epc[6], CODE + 3);
    assert_eq!(hart.sr().eps[6], ps);
    assert_eq!(hart.sr().debugcause, debugcause::BREAK);
    assert_eq!(ps_intlevel(&hart), 6);
    assert!(excm(&hart));
    assert_eq!(hart.instruction_count(), 2, "delivered = retired");

    // break.n sets the BN bit; at INTLEVEL >= DEBUGLEVEL it is a no-op.
    let (mut hart, mut bus) = booted();
    asm(&mut bus, CODE, &[Inst::BreakN(0)]);
    assert_eq!(hart.run_slice(&mut bus, 100), SliceEnd::Ebreak { pc: CODE });
    hart.deliver_breakpoint(CODE);
    assert_eq!(hart.sr().debugcause, debugcause::BREAK_N);
    let (mut hart, mut bus) = booted();
    hart.set_ps_raw(hart.ps() | 6);
    asm(&mut bus, CODE, &[Inst::BreakN(0)]);
    assert_eq!(hart.run_slice(&mut bus, 100), SliceEnd::Ebreak { pc: CODE });
    hart.deliver_breakpoint(CODE);
    assert_eq!(
        hart.pc(),
        CODE + 2,
        "a no-op: stepped past the 2-byte break.n"
    );
}

// --- 16. rsil ----------------------------------------------------------------

#[test]
fn rsil_returns_the_old_ps_and_sets_intlevel() {
    let (mut hart, mut bus) = booted();
    let before = hart.ps();
    asm(&mut bus, CODE, &[Inst::Rsil(a(2), 4), brk()]);
    hart.run_slice(&mut bus, 100);
    assert_eq!(hart.cpu().a(2), before);
    assert_eq!(ps_intlevel(&hart), 4);
    assert_eq!(hart.ps() & !PS_INTLEVEL_MASK, before & !PS_INTLEVEL_MASK);
}

// --- 17. timers --------------------------------------------------------------

#[test]
fn ccount_advances_with_the_cycle_counter_and_a_timer_match_interrupts() {
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_TIMER0_L1;
    // rsr.ccount a2 ; addi a2, a2, 8 ; wsr.ccompare0 a2 ; nop x 12 ; break
    let mut prog = vec![
        rsr(SpecialReg::Ccount, 2),
        addi(2, 2, 8),
        wsr(SpecialReg::Ccompare0, 2),
    ];
    let after_wsr = CODE + 9;
    prog.extend(core::iter::repeat_n(nop(), 12));
    prog.push(brk());
    asm(&mut bus, CODE, &prog);
    asm(&mut bus, VEC + VECOFS_USER, &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 1000),
        SliceEnd::Ebreak {
            pc: VEC + VECOFS_USER
        }
    );
    assert_eq!(hart.sr().exccause, cause::LEVEL1_INTERRUPT);
    // rsr read CCOUNT = 0 at cycle 0; compare = 8; the instruction that
    // brings the counter to 8 is the 5th nop, and delivery happens at that
    // cycle — the slice ended at the timer boundary.
    assert_eq!(hart.cycle_count(), 8);
    assert_eq!(hart.timers().ccount(hart.cycle_count()), 8);
    assert_eq!(hart.sr().epc[1], after_wsr + 5 * 3);
    assert_eq!(
        hart.interrupts().pending() & (1 << IRQ_TIMER0_L1),
        1 << IRQ_TIMER0_L1,
        "remembered"
    );

    // Writing CCOMPARE0 clears the request (and INTCLEAR does not).
    asm(
        &mut bus,
        VEC + VECOFS_USER,
        &[
            movi(3, 1 << IRQ_TIMER0_L1),
            wsr(SpecialReg::Intclear, 3),
            rsr(SpecialReg::Interrupt, 4),
            movi(3, 0),
            wsr(SpecialReg::Ccompare0, 3),
            rsr(SpecialReg::Interrupt, 5),
            brk(),
        ],
    );
    hart.run_slice(&mut bus, 1000);
    assert_eq!(
        hart.cpu().a(4) & (1 << IRQ_TIMER0_L1),
        1 << IRQ_TIMER0_L1,
        "INTCLEAR did not clear a timer"
    );
    assert_eq!(
        hart.cpu().a(5) & (1 << IRQ_TIMER0_L1),
        0,
        "the CCOMPARE write did"
    );

    // CCOUNT is the cycle counter through a writable offset.
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().set_a(2, 1000);
    asm(
        &mut bus,
        CODE,
        &[
            wsr(SpecialReg::Ccount, 2),
            nop(),
            nop(),
            rsr(SpecialReg::Ccount, 3),
            brk(),
        ],
    );
    hart.run_slice(&mut bus, 100);
    assert_eq!(
        hart.cpu().a(3),
        1003,
        "1000 at the wsr, +2 nops, read before the rsr is charged"
    );
    // With CCOMPARE0..2 = 0 and CCOUNT = 1000 at cycle 0, the next match is
    // a full wrap minus 1000 cycles away.
    assert_eq!(hart.next_timer_cycle(), Some((1u64 << 32) - 1000));
}

#[test]
fn advance_to_cycle_lets_a_timer_fire_inside_the_idle_skip() {
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_TIMER1_L3;
    hart.cpu_mut().set_a(2, 500);
    asm(
        &mut bus,
        CODE,
        &[wsr(SpecialReg::Ccompare1, 2), Inst::Waiti(0)],
    );
    assert_eq!(hart.run_slice(&mut bus, 100), SliceEnd::Wfi);
    assert_eq!(hart.next_timer_cycle(), Some(500));
    hart.advance_to_cycle(499);
    assert!(!hart.poll_interrupts(), "not yet");
    hart.advance_to_cycle(600);
    assert!(
        hart.poll_interrupts(),
        "fired during the skip, delivered at (d)"
    );
    assert_eq!(hart.sr().epc[3], CODE + 6);
    assert_eq!(hart.cycle_count(), 600);
}

// --- 18. DBREAK --------------------------------------------------------------

#[test]
fn dbreak_arms_a_bus_watchpoint_and_a_hit_is_the_debug_exception() {
    let (mut hart, mut bus) = booted();
    bus.set_word(DATA, 0x0BAD_F00D);
    hart.cpu_mut().set_a(2, DATA);
    hart.cpu_mut().set_a(4, 0x1234);
    // esp-hal's stack guard: dbreakc = 0b1111100 | STORE, a 4-byte block.
    let dbreakc = 0b111_1100u32 | (1 << 31);
    let store_pc = CODE + 12;
    // movi cannot make bit 31; the register is set directly.
    hart.cpu_mut().set_a(3, dbreakc);
    asm(
        &mut bus,
        CODE,
        &[
            wsr(SpecialReg::Dbreaka0, 2),
            wsr(SpecialReg::Dbreakc0, 3),
            nop(),
            nop(),
        ],
    );
    asm(&mut bus, store_pc, &[s32i(4, 2, 0), brk()]);
    asm(&mut bus, VEC + VECOFS_LEVEL6_DEBUG, &[brk()]);
    let ps = hart.ps();
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + VECOFS_LEVEL6_DEBUG
        }
    );
    assert_eq!(
        bus.watchpoints[0],
        Some(Watchpoint {
            address: DATA | 1,
            napot: true,
            on_store: true,
            on_load: false,
            on_execute: false,
        })
    );
    assert_eq!(hart.pc(), VEC + VECOFS_LEVEL6_DEBUG);
    assert_eq!(hart.sr().epc[6], store_pc, "EPC6 names the store");
    assert_eq!(hart.sr().eps[6], ps);
    assert_eq!(hart.sr().debugcause, debugcause::DBREAK, "DB, DBNUM 0");
    assert_eq!(bus.word_at(DATA), 0x0BAD_F00D, "the store did not happen");
    assert_eq!(ps_intlevel(&hart), 6);

    // A load does not trip a store-only slot.
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().set_a(2, DATA);
    hart.cpu_mut().set_a(3, dbreakc);
    asm(
        &mut bus,
        CODE,
        &[
            wsr(SpecialReg::Dbreaka0, 2),
            wsr(SpecialReg::Dbreakc0, 3),
            l32i(5, 2, 0),
            brk(),
        ],
    );
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: CODE + 9 }
    );

    // An exact-address slot with both directions.
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().set_a(2, DATA + 1);
    hart.cpu_mut().set_a(3, 0b11_1111 | (1 << 31) | (1 << 30));
    asm(
        &mut bus,
        CODE,
        &[
            wsr(SpecialReg::Dbreaka0, 2),
            wsr(SpecialReg::Dbreakc0, 3),
            brk(),
        ],
    );
    hart.run_slice(&mut bus, 100);
    assert_eq!(
        bus.watchpoints[0],
        Some(Watchpoint {
            address: DATA + 1,
            napot: false,
            on_store: true,
            on_load: true,
            on_execute: false,
        })
    );
}

#[test]
fn ibreak_fires_at_fetch() {
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().set_a(2, CODE + 9);
    asm(
        &mut bus,
        CODE,
        &[
            wsr(SpecialReg::Ibreaka0, 2),
            movi(3, 1),
            wsr(SpecialReg::IbreakEnable, 3),
            movi(4, 1),
            brk(),
        ],
    );
    asm(&mut bus, VEC + VECOFS_LEVEL6_DEBUG, &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + VECOFS_LEVEL6_DEBUG
        }
    );
    assert_eq!(hart.pc(), VEC + VECOFS_LEVEL6_DEBUG);
    assert_eq!(hart.sr().epc[6], CODE + 9);
    assert_eq!(hart.sr().debugcause, debugcause::IBREAK);
    assert_eq!(
        hart.cpu().a(4),
        0,
        "the instruction at IBREAKA0 did not run"
    );
}

// --- 19. cycle / instruction accounting ---------------------------------------

#[test]
fn a_trapping_instruction_costs_a_cycle_but_does_not_retire() {
    let (mut hart, mut bus) = booted();
    assert_eq!(hart.cycle_model(), CycleModel::InstructionCount);
    asm(&mut bus, CODE, &[ill()]);
    asm(&mut bus, VEC + VECOFS_USER, &[Inst::Rf(RfOp::Rfe), brk()]);
    // ill: attempted (1 cycle), not retired; entry costs nothing.
    hart.run_slice(&mut bus, 1);
    assert_eq!(hart.cycle_count(), 1);
    assert_eq!(hart.instruction_count(), 0);
    assert_eq!(hart.pc(), VEC + VECOFS_USER);
    // rfe: one instruction, one cycle, nothing extra for the return.
    hart.sr_mut().epc[1] = CODE + 3;
    asm(&mut bus, CODE + 3, &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: CODE + 3 }
    );
    assert_eq!(hart.cycle_count(), 2);
    assert_eq!(hart.instruction_count(), 1);
    assert!(!excm(&hart));
}

// --- 20. clone is the architectural state --------------------------------------

#[test]
fn a_cloned_hart_continues_identically_on_a_non_clone_bus() {
    let (mut hart, mut bus) = booted();
    let prog = [
        movi(2, 5),
        addi(2, 2, 7),
        Inst::Rsil(a(3), 2),
        Inst::Waiti(1),
    ];
    asm(&mut bus, CODE, &prog);
    hart.run_slice(&mut bus, 2);
    // `TestBus` is not `Clone`; the hart is.
    let mut twin = hart.clone();
    let mut bus2 = TestBus::new();
    asm(&mut bus2, CODE, &prog);
    assert_eq!(hart.run_slice(&mut bus, 100), SliceEnd::Wfi);
    assert_eq!(twin.run_slice(&mut bus2, 100), SliceEnd::Wfi);
    assert_eq!(hart.pc(), twin.pc());
    assert_eq!(hart.ps(), twin.ps());
    assert_eq!(hart.cycle_count(), twin.cycle_count());
    assert_eq!(hart.instruction_count(), twin.instruction_count());
    assert_eq!(hart.cpu().ar, twin.cpu().ar);
    assert_eq!(hart.sr(), twin.sr());
    assert_eq!(hart.cpu().a(2), 12);
}

// --- the rest of the surface -------------------------------------------------

#[test]
fn special_register_reads_and_writes_reach_their_state() {
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().set_a(2, 0x55);
    hart.cpu_mut().set_a(3, 0xAA);
    asm(
        &mut bus,
        CODE,
        &[
            rsr(SpecialReg::Prid, 4),
            wsr(SpecialReg::Excsave1, 2),
            Inst::Sr(SrOp::Xsr, SpecialReg::Excsave1, a(3)),
            rsr(SpecialReg::Excsave1, 5),
            rsr(SpecialReg::WindowBase, 6),
            rsr(SpecialReg::WindowStart, 7),
            Inst::Ur(UrOp::Wur, UserReg::Threadptr, a(2)),
            Inst::Ur(UrOp::Rur, UserReg::Threadptr, a(8)),
            movi(9, 0x1F),
            wsr(SpecialReg::Cpenable, 9),
            Inst::Ur(UrOp::Rur, UserReg::Fcr, a(10)),
            brk(),
        ],
    );
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: CODE + 33 }
    );
    assert_eq!(
        hart.cpu().a(4),
        0xCDCD,
        "PRID is the configured chip number"
    );
    assert_eq!(hart.cpu().a(3), 0x55, "xsr returned the old value");
    assert_eq!(hart.cpu().a(5), 0xAA, "and wrote the new");
    assert_eq!(hart.cpu().a(6), 0, "WindowBase: no entry has run");
    assert_eq!(hart.cpu().a(7), 1, "WindowStart: frame 0");
    assert_eq!(hart.cpu().a(8), 0x55, "THREADPTR is a user register");
    assert_eq!(hart.cpu().cpenable & CPENABLE_FPU, CPENABLE_FPU);
    assert_eq!(
        hart.cpu().a(10),
        0,
        "FCR readable once the coprocessor is armed"
    );
    // A read of a write-only register is illegal.
    let (mut hart, mut bus) = booted();
    asm(&mut bus, CODE, &[rsr(SpecialReg::Intclear, 2)]);
    hart.run_slice(&mut bus, 1);
    assert_eq!(hart.sr().exccause, cause::ILLEGAL_INSTRUCTION);
}

#[test]
fn a_software_interrupt_via_intset_is_delivered_at_poll_point_b() {
    let (mut hart, mut bus) = booted();
    hart.interrupts_mut().intenable = 1 << IRQ_SOFTWARE_L3;
    hart.cpu_mut().set_a(2, 1 << IRQ_SOFTWARE_L3);
    let after = asm(&mut bus, CODE, &[wsr(SpecialReg::Interrupt, 2)]);
    asm(&mut bus, after, &[brk()]);
    asm(&mut bus, VEC + VECOFS_LEVEL3, &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak {
            pc: VEC + VECOFS_LEVEL3
        }
    );
    assert_eq!(hart.sr().epc[3], after);
    // INTCLEAR clears it; a level line is not writable.
    hart.set_pc(VEC + VECOFS_LEVEL3);
    hart.cpu_mut()
        .set_a(2, (1 << IRQ_SOFTWARE_L3) | (1 << IRQ_LEVEL_L1));
    asm(
        &mut bus,
        VEC + VECOFS_LEVEL3,
        &[
            wsr(SpecialReg::Intclear, 2),
            rsr(SpecialReg::Interrupt, 3),
            brk(),
        ],
    );
    hart.set_external_mask(1 << IRQ_LEVEL_L1);
    hart.run_slice(&mut bus, 100);
    assert_eq!(
        hart.cpu().a(3),
        1 << IRQ_LEVEL_L1,
        "software cleared, level still asserted"
    );
}

#[test]
fn an_edge_line_latches_on_its_rising_edge_until_intclear() {
    let (mut hart, _bus) = booted();
    hart.set_external_mask(1 << IRQ_EDGE_L3);
    hart.set_external_mask(0);
    assert_eq!(
        hart.interrupts().pending(),
        1 << IRQ_EDGE_L3,
        "latched after the line dropped"
    );
    hart.interrupts_mut().intclear(1 << IRQ_EDGE_L3);
    assert_eq!(hart.interrupts().pending(), 0);
    hart.set_external_mask(1 << IRQ_EDGE_L3);
    assert_eq!(
        hart.interrupts().pending(),
        1 << IRQ_EDGE_L3,
        "a new rising edge"
    );
}

#[test]
fn rotw_and_the_window_registers_are_writable() {
    let (mut hart, mut bus) = booted();
    asm(
        &mut bus,
        CODE,
        &[
            Inst::Rotw(-1),
            rsr(SpecialReg::WindowBase, 2),
            Inst::Rotw(3),
            brk(),
        ],
    );
    hart.run_slice(&mut bus, 100);
    assert_eq!(
        hart.cpu().ar[crate::cpu::Cpu::phys_at(15, 2)],
        15,
        "0 - 1 wraps to 15, read into a2 of that window"
    );
    assert_eq!(hart.cpu().window_base, 2, "15 + 3 = 18 mod 16");
}

#[test]
fn mac16_multiplies_into_a_40_bit_accumulator() {
    use lp_xt_inst::{MReg, MacHalf, MacOp, MacSrc, MacY};
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().set_a(2, 0x0003_FFFF); // low half = -1, high half = 3
    hart.cpu_mut().set_a(3, 0x0007_0002); // low half = 2, high half = 7
    bus.set_word(DATA, 0x0000_0005);
    hart.cpu_mut().set_a(4, DATA - 4);
    asm(
        &mut bus,
        CODE,
        &[
            // acc = (-1) * 2 = -2
            Inst::Mac(MacOp::Mul, MacHalf::Ll, MacSrc::Aa(a(2), a(3))),
            rsr(SpecialReg::Acclo, 5),
            rsr(SpecialReg::Acchi, 6),
            // acc += 3 * 7 = 21 -> 19
            Inst::Mac(MacOp::Mula, MacHalf::Hh, MacSrc::Aa(a(2), a(3))),
            rsr(SpecialReg::Acclo, 7),
            // umul: 0xFFFF * 2 = 0x1FFFE, zero-extended
            Inst::Mac(MacOp::Umul, MacHalf::Ll, MacSrc::Aa(a(2), a(3))),
            rsr(SpecialReg::Acclo, 8),
            // ldinc m0 <- mem[a4 + 4]; a4 += 4
            Inst::MacLoad(false, MReg::new(0), a(4)),
            rsr(SpecialReg::M0, 9),
            // mula.da.ll.ldinc m2, a4, m0, a3: acc += m0.l * a3.l = 5 * 2,
            // using m0 before m2 loads (m2 <- mem[a4 + 4] = 0 here)
            Inst::MacLd(
                false,
                MacHalf::Ll,
                MReg::new(2),
                a(4),
                MReg::new(0),
                MacY::Ar(a(3)),
            ),
            rsr(SpecialReg::Acclo, 10),
            brk(),
        ],
    );
    assert_eq!(
        hart.run_slice(&mut bus, 100),
        SliceEnd::Ebreak { pc: CODE + 33 }
    );
    assert_eq!(hart.cpu().a(5), (-2i32) as u32);
    assert_eq!(hart.cpu().a(6), 0xFFFF_FFFF, "ACCHI sign-extended");
    assert_eq!(hart.cpu().a(7), 19);
    assert_eq!(hart.cpu().a(8), 0x1_FFFE);
    assert_eq!(hart.cpu().a(9), 5);
    assert_eq!(hart.cpu().a(4), DATA + 4);
    assert_eq!(hart.cpu().a(10), 0x1_FFFE + 10);
}

#[test]
fn clamps_and_the_boolean_reductions() {
    use lp_xt_inst::{BReg, BoolAllOp, BoolOp};
    let (mut hart, mut bus) = booted();
    hart.cpu_mut().set_a(2, 200);
    hart.cpu_mut().set_a(3, (-200i32) as u32);
    hart.cpu_mut().set_a(4, 100);
    hart.cpu_mut().br = 0b0000_0000_0000_0110; // b1, b2
    asm(
        &mut bus,
        CODE,
        &[
            Inst::Clamps(a(5), a(2), 7),
            Inst::Clamps(a(6), a(3), 7),
            Inst::Clamps(a(7), a(4), 7),
            Inst::BoolAll(BoolAllOp::Any4, BReg::new(8), BReg::new(0)),
            Inst::BoolAll(BoolAllOp::All4, BReg::new(9), BReg::new(0)),
            Inst::BoolLogic(BoolOp::Xorb, BReg::new(10), BReg::new(1), BReg::new(2)),
            Inst::BoolLogic(BoolOp::Orbc, BReg::new(11), BReg::new(0), BReg::new(0)),
            brk(),
        ],
    );
    hart.run_slice(&mut bus, 100);
    assert_eq!(hart.cpu().a(5), 127);
    assert_eq!(hart.cpu().a(6), (-128i32) as u32);
    assert_eq!(hart.cpu().a(7), 100);
    assert!(hart.cpu().b(8), "any4 of b0..b3");
    assert!(!hart.cpu().b(9), "all4 of b0..b3");
    assert!(!hart.cpu().b(10), "b1 xor b2");
    assert!(hart.cpu().b(11), "b0 or not b0");
}

#[test]
fn the_strict_stop_names_an_undecodable_word() {
    let (mut hart, mut bus) = booted();
    // `ee.*`-shaped garbage the decoder refuses: op0 = 0xE is reserved.
    let i = (CODE - RAM_BASE) as usize;
    bus.ram[i..i + 3].copy_from_slice(&[0x0E, 0x00, 0x00]);
    hart.set_strict_unsupported(true);
    let r = hart.run_slice(&mut bus, 10);
    assert!(
        matches!(r, SliceEnd::Fault(HartFault::UnsupportedInstruction { pc, .. }) if pc == CODE),
        "{r:?}"
    );
    // Off, the architectural answer.
    let (mut hart, mut bus) = booted();
    bus.ram[i..i + 3].copy_from_slice(&[0x0E, 0x00, 0x00]);
    hart.run_slice(&mut bus, 1);
    assert_eq!(hart.sr().exccause, cause::ILLEGAL_INSTRUCTION);
    assert_eq!(hart.pc(), VEC + VECOFS_USER);
}

#[test]
fn an_instruction_running_off_the_mapping_is_a_fetch_error_at_the_missing_byte() {
    let (mut hart, mut bus) = booted();
    let last = RAM_BASE + RAM_LEN as u32 - 2;
    hart.set_pc(last);
    // A 3-byte instruction whose third byte is off the end.
    let i = (last - RAM_BASE) as usize;
    bus.ram[i..i + 2].copy_from_slice(&encode(&nop())[..2]);
    // A failed fetch charges only what the bus billed (as on RV32), so the
    // budget does not stop the slice: the vector does.
    asm(&mut bus, VEC + VECOFS_USER, &[brk()]);
    assert_eq!(
        hart.run_slice(&mut bus, 1),
        SliceEnd::Ebreak {
            pc: VEC + VECOFS_USER
        }
    );
    assert_eq!(hart.sr().exccause, cause::INSTRUCTION_FETCH_ERROR);
    assert_eq!(hart.sr().excvaddr, last + 2);
    assert_eq!(hart.sr().epc[1], last);
}

#[test]
fn ar_group_names_the_highest_referenced_group() {
    use super::window::ar_group;
    assert_eq!(ar_group(&Inst::Entry(a(1), 16), 2), 2);
    assert_eq!(ar_group(&Inst::Entry(a(1), 16), 0), 0);
    assert_eq!(ar_group(&Inst::Call(CallOp::Call8, 0), 0), 2);
    assert_eq!(ar_group(&Inst::Call(CallOp::Call0, 0), 0), 0);
    assert_eq!(ar_group(&l32i(12, 1, 0), 0), 3);
    assert_eq!(ar_group(&movi(7, 0), 0), 1);
    assert_eq!(ar_group(&Inst::NullaryN(NullaryNarrowOp::RetwN), 0), 0);
    assert_eq!(ar_group(&Inst::J(0), 0), 0);
}

// --- the translated-core seam (M1 P5) --------------------------------------
//
// Nothing is translated here and nothing ever will be by this crate. These
// tests hold the seam to its one promise — that it idles — and check that the
// events a translator has to hear are actually raised.

use std::cell::RefCell;
use std::rc::Rc;

use super::translated::{BoxedCore, RunOutcome, TranslatedCore, entry_slot};
use crate::trace::TextTracer;

/// What a stub core did, shared with the test that installed it.
#[derive(Default)]
struct CoreLog {
    /// Every `invalidate` call, in order.
    invalidations: Vec<Option<(u32, u32)>>,
    /// How many times the hart entered the core.
    runs: u32,
}

/// What a stub core answers when the hart enters it.
#[derive(Clone, Copy)]
enum Answer {
    /// "I cannot be exact" — the interpreter continues.
    Refuse,
    /// A stay that retired one instruction's worth of work.
    Ran { bytes: u32, after_store: bool },
    /// A stay whose last instruction ended the slice.
    Ended(SliceEnd),
}

struct StubCore {
    log: Rc<RefCell<CoreLog>>,
    answer: Answer,
}

impl StubCore {
    fn install(
        hart: &mut XtHart<TestBus>,
        entries: &[u32],
        answer: Answer,
    ) -> Rc<RefCell<CoreLog>> {
        let log = Rc::new(RefCell::new(CoreLog::default()));
        let core: BoxedCore<TestBus> = Box::new(StubCore {
            log: Rc::clone(&log),
            answer,
        });
        hart.set_translated_core(core, entries);
        log
    }
}

impl TranslatedCore<TestBus> for StubCore {
    fn run(&mut self, hart: &mut XtHart<TestBus>, _bus: &mut TestBus, _end: u64) -> RunOutcome {
        self.log.borrow_mut().runs += 1;
        // The seam's own contract, asserted from the inside: the hart lifted
        // the core out before calling, so re-entry is impossible here.
        assert!(
            !hart.has_translated_core(),
            "the core must be lifted out of the hart for the length of a stay"
        );
        match self.answer {
            Answer::Refuse => RunOutcome::Refused,
            Answer::Ran { bytes, after_store } => RunOutcome::Ran {
                pc: hart.pc().wrapping_add(bytes),
                cycle_count: hart.cycle_count() + 1,
                instruction_count: hart.instruction_count() + 1,
                after_store,
            },
            Answer::Ended(end) => RunOutcome::Ended {
                pc: hart.pc(),
                cycle_count: hart.cycle_count(),
                instruction_count: hart.instruction_count(),
                end,
            },
        }
    }

    fn invalidate(&mut self, range: Option<(u32, u32)>) {
        self.log.borrow_mut().invalidations.push(range);
    }

    fn report(&self) -> String {
        format!("stub core: {} entries", self.log.borrow().runs)
    }
}

/// What a short program leaves behind, trace included.
struct Observed {
    trace: String,
    pc: u32,
    cycles: u64,
    instrs: u64,
}

/// Run the same six-instruction program every time, traced.
fn observe(hart: &mut XtHart<TestBus>, bus: &mut TestBus) -> Observed {
    asm(
        bus,
        CODE,
        &[
            movi(2, 0x1234),
            movi(3, 0),
            addi(4, 2, 1),
            s32i(2, 3, DATA),
            l32i(5, 3, DATA),
            nop(),
        ],
    );
    let mut tracer = TextTracer::new();
    let end = hart.run_slice_traced(bus, 6, &mut tracer);
    assert_eq!(end, SliceEnd::BudgetExhausted);
    Observed {
        trace: tracer.dump(),
        pc: hart.pc(),
        cycles: hart.cycle_count(),
        instrs: hart.instruction_count(),
    }
}

fn assert_same(a: &Observed, b: &Observed, what: &str) {
    assert_eq!(a.trace, b.trace, "{what}: the trace differs");
    assert_eq!(a.pc, b.pc, "{what}: pc differs");
    assert_eq!(a.cycles, b.cycles, "{what}: cycle count differs");
    assert_eq!(a.instrs, b.instrs, "{what}: instruction count differs");
}

/// `--interpreter` semantics: a hart that never had a core and a hart that
/// had one and had it cleared run byte-identically — same trace, same pc,
/// same counters.
#[test]
fn no_core_is_byte_identical() {
    let (mut bare, mut bus_a) = booted();
    let bare = observe(&mut bare, &mut bus_a);

    let (mut cleared, mut bus_b) = booted();
    // A core good enough to run the whole program, then taken away.
    StubCore::install(
        &mut cleared,
        &[CODE],
        Answer::Ran {
            bytes: 3,
            after_store: false,
        },
    );
    assert!(cleared.has_translated_core());
    assert!(cleared.translated_core_report().is_some());
    cleared.clear_translated_core();
    assert!(!cleared.has_translated_core());
    assert!(cleared.translated_core_report().is_none());
    let cleared = observe(&mut cleared, &mut bus_b);

    assert_same(&bare, &cleared, "installed-then-cleared");
}

/// A core that always refuses changes nothing: the interpreter continues from
/// the entry pc and leaves exactly what it leaves with no core installed —
/// and the core really was asked.
#[test]
fn refused_changes_nothing() {
    let (mut bare, mut bus_a) = booted();
    let bare = observe(&mut bare, &mut bus_a);

    let (mut refusing, mut bus_b) = booted();
    let log = StubCore::install(&mut refusing, &[CODE], Answer::Refuse);
    let refusing = observe(&mut refusing, &mut bus_b);

    assert_eq!(log.borrow().runs, 1, "the core was entered at CODE");
    assert!(
        log.borrow().invalidations.is_empty(),
        "nothing in this program invalidates"
    );
    assert_same(&bare, &refusing, "always-refusing core");
}

/// A stay that reports `after_store` makes the hart take polling point (c)
/// itself: `take_sideband`, then re-read the pending line, then poll — the
/// same three steps in the same order an interpreted store takes.
#[test]
fn ran_after_store_takes_poll_point_c() {
    let (mut hart, mut bus) = booted();
    // A level-1 line the hart will take the moment it learns of it.
    hart.interrupts_mut().intenable = 1 << IRQ_LEVEL_L1;
    asm(&mut bus, CODE, &[nop(), nop()]);
    asm(&mut bus, VEC + VECOFS_USER, &[brk()]);
    StubCore::install(
        &mut hart,
        &[CODE],
        Answer::Ran {
            bytes: 3,
            after_store: true,
        },
    );
    // What an MMIO store would have left behind: a raised side-band and a
    // line the bus can now name.
    bus.sideband = true;
    bus.pending = Some(IRQ_LEVEL_L1);

    let end = hart.run_slice(&mut bus, 4);

    assert!(
        !bus.sideband,
        "the side-band was consumed by the hart, not left standing"
    );
    assert_eq!(
        hart.external_mask(),
        1 << IRQ_LEVEL_L1,
        "the hart replaced its asserted-line mask from the bus"
    );
    assert_eq!(
        hart.pc(),
        VEC + VECOFS_USER,
        "and polled: the level-1 interrupt was delivered"
    );
    assert_eq!(hart.sr().exccause, cause::LEVEL1_INTERRUPT);
    // The `break` at the head of the vector is what stops the slice, which
    // proves the hart carried on interpreting from where the stay left it.
    assert_eq!(
        end,
        SliceEnd::Ebreak {
            pc: VEC + VECOFS_USER
        }
    );
}

/// A stay whose last instruction ended the slice hands the interpreter's own
/// answer back untouched, with the counters applied first.
#[test]
fn ended_is_handed_back_untouched() {
    let (mut hart, mut bus) = booted();
    asm(&mut bus, CODE, &[nop()]);
    StubCore::install(&mut hart, &[CODE], Answer::Ended(SliceEnd::Wfi));
    assert_eq!(hart.run_slice(&mut bus, 4), SliceEnd::Wfi);
    assert_eq!(hart.pc(), CODE, "the core reported where it left");
}

/// A cloned hart — the snapshot path — has no translated core and no entry
/// table, and the original still has both.
#[test]
fn clone_has_no_core() {
    let (mut hart, _bus) = booted();
    StubCore::install(&mut hart, &[CODE], Answer::Refuse);
    let twin = hart.clone();
    assert!(!twin.has_translated_core(), "a clone starts without a core");
    assert!(twin.translated_core_report().is_none());
    assert!(hart.has_translated_core(), "the original keeps its own");
}

/// The third invalidation event, and the one most likely to be dropped: a
/// `wsr` to `LBEG`, to `LEND` or to `LCOUNT` each invalidates.
#[test]
fn loop_register_write_invalidates() {
    for reg in [SpecialReg::Lbeg, SpecialReg::Lend, SpecialReg::Lcount] {
        let (mut hart, mut bus) = fresh();
        asm(&mut bus, CODE, &[wsr(reg, 2)]);
        let log = StubCore::install(&mut hart, &[], Answer::Refuse);
        hart.run_slice(&mut bus, 1);
        assert_eq!(
            log.borrow().invalidations.as_slice(),
            &[None],
            "wsr to {reg:?} must invalidate the whole image, exactly once"
        );
    }
}

/// `isync` is the Xtensa `fence.i`: the guest has published instructions.
#[test]
fn isync_invalidates() {
    let (mut hart, mut bus) = fresh();
    asm(&mut bus, CODE, &[Inst::Nullary(NullaryOp::Isync)]);
    let log = StubCore::install(&mut hart, &[], Answer::Refuse);
    hart.run_slice(&mut bus, 1);
    assert_eq!(log.borrow().invalidations.as_slice(), &[None]);
    assert_eq!(hart.isync_count(), 1);
    assert_eq!(hart.pc(), CODE + 3, "isync retires and advances, as before");
    assert_eq!(hart.instruction_count(), 1);
}

/// An invalidation raised while the core is lifted out is not lost: it is
/// recorded and applied when the core goes back.
#[test]
fn invalidation_survives_the_lift() {
    let (mut hart, mut bus) = fresh();
    // Two events in one slice collapse to one whole-image invalidation —
    // `PendingInvalidate` widens rather than losing precision unsafely.
    asm(
        &mut bus,
        CODE,
        &[Inst::Nullary(NullaryOp::Isync), wsr(SpecialReg::Lend, 2)],
    );
    let log = StubCore::install(&mut hart, &[], Answer::Refuse);
    hart.run_slice(&mut bus, 2);
    assert_eq!(log.borrow().invalidations.as_slice(), &[None]);
    // A range asked for with the core in the hart reaches it directly.
    hart.invalidate_block_range(CODE, CODE + 8);
    assert_eq!(
        log.borrow().invalidations.as_slice(),
        &[None, Some((CODE, CODE + 8))]
    );
}

/// The hart's entry check is the byte-indexed one: a core installed at `pc`
/// is not entered at `pc + 1`, `pc + 2` or `pc + 3`. `pc >> 1` — the RV32
/// rule — would enter at `pc + 1`.
#[test]
fn entry_check_is_byte_granular() {
    for delta in [0u32, 1, 2, 3] {
        let (mut hart, mut bus) = fresh();
        asm(&mut bus, CODE, &[nop(), nop()]);
        let log = StubCore::install(&mut hart, &[CODE], Answer::Refuse);
        hart.set_pc(CODE + delta);
        hart.run_slice(&mut bus, 1);
        let runs = log.borrow().runs;
        if delta == 0 {
            assert_eq!(runs, 1, "the core is entered at its own pc");
        } else {
            assert_eq!(runs, 0, "pc+{delta} is not an entry — the slots differ");
            assert_ne!(entry_slot(CODE), entry_slot(CODE + delta));
        }
    }
}

// --- external registers: `rer` / `wer` (M1 P6) -------------------------------

/// An ERI address in no block anything documents — the round-trip's scratch.
const ERI_SCRATCH: u32 = 0x0010_3210;

/// The two words both classic boot paths stop on, as **bytes from the shipped
/// images**, not as re-encodings of what this repo thinks they mean.
///
/// `0x0040_6890` is at pc `0x4010_01bd` in `esp_hal::debugger::
/// debugger_connected()` (reached from `CpuControl::start_app_core`);
/// `0x0040_6ee0` is at pc `0x4007_a526` in the IDF second-stage bootloader's
/// `esp_cpu_dbgr_is_attached()`. Both are `rer` of `XDM_OCD_DCR_SET`, whose
/// honest answer on this machine is 0 = no debugger attached.
///
/// The `decode` assertion is the operand-order claim in the open: the
/// destination is `at` and the **address** is `as`, matching
/// `xtensa_lx::is_debugger_attached`'s `asm!("rer {0}, {1}", out(reg) x,
/// in(reg) XDM_OCD_DCR_SET)` (xtensa-lx-0.13.0 `src/lib.rs:98-104`).
#[test]
fn the_firmware_rer_words_read_zero_for_no_debugger_attached() {
    for (word, addr_reg, dst_reg) in [(0x0040_6890u32, 8u8, 9u8), (0x0040_6ee0, 14, 14)] {
        let bytes = word.to_le_bytes();
        assert_eq!(
            lp_xt_inst::decode(&bytes[..3]).expect("decodes").0,
            Inst::ExtReg(false, a(dst_reg), a(addr_reg)),
            "{word:#010x}",
        );

        let (mut hart, mut bus) = booted();
        let i = (CODE - RAM_BASE) as usize;
        bus.ram[i..i + 3].copy_from_slice(&bytes[..3]);
        asm(&mut bus, CODE + 3, &[brk()]);
        hart.cpu_mut().set_a(addr_reg, XDM_OCD_DCR_SET);
        hart.cpu_mut().set_a(dst_reg, 0xFFFF_FFFF);

        // Retires, and the pc advanced by exactly the 3 bytes it is wide.
        assert_eq!(
            hart.run_slice(&mut bus, 10),
            SliceEnd::Ebreak { pc: CODE + 3 },
            "{word:#010x}",
        );
        assert_eq!(hart.cpu().a(dst_reg), 0, "{word:#010x}");
        assert_eq!(
            hart.cpu().a(dst_reg) & DCR_ENABLEOCD,
            0,
            "{word:#010x}: DCR_ENABLEOCD clear is 'no debugger attached'",
        );
        // A read invents no entry.
        assert!(hart.external_regs().is_empty(), "{word:#010x}");
    }
}

#[test]
fn wer_then_rer_round_trips_at_an_arbitrary_address() {
    let (mut hart, mut bus) = booted();
    asm(
        &mut bus,
        CODE,
        &[
            // wer a3, a2 : ExternalReg[a2] <- a3
            Inst::ExtReg(true, a(3), a(2)),
            // rer a4, a2 : a4 <- ExternalReg[a2]
            Inst::ExtReg(false, a(4), a(2)),
            brk(),
        ],
    );
    hart.cpu_mut().set_a(2, ERI_SCRATCH);
    hart.cpu_mut().set_a(3, 0xDEAD_BEEF);
    hart.cpu_mut().set_a(4, 0xFFFF_FFFF);
    hart.run_slice(&mut bus, 10);

    assert_eq!(hart.cpu().a(4), 0xDEAD_BEEF);
    assert_eq!(hart.external_regs().read(ERI_SCRATCH), 0xDEAD_BEEF);
    // Exactly one address was touched — a write is not a range.
    assert_eq!(hart.external_regs().len(), 1);
    assert_eq!(
        hart.external_regs().iter().collect::<Vec<_>>(),
        vec![(ERI_SCRATCH, 0xDEAD_BEEF)],
    );
    // Neighbours are untouched and still read 0.
    assert_eq!(hart.external_regs().read(ERI_SCRATCH + 4), 0);
    assert_eq!(hart.external_regs().read(XDM_OCD_DCR_SET), 0);
    // And the store is architectural state: a snapshot carries it.
    assert_eq!(hart.clone().external_regs().read(ERI_SCRATCH), 0xDEAD_BEEF);
}

#[test]
fn an_unwritten_external_register_reads_zero() {
    for addr in [0u32, XDM_OCD_DCR_CLR, XDM_OCD_DSR, 0x1234_5678, u32::MAX] {
        let (mut hart, mut bus) = booted();
        asm(&mut bus, CODE, &[Inst::ExtReg(false, a(3), a(2)), brk()]);
        hart.cpu_mut().set_a(2, addr);
        hart.cpu_mut().set_a(3, 0xFFFF_FFFF);
        hart.run_slice(&mut bus, 10);
        assert_eq!(hart.cpu().a(3), 0, "{addr:#010x}");
        assert!(hart.external_regs().is_empty(), "{addr:#010x}");
    }
}

/// A machine can seed the space from the host side — the seam a SoC with
/// something real on the ERI window would use.
#[test]
fn a_machine_can_seed_what_rer_reads() {
    let (mut hart, mut bus) = booted();
    asm(&mut bus, CODE, &[Inst::ExtReg(false, a(3), a(2)), brk()]);
    hart.external_regs_mut()
        .write(XDM_OCD_DCR_SET, DCR_ENABLEOCD);
    hart.cpu_mut().set_a(2, XDM_OCD_DCR_SET);
    hart.run_slice(&mut bus, 10);
    assert_eq!(hart.cpu().a(3), DCR_ENABLEOCD);
}

/// Every access names itself in the trace, so a guest spinning on one OCD
/// address is diagnosable rather than a silence.
#[test]
fn every_external_register_access_is_traced() {
    let (mut hart, mut bus) = booted();
    asm(
        &mut bus,
        CODE,
        &[
            Inst::ExtReg(true, a(3), a(2)),
            Inst::ExtReg(false, a(4), a(2)),
            brk(),
        ],
    );
    hart.cpu_mut().set_a(2, XDM_OCD_DCR_SET);
    hart.cpu_mut().set_a(3, 1);
    let mut tracer = crate::TextTracer::new();
    hart.run_slice_traced(&mut bus, 10, &mut tracer);
    let dump = tracer.dump();
    assert!(dump.contains("wer ext[0x0010200c] <- 0x00000001"), "{dump}");
    assert!(dump.contains("rer ext[0x0010200c] -> 0x00000001"), "{dump}");
}

/// The user-mode runner is unchanged: `rer`/`wer` are machine-mode-only there
/// and stay an illegal-instruction trap. The hart gaining a model does not
/// quietly give the `Emulator` one.
#[test]
fn the_user_mode_emulator_still_refuses_extreg() {
    for write in [false, true] {
        let mut code = Vec::new();
        code.extend(encode(&Inst::Entry(a(1), 32)));
        code.extend(encode(&Inst::ExtReg(write, a(3), a(2))));
        code.extend(encode(&retw()));
        let mut emu = crate::Emulator::new();
        match emu.run(&code, 0, 0) {
            crate::RunOutcome::Trap(t) => {
                assert_eq!(t.kind, crate::TrapKind::Exception, "write={write}: {t:?}");
                assert_eq!(
                    t.cause,
                    crate::error::EXC_ILLEGAL_INSTRUCTION,
                    "write={write}: {t:?}",
                );
            }
            other => panic!("write={write}: expected a trap, got {other:?}"),
        }
    }
}
