//! Load and store instruction execution (LB, LH, LW, LBU, LHU, SB, SH, SW)

extern crate alloc;

use super::{ExecutionResult, LoggingMode, read_reg};
use crate::emu::{error::EmulatorError, logging::InstLog, memory::Memory};
use lp_riscv_inst::{
    Gpr,
    format::{TypeI, TypeS},
};

/// Decode and execute load instructions (I-type, opcode 0x03).
pub(super) fn decode_execute_load<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let i = TypeI::from_riscv(inst_word);
    let rd = Gpr::new(i.rd);
    let rs1 = Gpr::new(i.rs1);
    let funct3 = i.func;
    let imm = i.imm;

    match funct3 {
        0x0 => execute_lb::<M>(rd, rs1, imm, inst_word, pc, regs, memory),
        0x1 => execute_lh::<M>(rd, rs1, imm, inst_word, pc, regs, memory),
        0x2 => execute_lw::<M>(rd, rs1, imm, inst_word, pc, regs, memory),
        0x4 => execute_lbu::<M>(rd, rs1, imm, inst_word, pc, regs, memory),
        0x5 => execute_lhu::<M>(rd, rs1, imm, inst_word, pc, regs, memory),
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!(
                "Invalid load instruction: funct3=0x{:x} (reserved on RV32)",
                funct3
            ),
            regs: *regs,
        }),
    }
}

/// Decode and execute store instructions (S-type, opcode 0x23).
pub(super) fn decode_execute_store<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let s = TypeS::from_riscv(inst_word);
    let rs1 = Gpr::new(s.rs1);
    let rs2 = Gpr::new(s.rs2);
    let funct3 = s.func;
    let imm = s.imm;

    match funct3 {
        0x0 => execute_sb::<M>(rs1, rs2, imm, inst_word, pc, regs, memory),
        0x1 => execute_sh::<M>(rs1, rs2, imm, inst_word, pc, regs, memory),
        0x2 => execute_sw::<M>(rs1, rs2, imm, inst_word, pc, regs, memory),
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unknown store instruction: funct3=0x{:x}", funct3),
            regs: *regs,
        }),
    }
}

#[inline(always)]
fn execute_lb<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let base = read_reg(regs, rs1);
    let address = base.wrapping_add(imm) as u32;

    let error_regs = *regs;
    let byte_val = memory.read_byte(address).map_err(|mut e| {
        match &mut e {
            EmulatorError::InvalidMemoryAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            _ => {}
        }
        e
    })?;
    let value = byte_val as i32; // Sign extend

    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Load {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rs1_val: base,
            addr: address,
            mem_val: value,
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
fn execute_lh<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let base = read_reg(regs, rs1);
    let address = base.wrapping_add(imm) as u32;

    let error_regs = *regs;
    let half_val = memory.read_halfword(address).map_err(|mut e| {
        match &mut e {
            EmulatorError::InvalidMemoryAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            EmulatorError::UnalignedAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            _ => {}
        }
        e
    })?;
    let value = half_val as i32; // Sign extend

    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Load {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rs1_val: base,
            addr: address,
            mem_val: value,
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
fn execute_lw<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let base = read_reg(regs, rs1);
    let address = base.wrapping_add(imm) as u32;

    let error_regs = *regs;
    let value = memory.read_word(address).map_err(|mut e| {
        match &mut e {
            EmulatorError::InvalidMemoryAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            EmulatorError::UnalignedAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            _ => {}
        }
        e
    })?;

    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Load {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rs1_val: base,
            addr: address,
            mem_val: value,
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
fn execute_lbu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let base = read_reg(regs, rs1);
    let address = base.wrapping_add(imm) as u32;

    let error_regs = *regs;
    let byte_val = memory.read_byte(address).map_err(|mut e| {
        match &mut e {
            EmulatorError::InvalidMemoryAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            _ => {}
        }
        e
    })?;
    let value = (byte_val as u8) as i32; // Zero extend

    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Load {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rs1_val: base,
            addr: address,
            mem_val: value,
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
fn execute_lhu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let base = read_reg(regs, rs1);
    let address = base.wrapping_add(imm) as u32;

    let error_regs = *regs;
    let half_val = memory.read_halfword(address).map_err(|mut e| {
        match &mut e {
            EmulatorError::InvalidMemoryAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            EmulatorError::UnalignedAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            _ => {}
        }
        e
    })?;
    let value = (half_val as u16) as i32; // Zero extend

    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Load {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rs1_val: base,
            addr: address,
            mem_val: value,
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
fn execute_sb<M: LoggingMode>(
    rs1: Gpr,
    rs2: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let base = read_reg(regs, rs1);
    let value = read_reg(regs, rs2);
    let address = base.wrapping_add(imm) as u32;

    let old_byte = if M::ENABLED {
        memory.read_byte(address).unwrap_or(0)
    } else {
        0
    };
    let old_value = old_byte as i32;

    let error_regs = *regs;
    memory.write_byte(address, value as i8).map_err(|mut e| {
        match &mut e {
            EmulatorError::InvalidMemoryAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            _ => {}
        }
        e
    })?;

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rs1_val: base,
            rs2_val: value,
            addr: address,
            mem_old: old_value,
            mem_new: (value as i8) as i32,
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
fn execute_sh<M: LoggingMode>(
    rs1: Gpr,
    rs2: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let base = read_reg(regs, rs1);
    let value = read_reg(regs, rs2);
    let address = base.wrapping_add(imm) as u32;

    let old_half = if M::ENABLED {
        memory.read_halfword(address).unwrap_or(0)
    } else {
        0
    };
    let old_value = old_half as i32;

    let error_regs = *regs;
    memory
        .write_halfword(address, value as i16)
        .map_err(|mut e| {
            match &mut e {
                EmulatorError::InvalidMemoryAccess {
                    regs: err_regs,
                    pc: err_pc,
                    ..
                } => {
                    *err_regs = error_regs;
                    *err_pc = pc;
                }
                EmulatorError::UnalignedAccess {
                    regs: err_regs,
                    pc: err_pc,
                    ..
                } => {
                    *err_regs = error_regs;
                    *err_pc = pc;
                }
                _ => {}
            }
            e
        })?;

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rs1_val: base,
            rs2_val: value,
            addr: address,
            mem_old: old_value,
            mem_new: (value as i16) as i32,
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
fn execute_sw<M: LoggingMode>(
    rs1: Gpr,
    rs2: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let base = read_reg(regs, rs1);
    let value = read_reg(regs, rs2);
    let address = base.wrapping_add(imm) as u32;

    let old_value = if M::ENABLED {
        memory.read_word(address).unwrap_or(0)
    } else {
        0
    };

    let error_regs = *regs;
    memory.write_word(address, value).map_err(|mut e| {
        match &mut e {
            EmulatorError::InvalidMemoryAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            EmulatorError::UnalignedAccess {
                regs: err_regs,
                pc: err_pc,
                ..
            } => {
                *err_regs = error_regs;
                *err_pc = pc;
            }
            _ => {}
        }
        e
    })?;

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rs1_val: base,
            rs2_val: value,
            addr: address,
            mem_old: old_value,
            mem_new: value,
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
    use crate::emu::executor::{LoggingDisabled, LoggingEnabled};
    use crate::emu::memory::Memory;
    use lp_riscv_inst::{Gpr, encode};

    #[test]
    fn test_lw_fast_path() {
        let mut regs = [0i32; 32];
        let ram_addr = crate::emu::memory::DEFAULT_RAM_START;
        regs[1] = ram_addr as i32; // Base address
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(ram_addr, 0x12345678).unwrap();

        let inst_word = encode::lw(Gpr::new(3), Gpr::new(1), 0);
        let result =
            decode_execute_load::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(regs[3], 0x12345678);
        assert!(result.log.is_none());
    }

    #[test]
    fn test_lw_logging_path() {
        let mut regs = [0i32; 32];
        let ram_addr = crate::emu::memory::DEFAULT_RAM_START;
        regs[1] = ram_addr as i32;
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(ram_addr, 0x12345678).unwrap();

        let inst_word = encode::lw(Gpr::new(3), Gpr::new(1), 0);
        let result =
            decode_execute_load::<LoggingEnabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(regs[3], 0x12345678);
        assert!(result.log.is_some());
        if let Some(InstLog::Load { mem_val, .. }) = result.log {
            assert_eq!(mem_val, 0x12345678);
        }
    }

    #[test]
    fn test_sw_fast_path() {
        let mut regs = [0i32; 32];
        let ram_addr = crate::emu::memory::DEFAULT_RAM_START;
        regs[1] = ram_addr as i32; // Base address
        regs[2] = 0x12345678; // Value to store
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);

        let inst_word = encode::sw(Gpr::new(1), Gpr::new(2), 0);
        let result =
            decode_execute_store::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(memory.read_word(ram_addr).unwrap(), 0x12345678);
        assert!(result.log.is_none());
    }
}
