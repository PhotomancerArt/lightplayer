//! Jump and immediate generation instruction execution (JAL, JALR, LUI, AUIPC)

use super::{ExecutionResult, LoggingMode, read_reg};
use crate::emu::{error::EmulatorError, logging::InstLog, memory::Memory};
use lp_riscv_inst::{
    Gpr,
    format::{TypeI, TypeJ, TypeU},
};

/// Decode and execute JAL instruction (J-type, opcode 0x6f).
pub(super) fn decode_execute_jal<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let j = TypeJ::from_riscv(inst_word);
    let rd = Gpr::new(j.rd);
    let imm = j.imm;
    execute_jal::<M>(rd, imm, inst_word, pc, regs)
}

/// Decode and execute JALR instruction (I-type, opcode 0x67).
pub(super) fn decode_execute_jalr<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let i = TypeI::from_riscv(inst_word);
    let rd = Gpr::new(i.rd);
    let rs1 = Gpr::new(i.rs1);
    let imm = i.imm;
    execute_jalr::<M>(rd, rs1, imm, inst_word, pc, regs)
}

/// Decode and execute LUI instruction (U-type, opcode 0x37).
pub(super) fn decode_execute_lui<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let u = TypeU::from_riscv(inst_word);
    let rd = Gpr::new(u.rd);
    let imm = u.imm;
    execute_lui::<M>(rd, imm, inst_word, pc, regs)
}

/// Decode and execute AUIPC instruction (U-type, opcode 0x17).
pub(super) fn decode_execute_auipc<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let u = TypeU::from_riscv(inst_word);
    let rd = Gpr::new(u.rd);
    let imm = u.imm;
    execute_auipc::<M>(rd, imm, inst_word, pc, regs)
}

#[inline(always)]
fn execute_jal<M: LoggingMode>(
    rd: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let next_pc = pc.wrapping_add(4);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let target = (pc.wrapping_add(imm as u32)) & !1; // Ensure 2-byte alignment (RVC support)
    if rd.num() != 0 {
        regs[rd.num() as usize] = next_pc as i32;
    }

    let log = if M::ENABLED {
        Some(InstLog::Jump {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd_old,
            rd_new: if rd.num() == 0 {
                None
            } else {
                Some(next_pc as i32)
            },
            target_pc: target,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: Some(target),
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_jalr<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let base = read_reg(regs, rs1);
    let next_pc = pc.wrapping_add(4);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let target = (base.wrapping_add(imm) as u32) & !1; // Clear bottom bit for 2-byte alignment (RVC support)
    if rd.num() != 0 {
        regs[rd.num() as usize] = next_pc as i32;
    }

    let log = if M::ENABLED {
        Some(InstLog::Jump {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd_old,
            rd_new: if rd.num() == 0 {
                None
            } else {
                Some(next_pc as i32)
            },
            target_pc: target,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: Some(target),
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_lui<M: LoggingMode>(
    rd: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    // For LUI, the immediate is the upper 20 bits (bits [31:12] of the instruction)
    // TypeU extracts it as a signed i32, but we need to treat it as unsigned for shifting
    // to avoid overflow. The immediate is already in bits [31:12], so we extract the
    // upper 20 bits and shift left by 12.
    let imm_u32 = imm as u32;
    // Extract upper 20 bits: (imm_u32 >> 12) & 0xfffff, then shift left by 12
    let upper_20_bits = (imm_u32 >> 12) & 0xfffff;
    let value = (upper_20_bits << 12) as i32;
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Immediate {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rd_old,
            rd_new: value,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_auipc<M: LoggingMode>(
    rd: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    // AUIPC: rd = pc + imm
    // The imm field is already the sign-extended and shifted immediate value
    let value = (pc.wrapping_add(imm as u32)) as i32;
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Immediate {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rd_old,
            rd_new: value,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: None,
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
    use crate::emu::executor::LoggingDisabled;
    use crate::emu::memory::Memory;
    use lp_riscv_inst::{Gpr, encode};

    #[test]
    fn test_jal_fast_path() {
        let mut regs = [0i32; 32];
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        let inst_word = encode::jal(Gpr::new(1), 8);
        let result =
            decode_execute_jal::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(regs[1], 4); // PC + 4
        assert_eq!(result.new_pc, Some(8));
        assert!(result.log.is_none());
    }

    #[test]
    fn test_lui_fast_path() {
        let mut regs = [0i32; 32];
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // LUI x1, 0x12345 -> x1 = 0x12345000
        let inst_word = encode::lui(Gpr::new(1), 0x12345000);
        let result =
            decode_execute_lui::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(regs[1], 0x12345000);
        assert!(result.log.is_none());
    }
}
