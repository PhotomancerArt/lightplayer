//! Branch instruction execution (B-type: BEQ, BNE, BLT, BGE, BLTU, BGEU)

extern crate alloc;

use super::{ExecutionResult, LoggingMode, read_reg};
use crate::emu::{error::EmulatorError, logging::InstLog, memory::Memory};
use lp_riscv_inst::{Gpr, format::TypeB};

/// Decode and execute branch instructions (B-type, opcode 0x63).
pub(super) fn decode_execute_branch<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let b = TypeB::from_riscv(inst_word);
    let rs1 = Gpr::new(b.rs1);
    let rs2 = Gpr::new(b.rs2);
    let funct3 = b.func;
    let imm = b.imm;

    match funct3 {
        0x0 => execute_beq::<M>(rs1, rs2, imm, inst_word, pc, regs),
        0x1 => execute_bne::<M>(rs1, rs2, imm, inst_word, pc, regs),
        0x4 => execute_blt::<M>(rs1, rs2, imm, inst_word, pc, regs),
        0x5 => execute_bge::<M>(rs1, rs2, imm, inst_word, pc, regs),
        0x6 => execute_bltu::<M>(rs1, rs2, imm, inst_word, pc, regs),
        0x7 => execute_bgeu::<M>(rs1, rs2, imm, inst_word, pc, regs),
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unknown branch instruction: funct3=0x{:x}", funct3),
            regs: *regs,
        }),
    }
}

#[inline(always)]
fn execute_beq<M: LoggingMode>(
    rs1: Gpr,
    rs2: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let taken = val1 == val2;

    let new_pc = if taken {
        Some(pc.wrapping_add(imm as u32))
    } else {
        None
    };

    let log = if M::ENABLED {
        Some(InstLog::Branch {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rs1_val: val1,
            rs2_val: val2,
            taken,
            target_pc: new_pc,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_bne<M: LoggingMode>(
    rs1: Gpr,
    rs2: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let taken = val1 != val2;

    let new_pc = if taken {
        Some(pc.wrapping_add(imm as u32))
    } else {
        None
    };

    let log = if M::ENABLED {
        Some(InstLog::Branch {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rs1_val: val1,
            rs2_val: val2,
            taken,
            target_pc: new_pc,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_blt<M: LoggingMode>(
    rs1: Gpr,
    rs2: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let taken = val1 < val2;

    let new_pc = if taken {
        Some(pc.wrapping_add(imm as u32))
    } else {
        None
    };

    let log = if M::ENABLED {
        Some(InstLog::Branch {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rs1_val: val1,
            rs2_val: val2,
            taken,
            target_pc: new_pc,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_bge<M: LoggingMode>(
    rs1: Gpr,
    rs2: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let taken = val1 >= val2;

    let new_pc = if taken {
        Some(pc.wrapping_add(imm as u32))
    } else {
        None
    };

    let log = if M::ENABLED {
        Some(InstLog::Branch {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rs1_val: val1,
            rs2_val: val2,
            taken,
            target_pc: new_pc,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_bltu<M: LoggingMode>(
    rs1: Gpr,
    rs2: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1) as u32;
    let val2 = read_reg(regs, rs2) as u32;
    let taken = val1 < val2;

    let new_pc = if taken {
        Some(pc.wrapping_add(imm as u32))
    } else {
        None
    };

    let log = if M::ENABLED {
        Some(InstLog::Branch {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rs1_val: val1 as i32,
            rs2_val: val2 as i32,
            taken,
            target_pc: new_pc,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_bgeu<M: LoggingMode>(
    rs1: Gpr,
    rs2: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1) as u32;
    let val2 = read_reg(regs, rs2) as u32;
    let taken = val1 >= val2;

    let new_pc = if taken {
        Some(pc.wrapping_add(imm as u32))
    } else {
        None
    };

    let log = if M::ENABLED {
        Some(InstLog::Branch {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rs1_val: val1 as i32,
            rs2_val: val2 as i32,
            taken,
            target_pc: new_pc,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec;

    use super::*;
    use crate::emu::executor::{LoggingDisabled, LoggingEnabled};
    use crate::emu::memory::Memory;
    use lp_riscv_inst::{encode, Gpr};

    #[test]
    fn test_beq_taken_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 10;
        regs[2] = 10;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        let inst_word = encode::beq(Gpr::new(1), Gpr::new(2), 4);
        let result =
            decode_execute_branch::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(result.new_pc, Some(4));
        assert!(result.log.is_none());
    }

    #[test]
    fn test_beq_not_taken_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 10;
        regs[2] = 20;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        let inst_word = encode::beq(Gpr::new(1), Gpr::new(2), 4);
        let result =
            decode_execute_branch::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(result.new_pc, None);
        assert!(result.log.is_none());
    }

    #[test]
    fn test_beq_logging_path() {
        let mut regs = [0i32; 32];
        regs[1] = 10;
        regs[2] = 10;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        let inst_word = encode::beq(Gpr::new(1), Gpr::new(2), 4);
        let result =
            decode_execute_branch::<LoggingEnabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(result.new_pc, Some(4));
        assert!(result.log.is_some());
        if let Some(InstLog::Branch { taken, .. }) = result.log {
            assert!(taken);
        }
    }
}
