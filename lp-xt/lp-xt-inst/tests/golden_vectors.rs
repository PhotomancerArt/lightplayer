//! Spike golden vectors GV1–GV3b (FINDINGS.md / fw/spike-esp32s3/src/e4.rs).
//!
//! All bytes are objdump-derived from toolchain-assembled or hardware-verified
//! references — never hand-recalled (spike lesson: recalls were wrong 2/3).

use lp_xt_inst::*;

fn r(n: u8) -> Reg {
    Reg::new(n)
}

/// Decode one instruction, asserting it round-trips to the exact input bytes.
#[track_caller]
fn dec(bytes: &[u8]) -> (Inst, usize) {
    let (inst, len) = decode(bytes).expect("decode");
    assert_eq!(len, bytes.len(), "length for {inst:?}");
    assert_eq!(encode(&inst), bytes, "round-trip for {inst:?}");
    (inst, len)
}

// ---- GV1: spike_stub42 (minimal windowed function) ----
#[test]
fn gv1_stub42() {
    assert_eq!(dec(&[0x36, 0x41, 0x00]).0, Inst::Entry(r(1), 32));
    assert_eq!(dec(&[0x22, 0xa0, 0x2a]).0, Inst::Movi(r(2), 42));
    assert_eq!(dec(&[0x90, 0x00, 0x00]).0, Inst::Nullary(NullaryOp::Retw));
}

// ---- GV2: spike_call_blob (literal pool + CALLX8 builtin call) ----
#[test]
fn gv2_call_blob() {
    assert_eq!(dec(&[0x36, 0x61, 0x00]).0, Inst::Entry(r(1), 48));
    assert_eq!(dec(&[0x81, 0xfe, 0xff]).0, Inst::L32r(r(8), 0xfffe));
    assert_eq!(dec(&[0xa2, 0xa0, 0x2a]).0, Inst::Movi(r(10), 42));
    assert_eq!(
        dec(&[0xe0, 0x08, 0x00]).0,
        Inst::Callx(CallxOp::Callx8, r(8))
    );
    // `mov a2, a10` is assembled as a wide `or a2, a10, a10`.
    assert_eq!(
        dec(&[0xa0, 0x2a, 0x20]).0,
        Inst::Rrr(AluRrr::Or, r(2), r(10), r(10))
    );
    assert_eq!(dec(&[0x90, 0x00, 0x00]).0, Inst::Nullary(NullaryOp::Retw));
}

// ---- GV3a: spike_rec (self-recursive windowed stub, f(d) = d) ----
const REC_BLOB: [u8; 31] = [
    0x00, 0x00, 0x00, 0x00, // +0  literal
    0x36, 0x41, 0x00, // +4  entry a1, 32
    0x16, 0xe2, 0x00, // +7  beqz a2, +14
    0x81, 0xfd, 0xff, // +10 l32r a8, <slot +0>
    0xa2, 0xc2, 0xff, // +13 addi a10, a2, -1
    0xe0, 0x08, 0x00, // +16 callx8 a8
    0x22, 0xca, 0x01, // +19 addi a2, a10, 1
    0x90, 0x00, 0x00, // +22 retw
    0x22, 0xa0, 0x00, // +25 movi a2, 0
    0x90, 0x00, 0x00, // +28 retw
];

#[test]
fn gv3a_rec() {
    // Walk from the entry point (past the 4-byte literal pool) with variable length.
    let insts = walk(&REC_BLOB[4..]);
    assert_eq!(
        insts,
        vec![
            Inst::Entry(r(1), 32),
            Inst::BranchZ(BrZ::Beqz, r(2), 14),
            Inst::L32r(r(8), 0xfffd),
            Inst::Addi(r(10), r(2), -1),
            Inst::Callx(CallxOp::Callx8, r(8)),
            Inst::Addi(r(2), r(10), 1),
            Inst::Nullary(NullaryOp::Retw),
            Inst::Movi(r(2), 0),
            Inst::Nullary(NullaryOp::Retw),
        ]
    );
    // beqz target resolves relative to its own PC: entry at pc=4, beqz at pc=7.
    assert_eq!(format_instruction(&REC_BLOB[7..10], 7), "beqz\ta2, 0x19");
}

// ---- GV3b: spike_recb (recursion + builtin base case, f(d) = d + 21) ----
const RECB_BLOB: [u8; 44] = [
    0x00, 0x00, 0x00, 0x00, // +0 literal: self
    0x00, 0x00, 0x00, 0x00, // +4 literal: builtin
    0x36, 0x41, 0x00, // +8  entry a1, 32
    0x16, 0xe2, 0x00, // +11 beqz a2, +14
    0x81, 0xfc, 0xff, // +14 l32r a8, <slot +0>
    0xa2, 0xc2, 0xff, // +17 addi a10, a2, -1
    0xe0, 0x08, 0x00, // +20 callx8 a8
    0x22, 0xca, 0x01, // +23 addi a2, a10, 1
    0x90, 0x00, 0x00, // +26 retw
    0x81, 0xf9, 0xff, // +29 l32r a8, <slot +4>
    0xa2, 0xa0, 0x07, // +32 movi a10, 7
    0xe0, 0x08, 0x00, // +35 callx8 a8
    0xa0, 0x2a, 0x20, // +38 mov a2, a10 (wide or)
    0x90, 0x00, 0x00, // +41 retw
];

#[test]
fn gv3b_recb() {
    let insts = walk(&RECB_BLOB[8..]);
    assert_eq!(
        insts,
        vec![
            Inst::Entry(r(1), 32),
            Inst::BranchZ(BrZ::Beqz, r(2), 14),
            Inst::L32r(r(8), 0xfffc),
            Inst::Addi(r(10), r(2), -1),
            Inst::Callx(CallxOp::Callx8, r(8)),
            Inst::Addi(r(2), r(10), 1),
            Inst::Nullary(NullaryOp::Retw),
            Inst::L32r(r(8), 0xfff9),
            Inst::Movi(r(10), 7),
            Inst::Callx(CallxOp::Callx8, r(8)),
            Inst::Rrr(AluRrr::Or, r(2), r(10), r(10)),
            Inst::Nullary(NullaryOp::Retw),
        ]
    );
}

/// Decode every instruction in a byte stream, checking each round-trips exactly.
fn walk(mut bytes: &[u8]) -> Vec<Inst> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let (inst, len) = decode(bytes).expect("decode in walk");
        assert_eq!(encode(&inst), &bytes[..len], "round-trip for {inst:?}");
        out.push(inst);
        bytes = &bytes[len..];
    }
    out
}

/// The L32R backward-target formula, cross-checked against objdump.
/// (objdump: `4201008e: 184201  l32r a0, 41fd6198`)
#[test]
fn l32r_target_formula() {
    assert_eq!(
        format_instruction(&[0x01, 0x42, 0x18], 0x4201_008e),
        "l32r\ta0, 0x41fd6198"
    );
}

/// SYSCALL, assembler-derived (`xtensa-esp32s3-elf-as`: `syscall` -> `00 50 00`).
#[test]
fn syscall_golden() {
    assert_eq!(
        dec(&[0x00, 0x50, 0x00]).0,
        Inst::Nullary(NullaryOp::Syscall)
    );
}
