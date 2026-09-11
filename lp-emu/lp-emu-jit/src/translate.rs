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
//! - **A store runs polling point (c) without leaving** (M7b P2). The store
//!   the bus serves fuses the poll into `mmio_store`'s own call; the inline
//!   RAM store that owes one because a load left a yield calls `poll`. The
//!   hart's own polling code runs, at the same instruction, with the
//!   post-store pc and counters — and the stay carries on unless it moved.
//! - **Every exit reports** pc, cycle count, retired-instruction count and the
//!   flags, so the interpreter resumes with identical state. The protocol is
//!   written out in [`crate::host`].
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
use wasm_encoder::{BlockType, Function, Instruction as I, MemArg, ValType};

use crate::blocks::{Block, BlockEnd, BlockSet};
use crate::decode::{Cond, Decoded, Inst, LoadKind, Op, OpI, StoreKind};
use crate::host::{
    EXCHANGE_CYCLE, EXCHANGE_EXIT_WHY, EXCHANGE_FLAGS, EXCHANGE_INDIRECT_MISS, EXCHANGE_INSTRET,
    EXCHANGE_REGS, EXCHANGE_STATUS, FLAG_PENDING, FLAG_SLICE_ENDED, MMIO_PENDING, MMIO_REFUSED,
    FAST_ARMED, FAST_SERVED, FAST_WORDS, MMIO_SLICE_ENDED, PERM_READ_WRITE, PERM_SHIFT,
    load_kind, store_kind, why,
};

/// The module the host imports `memory` from, and the module the four
/// functions are imported from. One name for all five: the emulator instance
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
    /// Where the indirect-target page map starts, when the host built one.
    ///
    /// This is what makes `jalr` — every guest **return** — an in-module
    /// branch instead of an exit (P5). See [`crate::dispatch`] for the two
    /// tables' shape and for why they are two. `None` emits P3's behaviour:
    /// every indirect jump leaves. A host that passes `Some` is promising the
    /// page map is fully initialised, because a partly-written one would read
    /// guest data as a block id.
    pub indirect: Option<u32>,
    /// The published MMIO word reads this machine guarantees, if any (M7b P3).
    ///
    /// `None` emits M7b P2's behaviour: every MMIO load crosses to the host.
    pub fast_reads: Option<FastReads>,
}

/// Where a published MMIO word read gets its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FastSource {
    /// The register reads as this constant, whatever the machine is doing.
    Constant(i32),
    /// The register reads the published word in this slot of the block, which
    /// the machine republishes whenever its model's own value moves.
    Published(u32),
}

/// One published MMIO word read: a guest **word** address and where its value
/// comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FastRead {
    pub address: u32,
    pub source: FastSource,
}

/// The published-read table folded into a module at emission time.
///
/// See [`crate::host`] for the block's layout and for why `armed` is the whole
/// correctness story.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FastReads {
    /// Where the published-read block sits in the imported memory.
    pub offset: u32,
    /// The addresses served. Order is the order they are compared in, so the
    /// machine puts its busiest register first.
    pub reads: [Option<FastRead>; crate::host::FAST_MAX_READS],
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
    /// The outer selector's shape — see [`crate::dispatch::Selector`].
    ///
    /// Not an instruction class, and on this struct anyway because `Emit` is
    /// already the emission policy every caller threads through
    /// [`crate::dispatch::emit_module`], and a second parameter would be a
    /// signature change at six call sites to carry one bool.
    pub selector: crate::dispatch::Selector,
}

impl Emit {
    /// Every instruction through the escape hatch: the complete, correct, slow
    /// translation this phase built first and keeps as a test.
    pub const NOTHING: Self = Self {
        alu: false,
        memory: false,
        control: false,
        selector: crate::dispatch::Selector::DEFAULT,
    };
    /// Everything the translator knows how to emit.
    pub const EVERYTHING: Self = Self {
        alu: true,
        memory: true,
        control: true,
        selector: crate::dispatch::Selector::DEFAULT,
    };
}

/// What one call to [`emit`](crate::dispatch::emit_module) produced.
#[derive(Clone, Debug)]
pub struct Emitted {
    pub wasm: Vec<u8>,
    /// Instructions the module runs itself.
    pub native_insts: usize,
    /// Instructions the module hands to `step_one`.
    pub escaped_insts: usize,
    /// Sub-dispatchers, not counting the outer selector.
    pub functions: usize,
    /// The largest body in the module, in bytes — **the selector included**.
    /// The number the wasm function-size limit applies to, and the reason a
    /// module is sized by blocks-per-function rather than by blocks (JD8,
    /// JD26).
    ///
    /// Until M7 P6c this was the largest *sub-dispatcher*, which is the same
    /// number only when the sub-dispatchers are the big functions. Below 64
    /// blocks a function they are not: P6b measured the nested selector at
    /// 730,452 B against a largest sub-dispatcher of 40,244 B at 8 blocks a
    /// function, so [`crate::dispatch::BODY_BUDGET`] — the check that lets
    /// `install` refuse a module and retry smaller — had a blind spot at
    /// exactly the sizes where the selector is the module's largest function.
    pub max_body_bytes: usize,
    /// The largest **sub-dispatcher** body, in bytes — what `max_body_bytes`
    /// alone used to mean. Kept because the two numbers diverging is the
    /// signal that the selector has taken over as the module's largest
    /// function, and a report that prints only the maximum cannot say which
    /// of the two it is.
    pub max_sub_body_bytes: usize,
    /// The outer selector's own body, in bytes.
    ///
    /// `O(count)` with [`crate::dispatch::Selector::Nested`] and `O(1)` with
    /// [`crate::dispatch::Selector::Flat`], and `count` is
    /// `blocks / fn_blocks` — so with the nested form this grows as the size
    /// knob shrinks, in the opposite direction to
    /// [`max_sub_body_bytes`](Self::max_sub_body_bytes).
    pub selector_bytes: usize,
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
/// Set when the value in [`L_EXIT_PC`] is a **global block index** to continue
/// at in another sub-dispatcher, rather than a guest pc to leave at (P5).
const L_CROSS: u32 = 49;
/// The global block index an indirect jump's target lookup produced, or `-1`.
const L_GID: u32 = 50;
/// Why the stay is ending. Reported at the exit, never acted on.
const L_WHY: u32 = 51;

/// The locals every sub-dispatcher declares, past its six parameters.
///
/// One list, in one place, because the indices above are hand-assigned and a
/// mismatch is a validation error a hundred kilobytes into a function body.
fn body_locals() -> Vec<(u32, ValType)> {
    alloc::vec![
        (31, ValType::I32), // x1..x31
        (2, ValType::I64),  // cycle, instret
        (8, ValType::I32),  // exit pc, flags, next, pending, address, perm x2, status
        (1, ValType::I64),  // the 64-bit scratch an mmio load returns into
        (1, ValType::I32),  // the 32-bit scratch a divide needs
        (3, ValType::I32),  // the cross-function flag, the resolved block id, the exit reason
    ]
}

pub(crate) const F_MMIO_LOAD: u32 = 0;
pub(crate) const F_MMIO_STORE: u32 = 1;
pub(crate) const F_STEP_ONE: u32 = 2;
/// Polling point (c) for a store the bus never saw (M7b P2).
pub(crate) const F_POLL: u32 = 3;
/// The first function index a sub-dispatcher can have: the four imports come
/// first, and the module defines everything after them.
pub(crate) const F_FIRST_BODY: u32 = 4;

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
    /// What an MMIO load calls: the import, or the module's own `$fast_load`
    /// when the machine published reads (M7b P3). The two have the same
    /// signature, so choosing between them costs the call site nothing.
    mmio_load_func: u32,
    /// The global index of this sub-dispatcher's first block. Every `k` in
    /// this emitter is **chunk-relative**; every index in `set.index` and in
    /// the target table is global, and `lo` is the one place the two meet.
    lo: usize,
    /// The index of the last block *in this chunk*, so branch depths can be
    /// computed.
    last: usize,
    /// Nesting added by `if`s inside the current block body.
    extra: u32,
    native_insts: usize,
    escaped_insts: usize,
}

impl<'a> Emitter<'a> {
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
    fn exit(&mut self, k: usize, pc: Option<u32>, cycles: u64, retired: u32, flags: i32, why: i32) {
        self.i(I::I32Const(why));
        self.i(I::LocalSet(L_WHY));
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

    /// Leave for the block whose **global** index is `g`, which this
    /// sub-dispatcher does not hold: hand it to the outer selector (JD8).
    ///
    /// Everything a stay carries is flushed by the same epilogue an exit uses
    /// — the counters, the live registers and the pending-yield obligation —
    /// and the next sub-dispatcher's prologue reloads exactly the same things
    /// (JD17). The one difference is the return value: `L_CROSS` says the
    /// `i32` is a block index rather than a guest pc.
    fn cross(&mut self, k: usize, g: usize) {
        self.i(I::I32Const(g as i32));
        self.i(I::LocalSet(L_EXIT_PC));
        self.i(I::I32Const(1));
        self.i(I::LocalSet(L_CROSS));
        self.i(I::Br(self.exit_depth(k)));
    }

    /// Continue at the block whose global index is `g`, wherever it lives.
    fn goto_index(&mut self, k: usize, g: usize) {
        let local = g.checked_sub(self.lo).filter(|&j| j <= self.last);
        match local {
            // A forward edge branches straight to the target's label.
            Some(j) if j > k => self.i(I::Br(self.body_depth(k, j))),
            // A back edge goes through this function's own dispatcher.
            Some(j) => {
                self.i(I::I32Const(j as i32));
                self.i(I::LocalSet(L_NEXT));
                self.i(I::Br(self.dispatch_depth(k)));
            }
            None => self.cross(k, g),
        }
    }

    /// Continue at `target`, wherever it is. Counters are already charged.
    fn goto(&mut self, k: usize, target: u32) {
        match self.set.index.get(&target).copied() {
            Some(g) => self.goto_index(k, g),
            None => self.exit(k, Some(target), 0, 0, 0, why::EDGE_OUT),
        }
    }

    /// Continue at `pc`, which costs nothing at all when it is the block laid
    /// out next **in this function** — control simply falls out of this body
    /// into that one. The block after the chunk's last is another function's,
    /// so there is nothing to fall into and the edge goes through the
    /// selector like any other.
    fn fall_through(&mut self, k: usize, pc: u32) {
        if k < self.last && self.set.blocks[self.lo + k + 1].pc == pc {
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
        self.i(I::I32Const(why::SLICE_ENDED));
        self.i(I::LocalSet(L_WHY));
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
        self.exit(k, None, 0, 0, 0, why::ESCAPE_DIVERGED);
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
        self.exit(k, None, 0, 0, 0, why::ESCAPE_TARGET);
    }

    // --- indirect jumps -----------------------------------------------------

    /// Continue at the address in [`L_ADDR`], resolved through the target
    /// table (P5): two loads, no search, no host call.
    ///
    /// `jalr` is every guest **return**, so P3's "an indirect jump always
    /// leaves" put a host round trip on the most common control transfer in
    /// the program and capped what installing more blocks could ever buy.
    /// The table answers "does a block start at this pc, and which one" in
    /// O(1) for every executable address, and the answer is a *global* block
    /// index, so a return into another sub-dispatcher is a cross-function
    /// edge rather than an exit.
    ///
    /// Counters are charged before the lookup, so every path out of here —
    /// resolved, cross, or missed — leaves them exactly where an exit would
    /// (JD17).
    fn indirect(&mut self, k: usize, cycles: u64, retired: u32) {
        self.add_cycles(cycles);
        self.add_retired(retired);
        let Some(pagemap) = self.layout.indirect else {
            // No table: P3's behaviour, and the shape every test that does
            // not build one still gets.
            self.exit(k, None, 0, 0, 0, why::INDIRECT_NO_TABLE);
            return;
        };

        // slots = pagemap[addr >> PERM_SHIFT]; gid = slots[(addr & mask) >> 1]
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(PERM_SHIFT as i32));
        self.i(I::I32ShrU);
        self.i(I::I32Const(2));
        self.i(I::I32Shl);
        self.i(I::I32Load(memarg(u64::from(pagemap))));
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(((1u32 << PERM_SHIFT) - 1) as i32));
        self.i(I::I32And);
        // The pc has already had its low bit cleared, so halving the offset
        // and scaling by the four-byte slot is one left shift.
        self.i(I::I32Const(1));
        self.i(I::I32Shl);
        self.i(I::I32Add);
        self.i(I::I32Load(memarg(0)));
        self.i(I::LocalTee(L_GID));

        // Miss: no block starts there. Count it and leave.
        self.i(I::I32Const(-1));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        let miss_at = self.exchange(EXCHANGE_INDIRECT_MISS);
        self.i(I::I32Const(0));
        self.i(I::I32Const(0));
        self.i(I::I64Load(miss_at));
        self.i(I::I64Const(1));
        self.i(I::I64Add);
        self.i(I::I64Store(miss_at));
        self.exit(k, None, 0, 0, 0, why::INDIRECT_MISS);
        self.extra -= 1;
        self.i(I::End);

        // In this function: straight into the dispatcher. `lo` is folded in,
        // so this is one subtract and one unsigned compare.
        self.i(I::LocalGet(L_GID));
        self.i(I::I32Const(self.lo as i32));
        self.i(I::I32Sub);
        self.i(I::LocalTee(L_T));
        self.i(I::I32Const(self.last as i32 + 1));
        self.i(I::I32LtU);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.i(I::LocalGet(L_T));
        self.i(I::LocalSet(L_NEXT));
        self.i(I::Br(self.dispatch_depth(k)));
        self.extra -= 1;
        self.i(I::End);

        // Somewhere else in the module.
        self.i(I::LocalGet(L_GID));
        self.i(I::LocalSet(L_EXIT_PC));
        self.i(I::I32Const(1));
        self.i(I::LocalSet(L_CROSS));
        self.i(I::Br(self.exit_depth(k)));
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
        self.i(I::Call(self.mmio_load_func));
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
        self.exit(k, Some(pc), cycles, retired, 0, why::LOAD_REFUSED);
        self.extra -= 1;
        self.i(I::End);
        self.i(I::LocalGet(L_T64));
        self.i(I::I32WrapI64);
        self.i(I::Else);
        // Straddling a RAM page and a non-RAM page: one guest access the bus
        // would serve as one and this cannot. Leave and let it.
        self.exit(k, Some(pc), cycles, retired, 0, why::LOAD_STRADDLE);
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
        self.exit(k, Some(pc), cycles, retired, 0, why::STORE_PERM);
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
        // The bus serves the store and runs the polling point in the same
        // crossing (M7b P2): the store's own `(pc, cycle)` first, then the
        // post-store state the hart would be holding when it polled.
        self.i(I::I32Const(pc as i32));
        self.i(I::LocalGet(L_CYC));
        self.i(I::I64Const(cycles as i64));
        self.i(I::I64Add);
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(kind_code as i32));
        self.get(rs2);
        self.post_store_state(pc, inst_width, cost, cycles, retired);
        self.i(I::Call(F_MMIO_STORE));
        self.unpack_poll();
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_REFUSED as i32));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        // The access did not happen, so no polling point ran and the
        // obligation an earlier load left is still live.
        self.exit(k, Some(pc), cycles, retired, 0, why::STORE_REFUSED);
        self.extra -= 1;
        self.i(I::End);
        // The store retired and the host polled, so whatever the bus was
        // holding is taken.
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_PENDING));
        self.i(I::Else);
        // Read-only RAM, or an access straddling two kinds of page.
        self.exit(k, Some(pc), cycles, retired, 0, why::STORE_STRADDLE);
        self.i(I::End);
        self.extra -= 1;
        self.i(I::End);
        self.extra -= 1;

        // Polling point (c)'s tail, and the **only** thing the common path
        // pays for it: one `or` and one never-taken branch, which is a shape
        // cheaper than the pre-P2 `status == MMIO_LEAVE_AFTER || pending`
        // test it replaces. Everything else lives inside, emitted once per
        // store and executed almost never — which matters twice, because
        // emitted bytes are the module's compile time as well as its size.
        //
        // [`MMIO_OK`] is zero and is the overwhelming case: the store retired,
        // the hart did not move, and the stay carries on inside the same
        // block, exactly as an interpreted block does after a store that
        // raised nothing.
        self.i(I::LocalGet(L_STATUS));
        self.i(I::LocalGet(L_PENDING));
        self.i(I::I32Or);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;

        // The bus never saw an inline RAM store, so the only thing polling
        // point (c) can have to do there is the obligation an earlier MMIO
        // load left ([`FLAG_PENDING`]). That one needs its own crossing,
        // because there is no store call to fuse into. A store the bus *did*
        // see has already cleared this, so it cannot poll twice.
        self.i(I::LocalGet(L_PENDING));
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_PENDING));
        self.post_store_state(pc, inst_width, cost, cycles, retired);
        self.i(I::Call(F_POLL));
        self.unpack_poll();
        self.extra -= 1;
        self.i(I::End);

        // What the polling point said.
        self.i(I::LocalGet(L_STATUS));
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit_after_poll(k, cycles + cost, retired + 1);
        self.extra -= 1;
        self.i(I::End);

        self.extra -= 1;
        self.i(I::End);
    }

    /// Leave because polling point (c) said so, at the pc it left the hart on
    /// ([`L_ADDR`]).
    ///
    /// One exit rather than two, because the two differ only in the flag and
    /// the reason: [`MMIO_SLICE_ENDED`] is the bus taking the slice, and
    /// anything else is the hart having moved — a delivered interrupt's
    /// vector, or a bus that stopped claiming `fetch_is_pure`. The latter
    /// carries **no** flag: the polling point has already run, and
    /// `run_blocks`'s own `fetch_is_pure` check after every `core.run` is what
    /// answers the second case.
    fn exit_after_poll(&mut self, k: usize, cycles: u64, retired: u32) {
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_SLICE_ENDED as i32));
        self.i(I::I32Eq);
        self.i(I::LocalSet(L_T));
        self.i(I::I32Const(why::SLICE_ENDED));
        self.i(I::I32Const(why::AFTER_STORE));
        self.i(I::LocalGet(L_T));
        self.i(I::Select);
        self.i(I::LocalSet(L_WHY));
        self.i(I::I32Const(FLAG_SLICE_ENDED));
        self.i(I::I32Const(0));
        self.i(I::LocalGet(L_T));
        self.i(I::Select);
        self.i(I::LocalSet(L_FLAGS));
        self.add_cycles(cycles);
        self.add_retired(retired);
        self.i(I::LocalGet(L_ADDR));
        self.i(I::LocalSet(L_EXIT_PC));
        self.i(I::Br(self.exit_depth(k)));
    }

    /// Push the three post-store arguments every polling point takes: the pc
    /// the instruction retires to, and the counters with this instruction
    /// charged (JD17).
    fn post_store_state(&mut self, pc: u32, inst_width: u8, cost: u64, cycles: u64, retired: u32) {
        self.i(I::I32Const(pc.wrapping_add(u32::from(inst_width)) as i32));
        self.i(I::LocalGet(L_CYC));
        self.i(I::I64Const((cycles + cost) as i64));
        self.i(I::I64Add);
        self.i(I::LocalGet(L_INSTRET));
        self.i(I::I64Const(i64::from(retired) + 1));
        self.i(I::I64Add);
    }

    /// Unpack a polling point's `(status << 32) | pc` into [`L_STATUS`] and
    /// [`L_ADDR`], which is where [`Emitter::exit`] looks for a dynamic pc.
    fn unpack_poll(&mut self) {
        self.i(I::LocalSet(L_T64));
        self.i(I::LocalGet(L_T64));
        self.i(I::I64Const(32));
        self.i(I::I64ShrU);
        self.i(I::I32WrapI64);
        self.i(I::LocalSet(L_STATUS));
        self.i(I::LocalGet(L_T64));
        self.i(I::I32WrapI64);
        self.i(I::LocalSet(L_ADDR));
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
        // The block set outlives this emitter, so a reference into it is not a
        // reborrow of `self` and the `&mut self` calls below do not conflict
        // with it. M7b P1: this used to be `b.insts.clone()` — a heap
        // allocation and a copy **per block**, 201,244 times per emit, purely
        // to dodge that borrow. Taking the `&'a BlockSet` out of `self` first
        // is the same emitted bytes with no allocation; `tests/
        // translate_roundtrip.rs` and an emit-only sha256 of the whole
        // `render-basic` module are the oracle that says so.
        let set: &'a BlockSet = self.set;
        let b: &'a Block = &set.blocks[self.lo + k];
        let block_pc = b.pc;
        let pcs: &'a [(u32, Decoded)] = &b.insts;
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
        self.exit(k, Some(block_pc), 0, 0, 0, why::BUDGET);
        self.extra -= 1;
        self.i(I::End);

        let mut cycles = 0u64;
        let mut retired = 0u32;
        for (pc, d) in pcs {
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
                    self.indirect(k, cycles + cost, retired + 1);
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
            BlockEnd::Undecodable(pc) => self.exit(k, Some(pc), 0, 0, 0, why::UNDECODABLE),
        }
    }
}

/// The registers the block set reads or writes anywhere.
///
/// The prologue loads exactly these and the epilogue stores exactly these;
/// everything else stays in the exchange area untouched, which is what makes
/// the escape hatch see a complete register file for free.
fn live_regs(blocks: &[Block]) -> Vec<u8> {
    let mut live = [false; 32];
    for b in blocks {
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

/// `$fast_load`'s parameters: [`crate::host::HostOps::mmio_load`]'s own.
const F_ARG_PC: u32 = 0;
const F_ARG_CYCLE: u32 = 1;
const F_ARG_ADDRESS: u32 = 2;
const F_ARG_KIND: u32 = 3;

/// The module's own `$fast_load`: the published MMIO word reads, and the
/// import for everything else (M7b P3).
///
/// It has [`crate::host::HostOps::mmio_load`]'s exact signature, so every
/// emitted load site is byte-for-byte what it was — only the callee index
/// changed. That is deliberate: [`crate::dispatch::BODY_BUDGET`] is 80 % of
/// wasm's function limit, and an arm at every memory instruction across
/// 200,000 blocks is exactly the pressure M7 P6c relieved. The whole decision
/// lives here, once.
///
/// **The refusals are the design.** Anything that is not a *word* load of a
/// published address, and anything at all while `armed` is clear, falls
/// through to the import and is served by the bus exactly as it always was —
/// with its trace line, its grade check, its census note and its watchpoints.
#[must_use]
pub(crate) fn fast_load(fast: FastReads) -> Function {
    let mut f = Function::new(alloc::vec![]);
    let mut e = |ins: I<'static>| {
        f.instruction(&ins);
    };
    let at = |field: u64| memarg(u64::from(fast.offset) + field);

    // One guard for the whole table: the machine says the published words are
    // current, and this is a word load. Everything else is the import's.
    e(I::I32Const(0));
    e(I::I32Load(at(FAST_ARMED)));
    e(I::LocalGet(F_ARG_KIND));
    e(I::I32Const(load_kind::W as i32));
    e(I::I32Eq);
    e(I::I32And);
    e(I::If(BlockType::Empty));
    for read in fast.reads.iter().flatten() {
        e(I::LocalGet(F_ARG_ADDRESS));
        e(I::I32Const(read.address as i32));
        e(I::I32Eq);
        e(I::If(BlockType::Empty));
        // The served counter. The host's MMIO census counts what reached the
        // bus, so this is the only place a read that never did can be counted
        // at all — which is why it is permanent rather than a debug build's.
        e(I::I32Const(0));
        e(I::I32Const(0));
        e(I::I64Load(at(FAST_SERVED)));
        e(I::I64Const(1));
        e(I::I64Add);
        e(I::I64Store(at(FAST_SERVED)));
        match read.source {
            FastSource::Constant(v) => e(I::I32Const(v)),
            FastSource::Published(slot) => {
                e(I::I32Const(0));
                e(I::I32Load(at(FAST_WORDS + 4 * u64::from(slot))));
            }
        }
        // `MMIO_OK` is zero, so the answer is the value alone.
        e(I::I64ExtendI32U);
        e(I::Return);
        e(I::End);
    }
    e(I::End);

    e(I::LocalGet(F_ARG_PC));
    e(I::LocalGet(F_ARG_CYCLE));
    e(I::LocalGet(F_ARG_ADDRESS));
    e(I::LocalGet(F_ARG_KIND));
    e(I::Call(F_MMIO_LOAD));
    e(I::End);
    f
}

/// Emit the whole module for `set` in **one** sub-dispatcher.
///
/// The shape P3 shipped, kept as the name every test and the replay harness
/// already use. It is [`crate::dispatch::emit_module`] with no per-function
/// budget, so it is exactly what a block set small enough to fit one function
/// produced before the split — an outer selector over one sub-dispatcher — and
/// it is refused by the same body budget as any other module.
#[must_use]
pub fn emit(set: &BlockSet, model: CycleModel, layout: Layout, policy: Emit) -> Emitted {
    crate::dispatch::emit_module(set, model, layout, policy, usize::MAX)
}

/// The registers a chunk of `set` reads or writes.
///
/// Per sub-dispatcher rather than per module, so a small function flushes a
/// small set. A register live in one function and not in another is still
/// correct: the exchange area holds every register the module is not
/// currently carrying in a local, and a function that never names one never
/// writes it back over the value that is there.
#[must_use]
pub fn live_regs_in(set: &BlockSet, lo: usize, len: usize) -> Vec<u8> {
    live_regs(&set.blocks[lo..lo + len])
}

/// Emit one **sub-dispatcher**: the wasm function holding `set`'s blocks
/// `lo..lo + len`.
///
/// Its shape is P3's whole-module shape — a `loop` over a `br_table`, one
/// `block` per guest block, forward edges to labels and back edges through
/// the table — with two things added by the split (JD8):
///
/// - an edge to a block this function does not hold returns to the outer
///   selector with the target's **global** index, and
/// - the pending-yield obligation crosses that boundary through
///   [`FLAG_PENDING`] rather than dying with the function's locals.
///
/// The signature is `(entry_local, cycle, instret, end, watch_lo, watch_hi)
/// -> i64`, and the result is `(cross << 32) | value`: with `cross` clear the
/// value is the guest pc to leave at, and with it set the value is the global
/// block index to continue at.
///
/// # Panics
///
/// Panics on an empty chunk — there is nothing to emit and the dispatcher
/// would have no arms.
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
    let live = live_regs_in(set, lo, len);

    let mut e = Emitter {
        f: Function::new(body_locals()),
        set,
        model,
        layout,
        policy,
        live: &live,
        mmio_load_func,
        lo,
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
    // The obligation an earlier sub-dispatcher's MMIO load may have left.
    e.i(I::I32Const(0));
    e.i(I::I32Load(e.exchange(EXCHANGE_FLAGS)));
    e.i(I::I32Const(FLAG_PENDING));
    e.i(I::I32And);
    e.i(I::LocalSet(L_PENDING));
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

    // Epilogue: everything the stay carries goes back where the next reader
    // of it looks — the host at an exit, the next sub-dispatcher at a cross.
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
    e.i(I::LocalGet(L_PENDING));
    e.i(I::I32Or);
    e.i(I::I32Store(e.exchange(EXCHANGE_FLAGS)));
    e.i(I::I32Const(0));
    e.i(I::LocalGet(L_WHY));
    e.i(I::I32Store(e.exchange(EXCHANGE_EXIT_WHY)));
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
