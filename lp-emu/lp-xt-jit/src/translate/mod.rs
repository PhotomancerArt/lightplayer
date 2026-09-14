//! The Xtensa body emitter: the module shape, filled in with escapes.
//!
//! [`lp_emu_jit::dispatch::emit_module_with`] owns everything that is the ABI
//! — the four imports and their signatures, the imported memory, the outer
//! selector, the function table, the export, the body budget. What this module
//! supplies is the **body**: the prologue, the dispatcher loop, the per-block
//! budget compare, and one `step_one` call per guest instruction.
//!
//! # Every instruction escapes, and that is the point
//!
//! This is the RV32 side's `Emit::NOTHING` build, which is kept there as a
//! test because it is the one translation that cannot be wrong about a guest
//! instruction: it does not claim to know what any of them do. What it *can*
//! be wrong about is the seam — the counters at each observation point, the
//! exit protocol, the slice end, the way a stay hands the hart back — and that
//! is exactly the thing this phase has to get right once, so that P06's
//! emitted arms are the only new risk when they land.
//!
//! So: no register locals (the hart keeps the AR file — see the crate docs),
//! no permission checks (there is no emitted access to check), no
//! indirect-target table (every control transfer leaves), and no static edge
//! resolution.
//!
//! # What a terminator does here, and what P05/P06 will change
//!
//! The RV32 emitter, after escaping a terminator, compares the pc the
//! interpreter reported against the one or two targets the *decoded*
//! instruction names statically, and branches inside the module when one
//! matches. This emitter does not: every escaped terminator leaves the stay,
//! and the hart re-enters through its entry table at whatever pc the
//! interpreter produced.
//!
//! That is a **cost, not a correctness gap** — the machine's transcript is the
//! same either way, and it is the one this phase is measured on — and it is
//! deliberate. Resolving an Xtensa branch target statically means re-deriving
//! each variant's immediate encoding (`lp-xt-inst` stores the raw encoded
//! field, never an absolute address, so that `encode(decode(w)) == w` holds
//! independent of pc). That derivation belongs with P05's sweep, which needs
//! exactly the same answer to find block starts, and writing it twice — once
//! here from the emitter's side, once there — is how the two come to disagree.
//! Until then a stay is roughly one block long, which the coverage line says
//! plainly rather than hiding.

use alloc::borrow::Cow;
use alloc::vec::Vec;

use lp_emu_core::CycleModel;
use lp_emu_jit::dispatch::{Selector, emit_module_with};
use lp_emu_jit::host::{FLAG_PENDING, FLAG_SLICE_ENDED, why};
use lp_emu_jit::translate::{Emitted, F_STEP_ONE, Layout};
use wasm_encoder::{BlockType, Function, Instruction as I, MemArg, ValType};

use crate::blocks::{BlockEnd, BlockSet};

/// The sub-dispatcher's six parameters, in the order
/// [`lp_emu_jit::dispatch::emit_module_with`] declares them.
///
/// Positional and hand-written because the ABI declares the type and the body
/// emitter fills it in; the two sides of that contract are
/// `dispatch::emit_module_with`'s type section and this list. The two watch
/// parameters are the inline-store watchpoint window, which nothing this phase
/// emits looks at — they are named so the count is right.
const P_ENTRY: u32 = 0;
const P_CYCLE: u32 = 1;
const P_INSTRET: u32 = 2;
const P_END: u32 = 3;
const _P_WATCH_LO: u32 = 4;
const _P_WATCH_HI: u32 = 5;

/// The locals, past the six parameters. See [`body_locals`].
const L_CYC: u32 = 6;
const L_INSTRET: u32 = 7;
const L_EXIT_PC: u32 = 8;
const L_FLAGS: u32 = 9;
const L_NEXT: u32 = 10;
/// The obligation an MMIO load left. Never set by anything this phase emits —
/// carried through the prologue and epilogue so a cross-function edge behaves
/// the way the ABI says it does, and so P06 has it already wired.
const L_PENDING: u32 = 11;
/// The pc the interpreter reported from the last escape.
const L_ADDR: u32 = 12;
const L_CROSS: u32 = 13;
const L_WHY: u32 = 14;

/// The locals every sub-dispatcher declares, past its six parameters.
///
/// One list in one place, because the indices above are hand-assigned and a
/// mismatch is a validation error a long way into a function body.
fn body_locals() -> Vec<(u32, ValType)> {
    alloc::vec![(2, ValType::I64), (7, ValType::I32)]
}

fn memarg(offset: u64) -> MemArg {
    MemArg {
        offset,
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
pub fn emit_module(set: &BlockSet, model: CycleModel, layout: Layout, fn_blocks: usize) -> Emitted {
    emit_module_with(
        set,
        layout,
        crate::LAYOUT,
        Selector::DEFAULT,
        fn_blocks,
        &|set, lo, len, _load_func| emit_body(set, lo, len, model, layout),
    )
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
) -> (Function, usize, usize) {
    assert!(len > 0, "an empty chunk has nothing to emit");
    let last = len - 1;
    let mut e = Emitter {
        f: Function::new(body_locals()),
        set,
        model,
        layout,
        lo,
        last,
        extra: 0,
        escaped_insts: 0,
    };

    // Prologue. No register loads: the hart keeps the file (crate docs).
    e.i(I::LocalGet(P_CYCLE));
    e.i(I::LocalSet(L_CYC));
    e.i(I::LocalGet(P_INSTRET));
    e.i(I::LocalSet(L_INSTRET));
    let flags_at = e.exchange(crate::LAYOUT.flags());
    e.i(I::I32Const(0));
    e.i(I::I32Load(flags_at));
    e.i(I::I32Const(FLAG_PENDING));
    e.i(I::I32And);
    e.i(I::LocalSet(L_PENDING));
    e.i(I::LocalGet(P_ENTRY));
    e.i(I::LocalSet(L_NEXT));

    // The dispatcher: `block $exit`, `loop $dispatch`, one `block` per guest
    // block, and a `br_table` whose arms are those blocks' labels. Exactly the
    // RV32 shape — it is the ABI's, not the instruction set's.
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

    // Epilogue: everything the stay carries goes back where the next reader of
    // it looks — the host at an exit, the next sub-dispatcher at a cross.
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

    let escaped = e.escaped_insts;
    (e.f, 0, escaped)
}

struct Emitter<'a> {
    f: Function,
    set: &'a BlockSet,
    model: CycleModel,
    layout: Layout,
    /// The global index of this sub-dispatcher's first block. Every `k` here
    /// is chunk-relative and every index in `set.index` is global.
    lo: usize,
    /// The index of the last block *in this chunk*, so branch depths can be
    /// computed.
    last: usize,
    /// Nesting added by `if`s inside the current block body.
    extra: u32,
    escaped_insts: usize,
}

impl<'a> Emitter<'a> {
    fn i(&mut self, ins: I<'static>) {
        self.f.instruction(&ins);
    }

    fn cost(&self, class: lp_emu_core::InstClass) -> u64 {
        u64::from(self.model.cycles_for(class))
    }

    fn exchange(&self, field: u64) -> MemArg {
        memarg(u64::from(self.layout.exchange_offset) + field)
    }

    fn exit_depth(&self, k: usize) -> u32 {
        (self.last - k) as u32 + 1 + self.extra
    }
    fn dispatch_depth(&self, k: usize) -> u32 {
        (self.last - k) as u32 + self.extra
    }
    fn body_depth(&self, k: usize, j: usize) -> u32 {
        (j - k - 1) as u32 + self.extra
    }

    /// Leave the stay at `pc` — or at the address in [`L_ADDR`] when `None`.
    fn exit(&mut self, k: usize, pc: Option<u32>, flags: i32, why: i32) {
        self.i(I::I32Const(why));
        self.i(I::LocalSet(L_WHY));
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
    fn cross(&mut self, k: usize, g: usize) {
        self.i(I::I32Const(g as i32));
        self.i(I::LocalSet(L_EXIT_PC));
        self.i(I::I32Const(1));
        self.i(I::LocalSet(L_CROSS));
        self.i(I::Br(self.exit_depth(k)));
    }

    /// Continue at the block whose global index is `g`, wherever it lives.
    fn goto_index(&mut self, k: usize, g: usize) {
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

    /// Continue at `target`, which the block set holds.
    fn goto(&mut self, k: usize, target: u32) {
        match self.set.index.get(&target).copied() {
            Some(g) => self.goto_index(k, g),
            None => {
                self.i(I::I32Const(target as i32));
                self.i(I::LocalSet(L_ADDR));
                self.exit(k, None, 0, why::EDGE_OUT);
            }
        }
    }

    /// The escape: hand the counters over, run one guest instruction on the
    /// hart, take the counters back.
    ///
    /// No register file crosses here. The RV32 emitter flushes and reloads its
    /// live locals around this call because it keeps guest registers in them;
    /// this one keeps none, so the whole marshalling question — the one place
    /// a translator quietly corrupts a register file — does not exist yet. The
    /// driver's [`lp_emu_jit::host::HostOps::step_one_wide`] override runs
    /// `XtHart::step_one` on the hart itself.
    ///
    /// Leaves the pc the interpreter reported in [`L_ADDR`].
    fn escape(&mut self, k: usize, pc: u32) {
        let cycle_at = self.exchange(crate::LAYOUT.cycle());
        let instret_at = self.exchange(crate::LAYOUT.instret());
        let status_at = self.exchange(crate::LAYOUT.status());
        // JD17: the counters are handed back at every point the bus can
        // observe them, and an escape is one — the interpreter is about to
        // read `CCOUNT` off them.
        self.i(I::I32Const(0));
        self.i(I::LocalGet(L_CYC));
        self.i(I::I64Store(cycle_at));
        self.i(I::I32Const(0));
        self.i(I::LocalGet(L_INSTRET));
        self.i(I::I64Store(instret_at));

        self.i(I::I32Const(pc as i32));
        self.i(I::Call(F_STEP_ONE));
        self.i(I::LocalSet(L_ADDR));

        self.i(I::I32Const(0));
        self.i(I::I64Load(cycle_at));
        self.i(I::LocalSet(L_CYC));
        self.i(I::I32Const(0));
        self.i(I::I64Load(instret_at));
        self.i(I::LocalSet(L_INSTRET));

        // The interpreter does not get to have a `waiti`, a `break`, a bus
        // yield or a fault swallowed by a translated stay.
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
    /// instruction, a `loop` back-edge the decoder does not model, a width the
    /// interpreter disagrees about — leaves the block, exactly as
    /// `run_block`'s own straight-on check does.
    ///
    /// The `loop` back-edge is why this is load-bearing on Xtensa in a way it
    /// is not on RV32: `LCOUNT` is tested against `LEND` after **every**
    /// instruction, so any body slot can be the last one of a loop iteration
    /// and jump to `LBEG`. The check below catches that with no knowledge of
    /// loops at all.
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

    fn block(&mut self, k: usize) {
        let set: &'a BlockSet = self.set;
        let b = &set.blocks[self.lo + k];
        let block_pc = b.pc;
        let insts = &b.insts;
        let end = b.end;

        // The budget rule (M5 MD3): not one instruction may start at or past
        // `end`, so the block runs whole only if the most it can cost fits.
        // One that does not fit is handed back, and the interpreter runs it
        // with the per-instruction compare it already has.
        let max: u64 = insts.iter().map(|(_, d)| self.cost(d.class)).sum();
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

        for (pc, d) in insts {
            let (pc, d) = (*pc, *d);
            self.escaped_insts += 1;
            self.escape(k, pc);
            if d.control {
                // Every escaped terminator leaves: this emitter resolves no
                // static targets (see the module docs).
                self.exit(k, None, 0, why::ESCAPE_TARGET);
                return;
            }
            self.escape_straight_on(k, pc.wrapping_add(u32::from(d.width)));
        }

        match end {
            BlockEnd::Term => {
                unreachable!("a block ending in a terminator returns from the loop above")
            }
            BlockEnd::Fall(pc) => self.goto(k, pc),
            BlockEnd::Undecodable(pc) => self.exit(k, Some(pc), 0, why::UNDECODABLE),
        }
    }
}
