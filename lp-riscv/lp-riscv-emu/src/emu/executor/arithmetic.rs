//! Arithmetic instruction execution (R-type: ADD, SUB, MUL, etc.)

extern crate alloc;

use super::{ExecutionResult, LoggingMode, read_reg};
use crate::emu::{error::EmulatorError, logging::InstLog, memory::Memory};
use lp_riscv_inst::{Gpr, format::TypeR};

/// Decode and execute R-type arithmetic instructions.
pub(super) fn decode_execute_rtype<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let r = TypeR::from_riscv(inst_word);
    let rd = Gpr::new(r.rd);
    let rs1 = Gpr::new(r.rs1);
    let rs2 = Gpr::new(r.rs2);
    let funct3 = (r.func & 0x7) as u8;
    let funct7 = ((r.func >> 3) & 0x7f) as u8;

    match (funct3, funct7) {
        // Base arithmetic
        (0x0, 0x0) => execute_add::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x0, 0x20) => execute_sub::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // M extension (multiply/divide)
        (0x0, 0x01) => execute_mul::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x1, 0x01) => execute_mulh::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x2, 0x01) => execute_mulhsu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x3, 0x01) => execute_mulhu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x4, 0x01) => execute_div::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x5, 0x01) => execute_divu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x6, 0x01) => execute_rem::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x7, 0x01) => execute_remu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // Comparison
        (0x2, 0x0) => execute_slt::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x3, 0x0) => execute_sltu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // Logical
        (0x4, 0x0) => execute_xor::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x6, 0x0) => execute_or::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x7, 0x0) => execute_and::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // Shift
        (0x1, 0x0) => execute_sll::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x5, 0x0) => execute_srl::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x5, 0x20) => execute_sra::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // Zbb: Rotate instructions
        (0x1, 0x30) => execute_rol::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x5, 0x30) => execute_ror::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // Zbb: Logical operations
        (0x7, 0x20) => execute_andn::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x6, 0x20) => execute_orn::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x4, 0x20) => execute_xnor::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // Zbb: Min/Max instructions
        (0x4, 0x05) => execute_min::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x5, 0x05) => execute_minu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x6, 0x05) => execute_max::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x7, 0x05) => execute_maxu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // Zbs: Bit manipulation instructions
        (0x1, 0x24) => execute_bclr::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x5, 0x24) => execute_bext::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x1, 0x34) => execute_binv::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x1, 0x14) => execute_bset::<M>(rd, rs1, rs2, inst_word, pc, regs),
        // Zba: Address generation instructions
        (0x2, 0x10) => execute_sh1add::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x4, 0x10) => execute_sh2add::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x6, 0x10) => execute_sh3add::<M>(rd, rs1, rs2, inst_word, pc, regs),
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!(
                "Unknown R-type instruction: funct3=0x{:x}, funct7=0x{:x}",
                funct3,
                funct7
            ),
            regs: *regs,
        }),
    }
}

#[inline(always)]
fn execute_add<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1.wrapping_add(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_sub<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1.wrapping_sub(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_mul<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1.wrapping_mul(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_mulh<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    // MULH: high 32 bits of signed multiply
    let val1_i64 = val1 as i64;
    let val2_i64 = val2 as i64;
    let product = val1_i64.wrapping_mul(val2_i64);
    let result = (product >> 32) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_mulhsu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    // MULHSU: high 32 bits of signed * unsigned multiply
    let val1_i64 = val1 as i64;
    let val2_u64 = (val2 as u32) as u64;
    let product = ((val1_i64 as i128).wrapping_mul(val2_u64 as i128)) as i64;
    let result = (product >> 32) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_mulhu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    // MULHU: high 32 bits of unsigned multiply
    let val1_u64 = (val1 as u32) as u64;
    let val2_u64 = (val2 as u32) as u64;
    let product = val1_u64.wrapping_mul(val2_u64);
    let result = (product >> 32) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_div<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
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
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_divu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
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
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_rem<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
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
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_remu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
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
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_slt<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = if val1 < val2 { 1 } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_sltu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1) as u32;
    let val2 = read_reg(regs, rs2) as u32;
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = if val1 < val2 { 1 } else { 0 };
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_xor<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1 ^ val2;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_or<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1 | val2;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_and<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1 & val2;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_sll<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let shift_amount = (val2 & 0x1f) as u32; // Only use bottom 5 bits
    let result = (val1 as u32).wrapping_shl(shift_amount) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_srl<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let shift_amount = (val2 & 0x1f) as u32; // Only use bottom 5 bits
    let result = ((val1 as u32).wrapping_shr(shift_amount)) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_sra<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let shift_amount = (val2 & 0x1f) as u32; // Only use bottom 5 bits
    let result = val1.wrapping_shr(shift_amount);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_rol<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let shift_amount = (val2 as u32) & 0x1f; // Only use bottom 5 bits
    let val_u = val1 as u32;
    let result = (val_u.rotate_left(shift_amount)) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_ror<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let shift_amount = (val2 as u32) & 0x1f; // Only use bottom 5 bits
    let val_u = val1 as u32;
    let result = (val_u.rotate_right(shift_amount)) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_andn<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1 & !val2;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_orn<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1 | !val2;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_xnor<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = !(val1 ^ val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_min<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1.min(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_minu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let val1_u = val1 as u32;
    let val2_u = val2 as u32;
    let result = (val1_u.min(val2_u)) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_max<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1.max(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_maxu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let val1_u = val1 as u32;
    let val2_u = val2 as u32;
    let result = (val1_u.max(val2_u)) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_bclr<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let bit_pos = (val2 & 0x1f) as u32; // Only use bottom 5 bits
    let mask = !(1u32 << bit_pos);
    let result = ((val1 as u32) & mask) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_bext<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let bit_pos = (val2 & 0x1f) as u32; // Only use bottom 5 bits
    let result = (((val1 as u32) >> bit_pos) & 1) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_binv<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let bit_pos = (val2 & 0x1f) as u32; // Only use bottom 5 bits
    let mask = 1u32 << bit_pos;
    let result = ((val1 as u32) ^ mask) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_bset<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let bit_pos = (val2 & 0x1f) as u32; // Only use bottom 5 bits
    let mask = 1u32 << bit_pos;
    let result = ((val1 as u32) | mask) as i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_sh1add<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = (val1 << 1).wrapping_add(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_sh2add<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = (val1 << 2).wrapping_add(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_sh3add<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = (val1 << 3).wrapping_add(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
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

    #[test]
    fn test_add_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 10;
        regs[2] = 20;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test ADD instruction: add x3, x1, x2
        // Encoding: 0000000 00010 00001 000 00011 0110011
        //            funct7  rs2   rs1   f3  rd    opcode
        //            0x00    0x02  0x01  0x0 0x03  0x33
        let inst_word = 0x002081b3; // ADD x3, x1, x2
        let result =
            decode_execute_rtype::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(regs[3], 30);
        assert!(result.log.is_none());
    }

    #[test]
    fn test_add_logging_path() {
        let mut regs = [0i32; 32];
        regs[1] = 10;
        regs[2] = 20;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test ADD instruction: add x3, x1, x2
        let inst_word = 0x002081b3; // ADD x3, x1, x2
        let result =
            decode_execute_rtype::<LoggingEnabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(regs[3], 30);
        assert!(result.log.is_some());
        if let Some(InstLog::Arithmetic { rd_new, .. }) = result.log {
            assert_eq!(rd_new, 30);
        }
    }

    #[test]
    fn test_sub_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 30;
        regs[2] = 10;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test SUB instruction: sub x3, x1, x2
        // Encoding: 0100000 00010 00001 000 00011 0110011
        let inst_word = 0x402081b3; // SUB x3, x1, x2
        let result =
            decode_execute_rtype::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(regs[3], 20);
        assert!(result.log.is_none());
    }

    #[test]
    fn test_mul_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 6;
        regs[2] = 7;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test MUL instruction: mul x3, x1, x2
        // Encoding: 0000001 00010 00001 000 00011 0110011
        let inst_word = 0x022081b3; // MUL x3, x1, x2
        let result =
            decode_execute_rtype::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert_eq!(regs[3], 42);
        assert!(result.log.is_none());
    }
}
