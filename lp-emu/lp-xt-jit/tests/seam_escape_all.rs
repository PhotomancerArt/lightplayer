//! The seam, under a real engine: a module that escapes every instruction
//! must leave the machine exactly where the interpreter would have.
//!
//! The host here is a **scripted interpreter**, not a hart: it answers
//! `step_one` from a table of `(pc) -> (next pc, cycles, status)`. That is the
//! point — it lets the test say what the interpreter did and then assert what
//! the stay reported, with no machine in between to be the thing that is
//! actually right. The classic's own driver plugs the real `XtHart::step_one`
//! into the same override, and `scripts/emu/v3-oracle.sh` is the whole-machine
//! form of this test.
//!
//! Note what is *not* here: no register file. With every instruction escaping,
//! the hart owns the AR file and nothing marshals it, so the exchange area's
//! 64 register words are reserved and untouched — see the crate docs.

#![cfg(feature = "host-wasmtime")]

use std::collections::BTreeMap;

use lp_emu_core::{CycleModel, InstClass};
use lp_emu_jit::host::{
    FLAG_SLICE_ENDED, HostOps, MmioLoad, MmioStore, Polled, STEP_CONTINUE, STEP_SLICE_ENDED,
    StepOne, why,
};
use lp_emu_jit::host_wasmtime::WasmtimeCore;
use lp_emu_jit::translate::Layout;
use lp_xt_inst::{Inst, NullaryNarrowOp, NullaryOp};

const EXCHANGE_AT: u32 = 0;
const PERM_AT: u32 = 0x1000;
const ARENA_AT: u32 = PERM_AT + lp_emu_jit::host::PERM_ENTRIES;
const GUEST_BASE: u32 = 0x4000_0000;
/// Where the program sits, as a guest address.
const PROGRAM_AT: u32 = GUEST_BASE + 0x100;
const PAGES: u64 = 6;
const MODEL: CycleModel = CycleModel::Esp32C6;

/// What the scripted interpreter does at one pc.
#[derive(Clone, Copy)]
struct Step {
    next_pc: u32,
    cycles: u64,
    status: u32,
}

struct FakeHost {
    mem: *mut u8,
    script: BTreeMap<u32, Step>,
    /// Every pc the module escaped at, in call order.
    escapes: Vec<u32>,
}

// SAFETY: the pointer is only dereferenced from inside a `WasmtimeCore::enter`
// call on the thread that set it, and the allocation outlives the core.
unsafe impl Send for FakeHost {}

impl HostOps for FakeHost {
    /// **The override that makes this an Xtensa host.** The default
    /// `step_one_wide` marshals RV32's `[i32; 32]` and asserts the layout is
    /// 32 words wide; this one carries no register file at all, exactly as
    /// the classic's driver does — the hart owns it.
    fn layout(&self) -> lp_emu_jit::host::ExchangeLayout {
        lp_xt_jit::LAYOUT
    }

    fn step_one_wide(&mut self, pc: u32, cycle: u64, instret: u64) -> StepOne {
        self.escapes.push(pc);
        let step = self.script.get(&pc).copied().unwrap_or_else(|| {
            panic!("the module escaped at {pc:#010x}, which the script has no answer for")
        });
        StepOne {
            pc: step.next_pc,
            cycle: cycle + step.cycles,
            instret: instret + 1,
            status: step.status,
        }
    }

    fn step_one(&mut self, _pc: u32, _cycle: u64, _instret: u64, _regs: &mut [i32; 32]) -> StepOne {
        unreachable!("an Xtensa host escapes through `step_one_wide`; see the override above")
    }

    fn mmio_load(&mut self, _pc: u32, _cycle: u64, _address: u32, _kind: u32) -> MmioLoad {
        unreachable!("this phase emits no memory access: everything escapes")
    }

    fn mmio_store(
        &mut self,
        _pc: u32,
        _cycle: u64,
        _address: u32,
        _kind: u32,
        _value: u32,
        _post_pc: u32,
        _post_cycle: u64,
        _post_instret: u64,
    ) -> MmioStore {
        unreachable!("this phase emits no memory access: everything escapes")
    }

    fn poll(&mut self, _pc: u32, _cycle: u64, _instret: u64) -> Polled {
        unreachable!("this phase emits no store, so nothing fuses a polling point")
    }

    fn exchange(&mut self) -> &mut [u8] {
        // SAFETY: the exchange area is the first bytes of an allocation that
        // outlives this host and never moves.
        unsafe {
            core::slice::from_raw_parts_mut(
                self.mem.add(EXCHANGE_AT as usize),
                lp_xt_jit::LAYOUT.len() as usize,
            )
        }
    }
}

/// What one stay reported.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    exit_pc: u32,
    cycle: u64,
    instret: u64,
    flags: i32,
    why: i32,
    escapes: Vec<u32>,
}

/// Assemble `insts` at [`PROGRAM_AT`], walk it from there, emit the module and
/// run one stay.
fn run(insts: &[Inst], script: &[(u32, Step)], end: u64) -> Outcome {
    let mut image = Vec::new();
    for inst in insts {
        image.extend_from_slice(&lp_xt_inst::encode(inst));
    }
    let mut fetch = |pc: u32| {
        pc.checked_sub(PROGRAM_AT)
            .and_then(|o| image.get(o as usize))
            .copied()
    };
    let found = lp_xt_jit::discover::build(&[PROGRAM_AT], &mut fetch);
    assert_eq!(found.stats.blocks, 1, "one start, one block");

    let layout = Layout {
        memory_pages: PAGES,
        guest_base: GUEST_BASE,
        arena_offset: ARENA_AT,
        perm_offset: PERM_AT,
        exchange_offset: EXCHANGE_AT,
        // No target table and no published reads: this phase emits no
        // indirect resolution and no memory access.
        indirect: None,
        fast_reads: None,
    };
    let emitted = lp_xt_jit::translate::emit_module(&found.set, MODEL, layout, 64);
    assert_eq!(emitted.native_insts, 0, "nothing is emitted natively yet");
    assert_eq!(emitted.escaped_insts, found.stats.insts);

    let mut mem = lp_emu_core::arena::GuestArena::zeroed((PAGES as usize) * 65536);
    let guard = mem.guard();
    let base = mem.as_mut_ptr();
    let host = FakeHost {
        mem: base,
        script: script.iter().copied().collect(),
        escapes: Vec::new(),
    };
    // SAFETY: `mem` outlives `core`, and nothing else holds a reference into
    // it while `enter` is running.
    let mut core = unsafe { WasmtimeCore::new(&emitted.wasm, host, base, mem.len(), guard) }
        .expect("the emitted module compiles and instantiates");
    let exit = core
        .enter(0, 0, 0, end, (0, 0))
        .expect("translated code does not trap");

    // The flags are read here rather than from `exit.flags`, which
    // `WasmtimeCore::enter` reads at RV32's fixed `EXCHANGE_FLAGS`. Past a
    // 64-word register file that offset is inside the AR file, not the flags
    // word — a host whose layout is not RV32's reads the field at its own
    // layout's offset. Nothing else in the two hosts is layout-shaped.
    let x = core.ops_mut().exchange();
    let at = |field: u64| field as usize;
    let flags = i32::from_le_bytes(x[at(lp_xt_jit::LAYOUT.flags())..][..4].try_into().unwrap());
    let why = i32::from_le_bytes(
        x[at(lp_xt_jit::LAYOUT.exit_why())..][..4]
            .try_into()
            .unwrap(),
    );
    let cycle = u64::from_le_bytes(x[at(lp_xt_jit::LAYOUT.cycle())..][..8].try_into().unwrap());
    let instret = u64::from_le_bytes(
        x[at(lp_xt_jit::LAYOUT.instret())..][..8]
            .try_into()
            .unwrap(),
    );
    Outcome {
        exit_pc: exit.pc,
        cycle,
        instret,
        flags,
        why,
        escapes: core.ops_mut().escapes.clone(),
    }
}

fn step(next_pc: u32, cycles: u64) -> Step {
    Step {
        next_pc,
        cycles,
        status: STEP_CONTINUE,
    }
}

/// The straight-line case: three instructions escape in order, the counters
/// are the interpreter's, and the stay leaves where the terminator sent it.
#[test]
fn a_block_of_escapes_retires_every_instruction_and_leaves_where_the_interpreter_says() {
    let insts = [
        Inst::NullaryN(NullaryNarrowOp::NopN),
        Inst::Nullary(NullaryOp::Nop),
        Inst::Nullary(NullaryOp::Ret),
    ];
    let (a, b, c) = (PROGRAM_AT, PROGRAM_AT + 2, PROGRAM_AT + 5);
    let returned_to = 0x4000_9000;
    let out = run(
        &insts,
        &[(a, step(b, 1)), (b, step(c, 1)), (c, step(returned_to, 3))],
        1_000,
    );
    assert_eq!(out.escapes, vec![a, b, c]);
    assert_eq!(out.exit_pc, returned_to);
    assert_eq!(out.instret, 3, "every instruction retired");
    assert_eq!(out.cycle, 5, "the interpreter's cycles, not the emitter's");
    assert_eq!(out.flags, 0);
    assert_eq!(
        out.why,
        why::ESCAPE_TARGET,
        "an escaped terminator leaves: this phase resolves no static targets"
    );
}

/// The slice end an escaped instruction reports is not swallowed: the stay
/// leaves at once, at the pc the interpreter left the hart on.
#[test]
fn a_slice_end_inside_a_stay_leaves_immediately() {
    let insts = [
        Inst::Nullary(NullaryOp::Nop),
        Inst::Nullary(NullaryOp::Nop),
        Inst::Nullary(NullaryOp::Ret),
    ];
    let (a, b) = (PROGRAM_AT, PROGRAM_AT + 3);
    let out = run(
        &insts,
        &[
            (a, step(b, 1)),
            (
                b,
                Step {
                    next_pc: b + 3,
                    cycles: 1,
                    status: STEP_SLICE_ENDED,
                },
            ),
        ],
        1_000,
    );
    assert_eq!(out.escapes, vec![a, b], "the third never ran");
    assert_eq!(out.exit_pc, b + 3);
    assert_eq!(out.instret, 2);
    assert_eq!(out.flags, FLAG_SLICE_ENDED);
    assert_eq!(out.why, why::SLICE_ENDED);
}

/// An escaped instruction that did not leave the hart on the decoder's
/// straight line — a trap, an interrupt taken inside it, or a `loop` back-edge
/// this emitter models nothing about — ends the block.
///
/// The `loop` case is why this matters more on Xtensa than on RV32: `LCOUNT`
/// is tested against `LEND` after **every** instruction, so any body slot can
/// be the last of an iteration and jump to `LBEG`. The check catches it with
/// no knowledge of loops at all.
#[test]
fn an_escape_that_left_the_straight_line_ends_the_block() {
    let insts = [
        Inst::Nullary(NullaryOp::Nop),
        Inst::Nullary(NullaryOp::Nop),
        Inst::Nullary(NullaryOp::Ret),
    ];
    let a = PROGRAM_AT;
    let looped_back = 0x4000_0080;
    let out = run(&insts, &[(a, step(looped_back, 1))], 1_000);
    assert_eq!(out.escapes, vec![a]);
    assert_eq!(out.exit_pc, looped_back);
    assert_eq!(out.instret, 1);
    assert_eq!(out.why, why::ESCAPE_DIVERGED);
}

/// The budget rule (M5 MD3): not one instruction may start at or past the
/// slice deadline, so a block whose maximum cost does not fit is handed back
/// untouched and the interpreter runs it with its own per-instruction compare.
#[test]
fn a_block_that_cannot_fit_the_slice_is_handed_back_whole() {
    let insts = [
        Inst::Nullary(NullaryOp::Nop),
        Inst::Nullary(NullaryOp::Nop),
        Inst::Nullary(NullaryOp::Ret),
    ];
    // One cycle of slice left, against a block that costs more than that.
    let out = run(&insts, &[], 1);
    assert!(
        out.escapes.is_empty(),
        "not one instruction may start past the deadline"
    );
    assert_eq!(out.exit_pc, PROGRAM_AT, "handed back at the block's own pc");
    assert_eq!(out.instret, 0);
    assert_eq!(out.cycle, 0);
    assert_eq!(out.why, why::BUDGET);
}

/// The budget the block compares against is the **upper bound** on its cost,
/// which is what `lp_xt_emu::block::cost_bound` gives and what the block
/// cache's own `max_cycles` is computed from. A stay that charged less than
/// the bound would still be exact; one that charged less than the bound and
/// then ran past the deadline would not.
#[test]
fn the_budget_is_the_blocks_own_upper_bound() {
    let insts = [Inst::Nullary(NullaryOp::Ret)];
    let bound = u64::from(MODEL.cycles_for(InstClass::JalrReturn));
    // Exactly the bound fits; one less does not.
    let fits = run(&insts, &[(PROGRAM_AT, step(0x4000_9000, bound))], bound);
    assert_eq!(fits.instret, 1);
    let does_not = run(&insts, &[], bound - 1);
    assert_eq!(does_not.why, why::BUDGET);
    assert_eq!(does_not.instret, 0);
}
