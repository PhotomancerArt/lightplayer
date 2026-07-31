//! Instruction-level tests for RISC-V emulator.
//!
//! These tests verify individual instruction decoding, encoding, and execution.

use lp_emu_core::StepResult;
use lp_riscv_emu::Riscv32Emulator;
use lp_riscv_inst::{Gpr, Inst, decode_instruction, encode};

#[test]
fn test_fence_i_decode_encode() {
    // Test FENCE.I decoding (per RISC-V spec: imm[11:0]=0x001)
    let inst = decode_instruction(0x0010100f).expect("Failed to decode FENCE.I");
    match inst {
        Inst::FenceI => {}
        _ => panic!("Expected FenceI, got {inst:?}"),
    }

    // Test FENCE.I encoding
    let encoded = encode::fence_i();
    assert_eq!(encoded, 0x0010100f);

    // Round-trip test
    let decoded = decode_instruction(encoded).expect("Failed to decode encoded FENCE.I");
    match decoded {
        Inst::FenceI => {}
        _ => panic!("Expected FenceI after round-trip, got {decoded:?}"),
    }
}

#[test]
fn test_fence_i_execution() {
    // Create a minimal emulator with FENCE.I instruction (per RISC-V spec: imm[11:0]=0x001)
    let code: Vec<u8> = vec![
        0x0f, 0x10, 0x10, 0x00, // fence.i (little-endian: 0x0010100f)
        0x73, 0x00, 0x10, 0x00, // ebreak (halt) (little-endian)
    ];
    let ram = vec![0u8; 1024];

    let mut emu = Riscv32Emulator::new(code, ram);

    // Execute FENCE.I - should be a no-op and continue
    let result = emu.step();
    assert!(result.is_ok(), "FENCE.I execution should succeed");
    match result.unwrap() {
        StepResult::Continue => {}
        _ => panic!("FENCE.I should continue execution"),
    }

    // Next instruction should be EBREAK
    let result = emu.step();
    assert!(result.is_ok(), "EBREAK execution should succeed");
    match result.unwrap() {
        StepResult::Halted => {}
        _ => panic!("EBREAK should halt execution"),
    }
}

#[test]
fn test_fence_vs_fence_i() {
    // Verify FENCE and FENCE.I are distinguished correctly
    let fence = decode_instruction(0x0000000f).expect("Failed to decode FENCE");
    match fence {
        Inst::Fence => {}
        _ => panic!("Expected Fence, got {fence:?}"),
    }

    let fence_i = decode_instruction(0x0010100f).expect("Failed to decode FENCE.I");
    match fence_i {
        Inst::FenceI => {}
        _ => panic!("Expected FenceI, got {fence_i:?}"),
    }

    // They should be different
    assert_ne!(fence, fence_i);
}

#[test]
fn test_atomic_instructions() {
    // Test that atomic instructions decode correctly
    // LR.W: 0x1000252f (lr.w a0, (zero))
    let lr_w = decode_instruction(0x1000252f).expect("Failed to decode LR.W");
    match lr_w {
        Inst::LrW { rd, rs1 } => {
            assert_eq!(rd, Gpr::A0);
            assert_eq!(rs1, Gpr::Zero);
        }
        _ => panic!("Expected LrW, got {lr_w:?}"),
    }

    // SC.W: 0x1800252f (sc.w a0, zero, (zero))
    let sc_w = decode_instruction(0x1800252f).expect("Failed to decode SC.W");
    match sc_w {
        Inst::ScW { rd, rs1, rs2 } => {
            assert_eq!(rd, Gpr::A0);
            assert_eq!(rs1, Gpr::Zero);
            assert_eq!(rs2, Gpr::Zero);
        }
        _ => panic!("Expected ScW, got {sc_w:?}"),
    }
}

#[test]
fn test_compressed_instructions() {
    // Test compressed instruction decoding
    // C.ADDI: 0x0001 (c.addi x0, 0) - but this is actually C.NOP
    // C.NOP: 0x0001
    let c_nop = decode_instruction(0x0001).expect("Failed to decode C.NOP");
    match c_nop {
        Inst::CNop => {}
        _ => panic!("Expected CNop, got {c_nop:?}"),
    }
}

#[test]
fn test_division_by_zero() {
    // Test that division by zero returns correct result
    let code: Vec<u8> = vec![
        // li a0, 10
        0x13, 0x05, 0xa0, 0x00, // addi a0, zero, 10 (little-endian)
        // li a1, 0
        0x93, 0x05, 0x00, 0x00, // addi a1, zero, 0 (little-endian)
        // div a2, a0, a1 (should return -1 per RISC-V spec)
        0x33, 0x46, 0xb5, 0x02, // div a2, a0, a1 (little-endian: 0x02b54633, funct3=0x4)
        0x73, 0x00, 0x10, 0x00, // ebreak (little-endian)
    ];
    let ram = vec![0u8; 1024];

    let mut emu = Riscv32Emulator::new(code, ram);

    // Execute until halt
    loop {
        match emu.step() {
            Ok(StepResult::Halted) => break,
            Ok(_) => continue,
            Err(e) => panic!("Emulator error: {e:?}"),
        }
    }

    // Check result: division by zero should return -1
    let a2 = emu.get_register(Gpr::A2);
    assert_eq!(a2, -1, "Division by zero should return -1 per RISC-V spec");
}

#[test]
fn test_unaligned_access() {
    // Test that unaligned memory access is detected
    let code: Vec<u8> = vec![
        // li a0, 1 (unaligned address)
        0x13, 0x05, 0x10, 0x00, // addi a0, zero, 1 (little-endian)
        // lw a1, 0(a0) - should fail (unaligned)
        0x03, 0x25, 0x05, 0x00, // lw a1, 0(a0) (little-endian)
        0x73, 0x00, 0x10, 0x00, // ebreak (little-endian)
    ];
    let ram = vec![0u8; 1024];

    let mut emu = Riscv32Emulator::new(code, ram);

    // First instruction should succeed
    let result = emu.step();
    assert!(result.is_ok(), "Setting register should succeed");

    // Second instruction (unaligned load) should fail
    let result = emu.step();
    assert!(result.is_err(), "Unaligned load should fail");
}

/// `xori rd, rs1, 128` must decode as XORI for **every** immediate, including
/// 0x080.
///
/// The decoder used to carve funct12 == 0x080 out of OP-IMM funct3=100 and
/// call it `zext.h`. Those two are the same 32 bits, and the base-ISA
/// instruction is the one that actually occurs: LLVM emits `xori rd, rs, 128`
/// as the index bias of a large jump table, so the mis-decode turned a
/// `&'static str` lookup into a read of the wrong table slot. It stayed
/// invisible because `zext.h` is a no-op on the small values an enum
/// discriminant takes.
#[test]
fn xori_128_is_xori_not_zexth() {
    let encoded = encode::xori(Gpr::A0, Gpr::A0, 128);
    assert_eq!(encoded, 0x0805_4513, "xori a0, a0, 128");

    match decode_instruction(encoded).expect("decode xori a0, a0, 128") {
        Inst::Xori { rd, rs1, imm } => {
            assert_eq!(rd, Gpr::A0);
            assert_eq!(rs1, Gpr::A0);
            assert_eq!(imm, 128);
        }
        other => panic!("expected Xori, got {other:?}"),
    }
}

/// Executing that instruction must actually flip bit 7.
#[test]
fn xori_128_flips_bit_seven() {
    let code: Vec<u8> = vec![
        0x13, 0x05, 0x30, 0x00, // addi a0, zero, 3
        0x13, 0x45, 0x05, 0x08, // xori a0, a0, 128
        0x73, 0x00, 0x10, 0x00, // ebreak
    ];
    let mut emu = Riscv32Emulator::new(code, vec![0u8; 1024]);
    loop {
        match emu.step() {
            Ok(StepResult::Halted) => break,
            Ok(_) => continue,
            Err(e) => panic!("Emulator error: {e:?}"),
        }
    }
    assert_eq!(
        emu.get_register(Gpr::A0),
        3 ^ 128,
        "xori a0, a0, 128 must xor, not zero-extend"
    );
}

/// `zext.h` belongs to the OP space: RV32 spells it `pack rd, rs1, x0`.
#[test]
fn zexth_uses_the_op_encoding() {
    let encoded = encode::zexth(Gpr::A0, Gpr::A1);
    assert_eq!(encoded & 0x7f, 0x33, "zext.h is an OP (R-type) encoding");
    assert_eq!(encoded, 0x0805_C533, "zext.h a0, a1 == pack a0, a1, x0");

    match decode_instruction(encoded).expect("decode zext.h") {
        Inst::Zexth { rd, rs1 } => {
            assert_eq!(rd, Gpr::A0);
            assert_eq!(rs1, Gpr::A1);
        }
        other => panic!("expected Zexth, got {other:?}"),
    }
}

/// And it must still zero-extend when executed at its real encoding.
#[test]
fn zexth_zero_extends_halfword() {
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&encode::lui(Gpr::A1, 0xABCDC).to_le_bytes());
    code.extend_from_slice(&encode::zexth(Gpr::A0, Gpr::A1).to_le_bytes());
    code.extend_from_slice(&[0x73, 0x00, 0x10, 0x00]); // ebreak

    let mut emu = Riscv32Emulator::new(code, vec![0u8; 1024]);
    loop {
        match emu.step() {
            Ok(StepResult::Halted) => break,
            Ok(_) => continue,
            Err(e) => panic!("Emulator error: {e:?}"),
        }
    }
    let src = emu.get_register(Gpr::A1) as u32;
    assert!(src > 0xFFFF, "fixture must set bits above the halfword");
    assert_eq!(
        emu.get_register(Gpr::A0) as u32,
        src & 0xFFFF,
        "zext.h keeps only the low 16 bits"
    );
}
