//! Zero-overhead loops: `loop`, `loopnez`, `loopgtz`, the loop-back at the
//! walk's `LEND` (rule 6), a branch as the last body instruction (the
//! decrement stays, the branch wins), a store at the loop end (the poll sees
//! the looped pc), and the two ways the live registers can disagree with
//! the walk — a `LEND` the walk never marked (refused at entry) and a
//! `LBEG` that moved (`why::LOOP_BACK_MISS`).

#![cfg(feature = "host-wasmtime")]

mod common;

use common::{DATA, MMIO_BASE, PROGRAM_AT, Program, STOP, a, agree, exited_with, run};
use lp_xt_inst::{AluRrr, BrZ, Inst, LoadOp, LoopOp, NullaryNarrowOp, SpecialReg, SrOp, StoreOp};
use lp_xt_jit::translate::{Emit, why};

fn ret() -> Inst {
    Inst::NullaryN(NullaryNarrowOp::RetN)
}

/// `loop as, LEND` with a body of `n` three-byte instructions: `imm8 = LEND
/// - (pc + 4) = 3n - 1`.
fn loop_imm(body_bytes: u32) -> u8 {
    (body_bytes - 1) as u8
}

fn seed(hart: &mut lp_xt_emu::mach::XtHart<common::RamBus>, _: &mut common::RamBus) {
    let cpu = hart.cpu_mut();
    cpu.set_a(0, STOP);
    cpu.set_a(1, DATA);
    cpu.set_a(2, 0);
}

/// The three loop forms with counts 0, 1 and 5.
#[test]
fn the_three_loop_forms_agree_with_the_interpreter() {
    for op in [LoopOp::Loop, LoopOp::Loopnez, LoopOp::Loopgtz] {
        for count in [0i32, 1, 5, -3] {
            // `loop` with a count of 0 or below, and `loopnez` with a
            // negative one, run 2^32 times on hardware, and a stay does not
            // stop for the harness's budget.
            if (op == LoopOp::Loop && count <= 0) || (op == LoopOp::Loopnez && count < 0) {
                continue;
            }
            let insts = vec![
                Inst::Movi(a(3), count),
                Inst::Loop(op, a(3), loop_imm(6)),
                Inst::Addi(a(2), a(2), 1),
                Inst::Addi(a(4), a(4), 7),
                Inst::Movi(a(5), 9),
                ret(),
            ];
            let program = Program::new(insts).setup(seed);
            let run = agree(&format!("loop-{op:?}-{count}"), &program);
            assert_eq!(run.outcome.pc, STOP);
            assert!(run.escapes.is_empty(), "{:?}", run.escapes);
            let expect = match op {
                LoopOp::Loop => (count as u32).max(1),
                LoopOp::Loopnez => {
                    if count == 0 {
                        0
                    } else {
                        count as u32
                    }
                }
                LoopOp::Loopgtz => {
                    if count <= 0 {
                        0
                    } else {
                        count as u32
                    }
                }
            };
            assert_eq!(run.outcome.ar[2], expect, "{op:?} {count}");
        }
    }
}

/// A branch as the body's last instruction: when it is taken the decrement
/// still happens and the branch wins; when not, the loop-back fires.
#[test]
fn a_branch_at_the_loop_end_decrements_and_wins() {
    let insts = vec![
        Inst::Movi(a(3), 6),
        Inst::Loop(LoopOp::Loopnez, a(3), loop_imm(6)),
        Inst::Addi(a(2), a(2), 1),
        Inst::BranchZ(BrZ::Beqz, a(6), 2), // taken once a6 == 0: to the ret, over the movi
        Inst::Movi(a(5), 9),
        ret(),
    ];
    for a6 in [0u32, 1] {
        let program = Program::new(insts.clone()).setup(move |hart, bus| {
            seed(hart, bus);
            hart.cpu_mut().set_a(6, a6);
        });
        let run = agree(&format!("branch-at-lend-{a6}"), &program);
        assert_eq!(run.outcome.pc, STOP);
        assert!(run.escapes.is_empty());
        if a6 == 0 {
            assert_eq!(run.outcome.ar[2], 1, "the taken branch left the loop");
            assert_eq!(run.outcome.window.lcount, 4, "and still decremented");
        } else {
            assert_eq!(run.outcome.ar[2], 6);
        }
    }
}

/// A store as the body's last instruction, to MMIO: the fused poll is
/// handed the looped pc and the decremented `LCOUNT`.
#[test]
fn a_store_at_the_loop_end_polls_with_the_looped_pc() {
    let insts = vec![
        Inst::Movi(a(3), 4),
        Inst::Loop(LoopOp::Loop, a(3), loop_imm(6)),
        Inst::Addi(a(2), a(2), 1),
        Inst::Store(StoreOp::S32i, a(2), a(7), 0x40),
        Inst::Movi(a(5), 9),
        ret(),
    ];
    for sideband in [false, true] {
        let program = Program::new(insts.clone()).setup(move |hart, bus| {
            seed(hart, bus);
            hart.cpu_mut().set_a(7, MMIO_BASE);
            bus.store_raises_sideband = sideband;
        });
        let run = agree(&format!("store-at-lend-{sideband}"), &program);
        assert_eq!(run.outcome.pc, STOP);
        assert_eq!(run.outcome.device, 4);
        assert_eq!(run.outcome.ar[2], 4);
    }
}

/// A load at the loop end that the bus refuses: the decrement is undone.
#[test]
fn a_refused_access_at_the_loop_end_undoes_nothing_it_did_not_do() {
    let insts = vec![
        Inst::Movi(a(3), 4),
        Inst::Loop(LoopOp::Loop, a(3), loop_imm(6)),
        Inst::Addi(a(2), a(2), 1),
        Inst::Load(LoadOp::L8ui, a(4), a(7), 1), // byte from the word-only region: faults
        Inst::Movi(a(5), 9),
        ret(),
    ];
    let program = Program::new(insts).setup(|hart, bus| {
        seed(hart, bus);
        hart.cpu_mut().set_a(7, PROGRAM_AT & !3);
    });
    let run = agree("refused-at-lend", &program);
    assert_ne!(run.outcome.pc, STOP);
    assert_eq!(run.outcome.window.lcount, 3, "the first iteration's decrement never happened");
    assert_eq!(run.outcome.ar[2], 1);
}

/// `wsr.lend` moves the live `LEND` to an address the walk never marked:
/// a stay that starts with `LCOUNT != 0` is refused by the driver rule the
/// harness mirrors, and the interpreter loops back where the module could
/// not have.
#[test]
fn a_lend_the_walk_never_marked_keeps_the_stay_out() {
    // The loop's body is `wsr.lend; addi a2; addi a4` (9 bytes); the `wsr`
    // moves the live LEND to after the first addi, so the interpreter loops
    // on `addi a2` alone from then on.
    let insts = vec![
        Inst::Movi(a(3), 3),
        Inst::Loop(LoopOp::Loop, a(3), loop_imm(9)),
        Inst::Sr(SrOp::Wsr, SpecialReg::Lend, a(8)),
        Inst::Addi(a(2), a(2), 1),
        Inst::Addi(a(4), a(4), 1),
        Inst::Movi(a(5), 9),
        ret(),
    ];
    let pcs = Program::new(insts.clone()).pcs();
    // The first addi is at pcs[3] and ends at pcs[4].
    let lend_after_first_addi = pcs[4];
    let program = Program::new(insts).setup(move |hart, bus| {
        seed(hart, bus);
        hart.cpu_mut().set_a(8, lend_after_first_addi);
    });
    let run = agree("wsr-lend", &program);
    assert_eq!(run.outcome.pc, STOP);
    // First pass: wsr, addi a2 (loop-back at the moved LEND), then `addi a2`
    // twice more on its own, then addi a4 once.
    assert_eq!(run.outcome.ar[2], 3);
    assert_eq!(run.outcome.ar[4], 1);
    // The wsr is undecodable: the block ends before it and the interpreter
    // runs it; with LCOUNT != 0 and an unmarked LEND the stay is refused
    // until the loop is over.
    assert!(run.hart_steps > 0);
}

/// `wsr.lbeg` moves the live `LBEG` while `LEND` stays the marked one: the
/// loop-back fires in the module, sees the live register disagree with the
/// walk, and leaves at the live `LBEG` (`LOOP_BACK_MISS`).
#[test]
fn a_moved_lbeg_leaves_at_the_live_value() {
    // The body is `wsr.lbeg; addi a2; addi a4` (9 bytes); LBEG moves to the
    // second addi, so each loop-back skips the wsr and the first addi.
    let insts = vec![
        Inst::Movi(a(3), 3),
        Inst::Loop(LoopOp::Loop, a(3), loop_imm(9)),
        Inst::Sr(SrOp::Wsr, SpecialReg::Lbeg, a(8)),
        Inst::Addi(a(2), a(2), 1),
        Inst::Addi(a(4), a(4), 1),
        Inst::Movi(a(5), 9),
        ret(),
    ];
    let pcs = Program::new(insts.clone()).pcs();
    let second_addi = pcs[4];
    let program = Program::new(insts).setup(move |hart, bus| {
        seed(hart, bus);
        hart.cpu_mut().set_a(8, second_addi);
    });
    let run = agree("wsr-lbeg", &program);
    assert_eq!(run.outcome.pc, STOP);
    assert_eq!(run.outcome.ar[2], 1);
    assert_eq!(run.outcome.ar[4], 3);
    assert!(exited_with(&run, why::LOOP_BACK_MISS), "{:?}", run.exits);
}

/// The emitted module alone, at 1 block a function: the loop-back is a
/// back edge through the selector, and every iteration retires natively.
#[test]
fn the_loop_back_is_native_at_one_block_a_function() {
    let insts = vec![
        Inst::Movi(a(3), 50),
        Inst::Loop(LoopOp::Loop, a(3), loop_imm(6)),
        Inst::Addi(a(2), a(2), 1),
        Inst::Rrr(AluRrr::Add, a(4), a(4), a(2)),
        ret(),
    ];
    let program = Program::new(insts).setup(seed);
    let emitted = run(&program, Emit::EVERYTHING, 1, "loop-1");
    assert_eq!(emitted.outcome.pc, STOP);
    assert_eq!(emitted.outcome.ar[2], 50);
    assert_eq!(emitted.outcome.ar[4], 50 * 51 / 2);
    assert!(emitted.escapes.is_empty());
    assert_eq!(emitted.entries, 1, "one stay ran the whole loop: {:?}", emitted.exits);
}
