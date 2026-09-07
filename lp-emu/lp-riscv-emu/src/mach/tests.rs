//! The privileged hart's conformance claim.
//!
//! Every program here is hand-encoded through [`lp_riscv_inst::encode`] plus
//! literal words for the instructions that crate does not encode (`mret`,
//! `wfi`, the RV32F opcodes). The `Bus` is a test double written in this
//! file — a flat RAM with real watchpoint slots and a real side-band flag —
//! so the tests exercise the same trait boundary P3's SoC bus will implement.

extern crate alloc;

use alloc::{vec, vec::Vec};

use lp_emu_core::{Bus, CycleModel, InstClass, MemoryAccessKind, MemoryError, Watchpoint};
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

    fn pending_cpu_interrupt(&self) -> Option<u8> {
        self.pending
    }
}

// --- the rig ----------------------------------------------------------------

struct Rig {
    hart: MachineHart<TestBus>,
    bus: TestBus,
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
        self.hart.run_slice(&mut self.bus, budget)
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
