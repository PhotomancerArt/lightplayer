//! Trap handling tests for RISC-V emulator.

use cranelift_codegen::ir::TrapCode;
use lp_riscv_tools::{Riscv32Emulator, StepResult};

#[test]
fn test_trap_with_code() {
    // Test that ebreak at a known trap location returns the correct trap code
    let traps = vec![(0, TrapCode::INTEGER_DIVISION_BY_ZERO)];
    let mut emu = Riscv32Emulator::with_traps(vec![0x73, 0x00, 0x10, 0x00], vec![0; 1024], &traps);

    match emu.step() {
        Ok(StepResult::Trap(code)) => {
            assert_eq!(code, TrapCode::INTEGER_DIVISION_BY_ZERO);
        }
        Ok(other) => panic!("Expected Trap, got {other:?}"),
        Err(e) => panic!("Unexpected error: {e:?}"),
    }
}

#[test]
fn test_trap_without_metadata() {
    // Test that ebreak without trap metadata returns Halted (backward compatibility)
    let mut emu = Riscv32Emulator::new(vec![0x73, 0x00, 0x10, 0x00], vec![0; 1024]);

    match emu.step() {
        Ok(StepResult::Halted) => {} // Expected
        Ok(other) => panic!("Expected Halted, got {other:?}"),
        Err(e) => panic!("Unexpected error: {e:?}"),
    }
}

#[test]
fn test_trap_at_offset_4() {
    // Test trap at offset 4 (not offset 0)
    let code = vec![
        0x13, 0x00, 0x00, 0x00, // nop (addi x0, x0, 0)
        0x73, 0x00, 0x10, 0x00, // ebreak at offset 4
    ];
    let traps = vec![(4, TrapCode::INTEGER_OVERFLOW)];
    let mut emu = Riscv32Emulator::with_traps(code, vec![0; 1024], &traps);

    // First step: execute nop
    match emu.step() {
        Ok(StepResult::Continue) => {}
        Ok(other) => panic!("Expected Continue, got {other:?}"),
        Err(e) => panic!("Unexpected error: {e:?}"),
    }

    // Second step: execute ebreak at offset 4
    match emu.step() {
        Ok(StepResult::Trap(code)) => {
            assert_eq!(code, TrapCode::INTEGER_OVERFLOW);
        }
        Ok(other) => panic!("Expected Trap, got {other:?}"),
        Err(e) => panic!("Unexpected error: {e:?}"),
    }
}

#[test]
fn test_multiple_traps() {
    // Test multiple traps at different offsets
    let code = vec![
        0x73, 0x00, 0x10, 0x00, // ebreak at offset 0
        0x13, 0x00, 0x00, 0x00, // nop
        0x73, 0x00, 0x10, 0x00, // ebreak at offset 8
    ];
    let traps = vec![
        (0, TrapCode::INTEGER_DIVISION_BY_ZERO),
        (8, TrapCode::INTEGER_OVERFLOW),
    ];
    let mut emu = Riscv32Emulator::with_traps(code, vec![0; 1024], &traps);

    // First step: trap at offset 0
    match emu.step() {
        Ok(StepResult::Trap(code)) => {
            assert_eq!(code, TrapCode::INTEGER_DIVISION_BY_ZERO);
        }
        Ok(other) => panic!("Expected Trap, got {other:?}"),
        Err(e) => panic!("Unexpected error: {e:?}"),
    }

    // Reset PC to continue testing
    emu.set_pc(4);

    // Second step: execute nop
    match emu.step() {
        Ok(StepResult::Continue) => {}
        Ok(other) => panic!("Expected Continue, got {other:?}"),
        Err(e) => panic!("Unexpected error: {e:?}"),
    }

    // Third step: trap at offset 8
    match emu.step() {
        Ok(StepResult::Trap(code)) => {
            assert_eq!(code, TrapCode::INTEGER_OVERFLOW);
        }
        Ok(other) => panic!("Expected Trap, got {other:?}"),
        Err(e) => panic!("Unexpected error: {e:?}"),
    }
}

#[test]
fn test_trap_sorting() {
    // Test that traps are sorted correctly even if provided out of order
    let code = vec![
        0x73, 0x00, 0x10, 0x00, // ebreak at offset 0
        0x13, 0x00, 0x00, 0x00, // nop
        0x73, 0x00, 0x10, 0x00, // ebreak at offset 8
    ];
    // Provide traps out of order
    let traps = vec![
        (8, TrapCode::INTEGER_OVERFLOW),
        (0, TrapCode::INTEGER_DIVISION_BY_ZERO),
    ];
    let mut emu = Riscv32Emulator::with_traps(code, vec![0; 1024], &traps);

    // First step: should find trap at offset 0 (sorted correctly)
    match emu.step() {
        Ok(StepResult::Trap(code)) => {
            assert_eq!(code, TrapCode::INTEGER_DIVISION_BY_ZERO);
        }
        Ok(other) => panic!("Expected Trap, got {other:?}"),
        Err(e) => panic!("Unexpected error: {e:?}"),
    }
}

#[test]
fn test_trap_at_absolute_address() {
    // Test that traps registered as absolute addresses are correctly detected
    // when PC matches that absolute address (not offset relative to code_start)
    let code = vec![
        0x13, 0x00, 0x00, 0x00, // nop at 0x0
        0x13, 0x00, 0x00, 0x00, // nop at 0x4
        0x13, 0x00, 0x00, 0x00, // nop at 0x8
        0x73, 0x00, 0x10, 0x00, // ebreak at absolute address 0xc
    ];
    // Register trap at absolute address 0xc (where ebreak executes)
    let traps = vec![(0xc, TrapCode::INTEGER_DIVISION_BY_ZERO)];
    let mut emu = Riscv32Emulator::with_traps(code, vec![0; 1024], &traps);

    // Execute nops first
    emu.step().unwrap(); // nop at 0x0
    emu.step().unwrap(); // nop at 0x4
    emu.step().unwrap(); // nop at 0x8

    // Now PC should be at 0xc where ebreak is
    // This should trap but currently returns Halted (bug)
    match emu.step() {
        Ok(StepResult::Trap(code)) => {
            assert_eq!(code, TrapCode::INTEGER_DIVISION_BY_ZERO);
        }
        Ok(other) => panic!("Expected Trap, got {other:?} - this demonstrates the bug!"),
        Err(e) => panic!("Unexpected error: {e:?}"),
    }
}
