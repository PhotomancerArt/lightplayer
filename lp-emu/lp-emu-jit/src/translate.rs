//! Guest blocks to one wasm function.
//!
//! # The shape
//!
//! - **Guest registers live in wasm locals** for the length of a stay. The
//!   prologue loads only the registers the block set reads or writes
//!   (`live_regs`); the epilogue stores them back.
//! - **Guest memory is the imported linear memory**, with the arena's offset
//!   folded into every access as a `memarg` constant (JD4). No global, no
//!   indirection, no per-access region lookup.
//! - **RAM loads and stores are inline behind the permission byte.** The fast
//!   path is taken when the first and last byte of the access land on pages
//!   with the same non-zero permission; anything else exits to the import.
//! - **Anything that is not plain RAM goes out through `mmio_load` /
//!   `mmio_store`**, carrying the exact `(pc, cycle)` the interpreter would
//!   have set (JD17).
//! - **The budget is checked once per block**, against that block's exact
//!   maximum cost. A branch's class is a run-time choice, so the check uses
//!   the maximum (`BranchTaken`) and the charge uses the actual.
//! - **Control flow** is a dispatcher `loop` over a `br_table` with one arm per
//!   block and one nested `block` per guest block. Forward edges branch
//!   straight to the target's label; back edges go through the table; an edge
//!   out of the set exits.
//! - **Every exit reports** pc, cycle count, retired-instruction count and the
//!   after-store flag, so the interpreter resumes with identical state. The
//!   protocol is written out in [`crate::host`].
//!
//! # The escape hatch, and why bring-up cannot cliff
//!
//! [`Emit`] says which instruction classes this translator emits itself.
//! Everything else becomes a `step_one` import call: flush the counters and
//! the live registers to the exchange area, have the interpreter run exactly
//! that one instruction, reload, and carry on inside the same block.
//!
//! [`Emit::NOTHING`] therefore produces a **complete and correct translation
//! that emits no guest semantics at all** — every instruction goes through the
//! interpreter, and the transcript is byte-identical. That build is kept as a
//! test rather than as a stepping stone: it is the proof that a partial
//! translator can only be slow, never wrong (R9), and it is the thing that
//! makes every intermediate state of this milestone shippable.
//!
//! An instruction [`crate::decode::decode`] does not recognise cannot be
//! escaped this way, because its *width* is unknown and so the next pc is
//! unknown. Those end the block instead ([`crate::blocks::BlockEnd::Undecodable`],
//! JD7). Two escapes, one rule: never guess.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::vec::Vec;

use lp_emu_core::{CycleModel, InstClass};
use wasm_encoder::{
    BlockType, CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
    ImportSection, Instruction as I, MemArg, MemoryType, Module, TypeSection, ValType,
};

use crate::blocks::{Block, BlockEnd, BlockSet};
use crate::decode::{Cond, Decoded, Inst, LoadKind, Op, OpI, StoreKind};
use crate::host::{
    EXCHANGE_CYCLE, EXCHANGE_FLAGS, EXCHANGE_INSTRET, EXCHANGE_REGS, EXCHANGE_STATUS,
    FLAG_AFTER_STORE, FLAG_SLICE_ENDED, MMIO_LEAVE_AFTER, MMIO_PENDING, MMIO_REFUSED,
    PERM_READ_WRITE, PERM_SHIFT, load_kind, store_kind,
};

/// The module the host imports `memory` from, and the module the three
/// functions are imported from. One name for all four: the emulator instance
/// exports them and a translated module imports them, and P2's seam contract
/// says nothing about them beyond being ordinary wasm imports.
pub const IMPORT_MODULE: &str = "emu";
/// The name of the emitted entry function.
pub const ENTRY_FUNC: &str = "run";

/// Where the pieces the emitted code addresses sit in the imported memory.
///
/// Every field is a byte offset **into the linear memory**, not a guest
/// address, and every one is folded into the module as a constant at emission
/// time. The two hosts fill them very differently and that asymmetry is worth
/// stating:
///
/// - **in the browser** the emulator *is* a wasm module, so the arena, the
///   permission table and the exchange area are three ordinary Rust
///   allocations inside its own linear memory, and these offsets are just
///   their addresses;
/// - **natively** the linear memory has to be made to alias the bus's arena,
///   and the other two have to live somewhere inside it — see
///   `host_wasmtime`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The minimum size, in 64 KiB pages, of the memory the module imports.
    pub memory_pages: u64,
    /// The guest address that sits at `arena_offset`.
    pub guest_base: u32,
    /// Where the guest arena starts.
    pub arena_offset: u32,
    /// Where the permission table's entry for guest address 0 sits. The table
    /// covers the whole 32-bit guest space at [`PERM_SHIFT`] granularity, so
    /// an access needs one shift and one byte load and no bounds compare.
    pub perm_offset: u32,
    /// Where the exchange area starts. See [`crate::host`].
    pub exchange_offset: u32,
}

/// Which instruction classes the translator emits itself. Everything else goes
/// through the escape hatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emit {
    /// `lui`, `auipc`, the register-immediate and register-register ALU and M
    /// operations, `fence` and `c.nop`.
    pub alu: bool,
    /// Loads and stores.
    pub memory: bool,
    /// `jal`, `jalr` and the conditional branches.
    pub control: bool,
}

impl Emit {
    /// Every instruction through the escape hatch: the complete, correct, slow
    /// translation this phase built first and keeps as a test.
    pub const NOTHING: Self = Self {
        alu: false,
        memory: false,
        control: false,
    };
    /// Everything the translator knows how to emit.
    pub const EVERYTHING: Self = Self {
        alu: true,
        memory: true,
        control: true,
    };
}

/// What one call to [`emit`] produced.
#[derive(Clone, Debug)]
pub struct Emitted {
    pub wasm: Vec<u8>,
    /// Instructions the module runs itself.
    pub native_insts: usize,
    /// Instructions the module hands to `step_one`.
    pub escaped_insts: usize,
}

impl Emitted {
    /// Escaped instructions as a share of the block set, in per cent. A static
    /// figure: the run-time rate is what the hart counts.
    #[must_use]
    pub fn escape_share(&self) -> f64 {
        let total = self.native_insts + self.escaped_insts;
        if total == 0 {
            return 0.0;
        }
        100.0 * self.escaped_insts as f64 / total as f64
    }
}

// --- the emitted function's locals ------------------------------------------

const P_ENTRY: u32 = 0;
const P_CYCLE: u32 = 1;
const P_INSTRET: u32 = 2;
const P_END: u32 = 3;
const P_WATCH_LO: u32 = 4;
const P_WATCH_HI: u32 = 5;

/// `x1`..`x31` are locals 6..36. `x0` is never a local: it reads as a constant
/// zero and writing it is a `drop`.
const fn reg_local(r: u8) -> u32 {
    5 + r as u32
}

const L_CYC: u32 = 37;
const L_INSTRET: u32 = 38;
const L_EXIT_PC: u32 = 39;
const L_FLAGS: u32 = 40;
const L_NEXT: u32 = 41;
/// Set when an MMIO load left a yield on the bus. The interpreter does not
/// look after a load, so neither does translated code — but every store from
/// then on must leave, including an inline RAM store the bus never sees.
const L_PENDING: u32 = 42;
const L_ADDR: u32 = 43;
const L_PERM_LO: u32 = 44;
const L_PERM_HI: u32 = 45;
const L_STATUS: u32 = 46;
const L_T64: u32 = 47;
const L_T: u32 = 48;

const F_MMIO_LOAD: u32 = 0;
const F_MMIO_STORE: u32 = 1;
const F_STEP_ONE: u32 = 2;
const F_RUN: u32 = 3;

fn memarg(offset: u64) -> MemArg {
    MemArg {
        offset,
        // Alignment is a hint and a promise; a guest access has no alignment
        // this translator can promise, so every access says "one byte".
        align: 0,
        memory_index: 0,
    }
}

struct Emitter<'a> {
    f: Function,
    set: &'a BlockSet,
    model: CycleModel,
    layout: Layout,
    policy: Emit,
    live: &'a [u8],
    /// The index of the last block, so branch depths can be computed.
    last: usize,
    /// Nesting added by `if`s inside the current block body.
    extra: u32,
    native_insts: usize,
    escaped_insts: usize,
}

impl Emitter<'_> {
    fn i(&mut self, ins: I<'static>) {
        self.f.instruction(&ins);
    }

    fn cost(&self, class: InstClass) -> u64 {
        u64::from(self.model.cycles_for(class))
    }

    fn get(&mut self, r: u8) {
        if r == 0 {
            self.i(I::I32Const(0));
        } else {
            self.i(I::LocalGet(reg_local(r)));
        }
    }

    fn set(&mut self, r: u8) {
        if r == 0 {
            self.i(I::Drop);
        } else {
            self.i(I::LocalSet(reg_local(r)));
        }
    }

    // --- branch depths ------------------------------------------------------

    fn exit_depth(&self, k: usize) -> u32 {
        (self.last - k) as u32 + 1 + self.extra
    }
    fn dispatch_depth(&self, k: usize) -> u32 {
        (self.last - k) as u32 + self.extra
    }
    fn body_depth(&self, k: usize, j: usize) -> u32 {
        (j - k - 1) as u32 + self.extra
    }

    // --- counters -----------------------------------------------------------

    fn add_cycles(&mut self, cycles: u64) {
        if cycles != 0 {
            self.i(I::LocalGet(L_CYC));
            self.i(I::I64Const(cycles as i64));
            self.i(I::I64Add);
            self.i(I::LocalSet(L_CYC));
        }
    }

    fn add_retired(&mut self, retired: u32) {
        if retired != 0 {
            self.i(I::LocalGet(L_INSTRET));
            self.i(I::I64Const(i64::from(retired)));
            self.i(I::I64Add);
            self.i(I::LocalSet(L_INSTRET));
        }
    }

    fn exchange(&self, field: u64) -> MemArg {
        memarg(u64::from(self.layout.exchange_offset) + field)
    }

    // --- leaving ------------------------------------------------------------

    /// Leave the stay at `pc` — or at the address in [`L_ADDR`] when `None` —
    /// after charging `cycles` and retiring `retired`.
    fn exit(&mut self, k: usize, pc: Option<u32>, cycles: u64, retired: u32, flags: i32) {
        self.add_cycles(cycles);
        self.add_retired(retired);
        match pc {
            Some(pc) => self.i(I::I32Const(pc as i32)),
            None => self.i(I::LocalGet(L_ADDR)),
        }
        self.i(I::LocalSet(L_EXIT_PC));
        if flags != 0 {
            self.i(I::I32Const(flags));
            self.i(I::LocalSet(L_FLAGS));
        }
        self.i(I::Br(self.exit_depth(k)));
    }

    /// Continue at `target`, wherever it is. Counters are already charged.
    fn goto(&mut self, k: usize, target: u32) {
        match self.set.index.get(&target).copied() {
            // A forward edge branches straight to the target's label.
            Some(j) if j > k => self.i(I::Br(self.body_depth(k, j))),
            // A back edge goes through the dispatcher.
            Some(j) => {
                self.i(I::I32Const(j as i32));
                self.i(I::LocalSet(L_NEXT));
                self.i(I::Br(self.dispatch_depth(k)));
            }
            None => self.exit(k, Some(target), 0, 0, 0),
        }
    }

    /// Continue at `pc`, which costs nothing at all when it is the block laid
    /// out next — control simply falls out of this body into that one.
    fn fall_through(&mut self, k: usize, pc: u32) {
        if self.set.blocks.get(k + 1).is_some_and(|b| b.pc == pc) {
            return;
        }
        self.goto(k, pc);
    }

    // --- the escape hatch ---------------------------------------------------

    /// Hand one instruction to the interpreter and come back.
    ///
    /// The whole live set is flushed and reloaded around the call, which is the
    /// simplest rule that is certainly correct: `step_one` runs an arbitrary
    /// guest instruction, so anything it can write has to be somewhere it can
    /// write it. The registers *outside* the live set are already in the
    /// exchange area and never leave it, so the interpreter sees a complete
    /// register file either way.
    ///
    /// Leaves the pc the interpreter reported in [`L_ADDR`].
    fn escape(&mut self, k: usize, pc: u32, cycles: u64, retired: u32) {
        self.add_cycles(cycles);
        self.add_retired(retired);
        let live = self.live;
        for &r in live {
            let at = self.exchange(EXCHANGE_REGS + 4 * u64::from(r));
            self.i(I::I32Const(0));
            self.i(I::LocalGet(reg_local(r)));
            self.i(I::I32Store(at));
        }
        let cycle_at = self.exchange(EXCHANGE_CYCLE);
        let instret_at = self.exchange(EXCHANGE_INSTRET);
        self.i(I::I32Const(0));
        self.i(I::LocalGet(L_CYC));
        self.i(I::I64Store(cycle_at));
        self.i(I::I32Const(0));
        self.i(I::LocalGet(L_INSTRET));
        self.i(I::I64Store(instret_at));

        self.i(I::I32Const(pc as i32));
        self.i(I::Call(F_STEP_ONE));
        self.i(I::LocalSet(L_ADDR));

        for &r in live {
            let at = self.exchange(EXCHANGE_REGS + 4 * u64::from(r));
            self.i(I::I32Const(0));
            self.i(I::I32Load(at));
            self.i(I::LocalSet(reg_local(r)));
        }
        self.i(I::I32Const(0));
        self.i(I::I64Load(cycle_at));
        self.i(I::LocalSet(L_CYC));
        self.i(I::I32Const(0));
        self.i(I::I64Load(instret_at));
        self.i(I::LocalSet(L_INSTRET));

        // The interpreter does not get to have a `wfi`, an `ebreak` or a bus
        // yield swallowed by a translated stay.
        let status_at = self.exchange(EXCHANGE_STATUS);
        self.i(I::I32Const(0));
        self.i(I::I32Load(status_at));
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.i(I::LocalGet(L_ADDR));
        self.i(I::LocalSet(L_EXIT_PC));
        self.i(I::I32Const(FLAG_SLICE_ENDED));
        self.i(I::LocalSet(L_FLAGS));
        self.i(I::Br(self.exit_depth(k)));
        self.extra -= 1;
        self.i(I::End);
    }

    /// After escaping a non-terminator: anything that moved the hart off the
    /// decoder's straight line — a trap, an interrupt delivered inside the
    /// instruction, a width the interpreter disagrees about — leaves the
    /// block, exactly as `run_block`'s own `straight_on` check does.
    fn escape_straight_on(&mut self, k: usize, next_pc: u32) {
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(next_pc as i32));
        self.i(I::I32Ne);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, None, 0, 0, 0);
        self.extra -= 1;
        self.i(I::End);
    }

    /// After escaping a terminator: the interpreter has already decided the
    /// next pc, so the only thing left is to see whether it is somewhere this
    /// module can carry on.
    fn escape_terminator(&mut self, k: usize, pc: u32, d: &Decoded) {
        let mut candidates: [Option<u32>; 2] = [None, None];
        match d.inst {
            Inst::Branch { imm, .. } => {
                candidates[0] = Some(pc.wrapping_add(imm as u32));
                candidates[1] = Some(pc.wrapping_add(u32::from(d.width)));
            }
            Inst::Jal { imm, .. } => candidates[0] = Some(pc.wrapping_add(imm as u32) & !1),
            // An indirect jump names nothing statically. Leaving is what the
            // native emission does too.
            _ => {}
        }
        for target in candidates.into_iter().flatten() {
            if !self.set.index.contains_key(&target) {
                continue;
            }
            self.i(I::LocalGet(L_ADDR));
            self.i(I::I32Const(target as i32));
            self.i(I::I32Eq);
            self.i(I::If(BlockType::Empty));
            self.extra += 1;
            self.goto(k, target);
            self.extra -= 1;
            self.i(I::End);
        }
        self.exit(k, None, 0, 0, 0);
    }

    // --- memory -------------------------------------------------------------

    /// `perm_lo = perm[addr >> 14]`, `perm_hi = perm[(addr + width - 1) >> 14]`.
    fn perm_check(&mut self, width: u32) {
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(PERM_SHIFT as i32));
        self.i(I::I32ShrU);
        self.i(I::I32Load8U(memarg(u64::from(self.layout.perm_offset))));
        self.i(I::LocalSet(L_PERM_LO));
        if width > 1 {
            self.i(I::LocalGet(L_ADDR));
            self.i(I::I32Const(width as i32 - 1));
            self.i(I::I32Add);
            self.i(I::I32Const(PERM_SHIFT as i32));
            self.i(I::I32ShrU);
            self.i(I::I32Load8U(memarg(u64::from(self.layout.perm_offset))));
            self.i(I::LocalSet(L_PERM_HI));
        } else {
            self.i(I::LocalGet(L_PERM_LO));
            self.i(I::LocalSet(L_PERM_HI));
        }
    }

    /// The address in [`L_ADDR`] as an offset into the imported memory. One
    /// subtract; the arena's own offset rides in the access's `memarg`.
    fn guest_offset(&mut self) {
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(self.layout.guest_base as i32));
        self.i(I::I32Sub);
    }

    fn arena(&self) -> MemArg {
        memarg(u64::from(self.layout.arena_offset))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "an emitted access needs the instruction's operands and the block's \
                  pending counters; bundling them into a struct would move the same \
                  fields one indirection away for no reader's benefit"
    )]
    fn load(
        &mut self,
        k: usize,
        pc: u32,
        cycles: u64,
        retired: u32,
        kind: LoadKind,
        rd: u8,
        rs1: u8,
        imm: i32,
    ) {
        let (width, kind_code) = match kind {
            LoadKind::B => (1, load_kind::B),
            LoadKind::H => (2, load_kind::H),
            LoadKind::W => (4, load_kind::W),
            LoadKind::Bu => (1, load_kind::BU),
            LoadKind::Hu => (2, load_kind::HU),
        };
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_STATUS));
        self.get(rs1);
        self.i(I::I32Const(imm));
        self.i(I::I32Add);
        self.i(I::LocalSet(L_ADDR));
        self.perm_check(width);

        // Fast: both ends on the same page, and that page is plain RAM.
        self.i(I::LocalGet(L_PERM_LO));
        self.i(I::LocalGet(L_PERM_HI));
        self.i(I::I32Eq);
        self.i(I::LocalGet(L_PERM_LO));
        self.i(I::I32Const(0));
        self.i(I::I32Ne);
        self.i(I::I32And);
        self.i(I::If(BlockType::Result(ValType::I32)));
        self.extra += 1;
        self.guest_offset();
        let arena = self.arena();
        self.i(match kind {
            LoadKind::B => I::I32Load8S(arena),
            LoadKind::H => I::I32Load16S(arena),
            LoadKind::W => I::I32Load(arena),
            LoadKind::Bu => I::I32Load8U(arena),
            LoadKind::Hu => I::I32Load16U(arena),
        });
        self.i(I::Else);

        // Slow: the whole access is off plain RAM, so the bus serves it.
        self.i(I::LocalGet(L_PERM_LO));
        self.i(I::LocalGet(L_PERM_HI));
        self.i(I::I32Or);
        self.i(I::I32Eqz);
        self.i(I::If(BlockType::Result(ValType::I32)));
        self.extra += 1;
        self.i(I::I32Const(pc as i32));
        self.i(I::LocalGet(L_CYC));
        self.i(I::I64Const(cycles as i64));
        self.i(I::I64Add);
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(kind_code as i32));
        self.i(I::Call(F_MMIO_LOAD));
        self.i(I::LocalSet(L_T64));
        self.i(I::LocalGet(L_T64));
        self.i(I::I64Const(32));
        self.i(I::I64ShrU);
        self.i(I::I32WrapI64);
        self.i(I::LocalSet(L_STATUS));
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_REFUSED as i32));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        // The access did not happen. Leave at this instruction, having
        // retired nothing of it, and let the interpreter take the trap.
        self.exit(k, Some(pc), cycles, retired, 0);
        self.extra -= 1;
        self.i(I::End);
        self.i(I::LocalGet(L_T64));
        self.i(I::I32WrapI64);
        self.i(I::Else);
        // Straddling a RAM page and a non-RAM page: one guest access the bus
        // would serve as one and this cannot. Leave and let it.
        self.exit(k, Some(pc), cycles, retired, 0);
        self.i(I::I32Const(0));
        self.i(I::End);
        self.extra -= 1;
        self.i(I::End);
        self.extra -= 1;
        self.set(rd);

        // The bus is holding a yield now. Nothing happens here — the
        // interpreter does not look after a load either — but the next store
        // has to leave.
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_PENDING as i32));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.i(I::I32Const(1));
        self.i(I::LocalSet(L_PENDING));
        self.extra -= 1;
        self.i(I::End);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "see `load` — the same operands plus the value being stored"
    )]
    fn store(
        &mut self,
        k: usize,
        pc: u32,
        inst_width: u8,
        cost: u64,
        cycles: u64,
        retired: u32,
        kind: StoreKind,
        rs1: u8,
        rs2: u8,
        imm: i32,
    ) {
        let (width, kind_code) = match kind {
            StoreKind::B => (1u32, store_kind::B),
            StoreKind::H => (2, store_kind::H),
            StoreKind::W => (4, store_kind::W),
        };
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_STATUS));
        self.get(rs1);
        self.i(I::I32Const(imm));
        self.i(I::I32Add);
        self.i(I::LocalSet(L_ADDR));

        // The one store watchpoint the bus can hand over, checked exactly as
        // `check_store_watchpoint` does: a hit when `a < hi && lo < a + len`,
        // in 64 bits, before anything else. `lo == hi == 0` is "nothing armed"
        // and never hits.
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I64ExtendI32U);
        self.i(I::LocalGet(P_WATCH_HI));
        self.i(I::I64LtU);
        self.i(I::LocalGet(P_WATCH_LO));
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I64ExtendI32U);
        self.i(I::I64Const(i64::from(width)));
        self.i(I::I64Add);
        self.i(I::I64LtU);
        self.i(I::I32And);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc), cycles, retired, 0);
        self.extra -= 1;
        self.i(I::End);

        self.perm_check(width);
        // Fast: both ends on the same page, and that page is writable RAM.
        self.i(I::LocalGet(L_PERM_LO));
        self.i(I::LocalGet(L_PERM_HI));
        self.i(I::I32Eq);
        self.i(I::LocalGet(L_PERM_LO));
        self.i(I::I32Const(i32::from(PERM_READ_WRITE)));
        self.i(I::I32Eq);
        self.i(I::I32And);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.guest_offset();
        self.get(rs2);
        let arena = self.arena();
        self.i(match kind {
            StoreKind::B => I::I32Store8(arena),
            StoreKind::H => I::I32Store16(arena),
            StoreKind::W => I::I32Store(arena),
        });
        self.i(I::Else);
        self.i(I::LocalGet(L_PERM_LO));
        self.i(I::LocalGet(L_PERM_HI));
        self.i(I::I32Or);
        self.i(I::I32Eqz);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.i(I::I32Const(pc as i32));
        self.i(I::LocalGet(L_CYC));
        self.i(I::I64Const(cycles as i64));
        self.i(I::I64Add);
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(kind_code as i32));
        self.get(rs2);
        self.i(I::Call(F_MMIO_STORE));
        self.i(I::LocalSet(L_STATUS));
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_REFUSED as i32));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc), cycles, retired, 0);
        self.extra -= 1;
        self.i(I::End);
        self.i(I::Else);
        // Read-only RAM, or an access straddling two kinds of page.
        self.exit(k, Some(pc), cycles, retired, 0);
        self.i(I::End);
        self.extra -= 1;
        self.i(I::End);
        self.extra -= 1;

        // Polling point (c). The store retired; the hart takes it from here.
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_LEAVE_AFTER as i32));
        self.i(I::I32Eq);
        self.i(I::LocalGet(L_PENDING));
        self.i(I::I32Or);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(
            k,
            Some(pc.wrapping_add(u32::from(inst_width))),
            cycles + cost,
            retired + 1,
            FLAG_AFTER_STORE,
        );
        self.extra -= 1;
        self.i(I::End);
    }

    // --- arithmetic ---------------------------------------------------------

    fn op(&mut self, op: Op, rd: u8, rs1: u8, rs2: u8) {
        match op {
            Op::Mulh | Op::Mulhsu | Op::Mulhu => {
                self.get(rs1);
                self.i(if op == Op::Mulhu {
                    I::I64ExtendI32U
                } else {
                    I::I64ExtendI32S
                });
                self.get(rs2);
                self.i(if op == Op::Mulh {
                    I::I64ExtendI32S
                } else {
                    I::I64ExtendI32U
                });
                self.i(I::I64Mul);
                self.i(I::I64Const(32));
                self.i(if op == Op::Mulhu {
                    I::I64ShrU
                } else {
                    I::I64ShrS
                });
                self.i(I::I32WrapI64);
            }
            // RISC-V defines division by zero and the signed overflow case;
            // wasm traps on both, so both are branched around. Spec §7.2:
            // `x / 0` is -1, `x % 0` is x, `INT_MIN / -1` is INT_MIN and
            // `INT_MIN % -1` is 0.
            Op::Div | Op::Divu | Op::Rem | Op::Remu => {
                self.get(rs2);
                self.i(I::LocalSet(L_T));
                self.i(I::LocalGet(L_T));
                self.i(I::I32Eqz);
                self.i(I::If(BlockType::Result(ValType::I32)));
                match op {
                    Op::Div | Op::Divu => self.i(I::I32Const(-1)),
                    _ => self.get(rs1),
                }
                self.i(I::Else);
                match op {
                    Op::Div | Op::Rem => {
                        self.i(I::LocalGet(L_T));
                        self.i(I::I32Const(-1));
                        self.i(I::I32Eq);
                        self.i(I::If(BlockType::Result(ValType::I32)));
                        if op == Op::Div {
                            // `0 - x` is `INT_MIN` for `INT_MIN` and the right
                            // answer for everything else, with no trap.
                            self.i(I::I32Const(0));
                            self.get(rs1);
                            self.i(I::I32Sub);
                        } else {
                            self.i(I::I32Const(0));
                        }
                        self.i(I::Else);
                        self.get(rs1);
                        self.i(I::LocalGet(L_T));
                        self.i(if op == Op::Div {
                            I::I32DivS
                        } else {
                            I::I32RemS
                        });
                        self.i(I::End);
                    }
                    _ => {
                        self.get(rs1);
                        self.i(I::LocalGet(L_T));
                        self.i(if op == Op::Divu {
                            I::I32DivU
                        } else {
                            I::I32RemU
                        });
                    }
                }
                self.i(I::End);
            }
            _ => {
                self.get(rs1);
                self.get(rs2);
                self.i(match op {
                    Op::Add => I::I32Add,
                    Op::Sub => I::I32Sub,
                    // Both ISAs take the shift amount modulo 32.
                    Op::Sll => I::I32Shl,
                    Op::Slt => I::I32LtS,
                    Op::Sltu => I::I32LtU,
                    Op::Xor => I::I32Xor,
                    Op::Srl => I::I32ShrU,
                    Op::Sra => I::I32ShrS,
                    Op::Or => I::I32Or,
                    Op::And => I::I32And,
                    Op::Mul => I::I32Mul,
                    _ => unreachable!("the wide and dividing forms are handled above"),
                });
            }
        }
        self.set(rd);
    }

    fn op_imm(&mut self, op: OpI, rd: u8, rs1: u8, imm: i32) {
        self.get(rs1);
        self.i(I::I32Const(imm));
        self.i(match op {
            OpI::Addi => I::I32Add,
            OpI::Slti => I::I32LtS,
            // `sltiu` compares the sign-extended immediate as unsigned.
            OpI::Sltiu => I::I32LtU,
            OpI::Xori => I::I32Xor,
            OpI::Ori => I::I32Or,
            OpI::Andi => I::I32And,
            OpI::Slli => I::I32Shl,
            OpI::Srli => I::I32ShrU,
            OpI::Srai => I::I32ShrS,
        });
        self.set(rd);
    }

    // --- a block ------------------------------------------------------------

    fn emits(&self, inst: &Inst) -> bool {
        match inst {
            Inst::Lui { .. }
            | Inst::Auipc { .. }
            | Inst::OpImm { .. }
            | Inst::Op { .. }
            | Inst::Fence
            | Inst::Nop => self.policy.alu,
            Inst::Load { .. } | Inst::Store { .. } => self.policy.memory,
            Inst::Jal { .. } | Inst::Jalr { .. } | Inst::Branch { .. } => self.policy.control,
        }
    }

    fn block(&mut self, k: usize) {
        let b: &Block = &self.set.blocks[k];
        let block_pc = b.pc;
        let pcs: Vec<(u32, Decoded)> = b.insts.clone();
        let end = b.end;

        // The budget rule (M5 MD3): not one instruction may start at or past
        // `end`, so the block runs whole only if the most it can cost fits.
        // One that does not fit is handed back, and the interpreter runs it
        // with the per-instruction compare it already has.
        let max: u64 = pcs.iter().map(|(_, d)| self.cost(d.class)).sum();
        self.i(I::LocalGet(L_CYC));
        self.i(I::I64Const(max as i64));
        self.i(I::I64Add);
        self.i(I::LocalGet(P_END));
        self.i(I::I64GtU);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(block_pc), 0, 0, 0);
        self.extra -= 1;
        self.i(I::End);

        let mut cycles = 0u64;
        let mut retired = 0u32;
        for (pc, d) in &pcs {
            let (pc, d) = (*pc, *d);
            let cost = self.cost(d.class);
            let next_pc = pc.wrapping_add(u32::from(d.width));

            if !self.emits(&d.inst) {
                self.escaped_insts += 1;
                self.escape(k, pc, cycles, retired);
                cycles = 0;
                retired = 0;
                if d.is_control() {
                    self.escape_terminator(k, pc, &d);
                    return;
                }
                self.escape_straight_on(k, next_pc);
                continue;
            }

            self.native_insts += 1;
            match d.inst {
                Inst::Lui { rd, imm } => {
                    self.i(I::I32Const(imm));
                    self.set(rd);
                }
                Inst::Auipc { rd, imm } => {
                    self.i(I::I32Const(pc.wrapping_add(imm as u32) as i32));
                    self.set(rd);
                }
                Inst::OpImm { op, rd, rs1, imm } => self.op_imm(op, rd, rs1, imm),
                Inst::Op { op, rd, rs1, rs2 } => self.op(op, rd, rs1, rs2),
                // A `fence` costs its class and does nothing else; `fence.i`
                // is refused by the decoder and never reaches here.
                Inst::Fence | Inst::Nop => {}
                Inst::Load { kind, rd, rs1, imm } => {
                    self.load(k, pc, cycles, retired, kind, rd, rs1, imm);
                }
                Inst::Store {
                    kind,
                    rs1,
                    rs2,
                    imm,
                } => {
                    self.store(k, pc, d.width, cost, cycles, retired, kind, rs1, rs2, imm);
                }
                Inst::Branch {
                    cond,
                    rs1,
                    rs2,
                    imm,
                } => {
                    let taken = self.cost(InstClass::BranchTaken);
                    let not_taken = self.cost(InstClass::BranchNotTaken);
                    self.get(rs1);
                    self.get(rs2);
                    self.i(match cond {
                        Cond::Eq => I::I32Eq,
                        Cond::Ne => I::I32Ne,
                        Cond::Lt => I::I32LtS,
                        Cond::Ge => I::I32GeS,
                        Cond::Ltu => I::I32LtU,
                        Cond::Geu => I::I32GeU,
                    });
                    self.i(I::If(BlockType::Empty));
                    self.extra += 1;
                    self.add_cycles(cycles + taken);
                    self.add_retired(retired + 1);
                    self.goto(k, pc.wrapping_add(imm as u32));
                    self.extra -= 1;
                    self.i(I::End);
                    self.add_cycles(cycles + not_taken);
                    self.add_retired(retired + 1);
                    self.fall_through(k, next_pc);
                    return;
                }
                Inst::Jal { rd, imm } => {
                    if rd != 0 {
                        self.i(I::I32Const(next_pc as i32));
                        self.set(rd);
                    }
                    self.add_cycles(cycles + cost);
                    self.add_retired(retired + 1);
                    self.goto(k, pc.wrapping_add(imm as u32) & !1);
                    return;
                }
                Inst::Jalr { rd, rs1, imm } => {
                    // The address is computed before the link register is
                    // written, because `rd` and `rs1` may be the same.
                    self.get(rs1);
                    self.i(I::I32Const(imm));
                    self.i(I::I32Add);
                    self.i(I::I32Const(-2));
                    self.i(I::I32And);
                    self.i(I::LocalSet(L_ADDR));
                    if rd != 0 {
                        self.i(I::I32Const(next_pc as i32));
                        self.set(rd);
                    }
                    // P3 has no observed-target list to dispatch against, so
                    // every indirect jump leaves. P4's discovery and P5's
                    // two-level dispatch are where that stops being true.
                    self.exit(k, None, cycles + cost, retired + 1, 0);
                    return;
                }
            }
            cycles += cost;
            retired += 1;
        }

        self.add_cycles(cycles);
        self.add_retired(retired);
        match end {
            BlockEnd::Term => {
                unreachable!("a block ending in a terminator returns from the loop above")
            }
            BlockEnd::Fall(pc) => self.fall_through(k, pc),
            BlockEnd::Undecodable(pc) => self.exit(k, Some(pc), 0, 0, 0),
        }
    }
}

/// The registers the block set reads or writes anywhere.
///
/// The prologue loads exactly these and the epilogue stores exactly these;
/// everything else stays in the exchange area untouched, which is what makes
/// the escape hatch see a complete register file for free.
fn live_regs(set: &BlockSet) -> Vec<u8> {
    let mut live = [false; 32];
    for b in &set.blocks {
        for (_, d) in &b.insts {
            let mut mark = |r: u8| live[r as usize] = true;
            match d.inst {
                Inst::Lui { rd, .. } | Inst::Auipc { rd, .. } | Inst::Jal { rd, .. } => mark(rd),
                Inst::Jalr { rd, rs1, .. } => {
                    mark(rd);
                    mark(rs1);
                }
                Inst::Branch { rs1, rs2, .. } | Inst::Store { rs1, rs2, .. } => {
                    mark(rs1);
                    mark(rs2);
                }
                Inst::Load { rd, rs1, .. } | Inst::OpImm { rd, rs1, .. } => {
                    mark(rd);
                    mark(rs1);
                }
                Inst::Op { rd, rs1, rs2, .. } => {
                    mark(rd);
                    mark(rs1);
                    mark(rs2);
                }
                Inst::Fence | Inst::Nop => {}
            }
        }
    }
    (1u8..32).filter(|&r| live[r as usize]).collect()
}

/// Emit the module for `set`.
///
/// # Panics
///
/// Panics on an empty block set — there is nothing to emit and the dispatcher
/// would have no arms. Callers ask [`BlockSet::is_empty`] first.
#[must_use]
pub fn emit(set: &BlockSet, model: CycleModel, layout: Layout, policy: Emit) -> Emitted {
    assert!(!set.is_empty(), "an empty block set has nothing to emit");
    let last = set.blocks.len() - 1;
    let live = live_regs(set);

    let mut module = Module::new();

    let mut types = TypeSection::new();
    // 0: mmio_load(pc, cycle, address, kind) -> (status << 32) | value
    types.ty().function(
        [ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        [ValType::I64],
    );
    // 1: mmio_store(pc, cycle, address, kind, value) -> status
    types.ty().function(
        [
            ValType::I32,
            ValType::I64,
            ValType::I32,
            ValType::I32,
            ValType::I32,
        ],
        [ValType::I32],
    );
    // 2: step_one(pc) -> pc
    types.ty().function([ValType::I32], [ValType::I32]);
    // 3: run(entry, cycle, instret, end, watch_lo, watch_hi) -> exit pc
    types.ty().function(
        [
            ValType::I32,
            ValType::I64,
            ValType::I64,
            ValType::I64,
            ValType::I64,
            ValType::I64,
        ],
        [ValType::I32],
    );
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import(IMPORT_MODULE, "mmio_load", EntityType::Function(0));
    imports.import(IMPORT_MODULE, "mmio_store", EntityType::Function(1));
    imports.import(IMPORT_MODULE, "step_one", EntityType::Function(2));
    imports.import(
        IMPORT_MODULE,
        "memory",
        EntityType::Memory(MemoryType {
            minimum: layout.memory_pages,
            // Deliberately unbounded: in the browser this imports the
            // emulator's own memory, which is larger than the arena and may
            // grow. A declared maximum would refuse it.
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    module.section(&imports);

    let mut funcs = FunctionSection::new();
    funcs.function(3);
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export(ENTRY_FUNC, ExportKind::Func, F_RUN);
    module.section(&exports);

    let locals = alloc::vec![
        (31, ValType::I32), // x1..x31
        (2, ValType::I64),  // cycle, instret
        (8, ValType::I32),  // exit pc, flags, next, pending, address, perm x2, status
        (1, ValType::I64),  // the 64-bit scratch an mmio load returns into
        (1, ValType::I32),  // the 32-bit scratch a divide needs
    ];
    let mut e = Emitter {
        f: Function::new(locals),
        set,
        model,
        layout,
        policy,
        live: &live,
        last,
        extra: 0,
        native_insts: 0,
        escaped_insts: 0,
    };

    // Prologue.
    e.i(I::LocalGet(P_CYCLE));
    e.i(I::LocalSet(L_CYC));
    e.i(I::LocalGet(P_INSTRET));
    e.i(I::LocalSet(L_INSTRET));
    for &r in &live {
        e.i(I::I32Const(0));
        e.i(I::I32Load(e.exchange(EXCHANGE_REGS + 4 * u64::from(r))));
        e.i(I::LocalSet(reg_local(r)));
    }
    e.i(I::LocalGet(P_ENTRY));
    e.i(I::LocalSet(L_NEXT));

    // The dispatcher: `block $exit`, `loop $dispatch`, one `block` per guest
    // block, and a `br_table` whose arms are those blocks' labels.
    e.i(I::Block(BlockType::Empty));
    e.i(I::Loop(BlockType::Empty));
    for _ in 0..=last {
        e.i(I::Block(BlockType::Empty));
    }
    e.i(I::Block(BlockType::Empty));
    e.i(I::LocalGet(L_NEXT));
    let table: Vec<u32> = (1..=(last as u32 + 1)).collect();
    e.i(I::BrTable(Cow::Owned(table), 0));
    e.i(I::End);
    // The default arm: an entry index the host invented. Unreachable by
    // construction — the host maps a pc to an index it got from this very
    // block set — and stated as such rather than papered over.
    e.i(I::Unreachable);
    for k in 0..=last {
        e.i(I::End);
        e.block(k);
    }
    e.i(I::End);
    e.i(I::End);

    // Epilogue.
    for &r in &live {
        e.i(I::I32Const(0));
        e.i(I::LocalGet(reg_local(r)));
        e.i(I::I32Store(e.exchange(EXCHANGE_REGS + 4 * u64::from(r))));
    }
    e.i(I::I32Const(0));
    e.i(I::LocalGet(L_CYC));
    e.i(I::I64Store(e.exchange(EXCHANGE_CYCLE)));
    e.i(I::I32Const(0));
    e.i(I::LocalGet(L_INSTRET));
    e.i(I::I64Store(e.exchange(EXCHANGE_INSTRET)));
    e.i(I::I32Const(0));
    e.i(I::LocalGet(L_FLAGS));
    e.i(I::I32Store(e.exchange(EXCHANGE_FLAGS)));
    e.i(I::LocalGet(L_EXIT_PC));
    e.i(I::End);

    let (native_insts, escaped_insts) = (e.native_insts, e.escaped_insts);
    let mut codes = CodeSection::new();
    codes.function(&e.f);
    module.section(&codes);

    Emitted {
        wasm: module.finish(),
        native_insts,
        escaped_insts,
    }
}
