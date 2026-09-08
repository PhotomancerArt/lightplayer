//! The privileged hart's conformance claim.
//!
//! Every program here is hand-encoded through [`lp_riscv_inst::encode`] plus
//! literal words for the instructions that crate does not encode (`mret`,
//! `wfi`, the RV32F opcodes). The `Bus` is a test double written in this
//! file — a flat RAM with real watchpoint slots and a real side-band flag —
//! so the tests exercise the same trait boundary P3's SoC bus will implement.

extern crate alloc;

use alloc::{vec, vec::Vec};

use lp_emu_core::{
    Bus, CycleModel, InstClass, MemoryAccessKind, MemoryError, PureRead, Watchpoint,
};
use lp_riscv_inst::{Gpr, encode};

use super::csr::{
    MCAUSE, MCYCLE, MCYCLEH, MEPC, MHARTID, MIE, MINSTRET, MSCRATCH, MSTATUS, MSTATUS_BOOT,
    MSTATUS_MPIE, MSTATUS_MPP, MTVAL, MTVEC, PCCR_MACHINE, TCONTROL, TDATA1, TDATA2, TSELECT,
};
use super::{HartFault, MachineHart, SliceEnd};

/// `mret` — `funct12 = 0x302`, rs1 = rd = 0 (spec §3.3.2).
const MRET: u32 = 0x3020_0073;
/// `wfi` — `funct12 = 0x105` (spec §3.3.3).
const WFI: u32 = 0x1050_0073;
/// `flw f1, 0(x0)` — opcode `LOAD-FP` (0x07), which this hart must refuse.
const FLW: u32 = 0x0000_2087;
/// `fadd.s f1, f2, f3` — opcode `OP-FP` (0x53).
const FADD_S: u32 = 0x0031_00D3;

const RAM_BASE: u32 = 0x4080_0000;
const RAM_LEN: usize = 0x1000;
/// Trap vector base, well clear of the program at [`RAM_BASE`] and far
/// enough in that slot 17 (`VEC + 68`) still lands inside RAM.
const VEC: u32 = RAM_BASE + 0x200;
/// A word the tests watch with a trigger.
const GUARD: u32 = RAM_BASE + 0x300;
/// The address whose accesses raise the bus's side-band.
const MMIO: u32 = RAM_BASE + 0x380;
/// A register the bus declares **pure**: reading it has no side effect and
/// its value moves only when the test moves it. M4's poll-loop skip.
const PURE: u32 = RAM_BASE + 0x390;
/// A register whose read is a **pop** — the FIFO shape. Never pure, however
/// tightly the guest spins on it.
const FIFO: u32 = RAM_BASE + 0x394;

// --- the bus double ---------------------------------------------------------

/// Flat RAM at [`RAM_BASE`] with four real watchpoint slots and a side-band
/// flag raised by any access to [`MMIO`].
///
/// The watchpoint slots honour the [`Watchpoint`] contract the hart's trigger
/// unit produces: a match returns [`MemoryError::Watchpoint`] **instead of**
/// performing the access, so the store never happens.
struct TestBus {
    ram: Vec<u8>,
    watchpoints: [Option<Watchpoint>; 4],
    sideband: bool,
    /// How many times the hart has called [`Bus::take_sideband`].
    sideband_reads: u32,
    /// The stand-in for an SoC interrupt matrix: what
    /// [`Bus::pending_cpu_interrupt`] answers.
    pending: Option<u8>,
    /// When set, an access to [`MMIO`] latches this into `pending` — a
    /// peripheral raising its line from inside the store.
    raise_on_mmio: Option<u8>,
    /// What [`PURE`] reads as. The test moves it; nothing else does.
    pure_value: u32,
    /// The pure-read side-band, cleared per instruction in `set_issuing`
    /// exactly as `SocBus` does it.
    pure_read: Option<PureRead>,
    /// Every [`Bus::note_poll_skip`] the hart emitted: `(pc, address, n)`.
    poll_notes: Vec<(u32, u32, u64)>,
    /// [`FIFO`] pops on every read.
    fifo_pops: u32,
}

impl TestBus {
    fn new() -> Self {
        Self {
            ram: vec![0u8; RAM_LEN],
            watchpoints: [None; 4],
            sideband: false,
            sideband_reads: 0,
            pending: None,
            raise_on_mmio: None,
            pure_value: 0,
            pure_read: None,
            poll_notes: Vec::new(),
            fifo_pops: 0,
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
        Ok((address - RAM_BASE) as usize)
    }

    /// The `[base, base+len)` region a [`Watchpoint`] covers. NAPOT decoding
    /// is the mirror of esp-hal's encoder: a run of low one-bits terminated
    /// by a zero names a `2^(ones+1)`-byte naturally aligned region.
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
            self.sideband = true;
            if let Some(n) = self.raise_on_mmio {
                self.pending = Some(n);
            }
        }
    }

    fn word_at(&self, address: u32) -> u32 {
        let o = (address - RAM_BASE) as usize;
        u32::from_le_bytes([
            self.ram[o],
            self.ram[o + 1],
            self.ram[o + 2],
            self.ram[o + 3],
        ])
    }
}

impl Bus for TestBus {
    fn fetch_instruction(&mut self, address: u32) -> Result<u32, MemoryError> {
        self.check(address, 4, MemoryAccessKind::InstructionFetch)?;
        let o = self.offset(address, 4, MemoryAccessKind::InstructionFetch)?;
        Ok(u32::from_le_bytes([
            self.ram[o],
            self.ram[o + 1],
            self.ram[o + 2],
            self.ram[o + 3],
        ]))
    }

    fn read_word(&mut self, address: u32) -> Result<i32, MemoryError> {
        self.check(address, 4, MemoryAccessKind::Read)?;
        let o = self.offset(address, 4, MemoryAccessKind::Read)?;
        self.note_access(address);
        // The two register shapes M4 is written against, ahead of the plain
        // RAM answer.
        if address == PURE {
            self.pure_read = Some(PureRead {
                address,
                value: self.pure_value,
            });
            return Ok(self.pure_value as i32);
        }
        if address == FIFO {
            // The FIFO shape in its sharpest form: the *value* repeats, so
            // `(pc, address, value)` and the register file both look like a
            // fixed point — and the read still changes hidden state. Only
            // the bus's refusal to call it pure stops the skip.
            self.fifo_pops += 1;
            return Ok(0);
        }
        Ok(i32::from_le_bytes([
            self.ram[o],
            self.ram[o + 1],
            self.ram[o + 2],
            self.ram[o + 3],
        ]))
    }

    fn read_halfword(&mut self, address: u32) -> Result<i16, MemoryError> {
        self.check(address, 2, MemoryAccessKind::Read)?;
        let o = self.offset(address, 2, MemoryAccessKind::Read)?;
        self.note_access(address);
        Ok(i16::from_le_bytes([self.ram[o], self.ram[o + 1]]))
    }

    fn read_byte(&mut self, address: u32) -> Result<i8, MemoryError> {
        Ok(self.read_u8(address)? as i8)
    }

    fn read_u8(&mut self, address: u32) -> Result<u8, MemoryError> {
        self.check(address, 1, MemoryAccessKind::Read)?;
        let o = self.offset(address, 1, MemoryAccessKind::Read)?;
        self.note_access(address);
        Ok(self.ram[o])
    }

    fn write_word(&mut self, address: u32, value: i32) -> Result<(), MemoryError> {
        self.check(address, 4, MemoryAccessKind::Write)?;
        let o = self.offset(address, 4, MemoryAccessKind::Write)?;
        self.ram[o..o + 4].copy_from_slice(&value.to_le_bytes());
        self.note_access(address);
        Ok(())
    }

    fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), MemoryError> {
        self.check(address, 2, MemoryAccessKind::Write)?;
        let o = self.offset(address, 2, MemoryAccessKind::Write)?;
        self.ram[o..o + 2].copy_from_slice(&value.to_le_bytes());
        self.note_access(address);
        Ok(())
    }

    fn write_byte(&mut self, address: u32, value: i8) -> Result<(), MemoryError> {
        self.check(address, 1, MemoryAccessKind::Write)?;
        let o = self.offset(address, 1, MemoryAccessKind::Write)?;
        self.ram[o] = value as u8;
        self.note_access(address);
        Ok(())
    }

    fn set_watchpoint(&mut self, slot: usize, wp: Option<Watchpoint>) {
        self.watchpoints[slot] = wp;
    }

    fn take_sideband(&mut self) -> bool {
        self.sideband_reads += 1;
        core::mem::take(&mut self.sideband)
    }

    /// One clear per instruction, as `SocBus` does it: a load that reached
    /// plain RAM must never inherit the previous load's answer.
    fn set_issuing(&mut self, _pc: u32, _cycle: u64) {
        self.pure_read = None;
    }

    fn take_pure_read(&mut self) -> Option<PureRead> {
        self.pure_read.take()
    }

    fn note_poll_skip(&mut self, pc: u32, address: u32, iterations: u64) {
        self.poll_notes.push((pc, address, iterations));
    }

    fn pending_cpu_interrupt(&self) -> Option<u8> {
        self.pending
    }
}

// --- the rig ----------------------------------------------------------------

struct Rig {
    hart: MachineHart<TestBus>,
    bus: TestBus,
    /// The poll-skip horizon `run` passes. `u64::MAX` — "nothing outside can
    /// change anything, ever" — for every test that is not about the skip;
    /// the poll tests set it.
    horizon: u64,
}

impl Rig {
    /// A hart the way P4's machine will hand it over: `pc` at [`RAM_BASE`],
    /// `mtvec` at [`VEC`] in Direct mode, and `mstatus` **seeded** to
    /// `0x1888` — the reset value has `MIE = 0` and nothing in the guest ever
    /// sets it.
    fn new() -> Self {
        let mut hart = MachineHart::new(0);
        hart.set_pc(RAM_BASE);
        assert!(hart.set_csr_raw(MSTATUS, MSTATUS_BOOT));
        assert!(hart.set_csr_raw(MTVEC, VEC));
        Self {
            hart,
            bus: TestBus::new(),
            horizon: u64::MAX,
        }
    }

    fn load(&mut self, address: u32, words: &[u32]) {
        let mut o = (address - RAM_BASE) as usize;
        for word in words {
            self.bus.ram[o..o + 4].copy_from_slice(&word.to_le_bytes());
            o += 4;
        }
    }

    /// The default trap handler: stop the slice so the test can read state.
    fn trap_to_ebreak(&mut self) {
        self.load(VEC, &[encode::ebreak()]);
    }

    fn run(&mut self, budget: u64) -> SliceEnd {
        self.hart.run_slice(&mut self.bus, budget, self.horizon)
    }

    fn reg(&self, n: u8) -> u32 {
        self.hart.regs()[n as usize] as u32
    }

    fn set_reg(&mut self, n: u8, value: u32) {
        self.hart.regs_mut()[n as usize] = value as i32;
    }
}

// --- 1 ----------------------------------------------------------------------

#[test]
fn csr_round_trips() {
    let mut rig = Rig::new();
    rig.set_reg(5, 0xDEAD_BEEF);
    rig.set_reg(8, VEC | 1);
    rig.load(
        RAM_BASE,
        &[
            encode::csrrw(Gpr::new(0), Gpr::new(5), MSCRATCH),
            encode::csrrs(Gpr::new(6), Gpr::new(0), MSCRATCH),
            encode::csrrw(Gpr::new(0), Gpr::new(5), MEPC),
            encode::csrrs(Gpr::new(7), Gpr::new(0), MEPC),
            encode::csrrw(Gpr::new(0), Gpr::new(8), MTVEC),
            encode::csrrs(Gpr::new(9), Gpr::new(0), MTVEC),
            encode::csrrw(Gpr::new(0), Gpr::new(5), MCAUSE),
            encode::csrrs(Gpr::new(10), Gpr::new(0), MCAUSE),
            encode::csrrw(Gpr::new(0), Gpr::new(5), MTVAL),
            encode::csrrs(Gpr::new(11), Gpr::new(0), MTVAL),
            // A read-only CSR read through `csrrs` with `x0`: the spec's
            // read-only form, which must NOT attempt a write (an attempted
            // write to 0xF14 would be an illegal instruction).
            encode::csrrs(Gpr::new(12), Gpr::new(0), MHARTID),
            encode::ebreak(),
        ],
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE + 44 });

    assert_eq!(rig.reg(6), 0xDEAD_BEEF, "mscratch");
    assert_eq!(rig.reg(7), 0xDEAD_BEEF, "mepc");
    assert_eq!(rig.reg(9), VEC | 1, "mtvec keeps its mode bit");
    assert_eq!(rig.reg(10), 0xDEAD_BEEF, "mcause");
    assert_eq!(rig.reg(11), 0xDEAD_BEEF, "mtval");
    assert_eq!(rig.reg(12), 0, "mhartid");

    assert!(
        rig.hart.csr().mtvec_vectored(),
        "mode bit 1 survives the round trip"
    );
    assert_eq!(rig.hart.csr().mcause, 0xDEAD_BEEF);
}

// --- 2 ----------------------------------------------------------------------

#[test]
fn illegal_csr_traps() {
    let mut rig = Rig::new();
    rig.trap_to_ebreak();
    // 0x7C0 is not implemented — an unknown CSR is an illegal instruction,
    // never a silent zero.
    let word = encode::csrrs(Gpr::new(6), Gpr::new(0), 0x7C0);
    rig.load(RAM_BASE, &[word]);

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: VEC });

    assert_eq!(rig.hart.csr().mcause, 2, "illegal instruction");
    assert_eq!(rig.hart.csr().mtval, word, "mtval carries the instruction");
    assert_eq!(rig.hart.csr().mepc, RAM_BASE);
    assert_eq!(rig.reg(6), 0, "the destination register is not written");
}

// --- 3 ----------------------------------------------------------------------

#[test]
fn ecall_traps_to_mtvec_base_and_mret_returns() {
    let mut rig = Rig::new();
    rig.load(RAM_BASE, &[encode::ecall(), encode::ebreak()]);
    rig.load(
        VEC,
        &[
            encode::csrrs(Gpr::new(5), Gpr::new(0), MEPC),
            encode::addi(Gpr::new(5), Gpr::new(5), 4),
            encode::csrrw(Gpr::new(0), Gpr::new(5), MEPC),
            MRET,
        ],
    );

    // One instruction's worth of budget: just the `ecall`.
    assert_eq!(rig.run(1), SliceEnd::BudgetExhausted);
    assert_eq!(rig.hart.pc(), VEC, "exceptions vector to mtvec.base");
    assert_eq!(rig.hart.csr().mcause, 11, "environment call from M-mode");
    assert_eq!(rig.hart.csr().mepc, RAM_BASE, "mepc is the ecall itself");
    assert_eq!(rig.hart.csr().mtval, 0, "mtval is 0 for an ecall");
    assert_eq!(
        rig.hart.csr().mstatus,
        MSTATUS_MPP | MSTATUS_MPIE,
        "MPIE takes the old MIE (1), MIE clears, MPP stays 3"
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE + 4 });
    assert_eq!(
        rig.hart.csr().mstatus,
        MSTATUS_BOOT,
        "mret restores MIE from MPIE and sets MPIE back to 1"
    );
}

// --- 4 ----------------------------------------------------------------------

#[test]
fn vectored_interrupt_lands_at_base_plus_4n() {
    let mut rig = Rig::new();
    assert!(rig.hart.set_csr_raw(MTVEC, VEC | 1));
    assert!(rig.hart.set_csr_raw(MIE, 0xFFFF_FFFF));
    rig.hart.set_external(Some(17));

    assert!(rig.hart.poll_interrupts(), "a trap was taken");

    assert_eq!(rig.hart.pc(), VEC + 4 * 17);
    assert_eq!(rig.hart.csr().mcause, 0x8000_0011);
    assert_eq!(
        rig.hart.csr().mepc,
        RAM_BASE,
        "mepc is the next instruction"
    );
    assert_eq!(rig.hart.csr().mstatus, MSTATUS_MPP | MSTATUS_MPIE);
}

// --- 5 ----------------------------------------------------------------------

#[test]
fn mie_masks_and_wfi_wakes() {
    let mut rig = Rig::new();
    // Inside a critical section: MIE clear, but CPU interrupt 5 enabled.
    assert!(rig.hart.set_csr_raw(MSTATUS, MSTATUS_MPP | MSTATUS_MPIE));
    assert!(rig.hart.set_csr_raw(MIE, 1 << 5));
    rig.load(RAM_BASE, &[WFI, encode::ebreak()]);

    assert_eq!(rig.run(500), SliceEnd::Wfi);
    assert!(rig.hart.is_wfi());
    assert_eq!(rig.hart.pc(), RAM_BASE + 4, "pc is already past the wfi");

    // An interrupt `mie` masks out neither wakes nor traps.
    rig.hart.set_external(Some(6));
    assert!(!rig.hart.poll_interrupts());
    assert!(rig.hart.is_wfi(), "mie masked it out");

    // An enabled interrupt wakes the hart even though MIE is clear
    // (spec §3.3.3: the wake condition ignores the global enable) — and does
    // not trap.
    rig.hart.set_external(Some(5));
    assert!(!rig.hart.poll_interrupts(), "MIE is clear: no trap");
    assert!(!rig.hart.is_wfi(), "but the hart woke");
    assert_eq!(rig.hart.pc(), RAM_BASE + 4, "nothing was delivered");
    assert_eq!(rig.hart.csr().mcause, 0);

    // With MIE set, the same interrupt traps.
    assert!(rig.hart.set_csr_raw(MSTATUS, MSTATUS_BOOT));
    assert!(rig.hart.poll_interrupts());
    assert_eq!(rig.hart.pc(), VEC, "mtvec is Direct here, so base");
    assert_eq!(rig.hart.csr().mcause, 0x8000_0005);
    assert_eq!(rig.hart.csr().mepc, RAM_BASE + 4);
}

// --- 6 ----------------------------------------------------------------------

#[test]
fn nested_trap_preserves_mepc_mstatus_when_software_saves_them() {
    // `riscv::interrupt::nested` (discovery §2f): save `mstatus` and `mepc`,
    // `enable()`, run the handler body with interrupts on, then `disable()`
    // if MIE had been clear, `set_mpie()`, write `mepc` back, and `mret`.
    // The nested interrupt clobbers `mepc`; only the software save/restore
    // gets the outer trap back to the right place.
    let mut rig = Rig::new();
    assert!(rig.hart.set_csr_raw(MSTATUS, MSTATUS_MPP | MSTATUS_MPIE));
    assert!(rig.hart.set_csr_raw(MTVEC, VEC | 1));
    assert!(rig.hart.set_csr_raw(MIE, 1 << 17));
    rig.hart.set_external(Some(17));

    rig.load(RAM_BASE, &[encode::ecall(), encode::ebreak()]);
    rig.load(
        VEC,
        &[
            encode::csrrs(Gpr::new(8), Gpr::new(0), MSTATUS), // save mstatus
            encode::csrrs(Gpr::new(9), Gpr::new(0), MEPC),    // save mepc
            encode::addi(Gpr::new(9), Gpr::new(9), 4),        // skip the ecall
            encode::csrrsi(Gpr::new(0), 8, MSTATUS),          // enable() -> nested trap here
            encode::csrrci(Gpr::new(0), 8, MSTATUS),          // disable()
            encode::addi(Gpr::new(10), Gpr::new(0), 0x80),
            encode::csrrs(Gpr::new(0), Gpr::new(10), MSTATUS), // set_mpie()
            encode::csrrw(Gpr::new(0), Gpr::new(9), MEPC),     // mepc::write(saved)
            MRET,
        ],
    );
    // Vector slot 17 for the nested interrupt: clear the source, then return
    // into the middle of the outer handler.
    rig.load(
        VEC + 4 * 17,
        &[encode::csrrw(Gpr::new(0), Gpr::new(0), MIE), MRET],
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE + 4 });

    assert_eq!(
        rig.reg(8),
        MSTATUS_MPP,
        "the outer trap left MIE and MPIE clear; that is what software saved"
    );
    assert_eq!(rig.reg(9), RAM_BASE + 4);
    assert_eq!(
        rig.hart.csr().mepc,
        RAM_BASE + 4,
        "the nested trap overwrote mepc; the restore put it back"
    );
    assert_eq!(rig.hart.csr().mstatus, MSTATUS_BOOT);
}

// --- 7 ----------------------------------------------------------------------

#[test]
fn trigger_store_watchpoint_fires_before_the_store() {
    let mut rig = Rig::new();
    rig.trap_to_ebreak();

    // esp-hal `set_watchpoint(0, GUARD, 4)`, verbatim (discovery §4a/§4b).
    rig.set_reg(5, 0); // tselect = trigger 0
    rig.set_reg(6, 0x8); // tcontrol = mte
    rig.set_reg(7, 0xC2); // tdata1 = store | m | match=NAPOT
    rig.set_reg(8, (GUARD & !3) | 1); // tdata2 = 4-byte NAPOT
    rig.set_reg(9, 0xDEED_BAAD); // the stack-guard value
    rig.set_reg(10, GUARD);

    rig.load(
        RAM_BASE,
        &[
            encode::csrrw(Gpr::new(0), Gpr::new(5), TSELECT),
            encode::csrrw(Gpr::new(0), Gpr::new(6), TCONTROL),
            encode::csrrw(Gpr::new(0), Gpr::new(7), TDATA1),
            encode::csrrw(Gpr::new(0), Gpr::new(8), TDATA2),
            encode::sw(Gpr::new(10), Gpr::new(9), 0),
            encode::addi(Gpr::new(11), Gpr::new(0), 1), // must never run
        ],
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: VEC });

    assert_eq!(rig.hart.csr().mcause, 3, "breakpoint");
    assert_eq!(rig.hart.csr().mtval, GUARD, "mtval is the faulting address");
    assert_eq!(
        rig.hart.csr().mepc,
        RAM_BASE + 16,
        "action 0 is precise: mepc points AT the store"
    );
    assert_eq!(rig.bus.word_at(GUARD), 0, "the store did not happen");
    assert!(rig.hart.triggers().hit(0), "tdata1.hit is set");
    assert_eq!(rig.reg(11), 0, "the instruction after the store never ran");
}

// --- 8 ----------------------------------------------------------------------

#[test]
fn mcause_writable_with_arbitrary_code() {
    // `_pre_default_start_trap` does `csrw mcause, 14` as a synthetic
    // stack-overflow marker (discovery §1f).
    let mut rig = Rig::new();
    rig.load(
        RAM_BASE,
        &[
            encode::addi(Gpr::new(5), Gpr::new(0), 14),
            encode::csrrw(Gpr::new(0), Gpr::new(5), MCAUSE),
            encode::csrrs(Gpr::new(6), Gpr::new(0), MCAUSE),
            encode::ebreak(),
        ],
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE + 12 });
    assert_eq!(rig.hart.csr().mcause, 14);
    assert_eq!(rig.reg(6), 14);
}

// --- 9 ----------------------------------------------------------------------

#[test]
fn mcycle_tracks_cycle_model() {
    // The costs `tests/cycle_model.rs` pins for this model.
    let m = CycleModel::Esp32C6;
    assert_eq!(m.cycles_for(InstClass::Alu), 1);
    assert_eq!(m.cycles_for(InstClass::BranchTaken), 2);
    assert_eq!(m.cycles_for(InstClass::BranchNotTaken), 1);
    assert_eq!(m.cycles_for(InstClass::System), 4);

    let mut rig = Rig::new();
    assert_eq!(rig.hart.cycle_model(), CycleModel::Esp32C6, "the default");
    rig.load(
        RAM_BASE,
        &[
            encode::addi(Gpr::new(5), Gpr::new(0), 3),             // 1
            encode::addi(Gpr::new(5), Gpr::new(5), -1),            // 1, x3
            encode::bne(Gpr::new(5), Gpr::new(0), -4),             // 2 taken x2, 1 not-taken
            encode::csrrs(Gpr::new(6), Gpr::new(0), MCYCLE),       // 4
            encode::csrrs(Gpr::new(7), Gpr::new(0), PCCR_MACHINE), // 4
            encode::csrrs(Gpr::new(8), Gpr::new(0), MINSTRET),     // 4
            encode::csrrs(Gpr::new(9), Gpr::new(0), MCYCLEH),      // 4
            encode::ebreak(),
        ],
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE + 28 });

    // 1 + 3×1 (the addi in the loop) + 2 + 2 + 1 (the three branches) = 9.
    assert_eq!(rig.reg(6), 9, "mcycle at the moment of the read");
    assert_eq!(
        rig.reg(7),
        13,
        "0x7E2 is the same counter, one System instruction later"
    );
    assert_eq!(rig.reg(8), 9, "minstret counts retired instructions");
    assert_eq!(rig.reg(9), 0, "mcycleh: 0x7E2 is the low 32 bits");
    assert_eq!(rig.hart.cycle_count(), 25);
    assert_eq!(
        rig.hart.instruction_count(),
        11,
        "the ebreak did not retire"
    );
}

// --- 10 ---------------------------------------------------------------------

#[test]
fn fp_opcode_is_illegal_on_this_hart() {
    // The C6 is RV32IMAC with no FPU, and this crate's executors decode `F`
    // unconditionally — so the hart has to refuse before dispatching.
    for word in [FLW, FADD_S] {
        let mut rig = Rig::new();
        rig.trap_to_ebreak();
        rig.load(RAM_BASE, &[word]);

        assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: VEC });
        assert_eq!(
            rig.hart.csr().mcause,
            2,
            "illegal instruction: {word:#010x}"
        );
        assert_eq!(rig.hart.csr().mtval, word);
        assert_eq!(rig.hart.csr().mepc, RAM_BASE);
    }
}

// --- 11 ---------------------------------------------------------------------

#[test]
fn slice_budget_is_respected() {
    // Eight 1-cycle instructions and a budget of 5: the slice stops exactly
    // on the budget, having retired five.
    let mut rig = Rig::new();
    let addi = encode::addi(Gpr::new(5), Gpr::new(5), 1);
    rig.load(RAM_BASE, &[addi; 8]);

    assert_eq!(rig.run(5), SliceEnd::BudgetExhausted);
    assert_eq!(rig.hart.cycle_count(), 5);
    assert_eq!(rig.hart.instruction_count(), 5);
    assert_eq!(rig.hart.pc(), RAM_BASE + 20);

    // The overshoot bound: the check is before each fetch, so the last
    // instruction may carry the slice past the budget by its own cost and no
    // more. `div` is the most expensive class in this model.
    let mut rig = Rig::new();
    rig.load(
        RAM_BASE,
        &[
            encode::div(Gpr::new(5), Gpr::new(6), Gpr::new(7)),
            encode::ebreak(),
        ],
    );
    assert_eq!(rig.run(1), SliceEnd::BudgetExhausted);
    let cost = u64::from(CycleModel::Esp32C6.cycles_for(InstClass::DivRem));
    assert_eq!(rig.hart.cycle_count(), cost);
    assert_eq!(rig.hart.instruction_count(), 1);
    assert!(
        rig.hart.cycle_count() - 1 < cost,
        "the overshoot never exceeds one instruction's cost"
    );
}

// --- 12 ---------------------------------------------------------------------

#[test]
fn sideband_triggers_a_poll() {
    // Polling point (c): the hart reads the bus's side-band after a Store-
    // or System-class instruction, and polls when it is raised. A load must
    // not consume it — a raised flag has to survive until an instruction
    // class the hart actually checks.
    //
    // This test pins the *wiring* — who reads the flag, and after which
    // instruction classes. That a raised side-band can change an outcome is
    // pinned separately by `an_mmio_store_that_raises_a_line_traps_before_
    // the_next_instruction_retires`, which is the half P4 closed by adding
    // `Bus::pending_cpu_interrupt`.
    let mut rig = Rig::new();
    rig.set_reg(10, MMIO);
    rig.load(
        RAM_BASE,
        &[
            encode::lw(Gpr::new(11), Gpr::new(10), 0), // Load: raises, must not consume
            encode::csrrs(Gpr::new(12), Gpr::new(0), MHARTID), // System: consumes
            encode::ebreak(),
        ],
    );

    // One `lw` (2 cycles) and no more.
    assert_eq!(rig.run(2), SliceEnd::BudgetExhausted);
    assert_eq!(rig.hart.pc(), RAM_BASE + 4);
    assert!(rig.bus.sideband, "the load raised the side-band");
    assert_eq!(
        rig.bus.sideband_reads, 0,
        "a Load-class instruction does not read the side-band"
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE + 8 });
    assert!(
        !rig.bus.sideband,
        "the System-class instruction consumed it"
    );
    assert_eq!(rig.bus.sideband_reads, 1);

    // And a store consumes it too, in the same slice it raised it.
    let mut rig = Rig::new();
    rig.set_reg(10, MMIO);
    rig.load(
        RAM_BASE,
        &[encode::sw(Gpr::new(10), Gpr::new(0), 0), encode::ebreak()],
    );
    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE + 4 });
    assert_eq!(
        rig.bus.sideband_reads, 1,
        "read once, right after the store"
    );
    assert!(!rig.bus.sideband, "and consumed");
}

#[test]
fn an_mmio_store_that_raises_a_line_traps_before_the_next_instruction_retires() {
    // The gap DD21 named and P4 closed. Polling point (c) now re-reads
    // `Bus::pending_cpu_interrupt` instead of trusting a field only the
    // machine can write, so a peripheral that raises its line *inside* a
    // store is delivered without waiting for the next scheduler event.
    let mut rig = Rig::new();
    assert!(rig.hart.set_csr_raw(MIE, 0xFFFF_FFFF));
    rig.bus.raise_on_mmio = Some(9);
    rig.trap_to_ebreak();

    rig.set_reg(10, MMIO);
    rig.load(
        RAM_BASE,
        &[
            encode::sw(Gpr::new(10), Gpr::new(0), 0),
            // The instruction that must NOT retire.
            encode::addi(Gpr::new(11), Gpr::new(0), 0x7f),
            encode::ebreak(),
        ],
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: VEC });
    assert_eq!(rig.hart.csr().mcause, 0x8000_0009, "interrupt 9, not 3");
    assert_eq!(
        rig.hart.csr().mepc,
        RAM_BASE + 4,
        "mepc names the instruction the store was followed by"
    );
    assert_eq!(rig.reg(11), 0, "and that instruction has not retired");
    assert_eq!(rig.hart.external(), Some(9), "resampled from the bus");
}

#[test]
fn a_store_that_lowers_the_last_line_leaves_nothing_pending() {
    // The other direction, and the reason (c) *replaces* rather than
    // or-ing: an `int_clr` write that drops the only asserted source must
    // leave the hart with nothing to take.
    let mut rig = Rig::new();
    // Inside a critical section (MIE clear) so the entry poll does not
    // deliver interrupt 9 before the store ever runs.
    assert!(rig.hart.set_csr_raw(MSTATUS, MSTATUS_MPP | MSTATUS_MPIE));
    assert!(rig.hart.set_csr_raw(MIE, 0xFFFF_FFFF));
    rig.hart.set_external(Some(9));
    // The bus's matrix says "nothing asserted" — the store cleared it.
    rig.bus.raise_on_mmio = None;

    rig.set_reg(10, MMIO);
    rig.load(
        RAM_BASE,
        &[encode::sw(Gpr::new(10), Gpr::new(0), 0), encode::ebreak()],
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE + 4 });
    assert_eq!(rig.hart.external(), None, "the store cleared the line");
}

// --- beyond the twelve ------------------------------------------------------

#[test]
fn a_fetch_fault_at_the_trap_vector_is_a_double_fault_not_a_hang() {
    let mut rig = Rig::new();
    // Point mtvec outside RAM: the trap can be raised but not entered.
    assert!(rig.hart.set_csr_raw(MTVEC, 0x1000_0000));
    rig.load(RAM_BASE, &[encode::ecall()]);

    // The ecall vectors to 0x1000_0000; fetching there faults; the fault's
    // own vector is the same address, so the hart stops instead of looping.
    assert_eq!(
        rig.run(500),
        SliceEnd::Fault(HartFault::TrapVectorFetch {
            vector: 0x1000_0000
        })
    );
}

#[test]
fn a_store_access_fault_carries_the_address_and_cause_7() {
    let mut rig = Rig::new();
    rig.trap_to_ebreak();
    rig.set_reg(10, RAM_BASE + RAM_LEN as u32 + 16); // past the end of RAM
    rig.load(
        RAM_BASE,
        &[encode::sw(Gpr::new(10), Gpr::new(0), 0), encode::ebreak()],
    );

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: VEC });
    assert_eq!(rig.hart.csr().mcause, 7, "store access fault");
    assert_eq!(rig.hart.csr().mtval, RAM_BASE + RAM_LEN as u32 + 16);
    assert_eq!(rig.hart.csr().mepc, RAM_BASE);
}

#[test]
fn a_compressed_ebreak_hands_the_pc_back_untouched() {
    let mut rig = Rig::new();
    // `c.ebreak` (0x9002) in the low half; the high half is a `c.nop`.
    rig.load(RAM_BASE, &[0x0001_9002]);
    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE });
    assert_eq!(rig.hart.pc(), RAM_BASE, "pc is not advanced");
    assert_eq!(rig.hart.cycle_count(), 0, "and it is not charged");

    // The machine declines to claim it, so the guest gets the exception.
    rig.hart.deliver_breakpoint(RAM_BASE);
    assert_eq!(rig.hart.csr().mcause, 3);
    assert_eq!(rig.hart.csr().mepc, RAM_BASE);
    assert_eq!(rig.hart.pc(), VEC);
}

#[test]
fn a_write_to_a_read_only_csr_is_an_illegal_instruction() {
    let mut rig = Rig::new();
    rig.trap_to_ebreak();
    rig.set_reg(5, 1);
    // `csrrs x0, x5, cycle` attempts a write to a read-only CSR.
    let word = encode::csrrs(Gpr::new(0), Gpr::new(5), super::csr::CYCLE);
    rig.load(RAM_BASE, &[word]);

    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: VEC });
    assert_eq!(rig.hart.csr().mcause, 2);
    assert_eq!(rig.hart.csr().mtval, word);
}

#[test]
fn scratch_csrs_read_back_what_was_written() {
    let mut rig = Rig::new();
    rig.set_reg(5, 0x1234_5678);
    rig.load(
        RAM_BASE,
        &[
            // The dedicated-GPIO CSRs and `mhcr`: stored, not consulted.
            encode::csrrw(Gpr::new(0), Gpr::new(5), 0x803),
            encode::csrrs(Gpr::new(6), Gpr::new(0), 0x803),
            encode::csrrw(Gpr::new(0), Gpr::new(5), super::csr::MHCR),
            encode::csrrs(Gpr::new(7), Gpr::new(0), super::csr::MHCR),
            encode::ebreak(),
        ],
    );
    assert_eq!(rig.run(500), SliceEnd::Ebreak { pc: RAM_BASE + 16 });
    assert_eq!(rig.reg(6), 0x1234_5678);
    assert_eq!(rig.reg(7), 0x1234_5678);
}

// --- M4: the pure poll-loop skip --------------------------------------------
//
// The claim under test is *exactness*, not speed: a run that credits whole
// iterations of a pure poll loop must reach the same cycle, the same
// instruction count and the same architectural state as a run that executes
// every one of them. So every test here runs the same program twice — once
// with the skip and once with `set_poll_skip(false)` — and compares.
//
// Tests 2, 3, 4 and 6 are the other half: each takes the same loop and adds
// one thing that makes it *not* a fixed point, and asserts that nothing was
// skipped at all. A detector that only ever fired would pass test 1.

/// Where one drive ended, and what it cost.
#[derive(Debug, PartialEq, Eq)]
struct Drive {
    end: SliceEnd,
    cycles: u64,
    instructions: u64,
    pc: u32,
    skips: u64,
    iterations: u64,
}

impl Rig {
    /// `lw a0, 0(a1); <body>; andi a2, a0, 1; beq a2, x0, loop; ebreak`
    ///
    /// The shape every real poll loop has: load a status word, test a bit,
    /// go round while it is clear. `a1` holds the register's address.
    fn poll_loop(body: &[u32]) -> Vec<u32> {
        let mut words = vec![encode::lw(Gpr::new(10), Gpr::new(11), 0)];
        words.extend_from_slice(body);
        words.push(encode::andi(Gpr::new(12), Gpr::new(10), 1));
        let back = -4 * (words.len() as i32);
        words.push(encode::beq(Gpr::new(12), Gpr::new(0), back));
        words.push(encode::ebreak());
        words
    }

    /// Load a poll loop on `reg` and put the hart at its first instruction.
    fn load_poll_loop(&mut self, reg: u32, body: &[u32]) {
        let program = Self::poll_loop(body);
        self.load(RAM_BASE, &program);
        self.set_reg(11, reg);
    }

    /// Drive the hart the way `Esp32C6Machine::run_until` does: short capped
    /// slices, an *event* at `event_at` that gives the pure register a new
    /// value, and a **horizon** that names the earliest cycle at which
    /// anything outside the hart can change what the guest sees — uncapped,
    /// which is the whole point of M4's second `run_slice` argument.
    ///
    /// A 64-cycle slice cap stands in for the machine's 8,192: the same
    /// shape, small enough that a run to `stop` crosses many boundaries.
    fn drive(&mut self, stop: u64, event_at: u64, event_value: u32) -> Drive {
        const SLICE: u64 = 64;
        loop {
            let now = self.hart.cycle_count();
            if now >= event_at {
                self.bus.pure_value = event_value;
            }
            if now >= stop {
                return self.drive_end(SliceEnd::BudgetExhausted);
            }
            let horizon = if now < event_at { event_at } else { stop };
            let deadline = horizon.min(now + SLICE).min(stop);
            let budget = deadline.saturating_sub(now).max(1);
            let end = self
                .hart
                .run_slice(&mut self.bus, budget, horizon.max(now + budget));
            if !matches!(end, SliceEnd::BudgetExhausted) {
                return self.drive_end(end);
            }
        }
    }

    /// The same, but what arrives at `irq_at` is a CPU interrupt rather than
    /// a new register value. The horizon is bounded by it exactly as the
    /// real machine's is, because an interrupt reaches the hart from a
    /// scheduled event.
    fn drive_irq(&mut self, stop: u64, irq_at: u64, cpu_int: u8) -> Drive {
        const SLICE: u64 = 64;
        loop {
            let now = self.hart.cycle_count();
            if now >= irq_at {
                self.bus.pending = Some(cpu_int);
                self.hart.set_external(Some(cpu_int));
            }
            if now >= stop {
                return self.drive_end(SliceEnd::BudgetExhausted);
            }
            let horizon = if now < irq_at { irq_at } else { stop };
            let deadline = horizon.min(now + SLICE).min(stop);
            let budget = deadline.saturating_sub(now).max(1);
            let end = self
                .hart
                .run_slice(&mut self.bus, budget, horizon.max(now + budget));
            if !matches!(end, SliceEnd::BudgetExhausted) {
                return self.drive_end(end);
            }
        }
    }

    fn drive_end(&self, end: SliceEnd) -> Drive {
        Drive {
            end,
            cycles: self.hart.cycle_count(),
            instructions: self.hart.instruction_count(),
            pc: self.hart.pc(),
            skips: self.hart.poll_skips(),
            iterations: self.hart.polls_skipped(),
        }
    }
}

/// Build a rig, load a poll loop on `reg` with `body` spliced into it, and
/// drive it to `stop` with the register changing at `event_at`. `skip` picks
/// which half of the oracle this is.
fn poll_run(reg: u32, body: &[u32], skip: bool, stop: u64, event_at: u64) -> (Drive, [i32; 32]) {
    let mut rig = Rig::new();
    rig.hart.set_poll_skip(skip);
    rig.load_poll_loop(reg, body);
    let drive = rig.drive(stop, event_at, 1);
    (drive, *rig.hart.regs())
}

/// **Test 1.** A pure `lw`/`andi`/`beq` poll is credited in whole
/// iterations, and the run that skipped is indistinguishable from the run
/// that did not — same stopping cycle, same instruction count, same `pc`,
/// same registers. The event that changes the register still lands on time:
/// both runs leave the loop at the same cycle.
#[test]
fn a_pure_poll_loop_is_skipped_and_the_run_is_unchanged() {
    let (skipped, skipped_regs) = poll_run(PURE, &[], true, 40_000, 30_000);
    let (executed, executed_regs) = poll_run(PURE, &[], false, 40_000, 30_000);

    assert_eq!(
        skipped.end,
        SliceEnd::Ebreak {
            pc: RAM_BASE + 4 * 3
        },
        "the loop leaves through its `ebreak` once the register changes"
    );
    assert_eq!(skipped.cycles, executed.cycles, "same cycle");
    assert_eq!(
        skipped.instructions, executed.instructions,
        "same instruction count"
    );
    assert_eq!(skipped.pc, executed.pc);
    assert_eq!(skipped.end, executed.end);
    assert_eq!(skipped_regs, executed_regs, "same register file");

    assert!(skipped.skips > 0, "the loop was recognised");
    assert!(
        skipped.iterations > 1_000,
        "and credited in bulk, not one at a time: {} iterations",
        skipped.iterations
    );
    assert_eq!(executed.skips, 0, "--no-poll-skip skips nothing");
    assert_eq!(executed.iterations, 0);
}

/// **Test 2.** The same loop with a countdown in a register. The register
/// file differs on every iteration, so it is not a fixed point and nothing
/// is credited — this is the timeout-in-a-register case, and the one a
/// register-file comparison exists to catch.
#[test]
fn a_poll_loop_with_a_countdown_is_never_skipped() {
    // `addi a3, a3, -1`
    let body = [encode::addi(Gpr::new(13), Gpr::new(13), -1)];
    let (skipped, skipped_regs) = poll_run(PURE, &body, true, 40_000, 30_000);
    let (executed, executed_regs) = poll_run(PURE, &body, false, 40_000, 30_000);

    assert_eq!(skipped.skips, 0, "a countdown is not a fixed point");
    assert_eq!(skipped.iterations, 0);
    assert_eq!(skipped.cycles, executed.cycles);
    assert_eq!(skipped.instructions, executed.instructions);
    assert_eq!(skipped_regs, executed_regs);
}

/// **Test 3.** The same loop with a store in the body. A store can change
/// anything, so the detector forgets everything it had — the
/// timeout-in-memory case.
#[test]
fn a_poll_loop_with_a_store_is_never_skipped() {
    // `sw a0, 0(a4)` with a4 pointing at a scratch word.
    let body = [encode::sw(Gpr::new(14), Gpr::new(10), 0)];
    let mut rig = Rig::new();
    rig.load_poll_loop(PURE, &body);
    rig.set_reg(14, GUARD);
    let skipped = rig.drive(40_000, 30_000, 1);

    let mut rig = Rig::new();
    rig.hart.set_poll_skip(false);
    rig.load_poll_loop(PURE, &body);
    rig.set_reg(14, GUARD);
    let executed = rig.drive(40_000, 30_000, 1);

    assert_eq!(skipped.skips, 0, "a store in the body is not a fixed point");
    assert_eq!(skipped.cycles, executed.cycles);
    assert_eq!(skipped.instructions, executed.instructions);
}

/// **Test 4.** The same loop reading `mcycle`. A cycle-based timeout writes
/// a different register value every iteration *and* is a `SYSTEM`
/// instruction, either of which is disqualifying; the test asserts the
/// outcome rather than which of the two rules did it.
#[test]
fn a_poll_loop_reading_mcycle_is_never_skipped() {
    // `csrrs a3, x0, mcycle`
    let body = [encode::csrrs(Gpr::new(13), Gpr::new(0), MCYCLE)];
    let (skipped, _) = poll_run(PURE, &body, true, 40_000, 30_000);
    let (executed, _) = poll_run(PURE, &body, false, 40_000, 30_000);

    assert_eq!(skipped.skips, 0, "a loop that reads the clock is a delay");
    assert_eq!(skipped.cycles, executed.cycles);
    assert_eq!(skipped.instructions, executed.instructions);
}

/// **Test 5.** An interrupt asserted at cycle X while a skip is in flight is
/// taken at the same cycle as it would have been without the skip. The
/// horizon is what makes this true: it never reaches past the event that
/// raises the line.
#[test]
fn an_interrupt_during_a_skip_is_taken_at_the_same_cycle() {
    fn run(skip: bool) -> Drive {
        let mut rig = Rig::new();
        rig.hart.set_poll_skip(skip);
        rig.trap_to_ebreak();
        assert!(rig.hart.set_csr_raw(MIE, 1 << 7));
        rig.load_poll_loop(PURE, &[]);
        // The register never changes: the only way out is the interrupt.
        rig.drive_irq(40_000, 30_000, 7)
    }

    let skipped = run(true);
    let executed = run(false);

    assert_eq!(skipped.end, SliceEnd::Ebreak { pc: VEC }, "the handler ran");
    assert_eq!(skipped.end, executed.end);
    assert_eq!(
        skipped.cycles, executed.cycles,
        "the trap was taken at the same cycle"
    );
    assert_eq!(skipped.instructions, executed.instructions);
    assert!(skipped.skips > 0, "there was a skip to be interrupted");
}

/// **Test 6.** A poll on a register the bus does *not* declare pure — a FIFO
/// whose read pops — is never skipped, however tight the loop is.
#[test]
fn a_poll_loop_on_an_impure_register_is_never_skipped() {
    let mut rig = Rig::new();
    rig.load_poll_loop(FIFO, &[]);
    // The FIFO answers 0 for ever, so the loop spins to the stop cycle and
    // looks *exactly* like the loop test 1 skips — same instructions, same
    // repeating value, same registers. The one difference is that the bus
    // does not declare the read pure.
    let drive = rig.drive(4_000, u64::MAX, 0);
    assert_eq!(drive.skips, 0, "reading a FIFO pops it");
    assert_eq!(drive.iterations, 0);
    assert_eq!(rig.bus.poll_notes, Vec::new());
    assert!(
        rig.bus.fifo_pops > 100,
        "the loop really did spin on it: {} reads",
        rig.bus.fifo_pops
    );
}

/// The trace note is one line per skip, naming the load's `pc`, the address
/// and how many iterations were credited — and the iterations it claims add
/// up to the counter the exit report prints.
#[test]
fn each_skip_emits_exactly_one_note() {
    let mut rig = Rig::new();
    rig.load_poll_loop(PURE, &[]);
    let drive = rig.drive(40_000, 30_000, 1);

    assert_eq!(rig.bus.poll_notes.len() as u64, drive.skips);
    assert!(!rig.bus.poll_notes.is_empty());
    for (pc, address, n) in &rig.bus.poll_notes {
        assert_eq!(*pc, RAM_BASE, "the note names the load, not the branch");
        assert_eq!(*address, PURE);
        assert!(*n > 0);
    }
    let claimed: u64 = rig.bus.poll_notes.iter().map(|(_, _, n)| *n).sum();
    assert_eq!(claimed, drive.iterations);
}

/// The detector is slice-scoped: evidence gathered before a `wfi` — which
/// moves guest time without retiring instructions — cannot be used after
/// one. Left in, the loop's measured cost would be a lie about the cycles
/// the idle skip jumped.
#[test]
fn moving_guest_time_forgets_the_detector() {
    let mut rig = Rig::new();
    rig.load_poll_loop(PURE, &[]);
    // Four iterations is exactly what the detector needs, so it is armed.
    let _ = rig.hart.run_slice(&mut rig.bus, 40, 40);
    rig.hart.advance_to_cycle(20_000);
    // With the evidence forgotten, the next four iterations rebuild it and
    // the credited cycles still land at the horizon, not past it.
    let drive = rig.drive(40_000, 30_000, 1);
    assert!(drive.cycles <= 40_000 + 64);
    assert_eq!(
        drive.end,
        SliceEnd::Ebreak {
            pc: RAM_BASE + 4 * 3
        }
    );
}
