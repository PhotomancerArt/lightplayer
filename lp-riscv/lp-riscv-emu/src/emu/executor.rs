//! Instruction executor for RISC-V 32-bit instructions.

use super::{
    error::EmulatorError,
    logging::{InstLog, LogLevel, SystemKind},
    memory::Memory,
};
use lp_riscv_inst::{Gpr, Inst};

/// Result of executing a single instruction.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// New PC value (None means PC += 4)
    pub new_pc: Option<u32>,
    /// Whether execution should stop (EBREAK)
    pub should_halt: bool,
    /// Whether a syscall was encountered (ECALL)
    pub syscall: bool,
    /// Log entry for this instruction (None if logging is disabled)
    pub log: Option<InstLog>,
}

/// Helper to read register (x0 always returns 0)
fn read_reg(regs: &[i32; 32], reg: Gpr) -> i32 {
    if reg.num() == 0 {
        0
    } else {
        regs[reg.num() as usize]
    }
}

/// Execute a decoded instruction.
pub fn execute_instruction(
    inst: Inst,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError> {
    let mut new_pc: Option<u32> = None;
    let mut should_halt = false;
    let mut syscall = false;

    // Execute instruction and conditionally create log
    let log = match inst {
        Inst::Add { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            let result = val1.wrapping_add(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Sub { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            let result = val1.wrapping_sub(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Mulh { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            // MULH: high 32 bits of signed multiply
            let val1_i64 = val1 as i64;
            let val2_i64 = val2 as i64;
            let product = val1_i64.wrapping_mul(val2_i64);
            let result = (product >> 32) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Mulhsu { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            // MULHSU: high 32 bits of signed * unsigned multiply
            let val1_i64 = val1 as i64;
            let val2_u64 = (val2 as u32) as u64;
            let product = ((val1_i64 as i128).wrapping_mul(val2_u64 as i128)) as i64;
            let result = (product >> 32) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Mulhu { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            // MULHU: high 32 bits of unsigned multiply
            let val1_u64 = (val1 as u32) as u64;
            let val2_u64 = (val2 as u32) as u64;
            let product = val1_u64.wrapping_mul(val2_u64);
            let result = (product >> 32) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Mul { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            let result = val1.wrapping_mul(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Div { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            // Handle division by zero: RISC-V specifies result is all 1s
            let result = if val2 == 0 {
                -1i32
            } else if val1 == i32::MIN && val2 == -1 {
                // Overflow case: -2^31 / -1 = 2^31, which overflows i32
                // RISC-V specifies result is -2^31 in this case
                i32::MIN
            } else {
                val1 / val2
            };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Divu { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            // DIVU: unsigned division
            // Handle division by zero: RISC-V specifies result is all 1s (max value)
            let val1_u = val1 as u32;
            let val2_u = val2 as u32;
            let result = if val2_u == 0 {
                -1i32 // All 1s in signed representation
            } else {
                (val1_u / val2_u) as i32
            };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Rem { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            // Handle division by zero: RISC-V specifies result is dividend
            let result = if val2 == 0 {
                val1
            } else if val1 == i32::MIN && val2 == -1 {
                // Overflow case: remainder is 0
                0i32
            } else {
                val1 % val2
            };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Remu { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            // REMU: unsigned remainder
            // Handle division by zero: RISC-V specifies result is dividend
            let val1_u = val1 as u32;
            let val2_u = val2 as u32;
            let result = if val2_u == 0 {
                val1
            } else {
                (val1_u % val2_u) as i32
            };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Addi { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            let result = val1.wrapping_add(imm);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old: rd_old.unwrap(),
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Lb { rd, rs1, imm } => {
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

            let rd_old = if log_level != LogLevel::None {
                Some(read_reg(regs, rd))
            } else {
                None
            };
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
                Some(InstLog::Load {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: base,
                    addr: address,
                    mem_val: value,
                    rd_old: rd_old.unwrap(),
                    rd_new: value,
                })
            } else {
                None
            }
        }
        Inst::Lh { rd, rs1, imm } => {
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

            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
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
            }
        }
        Inst::Lw { rd, rs1, imm } => {
            let base = read_reg(regs, rs1);
            let address = base.wrapping_add(imm) as u32;

            // Save register state for error context
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

            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
                Some(InstLog::Load {
                    cycle: 0, // Will be set by emu
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
            }
        }
        Inst::Lbu { rd, rs1, imm } => {
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

            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
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
            }
        }
        Inst::Lhu { rd, rs1, imm } => {
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

            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
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
            }
        }
        Inst::Sb { rs1, rs2, imm } => {
            let base = read_reg(regs, rs1);
            let value = read_reg(regs, rs2);
            let address = base.wrapping_add(imm) as u32;

            let old_byte = memory.read_byte(address).unwrap_or(0);
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

            if log_level != LogLevel::None {
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
            }
        }
        Inst::Sh { rs1, rs2, imm } => {
            let base = read_reg(regs, rs1);
            let value = read_reg(regs, rs2);
            let address = base.wrapping_add(imm) as u32;

            let old_half = memory.read_halfword(address).unwrap_or(0);
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

            if log_level != LogLevel::None {
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
            }
        }
        Inst::Sw { rs1, rs2, imm } => {
            let base = read_reg(regs, rs1);
            let value = read_reg(regs, rs2);
            let address = base.wrapping_add(imm) as u32;

            // Read old value before write
            let old_value = memory.read_word(address).unwrap_or(0);

            // Save register state for error context
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

            if log_level != LogLevel::None {
                Some(InstLog::Store {
                    cycle: 0, // Will be set by emu
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
            }
        }
        Inst::Jal { rd, imm } => {
            let next_pc = pc.wrapping_add(4);
            let rd_old = read_reg(regs, rd);
            let target = (pc.wrapping_add(imm as u32)) & !1; // Ensure 2-byte alignment (RVC support)
            if rd.num() != 0 {
                regs[rd.num() as usize] = next_pc as i32;
            }
            new_pc = Some(target);

            if log_level != LogLevel::None {
                Some(InstLog::Jump {
                    cycle: 0, // Will be set by emu
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
            }
        }
        Inst::Jalr { rd, rs1, imm } => {
            let base = read_reg(regs, rs1);
            let next_pc = pc.wrapping_add(4);
            let rd_old = read_reg(regs, rd);
            let target = (base.wrapping_add(imm) as u32) & !1; // Clear bottom bit for 2-byte alignment (RVC support)
            if rd.num() != 0 {
                regs[rd.num() as usize] = next_pc as i32;
            }
            new_pc = Some(target);

            if log_level != LogLevel::None {
                Some(InstLog::Jump {
                    cycle: 0, // Will be set by emu
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
            }
        }
        Inst::Beq { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let taken = val1 == val2;

            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };
            if log_level != LogLevel::None {
                Some(InstLog::Branch {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rs1_val: val1,
                    rs2_val: val2,
                    taken,
                    target_pc,
                })
            } else {
                None
            }
        }
        Inst::Bne { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let taken = val1 != val2;

            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };
            if log_level != LogLevel::None {
                Some(InstLog::Branch {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rs1_val: val1,
                    rs2_val: val2,
                    taken,
                    target_pc,
                })
            } else {
                None
            }
        }
        Inst::Blt { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let taken = val1 < val2;

            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };
            if log_level != LogLevel::None {
                Some(InstLog::Branch {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rs1_val: val1,
                    rs2_val: val2,
                    taken,
                    target_pc,
                })
            } else {
                None
            }
        }
        Inst::Bge { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let taken = val1 >= val2;

            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };
            if log_level != LogLevel::None {
                Some(InstLog::Branch {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rs1_val: val1,
                    rs2_val: val2,
                    taken,
                    target_pc,
                })
            } else {
                None
            }
        }
        Inst::Bltu { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1) as u32;
            let val2 = read_reg(regs, rs2) as u32;
            let taken = val1 < val2;

            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };
            if log_level != LogLevel::None {
                Some(InstLog::Branch {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rs1_val: val1 as i32,
                    rs2_val: val2 as i32,
                    taken,
                    target_pc,
                })
            } else {
                None
            }
        }
        Inst::Bgeu { rs1, rs2, imm } => {
            let val1 = read_reg(regs, rs1) as u32;
            let val2 = read_reg(regs, rs2) as u32;
            let taken = val1 >= val2;

            let target_pc = if taken {
                let target = pc.wrapping_add(imm as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };
            if log_level != LogLevel::None {
                Some(InstLog::Branch {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rs1_val: val1 as i32,
                    rs2_val: val2 as i32,
                    taken,
                    target_pc,
                })
            } else {
                None
            }
        }
        Inst::Lui { rd, imm } => {
            // For LUI, the immediate is the upper 20 bits (bits [31:12] of the instruction)
            // TypeU extracts it as a signed i32, but we need to treat it as unsigned for shifting
            // to avoid overflow. The immediate is already in bits [31:12], so we extract the
            // upper 20 bits and shift left by 12.
            let imm_u32 = imm as u32;
            // Extract upper 20 bits: (imm_u32 >> 12) & 0xfffff, then shift left by 12
            let upper_20_bits = (imm_u32 >> 12) & 0xfffff;
            let value = (upper_20_bits << 12) as i32;
            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
                Some(InstLog::Immediate {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rd_old,
                    rd_new: value,
                })
            } else {
                None
            }
        }
        Inst::Auipc { rd, imm } => {
            // AUIPC: rd = pc + imm
            // The imm field is already the sign-extended and shifted immediate value
            let value = (pc.wrapping_add(imm as u32)) as i32;
            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
                Some(InstLog::Immediate {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    rd,
                    rd_old,
                    rd_new: value,
                })
            } else {
                None
            }
        }
        Inst::Slt { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = if val1 < val2 { 1 } else { 0 };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Slti { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = if val1 < imm { 1 } else { 0 };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Sltu { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1) as u32;
            let val2 = read_reg(regs, rs2) as u32;
            let rd_old = read_reg(regs, rd);
            let result = if val1 < val2 { 1 } else { 0 };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1 as i32,
                    rs2_val: Some(val2 as i32),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Sltiu { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1) as u32;
            let imm_u = imm as u32;
            let rd_old = read_reg(regs, rd);
            let result = if val1 < imm_u { 1 } else { 0 };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1 as i32,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Xori { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = val1 ^ imm;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::And { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1 & val2;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Andi { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = val1 & imm;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Or { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1 | val2;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Ori { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = val1 | imm;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Xor { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1 ^ val2;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Sll { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (val2 & 0x1f) as u32; // Only use bottom 5 bits
            let result = (val1 as u32).wrapping_shl(shift_amount) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Slli { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (imm & 0x1f) as u32; // Only use bottom 5 bits
            let result = (val1 as u32).wrapping_shl(shift_amount) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Srl { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (val2 & 0x1f) as u32; // Only use bottom 5 bits
            let result = ((val1 as u32).wrapping_shr(shift_amount)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Srli { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (imm & 0x1f) as u32; // Only use bottom 5 bits
            let result = ((val1 as u32).wrapping_shr(shift_amount)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Sra { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (val2 & 0x1f) as u32; // Only use bottom 5 bits
            let result = val1.wrapping_shr(shift_amount);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }
        Inst::Srai { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (imm & 0x1f) as u32; // Only use bottom 5 bits
            let result = val1.wrapping_shr(shift_amount);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Zbs: Single-bit instructions (immediate)
        // ====================================================================
        Inst::Bclri { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let bit_pos = (imm & 0x1f) as u32; // Only use bottom 5 bits
            let mask = !(1u32 << bit_pos);
            let result = ((val1 as u32) & mask) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Bseti { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let bit_pos = (imm & 0x1f) as u32; // Only use bottom 5 bits
            let mask = 1u32 << bit_pos;
            let result = ((val1 as u32) | mask) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Binvi { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let bit_pos = (imm & 0x1f) as u32; // Only use bottom 5 bits
            let mask = 1u32 << bit_pos;
            let result = ((val1 as u32) ^ mask) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Bexti { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let bit_pos = (imm & 0x1f) as u32; // Only use bottom 5 bits
            let result = (((val1 as u32) >> bit_pos) & 1) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Zbs: Single-bit instructions (register)
        // ====================================================================
        Inst::Bclr { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let bit_pos = (val2 as u32) & 0x1f; // Only use bottom 5 bits
            let mask = !(1u32 << bit_pos);
            let result = ((val1 as u32) & mask) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Bset { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let bit_pos = (val2 as u32) & 0x1f; // Only use bottom 5 bits
            let mask = 1u32 << bit_pos;
            let result = ((val1 as u32) | mask) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Binv { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let bit_pos = (val2 as u32) & 0x1f; // Only use bottom 5 bits
            let mask = 1u32 << bit_pos;
            let result = ((val1 as u32) ^ mask) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Bext { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let bit_pos = (val2 as u32) & 0x1f; // Only use bottom 5 bits
            let result = (((val1 as u32) >> bit_pos) & 1) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Zbb: Count operations
        // ====================================================================
        Inst::Clz { rd, rs1 } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let val_u = val1 as u32;
            let result = if val_u == 0 {
                32
            } else {
                val_u.leading_zeros() as i32
            };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Ctz { rd, rs1 } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let val_u = val1 as u32;
            let result = if val_u == 0 {
                32
            } else {
                val_u.trailing_zeros() as i32
            };
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Cpop { rd, rs1 } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let val_u = val1 as u32;
            let result = val_u.count_ones() as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Zbb: Sign/zero extend
        // ====================================================================
        Inst::Sextb { rd, rs1 } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = ((val1 as u8) as i8) as i32; // Sign-extend byte
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Sexth { rd, rs1 } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = ((val1 as u16) as i16) as i32; // Sign-extend halfword
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Zexth { rd, rs1 } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let result = ((val1 as u32) & 0xffff) as i32; // Zero-extend halfword
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Zbb: Rotate instructions
        // ====================================================================
        Inst::Rori { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (imm & 0x1f) as u32; // Only use bottom 5 bits
            let val_u = val1 as u32;
            let result = (val_u.rotate_right(shift_amount)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Rol { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (val2 as u32) & 0x1f; // Only use bottom 5 bits
            let val_u = val1 as u32;
            let result = (val_u.rotate_left(shift_amount)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Ror { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (val2 as u32) & 0x1f; // Only use bottom 5 bits
            let val_u = val1 as u32;
            let result = (val_u.rotate_right(shift_amount)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Zbb: Byte reverse
        // ====================================================================
        Inst::Rev8 { rd, rs1 } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let val_u = val1 as u32;
            // Reverse bytes: swap byte 0<->3, 1<->2
            let result = ((val_u << 24)
                | ((val_u & 0xff00) << 8)
                | ((val_u & 0xff0000) >> 8)
                | (val_u >> 24)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Brev8 { rd, rs1 } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let val_u = val1 as u32;
            // Bit-reverse within each byte
            let mut result = 0u32;
            for i in 0..4 {
                let byte = ((val_u >> (i * 8)) & 0xff) as u8;
                let reversed = byte.reverse_bits();
                result |= (reversed as u32) << (i * 8);
            }
            if rd.num() != 0 {
                regs[rd.num() as usize] = result as i32;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result as i32,
                })
            } else {
                None
            }
        }

        Inst::Orcb { rd, rs1 } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let val_u = val1 as u32;
            // OR-combine bytes: each byte becomes 0x00 if byte is 0, 0xFF otherwise
            let mut result = 0u32;
            for i in 0..4 {
                let byte = ((val_u >> (i * 8)) & 0xff) as u8;
                if byte != 0 {
                    result |= 0xffu32 << (i * 8);
                }
            }
            if rd.num() != 0 {
                regs[rd.num() as usize] = result as i32;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result as i32,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Zbb: Min/Max
        // ====================================================================
        Inst::Min { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1.min(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Minu { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let val1_u = val1 as u32;
            let val2_u = val2 as u32;
            let result = (val1_u.min(val2_u)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Max { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1.max(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Maxu { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let val1_u = val1 as u32;
            let val2_u = val2 as u32;
            let result = (val1_u.max(val2_u)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Zbb: Logical operations
        // ====================================================================
        Inst::Andn { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1 & !val2;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Orn { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = val1 | !val2;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Xnor { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = !(val1 ^ val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Zba: Address generation
        // ====================================================================
        Inst::Sh1add { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = (val1 << 1).wrapping_add(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Sh2add { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = (val1 << 2).wrapping_add(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Sh3add { rd, rs1, rs2 } => {
            let val1 = read_reg(regs, rs1);
            let val2 = read_reg(regs, rs2);
            let rd_old = read_reg(regs, rd);
            let result = (val1 << 3).wrapping_add(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::SlliUw { rd, rs1, imm } => {
            let val1 = read_reg(regs, rs1);
            let rd_old = read_reg(regs, rd);
            let shift_amount = (imm & 0x1f) as u32; // Only use bottom 5 bits
            // On RV32, this is just a left shift (zero-extend is a no-op)
            let result = ((val1 as u32).wrapping_shl(shift_amount)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::Ecall => {
            syscall = true;
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ecall,
                })
            } else {
                None
            }
        }
        Inst::Ebreak => {
            should_halt = true;
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0, // Will be set by emu
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak,
                })
            } else {
                None
            }
        }
        Inst::Fence => {
            // FENCE: Memory ordering (no-op in single-threaded emulator)
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak, // Use existing kind (doesn't matter for logging)
                })
            } else {
                None
            }
        }
        Inst::FenceI => {
            // FENCE.I: Instruction cache synchronization (no-op in emulator)
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak, // Use existing kind (doesn't matter for logging)
                })
            } else {
                None
            }
        }
        Inst::Csrrw { rd, rs1: _, csr: _ } => {
            // CSRRW: rd = CSR; CSR = rs1
            // In emulator, CSR operations are no-ops (we don't track CSR state)
            // Just write 0 to rd (or preserve if rd is x0)
            let result = 0i32; // CSR reads return 0 (no CSR state tracked)
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak, // Use existing kind (doesn't matter for logging)
                })
            } else {
                None
            }
        }
        Inst::Csrrs { rd, rs1: _, csr: _ } => {
            // CSRRS: rd = CSR; CSR = CSR | rs1
            // In emulator, CSR operations are no-ops
            let result = 0i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak,
                })
            } else {
                None
            }
        }
        Inst::Csrrc { rd, rs1: _, csr: _ } => {
            // CSRRC: rd = CSR; CSR = CSR & ~rs1
            // In emulator, CSR operations are no-ops
            let result = 0i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak,
                })
            } else {
                None
            }
        }
        Inst::Csrrwi { rd, imm: _, csr: _ } => {
            // CSRRWI: rd = CSR; CSR = imm
            // In emulator, CSR operations are no-ops
            let result = 0i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak,
                })
            } else {
                None
            }
        }
        Inst::Csrrsi { rd, imm: _, csr: _ } => {
            // CSRRSI: rd = CSR; CSR = CSR | imm
            // In emulator, CSR operations are no-ops
            let result = 0i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak,
                })
            } else {
                None
            }
        }

        Inst::Csrrci { rd, imm: _, csr: _ } => {
            // CSRRCI: rd = CSR; CSR = CSR & ~imm
            // In emulator, CSR operations are no-ops
            let result = 0i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Compressed instructions
        // ====================================================================
        Inst::CAddi { rd, imm } => {
            // c.addi: rd = rd + imm
            let val1 = read_reg(regs, rd);
            let rd_old = val1;
            let result = val1.wrapping_add(imm);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CLi { rd, imm } => {
            // c.li: rd = imm (expands to addi rd, x0, imm)
            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = imm;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: 0,
                    rs2_val: None,
                    rd_old,
                    rd_new: imm,
                })
            } else {
                None
            }
        }

        Inst::CLui { rd, imm } => {
            // c.lui: rd = imm (imm is already shifted)
            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = imm;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Immediate {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rd_old,
                    rd_new: imm,
                })
            } else {
                None
            }
        }

        Inst::CMv { rd, rs } => {
            // c.mv: rd = rs (expands to add rd, x0, rs)
            let val = read_reg(regs, rs);
            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = val;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: 0,
                    rs2_val: Some(val),
                    rd_old,
                    rd_new: val,
                })
            } else {
                None
            }
        }

        Inst::CAdd { rd, rs } => {
            // c.add: rd = rd + rs
            let val1 = read_reg(regs, rd);
            let val2 = read_reg(regs, rs);
            let rd_old = val1;
            let result = val1.wrapping_add(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CSub { rd, rs } => {
            // c.sub: rd = rd - rs
            let val1 = read_reg(regs, rd);
            let val2 = read_reg(regs, rs);
            let rd_old = val1;
            let result = val1.wrapping_sub(val2);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CAnd { rd, rs } => {
            // c.and: rd = rd & rs
            let val1 = read_reg(regs, rd);
            let val2 = read_reg(regs, rs);
            let rd_old = val1;
            let result = val1 & val2;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::COr { rd, rs } => {
            // c.or: rd = rd | rs
            let val1 = read_reg(regs, rd);
            let val2 = read_reg(regs, rs);
            let rd_old = val1;
            let result = val1 | val2;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CXor { rd, rs } => {
            // c.xor: rd = rd ^ rs
            let val1 = read_reg(regs, rd);
            let val2 = read_reg(regs, rs);
            let rd_old = val1;
            let result = val1 ^ val2;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: Some(val2),
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CLw { rd, rs, offset } => {
            // c.lw: rd = mem[rs + offset]
            let base = read_reg(regs, rs);
            let address = base.wrapping_add(offset) as u32;

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

            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
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
            }
        }

        Inst::CSw { rs1, rs2, offset } => {
            // c.sw: mem[rs1 + offset] = rs2
            let base = read_reg(regs, rs1);
            let value = read_reg(regs, rs2);
            let address = base.wrapping_add(offset) as u32;

            let old_value = memory.read_word(address).unwrap_or(0);

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

            if log_level != LogLevel::None {
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
            }
        }

        Inst::CJ { offset } => {
            // c.j: pc = pc + offset
            let target = pc.wrapping_add(offset as u32);
            new_pc = Some(target);

            if log_level != LogLevel::None {
                Some(InstLog::Jump {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd_old: 0,
                    rd_new: None,
                    target_pc: target,
                })
            } else {
                None
            }
        }

        Inst::CJr { rs } => {
            // c.jr: pc = rs
            let base = read_reg(regs, rs);
            let target = (base as u32) & !1; // Clear bottom bit for alignment
            new_pc = Some(target);

            if log_level != LogLevel::None {
                Some(InstLog::Jump {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd_old: 0,
                    rd_new: None,
                    target_pc: target,
                })
            } else {
                None
            }
        }

        Inst::CJalr { rs } => {
            // c.jalr: ra = pc + 2; pc = rs
            let base = read_reg(regs, rs);
            let next_pc = pc.wrapping_add(2);
            let target = (base as u32) & !1; // Clear bottom bit for alignment
            regs[Gpr::Ra.num() as usize] = next_pc as i32;
            new_pc = Some(target);

            if log_level != LogLevel::None {
                Some(InstLog::Jump {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd_old: read_reg(regs, Gpr::Ra),
                    rd_new: Some(next_pc as i32),
                    target_pc: target,
                })
            } else {
                None
            }
        }

        Inst::CBeqz { rs, offset } => {
            // c.beqz: if rs == 0, pc = pc + offset
            let val = read_reg(regs, rs);
            let taken = val == 0;

            let target_pc = if taken {
                let target = pc.wrapping_add(offset as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };
            if log_level != LogLevel::None {
                Some(InstLog::Branch {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rs1_val: val,
                    rs2_val: 0,
                    taken,
                    target_pc,
                })
            } else {
                None
            }
        }

        Inst::CBnez { rs, offset } => {
            // c.bnez: if rs != 0, pc = pc + offset
            let val = read_reg(regs, rs);
            let taken = val != 0;

            let target_pc = if taken {
                let target = pc.wrapping_add(offset as u32);
                new_pc = Some(target);
                Some(target)
            } else {
                None
            };
            if log_level != LogLevel::None {
                Some(InstLog::Branch {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rs1_val: val,
                    rs2_val: 0,
                    taken,
                    target_pc,
                })
            } else {
                None
            }
        }

        Inst::CSlli { rd, imm } => {
            // c.slli: rd = rd << imm
            let val1 = read_reg(regs, rd);
            let rd_old = val1;
            let shift_amount = (imm & 0x1f) as u32;
            let result = (val1 as u32).wrapping_shl(shift_amount) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CSrli { rd, imm } => {
            // c.srli: rd = rd >> imm (logical)
            let val1 = read_reg(regs, rd);
            let rd_old = val1;
            let shift_amount = (imm & 0x1f) as u32;
            let result = ((val1 as u32).wrapping_shr(shift_amount)) as i32;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CSrai { rd, imm } => {
            // c.srai: rd = rd >> imm (arithmetic)
            let val1 = read_reg(regs, rd);
            let rd_old = val1;
            let shift_amount = (imm & 0x1f) as u32;
            let result = val1.wrapping_shr(shift_amount);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CAndi { rd, imm } => {
            // c.andi: rd = rd & imm
            let val1 = read_reg(regs, rd);
            let rd_old = val1;
            let result = val1 & imm;
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CAddi16sp { imm } => {
            // c.addi16sp: sp = sp + imm
            let val1 = read_reg(regs, Gpr::Sp);
            let result = val1.wrapping_add(imm);
            regs[Gpr::Sp.num() as usize] = result;
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd: Gpr::Sp,
                    rs1_val: val1,
                    rs2_val: None,
                    rd_old: val1,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CAddi4spn { rd, imm } => {
            // c.addi4spn: rd = sp + imm
            let sp_val = read_reg(regs, Gpr::Sp);
            let rd_old = read_reg(regs, rd);
            let result = sp_val.wrapping_add(imm);
            if rd.num() != 0 {
                regs[rd.num() as usize] = result;
            }
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: sp_val,
                    rs2_val: None,
                    rd_old,
                    rd_new: result,
                })
            } else {
                None
            }
        }

        Inst::CLwsp { rd, offset } => {
            // c.lwsp: rd = mem[sp + offset]
            let sp_val = read_reg(regs, Gpr::Sp);
            let address = sp_val.wrapping_add(offset) as u32;

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

            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
                Some(InstLog::Load {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd,
                    rs1_val: sp_val,
                    addr: address,
                    mem_val: value,
                    rd_old,
                    rd_new: value,
                })
            } else {
                None
            }
        }

        Inst::CSwsp { rs, offset } => {
            // c.swsp: mem[sp + offset] = rs
            let sp_val = read_reg(regs, Gpr::Sp);
            let value = read_reg(regs, rs);
            let address = sp_val.wrapping_add(offset) as u32;

            let old_value = memory.read_word(address).unwrap_or(0);

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

            if log_level != LogLevel::None {
                Some(InstLog::Store {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rs1_val: sp_val,
                    rs2_val: value,
                    addr: address,
                    mem_old: old_value,
                    mem_new: value,
                })
            } else {
                None
            }
        }

        Inst::CJal { offset } => {
            // c.jal: ra = pc + 2; pc = pc + offset (RV32 only)
            let next_pc = pc.wrapping_add(2);
            let target = pc.wrapping_add(offset as u32);
            regs[Gpr::Ra.num() as usize] = next_pc as i32;
            new_pc = Some(target);

            if log_level != LogLevel::None {
                Some(InstLog::Jump {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd_old: read_reg(regs, Gpr::Ra),
                    rd_new: Some(next_pc as i32),
                    target_pc: target,
                })
            } else {
                None
            }
        }

        Inst::CNop => {
            // c.nop: no operation
            if log_level != LogLevel::None {
                Some(InstLog::Arithmetic {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rd: Gpr::Zero,
                    rs1_val: 0,
                    rs2_val: None,
                    rd_old: 0,
                    rd_new: 0,
                })
            } else {
                None
            }
        }

        Inst::CEbreak => {
            // c.ebreak: same as ebreak
            should_halt = true;
            if log_level != LogLevel::None {
                Some(InstLog::System {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    kind: SystemKind::Ebreak,
                })
            } else {
                None
            }
        }

        // ====================================================================
        // Atomic instructions (A extension)
        // For single-threaded emulator, these are just read-modify-write
        // ====================================================================
        Inst::LrW { rd, rs1 } => {
            // lr.w: Load reserved word (just a regular load in single-threaded)
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

            let rd_old = read_reg(regs, rd);
            if rd.num() != 0 {
                regs[rd.num() as usize] = value;
            }

            if log_level != LogLevel::None {
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
            }
        }

        Inst::ScW { rd, rs1, rs2 } => {
            // sc.w: Store conditional word (always succeeds in single-threaded)
            let base = read_reg(regs, rs1);
            let value = read_reg(regs, rs2);
            let address = base as u32;

            let old_value = memory.read_word(address).unwrap_or(0);

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

            // Return 0 in rd to indicate success
            if rd.num() != 0 {
                regs[rd.num() as usize] = 0;
            }

            if log_level != LogLevel::None {
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
            }
        }

        Inst::AmoswapW { rd, rs1, rs2 } => {
            // amoswap.w: Atomically swap word
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

            if log_level != LogLevel::None {
                Some(InstLog::Store {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rs1_val: base,
                    rs2_val: new_value,
                    addr: address,
                    mem_old: old_value,
                    mem_new: new_value,
                })
            } else {
                None
            }
        }

        Inst::AmoaddW { rd, rs1, rs2 } => {
            // amoadd.w: Atomically add word
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

            if log_level != LogLevel::None {
                Some(InstLog::Store {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rs1_val: base,
                    rs2_val: addend,
                    addr: address,
                    mem_old: old_value,
                    mem_new: new_value,
                })
            } else {
                None
            }
        }

        Inst::AmoxorW { rd, rs1, rs2 } => {
            // amoxor.w: Atomically XOR word
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

            if log_level != LogLevel::None {
                Some(InstLog::Store {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rs1_val: base,
                    rs2_val: xor_val,
                    addr: address,
                    mem_old: old_value,
                    mem_new: new_value,
                })
            } else {
                None
            }
        }

        Inst::AmoandW { rd, rs1, rs2 } => {
            // amoand.w: Atomically AND word
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

            if log_level != LogLevel::None {
                Some(InstLog::Store {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rs1_val: base,
                    rs2_val: and_val,
                    addr: address,
                    mem_old: old_value,
                    mem_new: new_value,
                })
            } else {
                None
            }
        }

        Inst::AmoorW { rd, rs1, rs2 } => {
            // amoor.w: Atomically OR word
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

            if log_level != LogLevel::None {
                Some(InstLog::Store {
                    cycle: 0,
                    pc,
                    instruction: instruction_word,
                    rs1_val: base,
                    rs2_val: or_val,
                    addr: address,
                    mem_old: old_value,
                    mem_new: new_value,
                })
            } else {
                None
            }
        }
    };

    Ok(ExecutionResult {
        new_pc,
        should_halt,
        syscall,
        log,
    })
}
