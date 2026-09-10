//! Immediate instruction execution (I-type: ADDI, SLLI, SRLI, SRAI, ANDI, ORI,
//! XORI, SLTI, SLTIU).
//!
//! # The Zb* encodings in OP-IMM
//!
//! This executor also carries a handful of bit-manipulation instructions. No
//! guest this repository builds contains one: every target here is
//! `riscv32imac`, the ESP32-C6 has no B extension and traps all of them, and no
//! backend emits one. So none of it was ever exercised, and it was transcribed
//! rather than derived — and a transcription of a dense encoding space is a bug
//! report until checked, the lesson of
//! `docs/defects/2026-07-31-zexth-encoding-steals-xori-128.md`.
//!
//! Checked against an assembler (`rustc --target riscv32imac-unknown-none-elf`
//! with `.option arch, +zba,+zbb,+zbs,+zbkb`), a bit over half of what was here
//! did not survive; see
//! `docs/defects/2026-09-09-op-imm-bitmanip-dispatch-dead-and-mislabelled.md`.
//! What remains — `binvi`, `orc.b`, `clz`, `ctz`, `cpop`, `sext.b`, `sext.h` —
//! is reachable and agrees with the assembler, and
//! [`tests::zb_immediates_are_shifts_or_faults`] pins the whole surface,
//! including the encodings that fall through to a shift and the ones that
//! fault.
//!
//! Whether an RV32IMAC emulator should execute *any* of the survivors, rather
//! than trapping them as the chip would, is the open question raised by
//! `docs/reports/2026-04-27-rv32-load16-issue.md`. This module does not answer
//! it; it only stops claiming instructions it does not implement.

extern crate alloc;

use super::{ExecutionResult, InstClass, LoggingMode, read_reg};
use crate::emu::{error::EmulatorError, logging::InstLog};
use lp_emu_core::Bus;
use lp_riscv_inst::{Gpr, format::TypeI};

/// Decode and execute I-type immediate instructions.
#[inline(always)]
pub(crate) fn decode_execute_itype<M: LoggingMode, B: Bus>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut B,
) -> Result<ExecutionResult, EmulatorError> {
    let i = TypeI::from_riscv(inst_word);
    let rd = Gpr::new(i.rd);
    let rs1 = Gpr::new(i.rs1);
    let funct3 = i.func;
    let imm = i.imm;

    // SRAI is distinguished from SRLI by bit 5 of funct7 (instruction bit 30):
    // SRAI has funct7 0b0100000, SRLI has 0b0000000. Every other funct7 in this
    // funct3 belongs to an extension the C6 does not have, and this arm swallows
    // all of them — see the module comment on the Zb* encodings.
    let funct7_bit5 = ((inst_word >> 25) & 0x20) != 0;

    match funct3 {
        0x0 => execute_addi::<M>(rd, rs1, imm, inst_word, pc, regs),
        0x1 => {
            // SLLI and other funct3=0x1 instructions
            let funct6 = ((inst_word >> 26) & 0x3f) as u8;
            let imm_5_0 = ((inst_word >> 20) & 0x3f) as u8;
            let funct12 = ((inst_word >> 20) & 0xfff) as u16;

            match funct6 {
                0x00 => execute_slli::<M>(rd, rs1, imm, inst_word, pc, regs),
                // Zbs `binvi`, funct6 0b011010. The only member of the
                // bit-immediate trio whose constant here matches the spec.
                0x1a => execute_binvi::<M>(rd, rs1, imm_5_0 as i32, inst_word, pc, regs),
                _ => {
                    // Check for funct12 encodings (CLZ, CTZ, CPOP, SEXTB, SEXTH)
                    match funct12 {
                        0x600 => execute_clz::<M>(rd, rs1, inst_word, pc, regs),
                        0x601 => execute_ctz::<M>(rd, rs1, inst_word, pc, regs),
                        0x602 => execute_cpop::<M>(rd, rs1, inst_word, pc, regs),
                        0x604 => execute_sextb::<M>(rd, rs1, inst_word, pc, regs),
                        0x605 => execute_sexth::<M>(rd, rs1, inst_word, pc, regs),
                        _ => Err(EmulatorError::InvalidInstruction {
                            pc,
                            instruction: inst_word,
                            reason: alloc::format!(
                                "Unknown I-type instruction: funct3=0x{funct3:x}, funct6=0x{funct6:x}, funct12=0x{funct12:x}"
                            ),
                            regs: *regs,
                        }),
                    }
                }
            }
        }
        0x2 => execute_slti::<M>(rd, rs1, imm, inst_word, pc, regs),
        0x3 => execute_sltiu::<M>(rd, rs1, imm, inst_word, pc, regs),
        // XORI, for every immediate. `zext.h` lives in the OP space
        // (`pack rd, rs, x0`); carving funct12 0x080 out of OP-IMM here
        // silently reinterpreted `xori rd, rs1, 128` — LLVM's jump-table
        // index bias — as a halfword zero-extend, which is a no-op for small
        // values and so failed without a trace. See
        // `docs/defects/2026-07-31-zexth-encoding-steals-xori-128.md`.
        0x4 => execute_xori::<M>(rd, rs1, imm, inst_word, pc, regs),
        0x5 => {
            let funct12 = ((inst_word >> 20) & 0xfff) as u16;

            if funct7_bit5 {
                // SRAI, and — because this test comes first and only looks at
                // one bit — every funct7 with bit 5 set. `rori`, `rev8`,
                // `brev8` and `bexti` all live up here, so the arms that used
                // to name them below could never run; they are gone. Their
                // encodings still execute as shifts, which is what they have
                // always done. See `tests::zb_immediates_are_shifts_or_faults`.
                execute_srai::<M>(rd, rs1, imm, inst_word, pc, regs)
            } else {
                // Zbb `orc.b`, funct12 0b001010000111 — the one Zb* encoding in
                // this funct3 whose funct7 bit 5 is clear, and so the one that
                // has ever reached its own arm.
                match funct12 {
                    0x287 => execute_orcb::<M>(rd, rs1, inst_word, pc, regs),
                    _ => execute_srli::<M>(rd, rs1, imm, inst_word, pc, regs),
                }
            }
        }
        0x6 => execute_ori::<M>(rd, rs1, imm, inst_word, pc, regs),
        0x7 => execute_andi::<M>(rd, rs1, imm, inst_word, pc, regs),
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unknown I-type instruction: funct3=0x{funct3:x}"),
            regs: *regs,
        }),
    }
}

#[inline(always)]
fn execute_addi<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1.wrapping_add(imm);
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_slli<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let shift_amount = (imm & 0x1f) as u32; // Only use bottom 5 bits
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_srli<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let shift_amount = (imm & 0x1f) as u32; // Only use bottom 5 bits
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_srai<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    // For SRAI on RV32, shift amount is only in imm[4:0] (bottom 5 bits)
    // imm[11:5] = 0x20 distinguishes SRAI from SRLI, but is not part of shift amount
    // Use imm from TypeI and mask to get only the shift amount (same as old executor)
    let shift_amount = (imm & 0x1f) as u32;
    // Use wrapping_shr for arithmetic right shift (sign-extending) - matches old executor
    // wrapping_shr on i32 performs arithmetic shift (sign-extending)
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_andi<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1 & imm;
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_ori<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1 | imm;
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_xori<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = val1 ^ imm;
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_slti<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = if val1 < imm { 1 } else { 0 };
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_sltiu<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1) as u32;
    let imm_u = imm as u32;
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = if val1 < imm_u { 1 } else { 0 };
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

// The bit-manipulation instructions that are both reachable and correct.
// See the module comment for the ones that were neither.

#[inline(always)]
fn execute_binvi<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    imm: i32,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let bit_pos = (imm & 0x1f) as u32; // Only use bottom 5 bits
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_orcb<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
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
    let log = if M::ENABLED {
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
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_clz<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let val_u = val1 as u32;
    let result = if val_u == 0 {
        32
    } else {
        val_u.leading_zeros() as i32
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_ctz<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let val_u = val1 as u32;
    let result = if val_u == 0 {
        32
    } else {
        val_u.trailing_zeros() as i32
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_cpop<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let val_u = val1 as u32;
    let result = val_u.count_ones() as i32;
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_sextb<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = ((val1 as u8) as i8) as i32; // Sign-extend byte
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[inline(always)]
fn execute_sexth<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let rd_old = if M::ENABLED { read_reg(regs, rd) } else { 0 };
    let result = ((val1 as u16) as i16) as i32; // Sign-extend halfword
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
            rs2_val: None,
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
        class: InstClass::Alu,
        inst_size: 4,
        log,
    })
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    #[cfg(test)]
    extern crate std;
    use alloc::vec;

    use super::*;
    use crate::emu::executor::{LoggingDisabled, LoggingEnabled};
    use lp_emu_core::Memory;
    use lp_riscv_inst::{Gpr, encode};

    #[test]
    fn test_addi_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 10;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test ADDI instruction: addi x3, x1, 5
        let inst_word = encode::addi(Gpr::new(3), Gpr::new(1), 5);
        let result =
            decode_execute_itype::<LoggingDisabled, _>(inst_word, 0, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[3], 15);
        assert!(result.log.is_none());
    }

    #[test]
    fn test_addi_logging_path() {
        let mut regs = [0i32; 32];
        regs[1] = 10;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        let inst_word = encode::addi(Gpr::new(3), Gpr::new(1), 5);
        let result =
            decode_execute_itype::<LoggingEnabled, _>(inst_word, 0, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[3], 15);
        assert!(result.log.is_some());
        if let Some(InstLog::Arithmetic { rd_new, .. }) = result.log {
            assert_eq!(rd_new, 15);
        }
    }

    #[test]
    fn test_slli_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 5;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test SLLI instruction: slli x3, x1, 2
        let inst_word = encode::slli(Gpr::new(3), Gpr::new(1), 2);
        let result =
            decode_execute_itype::<LoggingDisabled, _>(inst_word, 0, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[3], 20); // 5 << 2 = 20
        assert!(result.log.is_none());
    }

    #[test]
    fn test_srli_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 20;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test SRLI instruction: srli x3, x1, 2
        let inst_word = encode::srli(Gpr::new(3), Gpr::new(1), 2);
        let result =
            decode_execute_itype::<LoggingDisabled, _>(inst_word, 0, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[3], 5); // 20 >> 2 = 5
        assert!(result.log.is_none());
    }

    #[test]
    fn test_andi_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 0b1111;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test ANDI instruction: andi x3, x1, 0b1010
        let inst_word = encode::andi(Gpr::new(3), Gpr::new(1), 0b1010);
        let result =
            decode_execute_itype::<LoggingDisabled, _>(inst_word, 0, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[3], 0b1010); // 0b1111 & 0b1010 = 0b1010
        assert!(result.log.is_none());
    }

    #[test]
    fn test_srai_arithmetic_shift_sign_extension() {
        let mut regs = [0i32; 32];
        // Test case from failing GLSL test: -111412 >> 16 should be -2
        regs[27] = -111412; // s11 register
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test SRAI instruction: srai a0, s11, 16
        // Expected: -111412 >> 16 = -2 (arithmetic shift with sign extension)
        let inst_word = encode::srai(Gpr::new(10), Gpr::new(27), 16);
        let result =
            decode_execute_itype::<LoggingDisabled, _>(inst_word, 0, &mut regs, &mut memory)
                .unwrap();

        // Should be -2, not 65534 (which would be logical shift)
        assert_eq!(
            regs[10], -2,
            "SRAI should sign-extend: -111412 >> 16 = -2, got {} (0x{:08x})",
            regs[10], regs[10] as u32
        );
        assert!(result.log.is_none());
    }

    #[test]
    fn test_srai_negative_value_small_shift() {
        let mut regs = [0i32; 32];
        regs[1] = -1;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test SRAI: -1 >> 1 should be -1 (sign extension)
        let inst_word = encode::srai(Gpr::new(3), Gpr::new(1), 1);
        let result =
            decode_execute_itype::<LoggingDisabled, _>(inst_word, 0, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(regs[3], -1, "SRAI should sign-extend: -1 >> 1 = -1");
        assert!(result.log.is_none());
    }

    #[test]
    fn test_srai_positive_value() {
        let mut regs = [0i32; 32];
        regs[1] = 0x7FFFFFFF; // Large positive value (max i32)
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        // Test SRAI: 0x7FFFFFFF >> 16 = 0x00007FFF (arithmetic shift for positive)
        let inst_word = encode::srai(Gpr::new(3), Gpr::new(1), 16);
        let result =
            decode_execute_itype::<LoggingDisabled, _>(inst_word, 0, &mut regs, &mut memory)
                .unwrap();

        assert_eq!(
            regs[3], 0x7FFF,
            "SRAI on positive: 0x7FFFFFFF >> 16 = 0x7FFF"
        );
        assert!(result.log.is_none());
    }
}
