//! Atomic instruction execution (A extension: LR.W, SC.W, AMOSWAP.W, AMOADD.W, AMOXOR.W, AMOAND.W, AMOOR.W)
//!
//! These instructions provide atomic memory operations. In single-threaded emulation,
//! they are implemented as regular load/store operations.

extern crate alloc;

use super::{ExecutionResult, LoggingMode, read_reg};
use crate::emu::{error::EmulatorError, logging::InstLog, memory::Memory};
use lp_riscv_inst::{Gpr, format::TypeR};

/// Decode and execute atomic instructions (R-type, opcode 0x2f).
pub(super) fn decode_execute_atomic<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let r = TypeR::from_riscv(inst_word);
    let rd = Gpr::new(r.rd);
    let rs1 = Gpr::new(r.rs1);
    let rs2 = Gpr::new(r.rs2);
    let funct3 = (r.func & 0x7) as u8; // Extract funct3 from func field
    let funct5 = ((inst_word >> 27) & 0x1f) as u8; // bits [31:27]

    // Atomic instructions require funct3 = 0x2 (word width)
    if funct3 != 0x2 {
        return Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unsupported atomic width: funct3=0x{funct3:x}"),
            regs: *regs,
        });
    }

    match funct5 {
        0x02 => execute_lr_w::<M>(rd, rs1, inst_word, pc, regs, memory),
        0x03 => execute_sc_w::<M>(rd, rs1, rs2, inst_word, pc, regs, memory),
        0x01 => execute_amoswap_w::<M>(rd, rs1, rs2, inst_word, pc, regs, memory),
        0x00 => execute_amoadd_w::<M>(rd, rs1, rs2, inst_word, pc, regs, memory),
        0x04 => execute_amoxor_w::<M>(rd, rs1, rs2, inst_word, pc, regs, memory),
        0x0c => execute_amoand_w::<M>(rd, rs1, rs2, inst_word, pc, regs, memory),
        0x08 => execute_amoor_w::<M>(rd, rs1, rs2, inst_word, pc, regs, memory),
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unknown atomic instruction: funct5=0x{funct5:x}"),
            regs: *regs,
        }),
    }
}

#[inline(always)]
fn execute_lr_w<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    // LR.W: Load reserved word (just a regular load in single-threaded)
    let base = read_reg(regs, rs1);
    let address = base as u32;

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
            instruction: inst_word,
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
fn execute_sc_w<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    // SC.W: Store conditional word (always succeeds in single-threaded)
    let base = read_reg(regs, rs1);
    let value = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = if M::ENABLED {
        memory.read_word(address).unwrap_or(0)
    } else {
        0
    };
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

    // Return 0 in rd to indicate success
    if rd.num() != 0 {
        regs[rd.num() as usize] = 0;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
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

#[inline(always)]
fn execute_amoswap_w<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOSWAP.W: Atomically swap word
    let base = read_reg(regs, rs1);
    let new_value = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory.read_word(address).map_err(|mut e| {
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

    memory.write_word(address, new_value).map_err(|mut e| {
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

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: new_value,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
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
fn execute_amoadd_w<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOADD.W: Atomically add word
    let base = read_reg(regs, rs1);
    let addend = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory.read_word(address).map_err(|mut e| {
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

    let new_value = old_value.wrapping_add(addend);
    memory.write_word(address, new_value).map_err(|mut e| {
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

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: addend,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
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
fn execute_amoxor_w<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOXOR.W: Atomically XOR word
    let base = read_reg(regs, rs1);
    let xor_val = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory.read_word(address).map_err(|mut e| {
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

    let new_value = old_value ^ xor_val;
    memory.write_word(address, new_value).map_err(|mut e| {
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

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: xor_val,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
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
fn execute_amoand_w<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOAND.W: Atomically AND word
    let base = read_reg(regs, rs1);
    let and_val = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory.read_word(address).map_err(|mut e| {
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

    let new_value = old_value & and_val;
    memory.write_word(address, new_value).map_err(|mut e| {
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

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: and_val,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
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
fn execute_amoor_w<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    // AMOOR.W: Atomically OR word
    let base = read_reg(regs, rs1);
    let or_val = read_reg(regs, rs2);
    let address = base as u32;

    let error_regs = *regs;
    let old_value = memory.read_word(address).map_err(|mut e| {
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

    let new_value = old_value | or_val;
    memory.write_word(address, new_value).map_err(|mut e| {
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

    // Return old value in rd
    if rd.num() != 0 {
        regs[rd.num() as usize] = old_value;
    }

    let log = if M::ENABLED {
        Some(InstLog::Store {
            cycle: 0,
            pc,
            instruction: inst_word,
            rs1_val: base,
            rs2_val: or_val,
            addr: address,
            mem_old: old_value,
            mem_new: new_value,
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
    use super::super::{LoggingDisabled, LoggingEnabled};
    use super::*;
    use crate::emu::memory::{DEFAULT_RAM_START, Memory};
    use alloc::vec;
    use lp_riscv_inst::Gpr;

    // Helper to encode atomic instructions manually
    // Format: opcode=0x2f, funct3=0x2, funct5 in bits [31:27]
    // Atomic instructions use R-type format but with funct5 instead of funct7
    fn encode_atomic(rd: Gpr, rs1: Gpr, rs2: Gpr, funct5: u8) -> u32 {
        // Manually construct the instruction word
        // opcode[6:0] = 0x2f
        // rd[11:7] = rd
        // funct3[14:12] = 0x2
        // rs1[19:15] = rs1
        // rs2[24:20] = rs2
        // funct5[31:27] = funct5
        let opcode = 0x2f;
        let funct3 = 0x2;
        (opcode as u32)
            | ((rd.num() as u32) << 7)
            | ((funct3 as u32) << 12)
            | ((rs1.num() as u32) << 15)
            | ((rs2.num() as u32) << 20)
            | ((funct5 as u32) << 27)
    }

    fn encode_lr_w(rd: Gpr, rs1: Gpr) -> u32 {
        // LR.W: funct5=0x02, rs2=0
        encode_atomic(rd, rs1, Gpr::Zero, 0x02)
    }

    fn encode_sc_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // SC.W: funct5=0x03
        encode_atomic(rd, rs1, rs2, 0x03)
    }

    fn encode_amoswap_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOSWAP.W: funct5=0x01
        encode_atomic(rd, rs1, rs2, 0x01)
    }

    fn encode_amoadd_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOADD.W: funct5=0x00
        encode_atomic(rd, rs1, rs2, 0x00)
    }

    fn encode_amoxor_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOXOR.W: funct5=0x04
        encode_atomic(rd, rs1, rs2, 0x04)
    }

    fn encode_amoand_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOAND.W: funct5=0x0c
        encode_atomic(rd, rs1, rs2, 0x0c)
    }

    fn encode_amoor_w(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
        // AMOOR.W: funct5=0x08
        encode_atomic(rd, rs1, rs2, 0x08)
    }

    #[test]
    fn test_lr_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32; // x10 = base address
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_lr_w(Gpr::A0, Gpr::A0);
        let result =
            decode_execute_atomic::<LoggingEnabled>(inst_word, 0x1000, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[10], 0x12345678);
        assert_eq!(result.new_pc, None);
        assert_eq!(result.should_halt, false);
        assert_eq!(result.syscall, false);
    }

    #[test]
    fn test_sc_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32; // x10 = base address
        regs[11] = 0x12345678; // x11 = value to store
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory
            .write_word(DEFAULT_RAM_START, 0xdeadbeefu32 as i32)
            .unwrap();

        let inst_word = encode_sc_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let result =
            decode_execute_atomic::<LoggingEnabled>(inst_word, 0x1000, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[10], 0); // SC.W returns 0 on success
        assert_eq!(memory.read_word(DEFAULT_RAM_START).unwrap(), 0x12345678);
        assert_eq!(result.new_pc, None);
    }

    #[test]
    fn test_amoswap_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32; // x10 = base address
        regs[11] = 0xdeadbeefu32 as i32; // x11 = new value
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_amoswap_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let result =
            decode_execute_atomic::<LoggingEnabled>(inst_word, 0x1000, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[10], 0x12345678); // Returns old value
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            0xdeadbeefu32 as i32
        );
        assert_eq!(result.new_pc, None);
    }

    #[test]
    fn test_amoadd_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32; // x10 = base address
        regs[11] = 5; // x11 = addend
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 10).unwrap();

        let inst_word = encode_amoadd_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let result =
            decode_execute_atomic::<LoggingEnabled>(inst_word, 0x1000, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[10], 10); // Returns old value
        assert_eq!(memory.read_word(DEFAULT_RAM_START).unwrap(), 15);
        assert_eq!(result.new_pc, None);
    }

    #[test]
    fn test_amoxor_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32;
        regs[11] = 0xffff0000u32 as i32; // XOR mask
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_amoxor_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let _result =
            decode_execute_atomic::<LoggingEnabled>(inst_word, 0x1000, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[10], 0x12345678); // Returns old value
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            (0x12345678u32 ^ 0xffff0000u32) as i32
        );
    }

    #[test]
    fn test_amoand_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32;
        regs[11] = 0x0000ffff; // AND mask
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_amoand_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let _result =
            decode_execute_atomic::<LoggingEnabled>(inst_word, 0x1000, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[10], 0x12345678); // Returns old value
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            0x12345678 & 0x0000ffff
        );
    }

    #[test]
    fn test_amoor_w() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32;
        regs[11] = 0x0000ffff; // OR mask
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12340000).unwrap();

        let inst_word = encode_amoor_w(Gpr::A0, Gpr::A0, Gpr::A1);
        let _result =
            decode_execute_atomic::<LoggingEnabled>(inst_word, 0x1000, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[10], 0x12340000); // Returns old value
        assert_eq!(
            memory.read_word(DEFAULT_RAM_START).unwrap(),
            0x12340000 | 0x0000ffff
        );
    }

    #[test]
    fn test_fast_path() {
        let mut regs = [0i32; 32];
        regs[10] = DEFAULT_RAM_START as i32;
        let mut memory = Memory::with_default_addresses(vec![], vec![0u8; 1024]);
        memory.write_word(DEFAULT_RAM_START, 0x12345678).unwrap();

        let inst_word = encode_lr_w(Gpr::A0, Gpr::A0);
        let result =
            decode_execute_atomic::<LoggingDisabled>(inst_word, 0x1000, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[10], 0x12345678);
        assert!(result.log.is_none()); // Fast path has no logging
    }
}
