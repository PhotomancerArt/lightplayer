//! The Xtensa body emitter: guest blocks to one wasm sub-dispatcher.
//!
//! [`lp_emu_jit::dispatch::emit_module_with`] owns everything that is the ABI
//! — the four imports and their signatures, the imported memory, the outer
//! selector, the function table, the export, the body budget. What this
//! module supplies is the **body**: the prologue, the dispatcher loop, the
//! per-block budget compare, the per-block window precondition, and one arm
//! per guest instruction the emitter knows how to make exact. Everything it
//! cannot make exact is a **refusal** — an escape to the interpreter through
//! `step_one`, or an exit at the instruction's own pc with nothing of it
//! retired — and the list of refusals is the design ([`refusal_of`]).
//!
//! # The register model (XD8)
//!
//! Sixteen `i32` locals hold the **current window** `a0..a15`. The physical
//! `AR[0..64]` file lives in the exchange area, and `a_i` is
//! `AR[(WindowBase * 4 + i) mod 64]` — so the prologue loads the sixteen
//! from wherever the base says they are, and a rotate (`entry`, `retw`)
//! moves locals rather than renaming them. `WindowBase`, `WindowStart`,
//! `SAR`, the three loop registers and `PS.CALLINC` are in locals for the
//! length of a stay too.
//!
//! **The dirty mask.** A stay writes back only the four-register groups it
//! wrote. The mask is one local, OR'd with a per-block constant at each
//! block's start (the emitter knows statically which locals a block writes;
//! over-approximating a group as dirty writes back a local that already
//! equals the file, which is never wrong), shifted at each rotate, and read
//! at every point the file has to be complete: a rotate's *leaving* groups,
//! an escape, an exit, a cross-function edge. The invariant this keeps:
//!
//! > for every physical register outside the current window, `AR` in the
//! > exchange area holds the truth; for every register inside it, the local
//! > does, and `AR` does too unless the group's dirty bit is set.
//!
//! So a caller's registers are in `AR` whenever a `retw` may reload them
//! (they left the window at the `entry` that created the callee's frame,
//! and leaving groups are written back if dirty), and the escape hatch hands
//! the interpreter a complete file by writing back the dirty groups first.
//!
//! **What is always current in the exchange area** rather than only at an
//! exit: `WindowBase` and `WindowStart` (written at every rotate), the three
//! loop registers (written at every `loop` and every loop-back decrement)
//! and `PS.CALLINC` (written at every windowed call). Those are the words a
//! polling point can observe from inside a stay — interrupt entry saves `PS`
//! whole, and the handler's `save_context` reads the loop registers — so the
//! host's fused poll marshals them into the hart before it polls, and no
//! flush is needed at a store. `SAR` and the register file are written back
//! only where the file has to be complete.
//!
//! # The window precondition, hoisted per block (XD5)
//!
//! Every block opens with the interpreter's own test: if any `WindowStart`
//! bit lies within reach of the block's maximum register group from the
//! current base, **the block refuses at its own pc** with nothing retired
//! (`why::WINDOW`). The interpreter then runs the block slot by slot, raises
//! the overflow at the exact instruction, runs the guest's own handler and
//! re-enters translated code after it. No handler runs in wasm. `entry`
//! adds the same test for the callee's frame with the live `CALLINC`, and
//! `retw` its underflow test; both refuse at their own pc.
//!
//! # Counters, budget, escape
//!
//! Exactly the RV32 shapes (JD17): the cycle and retired counters in locals,
//! handed back at every import, before every `step_one` and at every exit;
//! the per-block budget compare against the block's own maximum cost; the
//! escape flushes the dirty groups and `SAR` and reloads *everything*
//! afterwards, because the interpreter may have rotated the window
//! (`rotw`, `rfwo`) or written any register.

pub mod alu;
pub mod control;
pub mod mem;
pub mod window;

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use lp_emu_core::{CycleModel, InstClass};
use lp_emu_jit::dispatch::{Selector, emit_module_with};
use lp_emu_jit::host::{FLAG_PENDING, FLAG_SLICE_ENDED};
use lp_emu_jit::translate::{Emitted, F_STEP_ONE, Layout};
use lp_xt_inst::{AluRs, Inst, NullaryNarrowOp, NullaryOp};
use wasm_encoder::{BlockType, Function, Instruction as I, MemArg, ValType};

use crate::blocks::{Block, BlockEnd, BlockSet};
use crate::decode::{Decoded, Edges, edges};

/// Why a stay ended: the ABI's codes plus the two this emitter adds.
///
/// `lp_emu_jit::host::why` is a closed module; the two Xtensa reasons live
/// here, numbered past the ABI's last one so a report bucketed by code reads
/// both sets from one table.
pub mod why {
    pub use lp_emu_jit::host::why::*;
    /// The window precondition refused: a `WindowStart` bit lies within
    /// reach of the block's maximum group (at the block's pc), the callee's
    /// frame would overflow (at an `entry`), or the caller's frame is not
    /// resident or the return is illegal (at a `retw`). Nothing of the
    /// instruction retired; the interpreter takes the exception.
    pub const WINDOW: i32 = 15;
    /// A loop-back fired with a live `LBEG` that is not the one the walk
    /// derived from the `loop` naming this `LEND`. The decrement happened
    /// (it is the interpreter's own order); the stay leaves at the live
    /// `LBEG`.
    pub const LOOP_BACK_MISS: i32 = 16;
}

/// Which instruction families the emitter emits itself. Everything else goes
/// through the escape hatch.
///
/// The register model — the window locals, the dirty mask, the marshalling
/// around an escape — is always on: it is what makes an escape exact, not an
/// optimisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emit {
    /// The integer core: the ALU, the immediates, the shifts and `SAR`, the
    /// multiplies and divides, the barriers and `nop`.
    pub alu: bool,
    /// `l32r`, the plain loads and stores.
    pub memory: bool,
    /// The branches, `j`, `jx`, the calls, `ret`, `retw`, `entry`, `loop`
    /// and the loop-back.
    pub control: bool,
    /// The outer selector's shape.
    pub selector: Selector,
}

impl Emit {
    /// Every instruction through the escape hatch, with the register model
    /// on: the complete, correct, slow translation the round-trip tests
    /// compare the emitted one against.
    pub const NOTHING: Self = Self {
        alu: false,
        memory: false,
        control: false,
        selector: Selector::DEFAULT,
    };
    /// Everything this emitter knows how to emit.
    pub const EVERYTHING: Self = Self {
        alu: true,
        memory: true,
        control: true,
        selector: Selector::DEFAULT,
    };
}

/// The sub-dispatcher's six parameters, in the order
/// [`lp_emu_jit::dispatch::emit_module_with`] declares them.
pub(crate) const P_ENTRY: u32 = 0;
pub(crate) const P_CYCLE: u32 = 1;
pub(crate) const P_INSTRET: u32 = 2;
pub(crate) const P_END: u32 = 3;
pub(crate) const P_WATCH_LO: u32 = 4;
pub(crate) const P_WATCH_HI: u32 = 5;

/// `a0..a15`, the current window, are locals 6..21.
pub(crate) const fn reg_local(r: u8) -> u32 {
    6 + r as u32
}

pub(crate) const L_CYC: u32 = 22;
pub(crate) const L_INSTRET: u32 = 23;
pub(crate) const L_EXIT_PC: u32 = 24;
pub(crate) const L_FLAGS: u32 = 25;
pub(crate) const L_NEXT: u32 = 26;
/// Set when an MMIO load left a yield on the bus (or an escaped Load-class
/// instruction may have): every store — and every System-class instruction
/// — from then on is a polling point.
pub(crate) const L_PENDING: u32 = 27;
pub(crate) const L_ADDR: u32 = 28;
pub(crate) const L_PERM_LO: u32 = 29;
pub(crate) const L_PERM_HI: u32 = 30;
pub(crate) const L_STATUS: u32 = 31;
pub(crate) const L_T64: u32 = 32;
pub(crate) const L_T: u32 = 33;
pub(crate) const L_CROSS: u32 = 34;
pub(crate) const L_GID: u32 = 35;
pub(crate) const L_WHY: u32 = 36;
/// `WindowBase`, in units of four registers.
pub(crate) const L_WBASE: u32 = 37;
/// `WindowStart`, sixteen bits.
pub(crate) const L_WSTART: u32 = 38;
pub(crate) const L_SAR: u32 = 39;
pub(crate) const L_LBEG: u32 = 40;
pub(crate) const L_LEND: u32 = 41;
pub(crate) const L_LCOUNT: u32 = 42;
pub(crate) const L_CALLINC: u32 = 43;
/// The dirty mask: bit `g` set means group `g` of the current window has
/// been written since it was last known to equal the file.
pub(crate) const L_DIRTY: u32 = 44;
pub(crate) const L_T2: u32 = 45;
/// The loop-back's verdict for the instruction being emitted: `LCOUNT != 0`
/// and the live `LEND` is this instruction's sequential successor.
pub(crate) const L_LOOPED: u32 = 46;

/// The locals every sub-dispatcher declares, past its six parameters.
///
/// One list, in one place, because the indices above are hand-assigned and a
/// mismatch is a validation error a hundred kilobytes into a function body.
fn body_locals() -> Vec<(u32, ValType)> {
    alloc::vec![
        (16, ValType::I32), // a0..a15
        (2, ValType::I64),  // cycle, instret
        (8, ValType::I32),  // exit pc, flags, next, pending, address, perm x2, status
        (1, ValType::I64),  // the 64-bit scratch an import returns into
        (4, ValType::I32),  // scratch, cross, gid, why
        (10, ValType::I32), // wbase, wstart, sar, lbeg, lend, lcount, callinc, dirty, scratch, looped
    ]
}

pub(crate) fn memarg(offset: u64) -> MemArg {
    MemArg {
        offset,
        // A guest access has no alignment this translator can promise, so
        // every access says "one byte".
        align: 0,
        memory_index: 0,
    }
}

/// Emit the whole module for `set`.
///
/// `fn_blocks` is how many blocks one sub-dispatcher holds — the size knob
/// every wasm engine refuses a large enough function body at, and the reason
/// the driver builds with halving.
///
/// # Panics
///
/// On an empty block set, or `fn_blocks == 0`: both are the caller's bug, not
/// a state a walk produces.
#[must_use]
pub fn emit_module(
    set: &BlockSet,
    model: CycleModel,
    layout: Layout,
    policy: Emit,
    fn_blocks: usize,
) -> Emitted {
    emit_module_with(
        set,
        layout,
        crate::LAYOUT,
        policy.selector,
        fn_blocks,
        &|set, lo, len, load_func| emit_body(set, lo, len, model, layout, policy, load_func),
    )
}

/// Why an instruction the block set holds is **not** emitted natively under
/// [`Emit::EVERYTHING`] — the design's refusal list, by name — or `None`
/// when the emitter has an exact arm for it.
///
/// Every name here is an instruction the interpreter runs through the escape
/// hatch, exactly, and the driver's escape census counts by these names so
/// the gap between "inside the walk" and "retired natively" names its
/// instructions.
#[must_use]
pub fn refusal_of(d: &Decoded) -> Option<&'static str> {
    Some(match d.inst {
        // DD113: FP is 0.08 % of the render loop; no FP arms (XD12).
        Inst::FpRrr(..)
        | Inst::FpRr(..)
        | Inst::ConstS(..)
        | Inst::Rfr(..)
        | Inst::Wfr(..)
        | Inst::FpMovAr(..)
        | Inst::FpMovBr(..)
        | Inst::FpCmp(..)
        | Inst::FpToInt(..)
        | Inst::IntToFp(..) => "fp",
        Inst::FpLsi(..) | Inst::FpLsx(..) => "fp load/store",
        // The Boolean option: `b0..b15` live on the hart and nothing here
        // models them.
        Inst::MovBool(..) => "movt/movf",
        Inst::BranchBool(..) => "bt/bf",
        Inst::BoolLogic(..) | Inst::BoolAll(..) => "boolean logic",
        // Hart-owned integer families the block cache also runs through
        // `exec_priv`.
        Inst::Clamps(..) => "clamps",
        // The window and PS families that are terminators rather than
        // undecodable: the escape runs them and the reload re-reads the
        // whole window afterwards.
        Inst::Rs(AluRs::Movsp, ..) => "movsp",
        Inst::Rotw(_) => "rotw",
        Inst::Rf(_) => "rfe/rfde/rfwo/rfwu",
        Inst::Rfi(_) => "rfi",
        Inst::Rsil(..) => "rsil",
        Inst::Waiti(_) => "waiti",
        // `entry` with `s > 3` is the RM's undefined case and this hart's
        // illegal-instruction trap: let the interpreter raise it.
        Inst::Entry(rs, _) if rs.num() > 3 => "entry s>3",
        // A `loop` whose own successor is another loop's `LEND`: the
        // interpreter's ordering (the old loop's decrement, then the new
        // registers, then a jump to the OLD `LBEG`) is not worth a second
        // arm for a shape no compiler emits.
        Inst::Loop(..) if d.lbeg.is_some() => "loop at a loop end",
        // Every other instruction the block set can hold has an arm.
        _ => return None,
    })
}

/// How many instructions of `set` each refusal name accounts for, statically.
#[must_use]
pub fn static_census(set: &BlockSet) -> BTreeMap<&'static str, usize> {
    let mut census = BTreeMap::new();
    for b in &set.blocks {
        for (_, d) in &b.insts {
            if let Some(name) = refusal_of(d) {
                *census.entry(name).or_insert(0) += 1;
            }
        }
    }
    census
}

/// The addresses the block set's loop-backs fire at — every `LEND` some
/// `loop` in the walk named.
///
/// A stay that starts with `LCOUNT != 0` and a live `LEND` outside this set
/// cannot be exact: the interpreter would loop back at an instruction the
/// walk never marked. The driver refuses such an entry.
#[must_use]
pub fn known_lends(set: &BlockSet) -> alloc::collections::BTreeSet<u32> {
    set.blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .filter(|(_, d)| d.lbeg.is_some())
        .map(|(pc, d)| pc.wrapping_add(u32::from(d.width)))
        .collect()
}

/// Emit one sub-dispatcher: the wasm function holding `set`'s blocks
/// `lo..lo + len`.
///
/// The signature is the ABI's `(entry, cycle, instret, end, watch_lo,
/// watch_hi) -> i64`, and the result is `(cross << 32) | value`: with `cross`
/// clear the value is the guest pc to leave at, and with it set the value is
/// the global block index to continue at.
///
/// # Panics
///
/// On an empty chunk — there is nothing to emit and the dispatcher would have
/// no arms.
#[must_use]
pub fn emit_body(
    set: &BlockSet,
    lo: usize,
    len: usize,
    model: CycleModel,
    layout: Layout,
    policy: Emit,
    mmio_load_func: u32,
) -> (Function, usize, usize) {
    assert!(len > 0, "an empty chunk has nothing to emit");
    let last = len - 1;
    let mut e = Emitter {
        f: Function::new(body_locals()),
        set,
        model,
        layout,
        policy,
        mmio_load_func,
        lo,
        last,
        extra: 0,
        cycles: 0,
        retired: 0,
        loop_mark: None,
        loop_committed: false,
        native_insts: 0,
        escaped_insts: 0,
    };

    // Prologue: the counters, the window state, the window itself.
    e.i(I::LocalGet(P_CYCLE));
    e.i(I::LocalSet(L_CYC));
    e.i(I::LocalGet(P_INSTRET));
    e.i(I::LocalSet(L_INSTRET));
    e.load_window_state();
    e.i(I::I32Const(0));
    e.i(I::LocalSet(L_DIRTY));
    e.fill_window();
    let flags_at = e.exchange(crate::LAYOUT.flags());
    e.i(I::I32Const(0));
    e.i(I::I32Load(flags_at));
    e.i(I::I32Const(FLAG_PENDING));
    e.i(I::I32And);
    e.i(I::LocalSet(L_PENDING));
    e.i(I::LocalGet(P_ENTRY));
    e.i(I::LocalSet(L_NEXT));

    // The dispatcher: `block $exit`, `loop $dispatch`, one `block` per guest
    // block, and a `br_table` whose arms are those blocks' labels. Exactly
    // the RV32 shape — it is the ABI's, not the instruction set's.
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

    // Epilogue: everything the stay carries goes back where the next reader
    // of it looks — the host at an exit, the next sub-dispatcher at a cross.
    e.spill_dirty();
    e.store_extra(crate::extra::SAR, L_SAR);
    // The mask itself, for the writeback census the host keeps: it is the
    // only number that says what the dirty mask bought.
    e.store_extra(crate::extra::DIRTY, L_DIRTY);
    let cycle_at = e.exchange(crate::LAYOUT.cycle());
    let instret_at = e.exchange(crate::LAYOUT.instret());
    let why_at = e.exchange(crate::LAYOUT.exit_why());
    e.i(I::I32Const(0));
    e.i(I::LocalGet(L_CYC));
    e.i(I::I64Store(cycle_at));
    e.i(I::I32Const(0));
    e.i(I::LocalGet(L_INSTRET));
    e.i(I::I64Store(instret_at));
    e.i(I::I32Const(0));
    e.i(I::LocalGet(L_FLAGS));
    e.i(I::LocalGet(L_PENDING));
    e.i(I::I32Or);
    e.i(I::I32Store(flags_at));
    e.i(I::I32Const(0));
    e.i(I::LocalGet(L_WHY));
    e.i(I::I32Store(why_at));
    // `(cross << 32) | value`.
    e.i(I::LocalGet(L_EXIT_PC));
    e.i(I::I64ExtendI32U);
    e.i(I::LocalGet(L_CROSS));
    e.i(I::I64ExtendI32U);
    e.i(I::I64Const(32));
    e.i(I::I64Shl);
    e.i(I::I64Or);
    e.i(I::End);

    (e.f, e.native_insts, e.escaped_insts)
}

pub(crate) struct Emitter<'a> {
    pub(crate) f: Function,
    pub(crate) set: &'a BlockSet,
    pub(crate) model: CycleModel,
    pub(crate) layout: Layout,
    pub(crate) policy: Emit,
    /// What an MMIO load calls: the import, or the module's own `$fast_load`
    /// when the machine published reads. The two have the same signature.
    pub(crate) mmio_load_func: u32,
    /// The global index of this sub-dispatcher's first block. Every `k` here
    /// is chunk-relative and every index in `set.index` is global.
    pub(crate) lo: usize,
    /// The index of the last block *in this chunk*, so branch depths can be
    /// computed.
    pub(crate) last: usize,
    /// Nesting added by `if`s and `block`s inside the current block body.
    pub(crate) extra: u32,
    /// Cycles and instructions retired natively since the counters were last
    /// handed back, within the block being emitted.
    pub(crate) cycles: u64,
    pub(crate) retired: u32,
    /// `Some(LBEG)` while emitting an instruction the walk marked as ending
    /// at a `LEND` (rule 6): the arm has to apply the loop-back.
    pub(crate) loop_mark: Option<u32>,
    /// Whether the marked instruction's decrement has been emitted yet: a
    /// store commits it before its poll, and the generic tail must not
    /// commit it twice.
    pub(crate) loop_committed: bool,
    pub(crate) native_insts: usize,
    pub(crate) escaped_insts: usize,
}

impl<'a> Emitter<'a> {
    pub(crate) fn i(&mut self, ins: I<'static>) {
        self.f.instruction(&ins);
    }

    pub(crate) fn cost(&self, class: InstClass) -> u64 {
        u64::from(self.model.cycles_for(class))
    }

    pub(crate) fn get(&mut self, r: u8) {
        self.i(I::LocalGet(reg_local(r)));
    }

    pub(crate) fn set(&mut self, r: u8) {
        self.i(I::LocalSet(reg_local(r)));
    }

    pub(crate) fn exchange(&self, field: u64) -> MemArg {
        memarg(u64::from(self.layout.exchange_offset) + field)
    }

    /// `i32.store` local `l` into extra word `w` of the exchange area.
    pub(crate) fn store_extra(&mut self, w: u32, l: u32) {
        let at = self.exchange(crate::extra(w));
        self.i(I::I32Const(0));
        self.i(I::LocalGet(l));
        self.i(I::I32Store(at));
    }

    /// `i32.load` extra word `w` of the exchange area into local `l`.
    pub(crate) fn load_extra(&mut self, w: u32, l: u32) {
        let at = self.exchange(crate::extra(w));
        self.i(I::I32Const(0));
        self.i(I::I32Load(at));
        self.i(I::LocalSet(l));
    }

    /// The seven window, shift and loop words, exchange area → locals.
    pub(crate) fn load_window_state(&mut self) {
        self.load_extra(crate::extra::WINDOW_BASE, L_WBASE);
        self.load_extra(crate::extra::WINDOW_START, L_WSTART);
        self.load_extra(crate::extra::SAR, L_SAR);
        self.load_extra(crate::extra::LBEG, L_LBEG);
        self.load_extra(crate::extra::LEND, L_LEND);
        self.load_extra(crate::extra::LCOUNT, L_LCOUNT);
        self.load_extra(crate::extra::PS_CALLINC, L_CALLINC);
    }

    // --- branch depths ------------------------------------------------------

    pub(crate) fn exit_depth(&self, k: usize) -> u32 {
        (self.last - k) as u32 + 1 + self.extra
    }
    pub(crate) fn dispatch_depth(&self, k: usize) -> u32 {
        (self.last - k) as u32 + self.extra
    }
    pub(crate) fn body_depth(&self, k: usize, j: usize) -> u32 {
        (j - k - 1) as u32 + self.extra
    }

    // --- counters -----------------------------------------------------------

    pub(crate) fn add_cycles(&mut self, cycles: u64) {
        if cycles != 0 {
            self.i(I::LocalGet(L_CYC));
            self.i(I::I64Const(cycles as i64));
            self.i(I::I64Add);
            self.i(I::LocalSet(L_CYC));
        }
    }

    pub(crate) fn add_retired(&mut self, retired: u32) {
        if retired != 0 {
            self.i(I::LocalGet(L_INSTRET));
            self.i(I::I64Const(i64::from(retired)));
            self.i(I::I64Add);
            self.i(I::LocalSet(L_INSTRET));
        }
    }

    /// Hand the pending cycles and retired instructions to the locals, so an
    /// import or an exit sees the counters the interpreter would hold.
    pub(crate) fn flush_counters(&mut self) {
        let (c, r) = (self.cycles, self.retired);
        self.cycles = 0;
        self.retired = 0;
        self.add_cycles(c);
        self.add_retired(r);
    }

    /// Store the counters into the exchange area (JD17): before every
    /// `step_one`, and wherever the host reads them.
    pub(crate) fn store_counters(&mut self) {
        let cycle_at = self.exchange(crate::LAYOUT.cycle());
        let instret_at = self.exchange(crate::LAYOUT.instret());
        self.i(I::I32Const(0));
        self.i(I::LocalGet(L_CYC));
        self.i(I::I64Store(cycle_at));
        self.i(I::I32Const(0));
        self.i(I::LocalGet(L_INSTRET));
        self.i(I::I64Store(instret_at));
    }

    // --- leaving ------------------------------------------------------------

    /// Leave the stay at `pc` — or at the address in [`L_ADDR`] when `None` —
    /// after handing back the pending counters.
    pub(crate) fn exit(&mut self, k: usize, pc: Option<u32>, flags: i32, why: i32) {
        self.i(I::I32Const(why));
        self.i(I::LocalSet(L_WHY));
        let (c, r) = (self.cycles, self.retired);
        self.add_cycles(c);
        self.add_retired(r);
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

    /// Hand a block this sub-dispatcher does not hold to the outer selector.
    pub(crate) fn cross(&mut self, k: usize, g: usize) {
        self.i(I::I32Const(g as i32));
        self.i(I::LocalSet(L_EXIT_PC));
        self.i(I::I32Const(1));
        self.i(I::LocalSet(L_CROSS));
        self.i(I::Br(self.exit_depth(k)));
    }

    /// Continue at the block whose global index is `g`, wherever it lives.
    /// Counters are already handed back.
    pub(crate) fn goto_index(&mut self, k: usize, g: usize) {
        match g.checked_sub(self.lo).filter(|&j| j <= self.last) {
            // A forward edge branches straight to the target's label.
            Some(j) if j > k => {
                let depth = self.body_depth(k, j);
                self.i(I::Br(depth));
            }
            // A backward edge, or this block again, goes round the dispatcher.
            Some(j) => {
                self.i(I::I32Const(j as i32));
                self.i(I::LocalSet(L_NEXT));
                self.i(I::Br(self.dispatch_depth(k)));
            }
            None => self.cross(k, g),
        }
    }

    /// Continue at `target`. Counters are already handed back.
    pub(crate) fn goto(&mut self, k: usize, target: u32) {
        match self.set.index.get(&target).copied() {
            Some(g) => self.goto_index(k, g),
            None => self.exit(k, Some(target), 0, why::EDGE_OUT),
        }
    }

    /// Continue at `pc`, which costs nothing at all when it is the block laid
    /// out next **in this function**.
    pub(crate) fn fall_through(&mut self, k: usize, pc: u32) {
        if k < self.last && self.set.blocks[self.lo + k + 1].pc == pc {
            return;
        }
        self.goto(k, pc);
    }

    // --- the escape hatch ---------------------------------------------------

    /// Hand one instruction to the interpreter and come back.
    ///
    /// The dirty groups and `SAR` are written back first, so the interpreter
    /// sees a complete file (the window, loop and `CALLINC` words are always
    /// current — see the module docs). Afterwards **everything** is reloaded:
    /// the instruction may have rotated the window, written any register, or
    /// moved the loop registers.
    ///
    /// Leaves the pc the interpreter reported in [`L_ADDR`].
    pub(crate) fn escape(&mut self, k: usize, pc: u32, d: &Decoded) {
        self.flush_counters();
        self.spill_dirty();
        self.store_extra(crate::extra::SAR, L_SAR);
        self.store_counters();

        self.i(I::I32Const(pc as i32));
        self.i(I::Call(F_STEP_ONE));
        self.i(I::LocalSet(L_ADDR));

        let cycle_at = self.exchange(crate::LAYOUT.cycle());
        let instret_at = self.exchange(crate::LAYOUT.instret());
        self.i(I::I32Const(0));
        self.i(I::I64Load(cycle_at));
        self.i(I::LocalSet(L_CYC));
        self.i(I::I32Const(0));
        self.i(I::I64Load(instret_at));
        self.i(I::LocalSet(L_INSTRET));
        self.load_window_state();
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_DIRTY));
        self.fill_window();

        // An escaped **load** may have left a yield on the bus that nothing
        // here can see (`step_one`'s status word means one thing, BD1). The
        // interpreter polls after the next Store- or System-class
        // instruction, so from here on every one of those is a polling
        // point. A poll that finds nothing pending is not observable.
        if d.class == InstClass::Load {
            self.i(I::I32Const(FLAG_PENDING));
            self.i(I::LocalSet(L_PENDING));
        }

        // The interpreter does not get to have a `waiti`, a `break`, a bus
        // yield or a fault swallowed by a translated stay.
        let status_at = self.exchange(crate::LAYOUT.status());
        self.i(I::I32Const(0));
        self.i(I::I32Load(status_at));
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.i(I::LocalGet(L_ADDR));
        self.i(I::LocalSet(L_EXIT_PC));
        self.i(I::I32Const(FLAG_SLICE_ENDED));
        self.i(I::LocalSet(L_FLAGS));
        self.i(I::I32Const(why::SLICE_ENDED));
        self.i(I::LocalSet(L_WHY));
        self.i(I::Br(self.exit_depth(k)));
        self.extra -= 1;
        self.i(I::End);
    }

    /// After escaping a non-terminator: anything that moved the hart off the
    /// decoder's straight line — a trap, an interrupt delivered inside the
    /// instruction, a width the interpreter disagrees about — leaves the
    /// block, exactly as `run_block`'s own straight-on check does.
    fn escape_straight_on(&mut self, k: usize, next_pc: u32) {
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(next_pc as i32));
        self.i(I::I32Ne);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, None, 0, why::ESCAPE_DIVERGED);
        self.extra -= 1;
        self.i(I::End);
    }

    /// After escaping a terminator: the interpreter has already decided the
    /// next pc, so the only thing left is to see whether it is somewhere this
    /// module can carry on. The candidates are the static edges
    /// ([`edges`]) plus, for an instruction the walk marked as a loop end,
    /// the walk's `LBEG` and the sequential successor.
    fn escape_terminator(&mut self, k: usize, pc: u32, d: &Decoded) {
        let next = pc.wrapping_add(u32::from(d.width));
        let mut candidates: [Option<u32>; 3] = [None, None, None];
        match edges(pc, d) {
            Edges::None => {}
            Edges::Jump(t) => candidates[0] = Some(t),
            Edges::Branch { target, next } => {
                candidates[0] = Some(target);
                candidates[1] = Some(next);
            }
            Edges::Call { target, .. } => candidates[0] = target,
            Edges::Loop { body, end } => {
                candidates[0] = Some(body);
                candidates[1] = Some(end);
            }
            Edges::Next(n) => candidates[0] = Some(n),
        }
        if let Some(lbeg) = d.lbeg {
            candidates[2] = Some(lbeg);
            if candidates[0].is_none() {
                candidates[0] = Some(next);
            }
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
        self.exit(k, None, 0, why::ESCAPE_TARGET);
    }

    // --- the policy ---------------------------------------------------------

    /// Is this instruction emitted natively under the policy in force?
    fn emits(&self, d: &Decoded) -> bool {
        if refusal_of(d).is_some() {
            return false;
        }
        match d.inst {
            Inst::Load(..)
            | Inst::Store(..)
            | Inst::L32iN(..)
            | Inst::S32iN(..)
            | Inst::L32r(..) => self.policy.memory,
            Inst::BranchRr(..)
            | Inst::BranchRi(..)
            | Inst::BranchRiu(..)
            | Inst::BranchZ(..)
            | Inst::BranchBiI(..)
            | Inst::BranchZN(..)
            | Inst::J(_)
            | Inst::Jx(_)
            | Inst::Call(..)
            | Inst::Callx(..)
            | Inst::Loop(..)
            | Inst::Entry(..)
            | Inst::Nullary(NullaryOp::Ret | NullaryOp::Retw)
            | Inst::NullaryN(NullaryNarrowOp::RetN | NullaryNarrowOp::RetwN) => {
                self.policy.control
            }
            // A body instruction the walk marked as a loop end is a
            // terminator whose flow is the loop-back's: it needs the control
            // arms as well as its own.
            _ if d.lbeg.is_some() => self.policy.alu && self.policy.control,
            _ => self.policy.alu,
        }
    }

    // --- a block ------------------------------------------------------------

    fn block(&mut self, k: usize) {
        let set: &'a BlockSet = self.set;
        let b: &'a Block = &set.blocks[self.lo + k];
        let block_pc = b.pc;
        let pcs: &'a [(u32, Decoded)] = &b.insts;
        let end = b.end;
        self.cycles = 0;
        self.retired = 0;

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
        self.exit(k, Some(block_pc), 0, why::BUDGET);
        self.extra -= 1;
        self.i(I::End);

        // The window precondition (XD5), hoisted over the block: the
        // maximum group any of its instructions reaches, `entry` counted at
        // its `as` alone (its `CALLINC` half is tested live at the arm).
        // Only a block that retires something natively needs it; an
        // all-escape block is checked by the interpreter, instruction by
        // instruction.
        let natives = pcs.iter().filter(|(_, d)| self.emits(d)).count();
        if natives > 0 {
            let group = pcs
                .iter()
                .map(|(_, d)| match d.inst {
                    Inst::Entry(rs, _) => rs.num() >> 2,
                    _ => d.group,
                })
                .max()
                .unwrap_or(0);
            self.window_precondition(k, block_pc, group);
            // The groups this block's native arms write, marked dirty up
            // front. Over-approximation is safe (module docs).
            let mut dirty = 0u8;
            for (_, d) in pcs {
                if self.emits(d) {
                    dirty |= window::written_groups(&d.inst);
                }
            }
            if dirty != 0 {
                self.i(I::LocalGet(L_DIRTY));
                self.i(I::I32Const(i32::from(dirty)));
                self.i(I::I32Or);
                self.i(I::LocalSet(L_DIRTY));
            }
        }

        for (pc, d) in pcs {
            let (pc, d) = (*pc, *d);
            let next_pc = pc.wrapping_add(u32::from(d.width));

            if !self.emits(&d) {
                self.escaped_insts += 1;
                self.escape(k, pc, &d);
                if d.control {
                    self.escape_terminator(k, pc, &d);
                    return;
                }
                self.escape_straight_on(k, next_pc);
                continue;
            }

            self.native_insts += 1;
            self.loop_mark = d.lbeg;
            if self.loop_mark.is_some() {
                self.loop_prelude(next_pc);
            }
            let terminated = self.arm(k, pc, &d);
            self.loop_mark = None;
            if terminated {
                return;
            }
        }

        self.flush_counters();
        match end {
            BlockEnd::Term => {
                unreachable!("a block ending in a terminator returns from the loop above")
            }
            BlockEnd::Fall(pc) => self.fall_through(k, pc),
            BlockEnd::Undecodable(pc) => self.exit(k, Some(pc), 0, why::UNDECODABLE),
        }
    }

    /// Emit one native arm. Returns `true` when the arm decided the next pc
    /// itself (the block is over).
    fn arm(&mut self, k: usize, pc: u32, d: &Decoded) -> bool {
        let next_pc = pc.wrapping_add(u32::from(d.width));
        match d.inst {
            Inst::Load(op, rt, rs, off) => {
                self.load(k, pc, mem::LoadShape::Op(op), rt.num(), rs.num(), off)
            }
            Inst::L32iN(rt, rs, off) => {
                self.load(k, pc, mem::LoadShape::Word, rt.num(), rs.num(), off)
            }
            Inst::L32r(rt, field) => self.l32r(k, pc, rt.num(), field),
            Inst::Store(op, rt, rs, off) => {
                self.store(k, pc, d.width, mem::StoreShape::Op(op), rt.num(), rs.num(), off)
            }
            Inst::S32iN(rt, rs, off) => {
                self.store(k, pc, d.width, mem::StoreShape::Word, rt.num(), rs.num(), off)
            }
            Inst::BranchRr(..)
            | Inst::BranchRi(..)
            | Inst::BranchRiu(..)
            | Inst::BranchZ(..)
            | Inst::BranchBiI(..)
            | Inst::BranchZN(..) => {
                self.branch(k, pc, d);
                return true;
            }
            Inst::J(_) => {
                self.jump(k, pc, d);
                return true;
            }
            Inst::Jx(rs) => {
                self.jx(k, pc, rs.num());
                return true;
            }
            Inst::Call(op, _) => {
                self.call(k, pc, d, op);
                return true;
            }
            Inst::Callx(op, rs) => {
                self.callx(k, pc, op, rs.num());
                return true;
            }
            Inst::Loop(op, rs, imm8) => {
                self.loop_inst(k, pc, d.width, op, rs.num(), imm8);
                return true;
            }
            Inst::Entry(rs, imm) => {
                self.entry(k, pc, rs.num(), imm);
                return true;
            }
            Inst::Nullary(NullaryOp::Ret) | Inst::NullaryN(NullaryNarrowOp::RetN) => {
                self.ret(k, pc);
                return true;
            }
            Inst::Nullary(NullaryOp::Retw) | Inst::NullaryN(NullaryNarrowOp::RetwN) => {
                self.retw(k, pc);
                return true;
            }
            _ => self.alu(k, pc, d),
        }
        // A straight-line instruction retired natively.
        self.cycles += self.cost(d.class);
        self.retired += 1;
        if self.loop_mark.is_some() {
            self.loop_back_next(k, next_pc);
            return true;
        }
        false
    }
}
