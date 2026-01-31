//! Instruction-level tests for RISC-V emulator.
//!
//! These tests verify individual instruction decoding, encoding, and execution.

use lp_riscv_emu::{Riscv32Emulator, StepResult};
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

    let mut emu = Riscv32Emulator::new(code, ram).with_max_instructions(10);

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

    let mut emu = Riscv32Emulator::new(code, ram).with_max_instructions(10);

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

    let mut emu = Riscv32Emulator::new(code, ram).with_max_instructions(10);

    // First instruction should succeed
    let result = emu.step();
    assert!(result.is_ok(), "Setting register should succeed");

    // Second instruction (unaligned load) should fail
    let result = emu.step();
    assert!(result.is_err(), "Unaligned load should fail");
}
