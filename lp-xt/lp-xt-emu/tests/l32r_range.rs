//! Regression: `l32r`'s 16-bit field is ONE-extended, not sign-extended.
//!
//! The field always denotes a negative word offset in `-65536..=-1`, so the
//! reachable byte range is `-262144..=-4`. An emulator that sign-extends
//! (`field as i16`) turns every field `0x0000..=0x7fff` into a *forward*
//! offset and silently loads the wrong word — code whose literal pool sits
//! more than 128 KiB back would read garbage instead of its constant.
//!
//! Pinned against `lp_xt_inst::disasm::l32r_target`, which is verified against
//! objdump, and matching `xt_mini_emit::imm`'s `L32rDisp` rule.

use lp_xt_emu::cpu::Cpu;
use lp_xt_emu::emu::{RunOutcome, STACK_DBUS_BASE};
use lp_xt_emu::memory::Memory;
use lp_xt_emu::{Emulator, NoopTracer, SyscallHandler, SyscallOutcome};
use lp_xt_inst::{Inst, NullaryOp, Reg, encode};

/// Byte displacement encoded by `field`, per the one-extended rule.
fn disp_of(field: u16) -> i32 {
    ((field as i32) - 0x1_0000) << 2
}

struct NoSyscalls;
impl SyscallHandler for NoSyscalls {
    fn syscall(&mut self, _cpu: &mut Cpu, _mem: &mut Memory) -> SyscallOutcome {
        panic!("fixture makes no syscalls")
    }
}

#[test]
fn field_encodes_negative_displacements_only() {
    assert_eq!(disp_of(0xFFFF), -4, "nearest literal");
    assert_eq!(disp_of(0x8000), -131_072, "last sign-extension-safe value");
    assert_eq!(
        disp_of(0x7FFF),
        -131_076,
        "first field a sign-extending decoder gets wrong (it would read +131068)"
    );
    assert_eq!(disp_of(0x0000), -262_144, "farthest literal");
}

/// Executing a far-half `l32r` must load the literal it actually points at.
/// Under the old sign-extending decode this computed a *forward* address and
/// returned the wrong word (or trapped).
#[test]
fn far_half_field_loads_the_right_literal() {
    // Place the code high in the SRAM1 window so a ~128 KiB backward reach
    // lands inside the (already mapped) stack region rather than off the map.
    const CODE_HIGH_DBUS: u32 = 0x3FCE_0000;
    const SENTINEL: u32 = 0xC0FF_EE01;
    let field: u16 = 0x7FFF; // first far-half value

    let mut emu = Emulator::new();
    emu.mem.add_sram1(CODE_HIGH_DBUS, 0x1000);

    // entry a1,32 ; l32r a2,<field> ; retw  — returns the loaded literal.
    let mut code = encode(&Inst::Entry(Reg::new(1), 32));
    code.extend(encode(&Inst::L32r(Reg::new(2), field)));
    code.extend(encode(&Inst::Nullary(NullaryOp::Retw)));
    emu.mem.load_bytes(CODE_HIGH_DBUS, &code);

    let entry_ibus = Memory::ibus_alias(CODE_HIGH_DBUS);
    let l32r_pc = entry_ibus + 3; // the l32r is the second instruction

    // Where the literal must live, per the objdump-verified formula.
    let lit_ibus = lp_xt_inst::disasm::l32r_target(l32r_pc, field);
    assert_eq!(
        lit_ibus as i64,
        ((l32r_pc.wrapping_add(3) & !3) as i64) + disp_of(field) as i64,
        "target math must match the one-extended rule"
    );
    let lit_dbus = lit_ibus - lp_xt_emu::memory::IBUS_ALIAS_OFFSET;
    assert_eq!(
        lit_dbus, STACK_DBUS_BASE,
        "test layout assumption: the literal lands at the stack region base"
    );
    emu.mem
        .write_u32(lit_dbus, SENTINEL)
        .expect("literal address must be mapped");

    match emu.run_loaded(entry_ibus, 0, &mut NoopTracer, &mut NoSyscalls) {
        RunOutcome::Ok(got) => assert_eq!(
            got, SENTINEL,
            "far-half l32r loaded the wrong word (sign-extension regression)"
        ),
        other => panic!("far-half l32r must execute, got {other:?}"),
    }
}
