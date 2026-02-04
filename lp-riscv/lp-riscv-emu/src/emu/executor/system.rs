//! System instruction execution (ECALL, EBREAK, CSR instructions)

extern crate alloc;

use super::{ExecutionResult, LoggingMode};
use crate::emu::{
    error::EmulatorError,
    logging::{InstLog, SystemKind},
    memory::Memory,
};
use lp_riscv_inst::{Gpr, format::TypeI};

/// Decode and execute system instructions (I-type, opcode 0x73).
pub(super) fn decode_execute_system<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let i = TypeI::from_riscv(inst_word);
    let funct3 = i.func;
    let imm = i.imm;

    // ECALL: funct3=0x0, imm[11:0]=0x000
    // EBREAK: funct3=0x0, imm[11:0]=0x001
    if funct3 == 0x0 {
        let funct12 = (imm & 0xfff) as u16;
        match funct12 {
            0x000 => execute_ecall::<M>(inst_word, pc),
            0x001 => execute_ebreak::<M>(inst_word, pc),
            _ => Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::format!(
                    "Unknown system instruction: funct3=0x{:x}, funct12=0x{:x}",
                    funct3,
                    funct12
                ),
                regs: *regs,
            }),
        }
    } else {
        // CSR instructions
        let rd = Gpr::new(i.rd);
        let csr = (imm & 0xfff) as u16;
        match funct3 {
            0b001 => {
                let rs1 = Gpr::new(i.rs1);
                execute_csrrw::<M>(rd, rs1, csr, inst_word, pc, regs)
            }
            0b010 => {
                let rs1 = Gpr::new(i.rs1);
                execute_csrrs::<M>(rd, rs1, csr, inst_word, pc, regs)
            }
            0b011 => {
                let rs1 = Gpr::new(i.rs1);
                execute_csrrc::<M>(rd, rs1, csr, inst_word, pc, regs)
            }
            0b101 => {
                // CSRRWI: imm is in rs1 field (bits [19:15])
                let imm_val = i.rs1 as i32;
                execute_csrrwi::<M>(rd, imm_val, csr, inst_word, pc, regs)
            }
            0b110 => {
                // CSRRSI: imm is in rs1 field
                let imm_val = i.rs1 as i32;
                execute_csrrsi::<M>(rd, imm_val, csr, inst_word, pc, regs)
            }
            0b111 => {
                // CSRRCI: imm is in rs1 field
                let imm_val = i.rs1 as i32;
                execute_csrrci::<M>(rd, imm_val, csr, inst_word, pc, regs)
            }
            _ => Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::format!("Unknown CSR instruction: funct3=0x{:x}", funct3),
                regs: *regs,
            }),
        }
    }
}

#[inline(always)]
fn execute_ecall<M: LoggingMode>(
    instruction_word: u32,
    pc: u32,
) -> Result<ExecutionResult, EmulatorError> {
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ecall,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: true,
        log,
    })
}

#[inline(always)]
fn execute_ebreak<M: LoggingMode>(
    instruction_word: u32,
    pc: u32,
) -> Result<ExecutionResult, EmulatorError> {
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ebreak,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: true,
        syscall: false,
        log,
    })
}

/// Decode and execute FENCE/FENCE.I instructions (opcode 0x0f).
pub(super) fn decode_execute_fence<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    let funct3 = ((inst_word >> 12) & 0x7) as u8;
    let imm = ((inst_word >> 20) & 0xfff) as u16;
    let rs1 = ((inst_word >> 15) & 0x1f) as u8;
    let rd = ((inst_word >> 7) & 0x1f) as u8;

    if funct3 == 0x1 && imm == 0x001 && rs1 == 0 && rd == 0 {
        // FENCE.I: funct3=0x1, imm[11:0]=0x001, rs1=0, rd=0
        execute_fence_i::<M>(inst_word, pc)
    } else {
        // FENCE: funct3=0x0 (or other values, but we treat as FENCE)
        execute_fence::<M>(inst_word, pc)
    }
}

#[inline(always)]
fn execute_fence<M: LoggingMode>(
    instruction_word: u32,
    pc: u32,
) -> Result<ExecutionResult, EmulatorError> {
    // FENCE: Memory ordering (no-op in single-threaded emulator)
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ebreak, // Use existing kind (doesn't matter for logging)
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
fn execute_fence_i<M: LoggingMode>(
    instruction_word: u32,
    pc: u32,
) -> Result<ExecutionResult, EmulatorError> {
    // FENCE.I: Instruction cache synchronization (no-op in emulator)
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ebreak, // Use existing kind (doesn't matter for logging)
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
fn execute_csrrw<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    _csr: u16,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    // CSRRW: rd = CSR; CSR = rs1
    // In emulator, CSR operations are no-ops (we don't track CSR state)
    // Just write 0 to rd (or preserve if rd is x0)
    let result = 0i32; // CSR reads return 0 (no CSR state tracked)
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ebreak, // Use existing kind (doesn't matter for logging)
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
fn execute_csrrs<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    _csr: u16,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    // CSRRS: rd = CSR; CSR = CSR | rs1
    // In emulator, CSR operations are no-ops
    let result = 0i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ebreak,
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
fn execute_csrrc<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    _csr: u16,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    // CSRRC: rd = CSR; CSR = CSR & ~rs1
    // In emulator, CSR operations are no-ops
    let result = 0i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ebreak,
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
fn execute_csrrwi<M: LoggingMode>(
    rd: Gpr,
    imm: i32,
    _csr: u16,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    // CSRRWI: rd = CSR; CSR = imm
    // In emulator, CSR operations are no-ops
    let result = 0i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ebreak,
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
fn execute_csrrsi<M: LoggingMode>(
    rd: Gpr,
    imm: i32,
    _csr: u16,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    // CSRRSI: rd = CSR; CSR = CSR | imm
    // In emulator, CSR operations are no-ops
    let result = 0i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ebreak,
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
fn execute_csrrci<M: LoggingMode>(
    rd: Gpr,
    imm: i32,
    _csr: u16,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    // CSRRCI: rd = CSR; CSR = CSR & ~imm
    // In emulator, CSR operations are no-ops
    let result = 0i32;
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::System {
            cycle: 0,
            pc,
            instruction: instruction_word,
            kind: SystemKind::Ebreak,
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
    use lp_riscv_inst::encode;

    #[test]
    fn test_ecall_fast_path() {
        let mut regs = [0i32; 32];
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        let inst_word = encode::ecall();
        let result =
            decode_execute_system::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert!(result.syscall);
        assert!(!result.should_halt);
        assert!(result.log.is_none());
    }

    #[test]
    fn test_ebreak_fast_path() {
        let mut regs = [0i32; 32];
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        let inst_word = encode::ebreak();
        let result =
            decode_execute_system::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert!(!result.syscall);
        assert!(result.should_halt);
        assert!(result.log.is_none());
    }

    #[test]
    fn test_ecall_logging_path() {
        let mut regs = [0i32; 32];
        let mut memory = Memory::with_default_addresses(vec![], vec![]);

        let inst_word = encode::ecall();
        let result =
            decode_execute_system::<LoggingEnabled>(inst_word, 0, &mut regs, &mut memory).unwrap();

        assert!(result.syscall);
        assert!(result.log.is_some());
        if let Some(InstLog::System { kind, .. }) = result.log {
            assert_eq!(kind, SystemKind::Ecall);
        }
    }
}
