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
    /// `fence.i` instructions the hart has reported through
    /// [`Bus::note_fence_i`].
    fences_seen: u32,
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
            fences_seen: 0,
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

    /// This bus charges nothing for a fetch, so a decode-ahead is free of
    /// consequence — **unless** an execute-kind watchpoint is armed, when a
    /// fetch can trap and reading one instruction early would take that trap
    /// early. Same shape as `SocBus`'s answer.
    fn fetch_is_pure(&self) -> bool {
        !self
            .watchpoints
            .iter()
            .any(|wp| wp.is_some_and(|wp| wp.on_execute))
    }

    fn note_fence_i(&mut self) {
        self.fences_seen += 1;
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

// --- the block cache's differential rig -------------------------------------

/// Everything a slice can have changed, as one comparable value.
///
/// The block cache's whole claim is that nothing observable depends on a hit
/// or a miss, so the tests below state that claim directly: run the same
/// program with the cache on and with `--no-block-cache`, and require every
/// field of this to match. It is the in-crate twin of the sweep the phase
/// runs over the pinned firmware images.
#[derive(Debug, PartialEq, Eq)]
struct SliceState {
    end: SliceEnd,
    pc: u32,
    regs: [i32; 32],
    cycles: u64,
    instructions: u64,
    csr: alloc::string::String,
    ram: Vec<u8>,
    sideband_reads: u32,
    fences_seen: u32,
}

impl Rig {
    /// The same rig with the block cache off — `--no-block-cache`.
    fn uncached() -> Self {
        let mut rig = Self::new();
        rig.hart.set_block_cache(false);
        rig
    }

    fn state(&self, end: SliceEnd) -> SliceState {
        SliceState {
            end,
            pc: self.hart.pc(),
            regs: *self.hart.regs(),
            cycles: self.hart.cycle_count(),
            instructions: self.hart.instruction_count(),
            csr: alloc::format!("{:?}", self.hart.csr()),
            ram: self.bus.ram.clone(),
            sideband_reads: self.bus.sideband_reads,
            fences_seen: self.bus.fences_seen,
        }
    }
}

/// Build the same rig twice, run the same slice through both, and require
/// the cached and single-stepping paths to have done exactly the same thing.
///
/// `setup` is handed a fresh rig; it loads the program and seeds whatever
/// state the case needs.
fn same_either_way(budget: u64, setup: impl Fn(&mut Rig)) -> SliceState {
    let mut cached = Rig::new();
    setup(&mut cached);
    assert!(
        cached.hart.block_cache(),
        "the cache is on by default; this rig should be exercising it"
    );
    let cached_end = cached.run(budget);
    let cached_state = cached.state(cached_end);

    let mut stepped = Rig::uncached();
    setup(&mut stepped);
    let stepped_end = stepped.run(budget);
    let stepped_state = stepped.state(stepped_end);

    assert_eq!(
        cached_state, stepped_state,
        "the block cache changed something observable"
    );
    cached_state
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

// --- the block cache --------------------------------------------------------
//
// M5 P2 (Step A). Every test here states the same claim twice: the cached
// path and the single-stepping path do exactly the same thing. `--no-block-
// cache` is the free identity oracle, and these are it — in the crate, on
// programs the pinned firmware images cannot construct.

/// `fence.i` — the encoding the hart's [`super::FENCE_I`] compares against.
const FENCE_I_WORD: u32 = 0x0000_100f;
/// `fence iorw, iorw` — funct3 0, so NOT a `fence.i`.
const FENCE_WORD: u32 = 0x0ff0_000f;

/// A straight run of body instructions is one block and nothing else.
#[test]
fn a_straight_run_of_body_instructions_is_one_block() {
    let program = [
        encode::addi(Gpr::new(5), Gpr::new(0), 1),
        encode::addi(Gpr::new(5), Gpr::new(5), 1),
        encode::addi(Gpr::new(5), Gpr::new(5), 1),
        encode::addi(Gpr::new(5), Gpr::new(5), 1),
        encode::ebreak(),
    ];
    let state = same_either_way(500, |rig| rig.load(RAM_BASE, &program));
    assert_eq!(state.end, SliceEnd::Ebreak { pc: RAM_BASE + 16 });
    assert_eq!(state.regs[5], 4);
    assert_eq!(state.instructions, 4);

    let mut rig = Rig::new();
    rig.load(RAM_BASE, &program);
    rig.run(500);
    let stats = rig.hart.block_stats().expect("the cache was built");
    assert_eq!(stats.decodes, 1, "four `addi`, then the refused `ebreak`");
    assert_eq!(stats.slots_run, 4);
    assert!((stats.mean_block_len() - 4.0).abs() < 1e-9);
}

/// The same branch taken and not taken must charge what single-stepping
/// charges — the reason the terminator's charge comes from the class the
/// executor returns rather than from the decoder's bound.
#[test]
fn a_taken_and_a_not_taken_branch_charge_what_single_stepping_charges() {
    for (name, seed, taken) in [("taken", 0u32, true), ("not taken", 1, false)] {
        let state = same_either_way(500, |rig| {
            rig.set_reg(6, seed);
            rig.load(
                RAM_BASE,
                &[
                    encode::addi(Gpr::new(5), Gpr::new(0), 7),
                    // beq x6, x0, +8 — over the `addi` that follows.
                    encode::beq(Gpr::new(6), Gpr::new(0), 8),
                    encode::addi(Gpr::new(5), Gpr::new(5), 100),
                    encode::ebreak(),
                ],
            );
        });
        if taken {
            assert_eq!(state.regs[5], 7, "{name}: the skipped `addi` must not run");
            assert_eq!(state.end, SliceEnd::Ebreak { pc: RAM_BASE + 12 });
        } else {
            assert_eq!(state.regs[5], 107, "{name}");
        }
    }
}

/// A `SYSTEM` instruction in the middle of a straight run ends the block
/// before itself and is never cached — the hart keeps handling it.
#[test]
fn a_system_instruction_mid_stream_is_never_cached() {
    let state = same_either_way(500, |rig| {
        rig.set_reg(5, 0x1234);
        rig.load(
            RAM_BASE,
            &[
                encode::addi(Gpr::new(6), Gpr::new(0), 1),
                encode::csrrw(Gpr::new(0), Gpr::new(5), MSCRATCH),
                encode::csrrs(Gpr::new(7), Gpr::new(0), MSCRATCH),
                encode::addi(Gpr::new(6), Gpr::new(6), 1),
                encode::ebreak(),
            ],
        );
    });
    assert_eq!(state.regs[7], 0x1234);
    assert_eq!(state.regs[6], 2);
}

/// An RV32F opcode is refused by the decoder and rejected by the hart, with
/// the same trap either way.
#[test]
fn an_fp_opcode_is_never_cached_and_still_traps() {
    let state = same_either_way(500, |rig| {
        rig.trap_to_ebreak();
        rig.load(
            RAM_BASE,
            &[encode::addi(Gpr::new(5), Gpr::new(0), 1), FADD_S],
        );
    });
    assert_eq!(state.end, SliceEnd::Ebreak { pc: VEC });
    assert_eq!(state.regs[5], 1);

    let state = same_either_way(500, |rig| {
        rig.trap_to_ebreak();
        rig.load(RAM_BASE, &[encode::addi(Gpr::new(5), Gpr::new(0), 1), FLW]);
    });
    assert_eq!(state.end, SliceEnd::Ebreak { pc: VEC });
}

/// A block decode that walks off the end of RAM stops there, and the fault is
/// delivered by the single-stepping path at the right `pc`.
#[test]
fn a_fetch_fault_mid_decode_falls_back_to_the_stepping_path() {
    let last = RAM_BASE + RAM_LEN as u32 - 8;
    let state = same_either_way(500, |rig| {
        rig.trap_to_ebreak();
        rig.hart.set_pc(last);
        rig.load(
            last,
            &[
                encode::addi(Gpr::new(5), Gpr::new(0), 3),
                encode::addi(Gpr::new(5), Gpr::new(5), 4),
            ],
        );
    });
    assert_eq!(state.regs[5], 7);
    assert_eq!(state.end, SliceEnd::Ebreak { pc: VEC });
}

/// A deadline that lands inside a block stops at exactly the instruction the
/// single-stepping loop stops at, with exactly the same cycle count.
#[test]
fn a_deadline_inside_a_block_stops_where_single_stepping_stops() {
    // `addi` is one cycle under the C6 model, so budgets 1..=8 walk the
    // deadline through the middle of an eight-instruction block.
    for budget in 1..=8u64 {
        let state = same_either_way(budget, |rig| {
            let mut program = Vec::new();
            for _ in 0..8 {
                program.push(encode::addi(Gpr::new(5), Gpr::new(5), 1));
            }
            program.push(encode::ebreak());
            rig.load(RAM_BASE, &program);
        });
        assert_eq!(state.end, SliceEnd::BudgetExhausted, "budget {budget}");
        assert_eq!(state.regs[5], budget as i32, "budget {budget}");
        assert_eq!(state.pc, RAM_BASE + 4 * budget as u32, "budget {budget}");
        assert_eq!(state.cycles, budget, "budget {budget}");
    }
}

/// A `div` costs 32 cycles under the C6 model, so a block holding one falls
/// onto the exact-tail path for most of a small budget. Both paths must still
/// stop at the same instruction with the same count.
#[test]
fn an_expensive_instruction_in_a_block_still_stops_at_the_right_place() {
    for budget in 1..=40u64 {
        let state = same_either_way(budget, |rig| {
            rig.set_reg(6, 100);
            rig.set_reg(7, 7);
            rig.load(
                RAM_BASE,
                &[
                    encode::addi(Gpr::new(5), Gpr::new(5), 1),
                    encode::div(Gpr::new(8), Gpr::new(6), Gpr::new(7)),
                    encode::addi(Gpr::new(5), Gpr::new(5), 1),
                    encode::ebreak(),
                ],
            );
        });
        // Both paths agree, which is the claim; the exact stopping point is
        // the previous test's subject.
        let _ = state;
    }
}

/// A store that raises the bus's side-band inside a block is answered inside
/// the block, at the point `step` answers it.
#[test]
fn a_store_side_band_inside_a_block_is_answered_in_place() {
    let state = same_either_way(500, |rig| {
        rig.bus.raise_on_mmio = Some(7);
        assert!(rig.hart.set_csr_raw(MIE, 1 << 7));
        rig.trap_to_ebreak();
        rig.set_reg(10, MMIO);
        rig.load(
            RAM_BASE,
            &[
                encode::addi(Gpr::new(5), Gpr::new(0), 1),
                encode::sw(Gpr::new(10), Gpr::new(5), 0),
                encode::addi(Gpr::new(5), Gpr::new(5), 1),
                encode::ebreak(),
            ],
        );
    });
    // The interrupt is delivered at the store, so the `addi` after it does
    // not run before the handler does.
    assert_eq!(state.regs[5], 1);
    assert_eq!(state.end, SliceEnd::Ebreak { pc: VEC });
}

/// Write an instruction over an already-executed block, emit `fence.i`, jump
/// back in: the NEW instruction runs.
///
/// This is the contract's guest half. Its firmware half is the `fence.i`
/// `lpvm-native`'s `JitBuffer::from_code` emits after a JIT publish.
#[test]
fn code_rewritten_and_fenced_runs_the_new_instruction() {
    let target = RAM_BASE + 0x100;
    let state = same_either_way(500, |rig| {
        // The subroutine at `target`: `addi x5, x5, 1; ret`.
        rig.load(
            target,
            &[
                encode::addi(Gpr::new(5), Gpr::new(5), 1),
                encode::jalr(Gpr::new(0), Gpr::new(1), 0),
            ],
        );
        rig.set_reg(10, target);
        rig.set_reg(11, encode::addi(Gpr::new(5), Gpr::new(5), 16));
        rig.load(
            RAM_BASE,
            &[
                // call target        -> x5 = 1, and the block is now cached
                encode::jal(Gpr::new(1), 0x100),
                // sw x11, 0(x10)     -> overwrite the block's first word
                encode::sw(Gpr::new(10), Gpr::new(11), 0),
                // fence.i            -> publish it
                FENCE_I_WORD,
                // call target again  -> must see +16, not +1
                encode::jal(Gpr::new(1), 0x100 - 12),
                encode::ebreak(),
            ],
        );
    });
    assert_eq!(
        state.regs[5], 17,
        "the second call must run the rewritten instruction (1 + 16)"
    );
    assert_eq!(state.fences_seen, 1, "the bus must have been told");
}

/// A store to a cached page that does not change the bytes disturbs nothing.
#[test]
fn a_store_that_changes_no_bytes_disturbs_nothing() {
    let target = RAM_BASE + 0x100;
    let same = encode::addi(Gpr::new(5), Gpr::new(5), 1);
    let state = same_either_way(500, |rig| {
        rig.load(target, &[same, encode::jalr(Gpr::new(0), Gpr::new(1), 0)]);
        rig.set_reg(10, target);
        rig.set_reg(11, same);
        rig.load(
            RAM_BASE,
            &[
                encode::jal(Gpr::new(1), 0x100),
                encode::sw(Gpr::new(10), Gpr::new(11), 0),
                encode::jal(Gpr::new(1), 0x100 - 8),
                encode::ebreak(),
            ],
        );
    });
    assert_eq!(state.regs[5], 2);
}

/// `fence.i` is counted, reaches the bus, and flushes.
#[test]
fn a_fence_i_is_counted_and_flushes_the_cache() {
    let mut rig = Rig::new();
    rig.load(
        RAM_BASE,
        &[
            encode::addi(Gpr::new(5), Gpr::new(0), 1),
            FENCE_I_WORD,
            encode::addi(Gpr::new(5), Gpr::new(5), 1),
            FENCE_I_WORD,
            encode::ebreak(),
        ],
    );
    rig.run(500);
    assert_eq!(rig.hart.fence_i_count(), 2);
    assert_eq!(rig.bus.fences_seen, 2);
    let stats = rig.hart.block_stats().expect("built");
    assert_eq!(stats.flushes, 2);
    assert_eq!(stats.hits, 0, "every block was flushed before re-use");
}

/// A plain `fence` is not a `fence.i` and must not flush anything.
#[test]
fn a_plain_fence_does_not_flush() {
    let mut rig = Rig::new();
    rig.load(
        RAM_BASE,
        &[
            encode::addi(Gpr::new(5), Gpr::new(0), 1),
            FENCE_WORD,
            encode::addi(Gpr::new(5), Gpr::new(5), 1),
            encode::ebreak(),
        ],
    );
    rig.run(500);
    assert_eq!(rig.hart.fence_i_count(), 0);
    assert_eq!(rig.bus.fences_seen, 0);
    assert_eq!(rig.hart.block_stats().expect("built").flushes, 0);
}

/// An execute watchpoint armed mid-slice takes the cache out of the loop for
/// the rest of it: a decode-ahead fetch would otherwise take the watchpoint's
/// trap one instruction early.
#[test]
fn an_execute_watchpoint_armed_mid_slice_traps_at_the_right_instruction() {
    let guarded = RAM_BASE + 0x40;
    let state = same_either_way(500, |rig| {
        rig.trap_to_ebreak();
        // esp-hal's `set_watchpoint` shape with the EXECUTE bit in place of
        // the store bit: tcontrol = mte, tdata1 = execute | m | NAPOT match.
        rig.set_reg(5, 0x8);
        rig.set_reg(6, 0xC4);
        rig.set_reg(7, (guarded & !3) | 1);
        rig.load(
            RAM_BASE,
            &[
                encode::csrrw(Gpr::new(0), Gpr::new(0), TSELECT),
                encode::csrrw(Gpr::new(0), Gpr::new(5), TCONTROL),
                encode::csrrw(Gpr::new(0), Gpr::new(6), TDATA1),
                encode::csrrw(Gpr::new(0), Gpr::new(7), TDATA2),
                encode::addi(Gpr::new(8), Gpr::new(0), 1),
                encode::jal(Gpr::new(0), 0x40 - 20),
                encode::ebreak(),
            ],
        );
        rig.load(guarded, &[encode::addi(Gpr::new(9), Gpr::new(0), 1)]);
    });
    assert_eq!(state.regs[8], 1, "the instruction before the jump ran");
    assert_eq!(
        state.regs[9], 0,
        "the guarded instruction must trap instead of running"
    );
    assert_eq!(state.end, SliceEnd::Ebreak { pc: VEC });
}

/// `--no-block-cache` really does keep the cache from ever being built.
#[test]
fn no_block_cache_never_builds_one() {
    let mut rig = Rig::uncached();
    rig.load(
        RAM_BASE,
        &[
            encode::addi(Gpr::new(5), Gpr::new(0), 1),
            encode::addi(Gpr::new(5), Gpr::new(5), 1),
            encode::ebreak(),
        ],
    );
    rig.run(500);
    assert_eq!(rig.reg(5), 2);
    assert!(rig.hart.block_stats().is_none());
}

/// A cloned hart — the snapshot path — starts with an empty cache. The cache
/// is not architectural state, and the clone's bus may hold different bytes
/// at the same addresses.
#[test]
fn a_cloned_hart_starts_with_an_empty_cache() {
    let mut rig = Rig::new();
    rig.load(
        RAM_BASE,
        &[encode::addi(Gpr::new(5), Gpr::new(0), 1), encode::ebreak()],
    );
    rig.run(500);
    assert!(rig.hart.block_stats().is_some());
    let clone = rig.hart.clone();
    assert!(clone.block_stats().is_none(), "a clone caches nothing yet");
    assert!(clone.block_cache(), "but it is still allowed to");
}

/// Changing the cycle model invalidates: a block's `max_cycles` is computed
/// against the model it was built under.
#[test]
fn changing_the_cycle_model_invalidates_the_cache() {
    let mut rig = Rig::new();
    rig.load(
        RAM_BASE,
        &[encode::addi(Gpr::new(5), Gpr::new(0), 1), encode::ebreak()],
    );
    rig.run(500);
    let before = rig.hart.block_stats().expect("built");
    rig.hart.set_cycle_model(CycleModel::InstructionCount);
    let after = rig.hart.block_stats().expect("still allocated, but empty");
    assert_eq!(after.flushes, before.flushes + 1);
}

/// An explicit range invalidation drops only the blocks that overlap it.
#[test]
fn invalidating_a_range_drops_only_what_overlaps_it() {
    let mut rig = Rig::new();
    rig.load(
        RAM_BASE,
        &[
            encode::addi(Gpr::new(5), Gpr::new(0), 1),
            encode::jal(Gpr::new(0), 8),
            encode::ebreak(),
            encode::addi(Gpr::new(6), Gpr::new(0), 1),
            encode::ebreak(),
        ],
    );
    rig.run(500);
    let before = rig.hart.block_stats().expect("built");
    assert_eq!(before.decodes, 2, "the jump splits the run in two");
    rig.hart.invalidate_block_range(RAM_BASE, RAM_BASE + 4);
    let after = rig.hart.block_stats().expect("built");
    assert_eq!(after.range_entries_dropped, 1);
    assert_eq!(after.range_invalidations, 1);
}

/// A compressed instruction stream caches and runs identically.
#[test]
fn compressed_instructions_run_the_same_either_way() {
    let state = same_either_way(500, |rig| {
        // Four `c.addi a0, 1` (0x0505), then `ebreak`.
        rig.load(RAM_BASE, &[0x0505_0505, 0x0505_0505, encode::ebreak()]);
    });
    assert_eq!(state.regs[10], 4, "four `c.addi a0, 1`");
    assert_eq!(state.instructions, 4);
}

/// A `c.jr` (RVC quadrant 2, funct3 100) is a terminator, and the block it
/// ends runs the same either way.
#[test]
fn a_compressed_return_ends_its_block() {
    let target = RAM_BASE + 0x100;
    let state = same_either_way(500, |rig| {
        // `c.addi a0, 1` then `c.jr ra` (0x8082).
        rig.load(target, &[0x8082_0505]);
        rig.load(
            RAM_BASE,
            &[
                encode::jal(Gpr::new(1), 0x100),
                encode::jal(Gpr::new(1), 0x100 - 4),
                encode::ebreak(),
            ],
        );
    });
    assert_eq!(state.regs[10], 2);
}

// --- the translated-core seam (M7 P1) ---------------------------------------
//
// No translator exists yet. What is testable now is the seam's contract: the
// entry point, the no-progress guard, the after-store polling, and the rule
// that a core is not architectural state. A test double stands in for the
// core so each of those is exercised on its own.

/// A core that does whatever the test told it to, and counts.
struct FakeCore {
    outcome: super::translated::RunOutcome,
    entries: u32,
    invalidations: Vec<Option<(u32, u32)>>,
}

impl FakeCore {
    fn refusing() -> Self {
        Self {
            outcome: super::translated::RunOutcome::Refused,
            entries: 0,
            invalidations: Vec::new(),
        }
    }
}

impl super::translated::TranslatedCore<TestBus> for alloc::rc::Rc<core::cell::RefCell<FakeCore>> {
    fn run(
        &mut self,
        hart: &mut super::MachineHart<TestBus>,
        _bus: &mut TestBus,
        _end: u64,
    ) -> super::translated::RunOutcome {
        let mut me = self.borrow_mut();
        me.entries += 1;
        assert!(
            !hart.has_translated_core(),
            "the hart lifts the core out before calling it"
        );
        match me.outcome {
            super::translated::RunOutcome::Refused => super::translated::RunOutcome::Refused,
            super::translated::RunOutcome::Ended {
                pc,
                cycle_count,
                instruction_count,
                end,
            } => {
                hart.set_pc(pc);
                hart.set_counters(cycle_count, instruction_count);
                super::translated::RunOutcome::Ended {
                    pc,
                    cycle_count,
                    instruction_count,
                    end,
                }
            }
            super::translated::RunOutcome::Ran {
                pc,
                cycle_count,
                instruction_count,
                after_store,
            } => {
                // A real core would have run guest code, leaving the hart's
                // own pc and counters agreeing with what it reports; this one
                // reports the exit the test asked for and does the same.
                hart.set_pc(pc);
                hart.set_counters(cycle_count, instruction_count);
                super::translated::RunOutcome::Ran {
                    pc,
                    cycle_count,
                    instruction_count,
                    after_store,
                }
            }
        }
    }

    fn invalidate(&mut self, range: Option<(u32, u32)>) {
        self.borrow_mut().invalidations.push(range);
    }

    fn report(&self) -> alloc::string::String {
        alloc::format!("{} entr(ies)", self.borrow().entries)
    }

    fn retired(&self) -> u64 {
        // This core runs no guest instructions of its own; the outcomes it
        // reports are the test's, not a translation's.
        0
    }
}

fn shared(core: FakeCore) -> alloc::rc::Rc<core::cell::RefCell<FakeCore>> {
    alloc::rc::Rc::new(core::cell::RefCell::new(core))
}

/// The seam is only asked at the pcs the entry table names, and a core that
/// refuses changes nothing at all.
#[test]
fn a_refusing_core_is_asked_once_and_changes_nothing() {
    let program: &[u32] = &[
        encode::addi(Gpr::new(10), Gpr::new(0), 7),
        encode::addi(Gpr::new(11), Gpr::new(0), 9),
        encode::ebreak(),
    ];

    let mut plain = Rig::new();
    plain.load(RAM_BASE, program);
    let plain_end = plain.run(500);
    let plain_state = plain.state(plain_end);

    let mut with_core = Rig::new();
    with_core.load(RAM_BASE, program);
    let core = shared(FakeCore::refusing());
    with_core
        .hart
        .set_translated_core(alloc::boxed::Box::new(core.clone()), &[RAM_BASE]);
    let core_end = with_core.run(500);
    let core_state = with_core.state(core_end);

    assert_eq!(
        plain_state, core_state,
        "a refusing core is not architectural state"
    );
    assert_eq!(core.borrow().entries, 1, "asked at the one entry pc only");
}

/// A core that reports `Ran` without moving the pc or retiring anything is
/// what a real one does when its first block does not fit the remaining
/// budget. The interpreter must take the block rather than ask again forever.
#[test]
fn the_no_progress_guard_hands_the_block_back_to_the_interpreter() {
    let mut rig = Rig::new();
    rig.load(
        RAM_BASE,
        &[encode::addi(Gpr::new(10), Gpr::new(0), 7), encode::ebreak()],
    );
    let core = shared(FakeCore {
        outcome: super::translated::RunOutcome::Ran {
            pc: RAM_BASE,
            cycle_count: 0,
            instruction_count: 0,
            after_store: false,
        },
        entries: 0,
        invalidations: Vec::new(),
    });
    rig.hart
        .set_translated_core(alloc::boxed::Box::new(core.clone()), &[RAM_BASE]);
    let end = rig.run(500);

    assert!(
        matches!(end, SliceEnd::Ebreak { .. }),
        "the run finished rather than spun: {end:?}"
    );
    assert_eq!(rig.reg(10), 7, "the interpreter ran the block");
    assert_eq!(
        core.borrow().entries,
        1,
        "asked once at the entry pc; the interpreter left it behind"
    );
}

/// `invalidate_blocks` and `invalidate_block_range` reach the core, and a
/// guest `fence.i` reaches it through the first of them — that is the whole
/// of how a core learns its translated code is stale.
#[test]
fn invalidation_and_fence_i_reach_the_core() {
    let mut rig = Rig::new();
    rig.load(RAM_BASE, &[FENCE_I_WORD, encode::ebreak()]);
    let core = shared(FakeCore::refusing());
    rig.hart
        .set_translated_core(alloc::boxed::Box::new(core.clone()), &[]);

    rig.hart.invalidate_block_range(RAM_BASE, RAM_BASE + 0x100);
    rig.hart.invalidate_blocks();
    rig.run(500);

    let seen = core.borrow().invalidations.clone();
    assert_eq!(
        seen,
        alloc::vec![Some((RAM_BASE, RAM_BASE + 0x100)), None, None],
        "the range, the explicit flush, and the guest `fence.i`"
    );
    assert_eq!(rig.hart.fence_i_count(), 1);
}

/// Installing and clearing a core is what `--interpreter` reaches, and the
/// report line is what `--jit-report` prints.
#[test]
fn a_core_can_be_installed_reported_and_cleared() {
    let mut rig = Rig::new();
    assert!(!rig.hart.has_translated_core());
    assert_eq!(rig.hart.translated_core_report(), None);

    rig.hart.set_translated_core(
        alloc::boxed::Box::new(shared(FakeCore::refusing())),
        &[RAM_BASE],
    );
    assert!(rig.hart.has_translated_core());
    assert_eq!(
        rig.hart.translated_core_report().as_deref(),
        Some("0 entr(ies)")
    );

    rig.hart.clear_translated_core();
    assert!(!rig.hart.has_translated_core());
}

/// A cloned hart — the snapshot path — starts without a core, exactly as it
/// starts without a block cache and for a sharper reason.
#[test]
fn a_cloned_hart_has_no_translated_core() {
    let mut rig = Rig::new();
    rig.hart.set_translated_core(
        alloc::boxed::Box::new(shared(FakeCore::refusing())),
        &[RAM_BASE],
    );
    assert!(!rig.hart.clone().has_translated_core());
}

/// The entry index answers by halfword, because RVC puts 48.99 % of real
/// block starts at 2 mod 4 — indexing by `pc >> 2` would fold half the image
/// onto the other half — and it answers **exactly**, at every size. P4's
/// 64 K-slot direct-mapped filter left under two fifths of a whole-image
/// module's blocks reachable (M7 P5).
#[test]
fn the_entry_index_is_exact_at_every_size() {
    use super::translated::EntryIndex;
    let index = EntryIndex::build(&[RAM_BASE, RAM_BASE + 2]);
    assert!(index.contains(RAM_BASE));
    assert!(index.contains(RAM_BASE + 2));
    assert!(!index.contains(RAM_BASE + 4), "nothing starts there");

    // The addresses P4's table folded onto each other: one page apart, one
    // table apart, and a whole region apart.
    let far = [
        RAM_BASE + (1 << 15),
        RAM_BASE + (1 << 17),
        RAM_BASE ^ 0x0200_0000,
    ];
    let mut all = alloc::vec![RAM_BASE];
    all.extend_from_slice(&far);
    let index = EntryIndex::build(&all);
    for pc in all {
        assert!(index.contains(pc), "{pc:#010x} is an entry");
    }
    // A pc nothing claimed, anywhere in the 4 GiB space, needs no bounds
    // check of its own.
    for pc in [0u32, 2, 0x2000_0000, 0xffff_fffe] {
        assert!(!index.contains(pc), "{pc:#010x} starts nothing");
    }
    assert!(EntryIndex::default().is_empty());
    assert!(!EntryIndex::default().contains(RAM_BASE));
}

/// A core that **cannot** poll still works: it reports
/// `RunOutcome::Ran { after_store: true }` and the hart runs polling point (c)
/// for it, out in `run_blocks`.
///
/// M7b P2 made the translated core poll inside its own stay, so this arm is no
/// longer the path the ESP32-C6 core takes. It is still the contract
/// `translated::RunOutcome` publishes, and it is what a host with no hart to
/// poll on has to be able to rely on — so it keeps a test of its own.
#[test]
fn a_core_that_cannot_poll_gets_polling_point_c_run_for_it() {
    let mut rig = Rig::new();
    // The hart resumes at `RAM_BASE + 4` with the line already raised, the
    // way it would after a store the core reported.
    rig.load(
        RAM_BASE,
        &[
            encode::addi(Gpr::new(10), Gpr::new(0), 1),
            encode::addi(Gpr::new(11), Gpr::new(0), 2),
            encode::ebreak(),
        ],
    );
    rig.trap_to_ebreak();
    assert!(rig.hart.set_csr_raw(MIE, 1 << 7));
    rig.bus.pending = Some(7);
    rig.bus.sideband = true;

    let core = shared(FakeCore {
        outcome: super::translated::RunOutcome::Ran {
            pc: RAM_BASE + 4,
            cycle_count: 9,
            instruction_count: 1,
            after_store: true,
        },
        entries: 0,
        invalidations: Vec::new(),
    });
    rig.hart
        .set_translated_core(alloc::boxed::Box::new(core.clone()), &[RAM_BASE]);
    let end = rig.run(500);

    assert!(
        matches!(end, SliceEnd::Ebreak { .. }),
        "the handler's `ebreak` ended the slice: {end:?}"
    );
    assert_eq!(
        rig.hart.csr().mepc,
        RAM_BASE + 4,
        "the trap was taken at exactly the instruction the core left at"
    );
    assert_eq!(rig.hart.csr().mcause, 0x8000_0007);
    assert_eq!(
        rig.reg(11),
        0,
        "the instruction after the store did not retire"
    );
    assert_eq!(
        rig.bus.sideband_reads, 1,
        "the hart took the side-band exactly once"
    );
}
