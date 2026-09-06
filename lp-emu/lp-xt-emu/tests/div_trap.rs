//! Integer divide-by-zero trap model (emulator-only; the emulator-vs-hardware
//! crash-class parity for these traps is asserted by the P3 dual-run corpus in
//! `xt-mini-emit/tests/call_boundary_torture.rs`).
//!
//! Hardware raises `IntegerDivideByZero` (EXCCAUSE 6) from `quos`/`quou`/
//! `rems`/`remu` with a zero divisor; the emulator used to return 0 instead
//! (the gap flagged in `docs/BACKPORT.md`). These tests pin the fixed model:
//! zero divisor -> trap at the dividing instruction's pc; nonzero divisors
//! (including the INT_MIN / -1 overflow edge) produce values.
//!
//! Programs are built with `lp_xt_inst::encode` — no hand-encoded bytes.

use lp_xt_emu::emu::{CODE_DBUS_BASE, RunOutcome};
use lp_xt_emu::error::EXC_INTEGER_DIVIDE_BY_ZERO;
use lp_xt_emu::memory::Memory;
use lp_xt_emu::{Emulator, TrapKind};
use lp_xt_inst::{AluRrr, Inst, NullaryOp, Reg, encode};

const DIV_OPS: [AluRrr; 4] = [AluRrr::Quos, AluRrr::Quou, AluRrr::Rems, AluRrr::Remu];

/// `entry a1,32; movi a3,<divisor>; <op> a2, a2, a3; retw` — f(a) = a op divisor.
fn div_prog(op: AluRrr, divisor: i32) -> Vec<u8> {
    assert!((-2048..=2047).contains(&divisor), "movi range");
    let mut code = Vec::new();
    code.extend(encode(&Inst::Entry(Reg::new(1), 32)));
    code.extend(encode(&Inst::Movi(Reg::new(3), divisor)));
    code.extend(encode(&Inst::Rrr(
        op,
        Reg::new(2),
        Reg::new(2),
        Reg::new(3),
    )));
    code.extend(encode(&Inst::Nullary(NullaryOp::Retw)));
    code
}

fn run(op: AluRrr, divisor: i32, arg: u32) -> RunOutcome {
    let mut emu = Emulator::new();
    emu.run(&div_prog(op, divisor), 0, arg)
}

#[test]
fn divide_by_zero_traps_with_cause_6() {
    for op in DIV_OPS {
        for arg in [0u32, 1, 42, 0x8000_0000, u32::MAX] {
            match run(op, 0, arg) {
                RunOutcome::Trap(t) => {
                    assert_eq!(t.kind, TrapKind::Exception, "{op:?} arg={arg:#x}: {t:?}");
                    assert_eq!(
                        t.cause, EXC_INTEGER_DIVIDE_BY_ZERO,
                        "{op:?} arg={arg:#x}: {t:?}"
                    );
                    // The faulting pc is the dividing instruction: entry (3B)
                    // + movi (3B) after the load base's I-bus alias.
                    let div_pc = Memory::ibus_alias(CODE_DBUS_BASE) + 6;
                    assert_eq!(t.pc, div_pc, "{op:?} arg={arg:#x}: {t:?}");
                    assert_eq!(t.vaddr, 0, "{op:?} arg={arg:#x}: {t:?}");
                }
                other => panic!("{op:?} arg={arg:#x}: expected trap, got {other:?}"),
            }
        }
    }
}

#[test]
fn nonzero_divisors_still_produce_values() {
    let cases: [(AluRrr, i32, u32, u32); 8] = [
        (AluRrr::Quos, 2, (-7i32) as u32, (-3i32) as u32),
        (AluRrr::Rems, 2, (-7i32) as u32, (-1i32) as u32),
        (AluRrr::Quou, 7, 100, 14),
        (AluRrr::Remu, 7, 100, 2),
        // INT_MIN / -1 overflows; the model wraps (INT_MIN, remainder 0) —
        // hardware agreement for this edge is asserted in the P3 corpus.
        (AluRrr::Quos, -1, 0x8000_0000, 0x8000_0000),
        (AluRrr::Rems, -1, 0x8000_0000, 0),
        (AluRrr::Quou, -1, 5, 0), // divisor 0xFFFFFFFF unsigned
        (AluRrr::Remu, -1, 5, 5),
    ];
    for (op, divisor, arg, expect) in cases {
        match run(op, divisor, arg) {
            RunOutcome::Ok(v) => {
                assert_eq!(v, expect, "{op:?} divisor={divisor} arg={arg:#x}")
            }
            other => panic!("{op:?} divisor={divisor} arg={arg:#x}: {other:?}"),
        }
    }
}
