//! Calling **from SRAM into flash** — the reach a JIT'd shader needs, and the
//! reach a direct call does not have.
//!
//! On hardware, firmware `.text` executes from flash through the cache and
//! JIT-produced code lives in SRAM, tens of megabytes apart. A `CALL8` encodes
//! an 18-bit signed word displacement — ±512 KiB — so it *cannot* express that
//! jump, which is why `isa::xt`'s emitter reaches every call target through a
//! literal-pool slot and `CALLX8`.
//!
//! While the emulator modeled both in one 128 KiB region, that constraint was
//! invisible: an accidentally-direct call would have been in range on the host
//! and out of range on silicon — passing tests, failing hardware. These tests
//! pin both halves now that the two live where they live on the device.

use lp_xt_emu::board::BoardProfile;
use lp_xt_emu::emu::CallOutcome;
use lp_xt_emu::memory::EXC_INSTR_FETCH_ERROR;
use lp_xt_emu::{Emulator, NoopTracer, TrapKind};
use lp_xt_inst::{CallOp, Inst, NullaryOp, Reg, encode};

/// Callee frame size. Any multiple of 8 works.
const FRAME: u32 = 32;
/// What the flash-resident callee returns. Fits `movi`'s 12-bit signed
/// immediate, so the callee is four instructions with no literal pool of its
/// own — the value only has to be distinguishable from zero and from a wrapped
/// address.
const ANSWER: u32 = 0x7EF;

fn a(n: u8) -> Reg {
    Reg::new(n)
}

fn asm(insts: &[Inst]) -> Vec<u8> {
    insts.iter().flat_map(encode).collect()
}

/// A windowed callee: `entry a1,32; movi a2,ANSWER; retw`. Placed in flash.
fn flash_callee() -> Vec<u8> {
    asm(&[
        Inst::Entry(a(1), FRAME),
        Inst::Movi(a(2), ANSWER as i32),
        Inst::Nullary(NullaryOp::Retw),
    ])
}

/// The word displacement a `CALL` at `pc` would need to reach `target`, per the
/// executor's own rule: `target = (pc & !3) + (offset << 2) + 4`.
fn call_displacement(pc: u32, target: u32) -> i64 {
    (target as i64 - (pc & !3) as i64 - 4) / 4
}

/// The 18-bit signed field a `CALL` carries.
const CALL_OFFSET_MIN: i64 = -(1 << 17);
const CALL_OFFSET_MAX: i64 = (1 << 17) - 1;

/// The range fact, stated as arithmetic on the real addresses — and it is
/// **not the same fact on both boards**, which is the more useful half.
///
/// On the S3, IROM (`0x4200_0000`) is ~29 MB above the SRAM1 I-bus window: a
/// direct call cannot name it, by three orders of magnitude.
///
/// On classic, flash IROM (`0x400D_0000`) sits directly above the SRAM1 I-bus
/// window (`0x400A_0000..0x400C_0000`) — ~192 KiB away, comfortably inside the
/// ±512 KiB field. A direct call from JIT'd code to a builtin would *work*
/// there. That is a trap for anyone who "optimizes" the indirect form away
/// after testing on one board, so it is pinned rather than left to be
/// rediscovered: the emitter must stay indirect because the S3 requires it, not
/// because every Xtensa target does.
#[test]
fn only_the_s3_puts_flash_out_of_direct_call_range() {
    let s3 = BoardProfile::esp32s3();
    let d = call_displacement(s3.code_ibus_base(), s3.irom_base);
    assert!(
        d > CALL_OFFSET_MAX,
        "S3: CALL8 to IROM {:#010x} needs displacement {d}, which fits the \
         18-bit field — the range constraint this suite exists for would not bite",
        s3.irom_base,
    );
    assert!(
        d.unsigned_abs() > 1_000_000,
        "S3: displacement {d} is suspiciously close to in-range"
    );

    let classic = BoardProfile::esp32();
    let d = call_displacement(classic.code_ibus_base(), classic.irom_base);
    assert!(
        (CALL_OFFSET_MIN..=CALL_OFFSET_MAX).contains(&d),
        "classic: IROM {:#010x} was expected within direct-call reach of the \
         SRAM1 I-bus window (displacement {d}); if the map moved, the comment \
         above needs rewriting, not this assertion relaxing",
        classic.irom_base,
    );
}

/// Executed rather than computed: a `CALL8` whose offset field holds the
/// truncated displacement lands somewhere that is **not** the callee.
///
/// This is the shape of the bug the old single-region model hid. The
/// displacement does not fit, so the field wraps; the emulator jumps where the
/// encoding actually says, which is unmapped, and the run traps on fetch.
#[test]
fn a_direct_call8_toward_flash_traps_instead_of_arriving() {
    let p = BoardProfile::esp32s3();
    let mut emu = Emulator::with_profile(p);

    let callee = p.irom_base + 0x100;
    emu.mem.load_bytes(callee, &flash_callee());

    let entry = p.code_ibus_base();
    // `entry a1,32` is 3 bytes, so the CALL8 sits at entry+3.
    let call_pc = entry + 3;
    let wanted = call_displacement(call_pc, callee);
    assert!(
        wanted > CALL_OFFSET_MAX,
        "expected the displacement to overflow the field, got {wanted}"
    );
    // What the encoder actually emits: the low 18 bits, sign-extended on decode.
    let truncated = ((wanted as i32) << 14) >> 14;
    let code = asm(&[
        Inst::Entry(a(1), FRAME),
        Inst::Call(CallOp::Call8, truncated),
        Inst::Nullary(NullaryOp::Retw),
    ]);
    emu.mem.load_bytes(entry, &code);

    match emu.run_loaded_with_args(entry, &[], &mut NoopTracer, None) {
        CallOutcome::Ok { lo, .. } => panic!(
            "a direct CALL8 must not reach a flash callee, but the run returned \
             {lo:#x} (callee returns {ANSWER:#x})"
        ),
        CallOutcome::Trap(t) => {
            assert_eq!(t.kind, TrapKind::Exception);
            assert_eq!(
                t.cause, EXC_INSTR_FETCH_ERROR,
                "the wrapped target should be unmapped, not merely wrong"
            );
        }
    }
}

/// The form the JIT actually emits — `l32r` a literal holding the absolute
/// address, then `callx8` — reaches the same flash callee and returns its value.
///
/// `l32r` is backward-only, so the literal precedes the code that loads it;
/// that is exactly how `isa::xt`'s emitter lays out its pools.
#[test]
fn an_indirect_callx8_reaches_a_flash_callee() {
    let p = BoardProfile::esp32s3();
    let mut emu = Emulator::with_profile(p);

    let callee = p.irom_base + 0x100;
    emu.mem.load_bytes(callee, &flash_callee());

    // Literal at the code region base, entry one word later.
    let lit = p.code_ibus_base();
    emu.mem.load_bytes(lit, &callee.to_le_bytes());
    let entry = lit + 4;

    // `l32r a8, <lit>` — the field is the one's-complement word distance the
    // disassembler's own helper computes, so build it by search rather than by
    // re-deriving the formula here.
    let l32r_pc = entry + 3; // after `entry a1,32`
    let field = (0u16..=0xFFFF)
        .find(|&f| lp_xt_inst::disasm::l32r_target(l32r_pc, f) == lit)
        .expect("the literal is within l32r's backward reach");

    // `callx8` rotates by 8, so the callee's `a2` result arrives in *our* `a10`;
    // move it to `a2` so our own `retw` returns it.
    let code = asm(&[
        Inst::Entry(a(1), FRAME),
        Inst::L32r(a(8), field),
        Inst::Callx(lp_xt_inst::CallxOp::Callx8, a(8)),
        Inst::Addi(a(2), a(10), 0),
        Inst::Nullary(NullaryOp::Retw),
    ]);
    emu.mem.load_bytes(entry, &code);

    match emu.run_loaded_with_args(entry, &[], &mut NoopTracer, None) {
        CallOutcome::Ok { lo, .. } => assert_eq!(lo, ANSWER),
        CallOutcome::Trap(t) => panic!("indirect call into flash trapped: {t:?}"),
    }
}

/// Flash is read-only: a guest store into either window faults, as it does on
/// hardware, while the loader path (a flasher, not the guest) may write it.
#[test]
fn guest_stores_into_flash_fault() {
    let p = BoardProfile::esp32s3();
    let mut emu = Emulator::with_profile(p);

    assert!(emu.mem.write_u32(p.irom_base, 0xDEAD).is_err(), "IROM store");
    assert!(emu.mem.write_u32(p.drom_base, 0xDEAD).is_err(), "DROM store");

    // Loads work in both windows; a fetch works only in the instruction one.
    emu.mem.load_bytes(p.drom_base, &0xC0DEu32.to_le_bytes());
    assert_eq!(emu.mem.read_u32(p.drom_base).unwrap(), 0xC0DE);
    let mut out = [0u8; 3];
    assert!(emu.mem.fetch(p.irom_base, &mut out).is_ok(), "IROM fetch");
    assert_eq!(
        emu.mem.fetch(p.drom_base, &mut out).unwrap_err().cause,
        EXC_INSTR_FETCH_ERROR,
        "DROM is data only"
    );
}
