//! The window model (XD8): `entry` and `retw` rotates in every increment,
//! the dirty-mask writeback, the ring wrapping past `AR[63]`, the hoisted
//! overflow refusal and the underflow refusal — each against the
//! interpreter's own run.
//!
//! The AR-file invariant ("a caller's registers are in `AR` whenever a
//! `retw` may reload them") is what these programs exercise: every callee
//! writes its whole window before returning, so a caller register that was
//! not written back at the `entry` would come back wrong.

#![cfg(feature = "host-wasmtime")]

mod common;

use common::{DATA, PROGRAM_AT, Program, SP, STOP, a, agree, exited_with};
use lp_xt_inst::{AluRrr, BrZ, CallOp, CallxOp, Inst, NullaryNarrowOp, NullaryOp, StoreOp};
use lp_xt_jit::translate::why;

fn retw() -> Inst {
    Inst::NullaryN(NullaryNarrowOp::RetwN)
}

/// Pad `insts` with nops so the next instruction starts word-aligned: a
/// `call0/4/8/12` reaches `(pc & !3) + (off << 2) + 4` and nothing else.
fn pad_to_word(insts: &mut Vec<Inst>) {
    let end = *Program::new(insts.clone()).pcs().last().unwrap();
    match end % 4 {
        1 => insts.push(Inst::Nullary(NullaryOp::Nop)),
        2 => insts.push(Inst::NullaryN(NullaryNarrowOp::NopN)),
        3 => {
            insts.push(Inst::NullaryN(NullaryNarrowOp::NopN));
            insts.push(Inst::Nullary(NullaryOp::Nop));
        }
        _ => {}
    }
}

/// A callee that writes every register of its window with a value derived
/// from its arguments, stores a marker, and returns.
fn callee(marker: i32) -> Vec<Inst> {
    let mut v = vec![Inst::Entry(a(1), 32)];
    for r in 2..16u8 {
        v.push(Inst::Addi(a(r), a(r), i32::from(r) + marker));
    }
    v.push(Inst::Store(StoreOp::S32i, a(2), a(1), 0));
    v.push(retw());
    v
}

/// Seed the window and the stack.
fn seed(hart: &mut lp_xt_emu::mach::XtHart<common::RamBus>, _: &mut common::RamBus) {
    let cpu = hart.cpu_mut();
    cpu.set_a(0, STOP);
    cpu.set_a(1, SP);
    for r in 2..16u8 {
        cpu.set_a(r, 0x1000 * u32::from(r) + 1);
    }
}

/// `call8` to a callee that writes its window, then `retw`; the caller's
/// registers come back through the file (`a0..a7` left the window at the
/// `entry` and were written back if dirty).
#[test]
fn call8_entry_retw_round_trips_the_callers_registers() {
    // `main` is entered as if a `call8` from frame 0 had just happened: a8
    // holds the mangled return to STOP and CALLINC is 2, so main's own
    // `retw` at the end returns to STOP through the window.
    let mut insts = vec![
        Inst::Entry(a(1), 64),
        Inst::Movi(a(2), 11),
        Inst::Movi(a(3), 22),
        Inst::Movi(a(4), 33),
        Inst::Movi(a(10), 44), // the callee's a2
        Inst::Movi(a(11), 55),
        Inst::Call(CallOp::Call8, 0), // patched below
        Inst::Rrr(AluRrr::Add, a(5), a(2), a(10)),
        Inst::Store(StoreOp::S32i, a(5), a(1), 4),
        retw(),
    ];
    pad_to_word(&mut insts);
    let pcs = Program::new(insts.clone()).pcs();
    let callee_at = *pcs.last().unwrap();
    let call_pc = pcs[6];
    insts[6] = Inst::Call(CallOp::Call8, call_words(call_pc, callee_at));
    insts.extend(callee(100));
    let program = Program::new(insts).setup(|hart, bus| {
        seed(hart, bus);
        let cpu = hart.cpu_mut();
        cpu.set_a(8, (2 << 30) | (STOP & 0x3FFF_FFFF));
        cpu.ps_callinc = 2;
    });
    let run = agree("call8", &program);
    assert_eq!(run.outcome.pc, STOP);
    assert!(run.escapes.is_empty(), "{:?}", run.escapes);
    assert!(
        !exited_with(&run, why::WINDOW),
        "no frame was within reach: {:?}",
        run.exits
    );
}

/// The word offset a `call` at `pc` needs to reach `target`.
fn call_words(pc: u32, target: u32) -> i32 {
    ((target as i32) - ((pc & !3) as i32) - 4) >> 2
}

/// Every increment: `call4`, `call8`, `call12` and `callx4/8/12`, into
/// callees that use the whole window, nested until the ring wraps past
/// `AR[63]` (base above 12) and back.
#[test]
fn every_call_increment_nests_and_wraps_the_ring() {
    // main: entry; call12 f1; store; retw
    // f1: entry; call8 f2; retw
    // f2: entry; call4 f3; retw
    // f3: entry; callx12 (a4 = f4); retw
    // f4: entry; callx8 (a4 = f5); retw
    // f5: entry; callx4 (a4 = f6); retw
    // f6: writes its window, retw
    let mut insts = vec![Inst::Entry(a(1), 32), Inst::Call(CallOp::Call12, 0)];
    insts.push(Inst::Store(StoreOp::S32i, a(2), a(1), 8));
    insts.push(retw());
    let mut frames: Vec<(usize, Vec<Inst>)> = Vec::new();
    for (i, op) in [CallOp::Call8, CallOp::Call4].iter().enumerate() {
        let mut f = callee(10 * (i as i32 + 1));
        // Replace the `retw` with a call, then a retw.
        f.pop();
        f.push(Inst::Call(*op, 0));
        f.push(retw());
        frames.push((f.len() - 2, f));
    }
    for (i, op) in [CallxOp::Callx12, CallxOp::Callx8, CallxOp::Callx4]
        .iter()
        .enumerate()
    {
        let mut f = callee(40 + 10 * i as i32);
        f.pop();
        // a4 = the next function's address, from the literal pool (the
        // callee's own window holds no caller register it could add to);
        // the field is patched once the layout is known.
        f.push(Inst::L32r(a(4), 0));
        f.push(Inst::Callx(*op, a(4)));
        f.push(retw());
        // The instruction to patch is the `l32r`, two before the `retw`.
        frames.push((f.len() - 3, f));
    }
    // The leaf sits at base + 13 of a 16-group ring: it writes three groups,
    // not four, so its reach stops at base + 15 and nothing overflows.
    let mut leaf = callee(90);
    leaf.retain(|i| !matches!(i, Inst::Addi(r, ..) if r.num() >= 12));
    frames.push((0, leaf));
    // Lay out and patch.
    let mut starts = Vec::new();
    let mut all = insts.clone();
    let mut frame_index = Vec::new();
    for (_, f) in &frames {
        pad_to_word(&mut all);
        starts.push(Program::new(all.clone()).pcs().last().copied().unwrap());
        frame_index.push(all.len());
        all.extend(f.iter().cloned());
    }
    let pcs = Program::new(all.clone()).pcs();
    // main's call12 → frames[0]
    all[1] = Inst::Call(CallOp::Call12, call_words(pcs[1], starts[0]));
    let mut literals = Vec::new();
    for (fi, (call_at, _)) in frames.iter().enumerate() {
        let next = starts.get(fi + 1).copied();
        let at = frame_index[fi] + call_at;
        match all[at] {
            Inst::Call(op, _) => {
                all[at] = Inst::Call(op, call_words(pcs[at], next.unwrap()));
            }
            Inst::L32r(rt, _) => {
                let slot = common::LITERALS_AT + 4 * literals.len() as u32;
                literals.push(next.unwrap());
                all[at] = Inst::L32r(rt, common::l32r_field(pcs[at], slot));
            }
            _ => {}
        }
    }
    for base in [0u8, 5, 10, 12, 13, 14, 15] {
        let literals = literals.clone();
        // The `callx` targets have no static edge: seed them, as the classic
        // seeds every function symbol.
        let seeds = starts[3..].to_vec();
        let program = Program::new(all.clone()).literals(literals).seeds(seeds).setup(move |hart, bus| {
            seed(hart, bus);
            let cpu = hart.cpu_mut();
            cpu.window_base = base;
            cpu.window_start = 1 << base;
            // `main` is entered as if a `call4` from the frame at `base` had
            // just happened: a4 holds the mangled return to STOP.
            cpu.ps_callinc = 1;
            // The window moved: re-seed the visible registers.
            cpu.set_a(1, SP);
            cpu.set_a(4, (1 << 30) | (STOP & 0x3FFF_FFFF));
            cpu.set_a(13, PROGRAM_AT);
        });
        let run = agree(&format!("nest-base{base}"), &program);
        assert!(run.escapes.is_empty(), "base {base}: {:?}", run.escapes);
        // Main's frame plus 3+2+1+3+2+1 groups is 13 of the ring's 16, so no
        // frame is ever within reach from any base — no overflow, and the
        // chain returns to STOP through seven `retw`s.
        assert_eq!(
            run.outcome.pc,
            STOP,
            "base {base}: exits {:?}, exccause {} excvaddr {:#x} epc1 {:#x}",
            run.exits,
            run.outcome.exccause,
            run.outcome.excvaddr,
            run.outcome.epc1
        );
        assert!(
            !exited_with(&run, why::WINDOW),
            "base {base}: {:?}",
            run.exits
        );
    }
}

/// The overflow refusal (XD5): a `WindowStart` bit within reach of the
/// block's group refuses the block at its own pc with nothing retired, and
/// the interpreter raises the exception at the exact instruction — here,
/// the trap lands at the vector because no handler is installed, and both
/// runs agree on `EPC1`, `PS.OWB` and the moved `WindowBase`.
#[test]
fn a_frame_within_reach_refuses_the_block_and_the_interpreter_takes_the_exception() {
    let insts = vec![
        Inst::Movi(a(2), 1),
        Inst::Movi(a(3), 2),
        Inst::Addi(a(9), a(2), 5), // group 2: within reach of a frame two above
        Inst::Movi(a(4), 3),
        Inst::NullaryN(NullaryNarrowOp::RetN),
    ];
    let program = Program::new(insts).setup(|hart, bus| {
        seed(hart, bus);
        let cpu = hart.cpu_mut();
        cpu.window_base = 3;
        cpu.window_start = (1 << 3) | (1 << 5);
        cpu.set_a(0, STOP);
    });
    let run = agree("overflow", &program);
    assert_ne!(run.outcome.pc, STOP, "the overflow trapped to the vector");
    assert_eq!(run.outcome.epc1, PROGRAM_AT + 6, "EPC1 is the addi's pc");
    assert_eq!(run.outcome.window.window_base, 5, "moved to the frame to spill");
    assert!(exited_with(&run, why::WINDOW), "{:?}", run.exits);
    assert_eq!(run.outcome.ar[3 * 4 + 4], 0, "a4 = 3 never ran");
}

/// `entry`'s own check: the callee's frame would overflow (a bit within
/// `CALLINC` of the base) — refused at the `entry`, taken by the
/// interpreter.
#[test]
fn an_entry_whose_frame_would_overflow_refuses_at_the_entry() {
    let insts = vec![Inst::Entry(a(1), 16), Inst::Movi(a(2), 9), retw()];
    let program = Program::new(insts).setup(|hart, bus| {
        seed(hart, bus);
        let cpu = hart.cpu_mut();
        cpu.window_base = 4;
        cpu.window_start = (1 << 4) | (1 << 6);
        cpu.ps_callinc = 2;
        cpu.set_a(0, STOP);
        cpu.set_a(1, SP);
    });
    let run = agree("entry-overflow", &program);
    assert_ne!(run.outcome.pc, STOP);
    assert_eq!(run.outcome.epc1, PROGRAM_AT);
    assert!(exited_with(&run, why::WINDOW), "{:?}", run.exits);
}

/// `retw` with the caller's frame not resident: the underflow exception,
/// refused at the `retw` and taken by the interpreter; and a `retw` with
/// `n = 0`, the illegal case.
#[test]
fn a_retw_into_a_spilled_caller_refuses_at_the_retw() {
    for (a0_top, start) in [(2u32, 1u16 << 6), (2, (1 << 6) | (1 << 5)), (0, 1 << 6)] {
        let insts = vec![Inst::Movi(a(2), 1), retw(), Inst::Movi(a(3), 2)];
        let program = Program::new(insts).setup(move |hart, bus| {
            seed(hart, bus);
            let cpu = hart.cpu_mut();
            cpu.window_base = 6;
            cpu.window_start = start;
            cpu.set_a(0, (a0_top << 30) | (STOP & 0x3FFF_FFFF));
        });
        let run = agree(&format!("underflow-{a0_top}-{start:#x}"), &program);
        assert_ne!(run.outcome.pc, STOP);
        assert_eq!(run.outcome.epc1, PROGRAM_AT + 3, "EPC1 is the retw's pc");
        assert!(exited_with(&run, why::WINDOW), "{:?}", run.exits);
    }
}

/// `call0`/`callx0` and `ret`: no rotate, `a0` is the link.
#[test]
fn call0_and_ret_agree() {
    let mut insts = vec![
        Inst::Movi(a(2), 1),
        Inst::Call(CallOp::Call0, 0),
        Inst::Addi(a(2), a(2), 10),
        Inst::Addi(a(5), a(13), 0), // patched: the leaf
        Inst::Callx(CallxOp::Callx0, a(5)),
        Inst::Addi(a(2), a(2), 30),
        Inst::Store(StoreOp::S32i, a(2), a(1), 0),
        // The final `ret` needs a0 = STOP again: restore it from a6.
        Inst::Rrr(AluRrr::Or, a(0), a(6), a(6)),
        Inst::Nullary(NullaryOp::Ret),
    ];
    // `addi` takes -128..=127.
    let leaf = vec![Inst::Addi(a(2), a(2), 100), Inst::NullaryN(NullaryNarrowOp::RetN)];
    pad_to_word(&mut insts);
    let pcs = Program::new(insts.clone()).pcs();
    let leaf_at = *pcs.last().unwrap();
    insts[1] = Inst::Call(CallOp::Call0, call_words(pcs[1], leaf_at));
    insts[3] = Inst::Addi(a(5), a(13), (leaf_at - PROGRAM_AT) as i32);
    insts.extend(leaf);
    let program = Program::new(insts).setup(|hart, bus| {
        seed(hart, bus);
        let cpu = hart.cpu_mut();
        cpu.set_a(1, DATA);
        cpu.set_a(6, STOP);
        cpu.set_a(13, PROGRAM_AT);
    });
    let run = agree("call0", &program);
    assert_eq!(run.outcome.pc, STOP);
    assert_eq!(run.outcome.ar[2], 1 + 100 + 10 + 100 + 30);
    assert!(run.escapes.is_empty());
}

/// `movsp` and `rotw` are refusals: escaped, and the reload after the
/// escape re-reads the whole window.
#[test]
fn movsp_and_rotw_escape_and_the_window_is_reloaded() {
    let insts = vec![
        Inst::Movi(a(2), 5),
        Inst::Rotw(1),
        Inst::Addi(a(2), a(2), 1), // the rotated window's a2 = old a6
        Inst::Rotw(-1),
        Inst::Rs(lp_xt_inst::AluRs::Movsp, a(1), a(3)),
        Inst::Addi(a(4), a(1), 4),
        Inst::BranchZ(BrZ::Beqz, a(4), 3),
        Inst::Movi(a(5), 1),
        Inst::NullaryN(NullaryNarrowOp::RetN),
    ];
    let program = Program::new(insts).setup(|hart, bus| {
        seed(hart, bus);
        let cpu = hart.cpu_mut();
        cpu.window_base = 2;
        cpu.window_start = 0b111;
        cpu.set_a(0, STOP);
        cpu.set_a(1, SP);
        cpu.set_a(3, SP - 64);
    });
    let run = agree("movsp-rotw", &program);
    assert_eq!(run.outcome.pc, STOP);
    assert_eq!(run.escapes.len(), 3, "{:?}", run.escapes);
}
