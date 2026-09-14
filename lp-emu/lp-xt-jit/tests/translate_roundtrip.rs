//! The integer core, the memory arms and the branches, emitted and run under
//! wasmtime against the same program escaped instruction by instruction to
//! a real `XtHart`. **The two runs must agree on everything** — the file,
//! the window, the counters, the memory — at 1, 8 and 64 blocks a function.
//!
//! With `LP_EMU_XT_JIT_ENGINE_CASE=<dir>` every run here is also written out
//! as an engine case for `scripts/emu/jit-engine-check.mjs` to replay in V8
//! and JavaScriptCore.

#![cfg(feature = "host-wasmtime")]

mod common;

use common::{
    DATA, DEVICE, MMIO_BASE, PROGRAM_AT, Program, SP, STOP, a, agree, exited_with, l32r_field,
    run,
};
use lp_emu_core::CycleModel;
use lp_xt_inst::{
    AluRrr, AluRs, AluRt, BrRi, BrRiu, BrRr, BrZ, Inst, LoadOp, NullaryNarrowOp, NullaryOp,
    ShiftSetOp, StoreOp,
};
use lp_xt_jit::translate::{Emit, why};

/// Seed `a2..a15` with distinct, sign-varied values, `a1` with the stack
/// pointer and `a0` with the stop address, so `ret` ends the run.
fn seed(hart: &mut lp_xt_emu::mach::XtHart<common::RamBus>) {
    let cpu = hart.cpu_mut();
    cpu.set_a(0, STOP);
    cpu.set_a(1, SP);
    let values: [u32; 14] = [
        0x0000_0007,
        0xFFFF_FFF9,
        0x1234_5678,
        0x8000_0000,
        0x7FFF_FFFF,
        0x0000_0000,
        0xDEAD_BEEF,
        0x0000_0021,
        0xFFFF_0000,
        0x0000_00FF,
        0x0001_0000,
        0xA5A5_A5A5,
        0x0000_0003,
        0xFFFF_FFFF,
    ];
    for (i, v) in values.iter().enumerate() {
        cpu.set_a(2 + i as u8, *v);
    }
}

fn ret() -> Inst {
    Inst::NullaryN(NullaryNarrowOp::RetN)
}

/// Every three-register ALU op except the divides, over every ordered pair
/// of a few registers, then `ret`.
#[test]
fn the_three_register_alu_agrees_with_the_interpreter() {
    let mut insts = Vec::new();
    let ops = [
        AluRrr::And,
        AluRrr::Or,
        AluRrr::Xor,
        AluRrr::Add,
        AluRrr::Sub,
        AluRrr::Addx2,
        AluRrr::Addx4,
        AluRrr::Addx8,
        AluRrr::Subx2,
        AluRrr::Subx4,
        AluRrr::Subx8,
        AluRrr::Src,
        AluRrr::Mull,
        AluRrr::Muluh,
        AluRrr::Mulsh,
        AluRrr::Min,
        AluRrr::Max,
        AluRrr::Minu,
        AluRrr::Maxu,
        AluRrr::Mul16u,
        AluRrr::Mul16s,
        AluRrr::Moveqz,
        AluRrr::Movnez,
        AluRrr::Movltz,
        AluRrr::Movgez,
    ];
    let mut rd = 2u8;
    for (i, op) in ops.iter().enumerate() {
        let rs = 2 + (i as u8 % 7);
        let rt = 3 + ((i * 3) as u8 % 9);
        insts.push(Inst::Ssai((i as u8 * 5) % 32));
        insts.push(Inst::Rrr(*op, a(rd), a(rs), a(rt)));
        rd = 2 + ((rd + 5) % 14);
    }
    insts.push(ret());
    let program = Program::new(insts).setup(|hart, _| seed(hart));
    let run = agree("alu-rrr", &program);
    assert_eq!(run.outcome.pc, STOP);
    assert!(run.escapes.is_empty(), "nothing in the integer core escapes");
}

/// The divides: a zero divisor is **refused and escaped** — the interpreter
/// traps with the exact `EPC1` and `EXCCAUSE` — and `INT_MIN / -1` is the
/// executor's wrapping answer, not a wasm trap.
#[test]
fn the_divides_agree_including_the_zero_divisor_trap() {
    for op in [AluRrr::Quou, AluRrr::Quos, AluRrr::Remu, AluRrr::Rems] {
        // a5 = INT_MIN, a15 = -1: the overflow pair. a7 = 0: the trap.
        let insts = vec![
            Inst::Rrr(op, a(2), a(5), a(15)),
            Inst::Rrr(op, a(3), a(4), a(2)),
            Inst::Rrr(op, a(4), a(10), a(3)),
            Inst::Rrr(op, a(6), a(4), a(7)),
            Inst::Movi(a(8), 99),
            ret(),
        ];
        let program = Program::new(insts).setup(|hart, _| seed(hart));
        let run = agree(&format!("div-{op:?}"), &program);
        assert_ne!(run.outcome.pc, STOP, "the zero divisor trapped");
        assert_eq!(
            run.outcome.exccause,
            lp_xt_emu::error::EXC_INTEGER_DIVIDE_BY_ZERO,
            "{op:?}"
        );
        assert_eq!(run.outcome.epc1, PROGRAM_AT + 9, "EPC1 is the divide's pc");
        assert_eq!(run.outcome.ar[8], 0, "nothing after the trap ran");
        assert!(
            exited_with(&run, why::ESCAPE_DIVERGED),
            "the escape left the straight line: {:?}",
            run.exits
        );
    }
}

/// The two-register ops, the shifts through `SAR`, the immediate forms and
/// the narrow forms.
#[test]
fn the_shifts_and_immediates_agree_with_the_interpreter() {
    let mut insts = vec![];
    for (i, op) in [
        AluRt::Neg,
        AluRt::Abs,
        AluRt::Sra,
        AluRt::Srl,
        AluRt::Nsa,
        AluRt::Nsau,
    ]
    .iter()
    .enumerate()
    {
        for src in [2u8, 3, 5, 6, 7, 15] {
            insts.push(Inst::ShiftSet(ShiftSetOp::Ssr, a(2 + (i as u8 % 3))));
            insts.push(Inst::Rt(*op, a(8 + (i as u8 % 4)), a(src)));
        }
    }
    for op in [
        ShiftSetOp::Ssl,
        ShiftSetOp::Ssr,
        ShiftSetOp::Ssa8l,
        ShiftSetOp::Ssa8b,
    ] {
        for src in [2u8, 9, 13, 15] {
            insts.push(Inst::ShiftSet(op, a(src)));
            insts.push(Inst::Rs(AluRs::Sll, a(12), a(4)));
            insts.push(Inst::Rt(AluRt::Sra, a(13), a(4)));
            insts.push(Inst::Rrr(AluRrr::Src, a(14), a(4), a(6)));
        }
    }
    insts.push(Inst::Ssai(0));
    insts.push(Inst::Rs(AluRs::Sll, a(12), a(4)));
    insts.push(Inst::Ssai(31));
    insts.push(Inst::Rt(AluRt::Srl, a(12), a(4)));
    for sa in [0u8, 1, 7, 15, 31] {
        insts.push(Inst::Slli(a(2), a(6), sa));
        insts.push(Inst::Srai(a(3), a(5), sa));
    }
    for sa in [0u8, 1, 8, 15] {
        insts.push(Inst::Srli(a(4), a(8), sa));
    }
    for (shift, mask) in [(0u8, 1u8), (4, 8), (16, 16), (31, 1), (12, 16)] {
        insts.push(Inst::Extui(a(9), a(4), shift, mask));
    }
    for bit in [7u8, 8, 15, 16, 22] {
        insts.push(Inst::Sext(a(10), a(4), bit));
        insts.push(Inst::Sext(a(11), a(6), bit));
    }
    insts.push(Inst::Movi(a(2), -2048));
    insts.push(Inst::Movi(a(3), 2047));
    insts.push(Inst::MoviN(a(4), -32));
    insts.push(Inst::MoviN(a(5), 95));
    insts.push(Inst::Addi(a(6), a(2), -128));
    insts.push(Inst::Addi(a(7), a(3), 127));
    insts.push(Inst::AddiN(a(8), a(4), -1));
    insts.push(Inst::AddiN(a(9), a(5), 15));
    insts.push(Inst::Addmi(a(10), a(6), -32768));
    insts.push(Inst::Addmi(a(11), a(7), 32512));
    insts.push(Inst::MovN(a(12), a(10)));
    insts.push(Inst::AddN(a(13), a(11), a(12)));
    insts.push(Inst::Nullary(NullaryOp::Nop));
    insts.push(Inst::NullaryN(NullaryNarrowOp::NopN));
    insts.push(Inst::Nullary(NullaryOp::Memw));
    insts.push(Inst::Nullary(NullaryOp::Extw));
    insts.push(Inst::Nullary(NullaryOp::Rsync));
    insts.push(Inst::Nullary(NullaryOp::Esync));
    insts.push(Inst::Nullary(NullaryOp::Dsync));
    insts.push(ret());
    let program = Program::new(insts).setup(|hart, _| seed(hart));
    let run = agree("shifts-imm", &program);
    assert_eq!(run.outcome.pc, STOP);
    assert!(run.escapes.is_empty());
}

/// Every load and store width to plain RAM, both narrow forms, and `l32r`
/// from the literal pool — plus the word-only code region: an aligned word
/// load inline, a sub-word or misaligned one to the bus, where it faults
/// exactly as the interpreter's does.
#[test]
fn loads_stores_and_l32r_agree_with_the_interpreter() {
    let mut insts = vec![
        // a1 = DATA (the setup puts it there); build a few values.
        Inst::Movi(a(2), -1234),
        Inst::Movi(a(3), 0x7ff),
        Inst::Store(StoreOp::S32i, a(2), a(1), 0),
        Inst::Store(StoreOp::S16i, a(3), a(1), 4),
        Inst::Store(StoreOp::S8i, a(2), a(1), 6),
        Inst::S32iN(a(3), a(1), 8),
        Inst::Store(StoreOp::S16i, a(2), a(1), 13), // misaligned halfword, plain RAM
        Inst::Load(LoadOp::L32i, a(4), a(1), 0),
        Inst::Load(LoadOp::L16si, a(5), a(1), 4),
        Inst::Load(LoadOp::L16ui, a(6), a(1), 4),
        Inst::Load(LoadOp::L8ui, a(7), a(1), 6),
        Inst::L32iN(a(8), a(1), 8),
        Inst::Load(LoadOp::L16ui, a(9), a(1), 13),
        Inst::Load(LoadOp::L32i, a(10), a(1), 1), // misaligned word, plain RAM
    ];
    // `l32r` from the pool: the field depends on the instruction's pc.
    let pcs = {
        let p = Program::new(insts.clone());
        p.pcs()
    };
    let at = *pcs.last().unwrap();
    insts.push(Inst::L32r(a(11), l32r_field(at, common::LITERALS_AT)));
    insts.push(Inst::L32r(
        a(12),
        l32r_field(at + 3, common::LITERALS_AT + 4),
    ));
    // The code region is word-only: an aligned word load is inline …
    insts.push(Inst::Load(LoadOp::L32i, a(13), a(14), 0));
    // … and a byte load there faults on the bus (LoadStoreError).
    insts.push(Inst::Load(LoadOp::L8ui, a(15), a(14), 1));
    insts.push(Inst::Movi(a(2), 77));
    insts.push(ret());
    let program = Program::new(insts)
        .literals(vec![0xCAFE_F00D, 0x0BAD_F00D])
        .setup(|hart, _| {
            seed(hart);
            let cpu = hart.cpu_mut();
            cpu.set_a(1, DATA);
            cpu.set_a(14, PROGRAM_AT & !3);
        });
    let run = agree("memory", &program);
    assert_ne!(run.outcome.pc, STOP, "the byte load from the word-only region trapped");
    assert_eq!(run.outcome.ar[11], 0xCAFE_F00D);
    assert_eq!(run.outcome.ar[12], 0x0BAD_F00D);
    assert_eq!(run.outcome.ar[2] as i32, -1234, "nothing after the trap ran");
    assert!(exited_with(&run, why::LOAD_REFUSED), "{:?}", run.exits);
    assert!(run.escapes.is_empty());
}

/// MMIO through the import: a load's value comes from the device, a store
/// reaches it, and the fused poll runs the hart's own polling point — a
/// yield left by a load is taken at the next store (`FLAG_PENDING`), a
/// side-band raised by a store is resampled inside the stay.
#[test]
fn mmio_and_the_fused_poll_agree_with_the_interpreter() {
    let insts = vec![
        Inst::Load(LoadOp::L32i, a(4), a(3), 0x40), // the device
        Inst::Addi(a(4), a(4), 1),
        Inst::Store(StoreOp::S32i, a(4), a(3), 0x40),
        Inst::Load(LoadOp::L32i, a(5), a(3), 0x40),
        Inst::Store(StoreOp::S32i, a(5), a(1), 0), // plain RAM, after a pending load
        Inst::Nullary(NullaryOp::Memw),
        Inst::Load(LoadOp::L32i, a(6), a(3), 0x40),
        Inst::Nullary(NullaryOp::Memw), // System class: polls the pending yield
        Inst::Addi(a(7), a(6), 5),
        Inst::Store(StoreOp::S32i, a(7), a(1), 4),
        ret(),
    ];
    for (leaves_yield, sideband) in [(false, false), (true, false), (false, true), (true, true)] {
        let program = Program::new(insts.clone()).setup(move |hart, bus| {
            let cpu = hart.cpu_mut();
            cpu.set_a(0, STOP);
            cpu.set_a(1, DATA);
            cpu.set_a(3, MMIO_BASE);
            bus.device = 0x100;
            bus.load_leaves_yield = leaves_yield;
            bus.store_raises_sideband = sideband;
        });
        let run = agree(&format!("mmio-{leaves_yield}-{sideband}"), &program);
        assert_eq!(run.outcome.pc, STOP);
        assert_eq!(run.outcome.device, 0x101);
        assert_eq!(run.outcome.ar[7], 0x106);
        assert!(run.mmio.len() >= 5, "{:?}", run.mmio);
        assert_eq!(DEVICE, MMIO_BASE + 0x40);
    }
}

/// Every conditional branch form, taken and not taken, `j`, and a `jx`
/// through the target table.
#[test]
fn branches_and_jumps_agree_with_the_interpreter() {
    // Each branch skips one `addi` when taken; the pattern of skipped adds
    // is the observable.
    let mut insts = Vec::new();
    let mut bump = 0i32;
    let mut push_branch = |insts: &mut Vec<Inst>, br: Inst| {
        insts.push(br);
        bump += 1;
        insts.push(Inst::Addi(a(15), a(15), bump));
    };
    for op in [
        BrRr::Beq,
        BrRr::Bne,
        BrRr::Blt,
        BrRr::Bge,
        BrRr::Bltu,
        BrRr::Bgeu,
        BrRr::Ball,
        BrRr::Bany,
        BrRr::Bnall,
        BrRr::Bnone,
        BrRr::Bbc,
        BrRr::Bbs,
    ] {
        for (s, t) in [(2u8, 3u8), (4, 4), (5, 9), (8, 2)] {
            // offset 3: past the following 3-byte `addi`.
            push_branch(&mut insts, Inst::BranchRr(op, a(s), a(t), 3));
        }
    }
    for op in [BrRi::Beqi, BrRi::Bnei, BrRi::Blti, BrRi::Bgei] {
        for (s, imm) in [(2u8, 7i32), (3, -1), (5, 32), (9, 256)] {
            push_branch(&mut insts, Inst::BranchRi(op, a(s), imm, 3));
        }
    }
    for op in [BrRiu::Bltui, BrRiu::Bgeui] {
        for (s, imm) in [(2u8, 8i32), (3, 32768), (10, 65536), (12, 2)] {
            push_branch(&mut insts, Inst::BranchRiu(op, a(s), imm, 3));
        }
    }
    for op in [BrZ::Beqz, BrZ::Bnez, BrZ::Bltz, BrZ::Bgez] {
        for s in [2u8, 3, 5, 7] {
            push_branch(&mut insts, Inst::BranchZ(op, a(s), 3));
        }
    }
    for set in [true, false] {
        for (s, bit) in [(2u8, 0u8), (3, 31), (4, 12), (7, 5)] {
            push_branch(&mut insts, Inst::BranchBiI(set, a(s), bit, 3));
        }
    }
    for nez in [true, false] {
        for s in [2u8, 7, 15] {
            // Narrow: the `addi` after it is 3 bytes.
            push_branch(&mut insts, Inst::BranchZN(nez, a(s), 3));
        }
    }
    // `j` over an `addi`, then `jx` to the `ret` via a register.
    insts.push(Inst::J(3));
    insts.push(Inst::Addi(a(15), a(15), 1000));
    let pcs = Program::new(insts.clone()).pcs();
    let here = *pcs.last().unwrap();
    // movi a14, <ret>: the target is a small offset from a known pc; use
    // addi on a13 (= PROGRAM_AT, seeded) instead of a movi that cannot hold
    // an address.
    let ret_at = here + 3 + 3;
    insts.push(Inst::Addi(a(14), a(13), (ret_at - PROGRAM_AT) as i32));
    insts.push(Inst::Jx(a(14)));
    insts.push(Inst::Addi(a(15), a(15), 2000));
    insts.push(ret());
    assert!(ret_at - PROGRAM_AT < 128, "the addi immediate reaches the ret");
    let program = Program::new(insts).setup(|hart, _| {
        let cpu = hart.cpu_mut();
        cpu.set_a(0, STOP);
        cpu.set_a(1, SP);
        cpu.set_a(2, 7);
        cpu.set_a(3, 0xFFFF_FFFF);
        cpu.set_a(4, 4);
        cpu.set_a(5, 0x8000_0000);
        cpu.set_a(7, 0);
        cpu.set_a(8, 0xF0F0);
        cpu.set_a(9, 256);
        cpu.set_a(10, 65535);
        cpu.set_a(12, 3);
        cpu.set_a(13, PROGRAM_AT);
        cpu.set_a(15, 0);
    });
    let run = agree("branches", &program);
    assert_eq!(run.outcome.pc, STOP);
    assert!(run.escapes.is_empty());
    // The `jx` resolved through the table rather than leaving.
    assert!(
        !exited_with(&run, why::INDIRECT_MISS),
        "the jx target is in the set: {:?}",
        run.exits
    );
}

/// The budget (M5 MD3) and the costs: under a cycle model that prices a
/// taken branch differently from a not-taken one, the counters still agree.
#[test]
fn the_costs_agree_under_a_model_with_classes() {
    let insts = vec![
        Inst::Movi(a(2), 3),
        Inst::Addi(a(2), a(2), -1),
        Inst::BranchZ(BrZ::Bnez, a(2), -6), // back to the addi
        Inst::Load(LoadOp::L32i, a(3), a(1), 0),
        Inst::Store(StoreOp::S32i, a(2), a(1), 4),
        Inst::Rrr(AluRrr::Mull, a(4), a(2), a(3)),
        ret(),
    ];
    let program = Program::new(insts)
        .model(CycleModel::Esp32C6)
        .setup(|hart, _| {
            let cpu = hart.cpu_mut();
            cpu.set_a(0, STOP);
            cpu.set_a(1, DATA);
        });
    let run = agree("costs", &program);
    assert_eq!(run.outcome.pc, STOP);
    assert!(run.outcome.cycle > run.outcome.instret, "taken branches cost more than one");
}

/// The escape-everything build alone: every instruction goes to the hart,
/// and the module's own bookkeeping — the marshalling around each escape —
/// leaves the hart exactly where a plain interpreted run would.
#[test]
fn the_escape_everything_build_is_the_interpreter() {
    let insts = vec![
        Inst::Movi(a(2), 5),
        Inst::Rrr(AluRrr::Add, a(3), a(2), a(2)),
        Inst::Store(StoreOp::S32i, a(3), a(1), 0),
        ret(),
    ];
    let program = Program::new(insts).setup(|hart, _| {
        let cpu = hart.cpu_mut();
        cpu.set_a(0, STOP);
        cpu.set_a(1, DATA);
    });
    let nothing = run(&program, Emit::NOTHING, 64, "nothing-alone");
    assert_eq!(nothing.outcome.pc, STOP);
    assert_eq!(nothing.outcome.ar[3], 10);
    assert_eq!(nothing.escapes.len(), 4);
    assert_eq!(nothing.outcome.instret, 4);
}
