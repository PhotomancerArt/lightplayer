//! System instruction execution (ECALL, EBREAK, CSR instructions)

extern crate alloc;

use super::{ExecutionResult, InstClass, LoggingMode, read_reg};
use crate::emu::{
    error::EmulatorError,
    fp_regs::FpRegs,
    logging::{InstLog, SystemKind},
};
use lp_emu_core::Memory;
use lp_riscv_inst::{Gpr, format::TypeI};

/// Decode and execute system instructions (I-type, opcode 0x73).
pub(super) fn decode_execute_system<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
    fp: &mut FpRegs,
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
                    "Unknown system instruction: funct3=0x{funct3:x}, funct12=0x{funct12:x}"
                ),
                regs: *regs,
            }),
        }
    } else {
        // CSR instructions
        let rd = Gpr::new(i.rd);
        let csr = (imm & 0xfff) as u16;
        // Register-sourced forms read rs1; immediate forms take the zero-
        // extended 5-bit `zimm` from the same instruction field.
        let (source, source_is_zero) = match funct3 {
            0b001 | 0b010 | 0b011 => {
                let rs1 = Gpr::new(i.rs1);
                (read_reg(regs, rs1) as u32, rs1.num() == 0)
            }
            0b101 | 0b110 | 0b111 => (u32::from(i.rs1), i.rs1 == 0),
            _ => {
                return Err(EmulatorError::InvalidInstruction {
                    pc,
                    instruction: inst_word,
                    reason: alloc::format!("Unknown CSR instruction: funct3=0x{funct3:x}"),
                    regs: *regs,
                });
            }
        };
        let op = match funct3 {
            0b001 | 0b101 => CsrOp::Write,
            0b010 | 0b110 => CsrOp::Set,
            _ => CsrOp::Clear,
        };
        execute_csr::<M>(
            rd,
            csr,
            op,
            source,
            source_is_zero,
            inst_word,
            pc,
            regs,
            fp,
        )
    }
}

/// The read-modify-write shape of a CSR instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CsrOp {
    /// `CSRRW` / `CSRRWI`: replace the CSR with the source value.
    Write,
    /// `CSRRS` / `CSRRSI`: set the bits present in the source value.
    Set,
    /// `CSRRC` / `CSRRCI`: clear the bits present in the source value.
    Clear,
}

/// Execute any of the six CSR instructions.
///
/// **Only the three F-extension CSRs are real state**: `fflags` (`0x001`),
/// `frm` (`0x002`) and `fcsr` (`0x003`), which RV32F requires
/// (RISC-V Unprivileged ISA v20240411 §21.2, `Floating-Point Control and
/// Status Register`). Every other CSR keeps this emulator's long-standing
/// behaviour — reads return 0 and writes are discarded — because nothing in
/// this codebase models `mstatus`, the counters, or the machine-mode CSRs, and
/// pretending otherwise would be worse than the honest no-op.
#[expect(
    clippy::too_many_arguments,
    reason = "one call site; splitting the CSR number, op, and source into a struct would only move the arguments"
)]
#[inline(always)]
fn execute_csr<M: LoggingMode>(
    rd: Gpr,
    csr: u16,
    op: CsrOp,
    source: u32,
    source_is_zero: bool,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    fp: &mut FpRegs,
) -> Result<ExecutionResult, EmulatorError> {
    let old = fp.read_csr(csr).unwrap_or(0);

    // CSRRS/CSRRC with a zero source (x0, or zimm == 0) must not write —
    // they are the spec's read-only forms. CSRRW/CSRRWI always write.
    let write = match op {
        CsrOp::Write => Some(source),
        CsrOp::Set if !source_is_zero => Some(old | source),
        CsrOp::Clear if !source_is_zero => Some(old & !source),
        _ => None,
    };
    if let Some(value) = write {
        // `false` here means "not an F CSR"; that is the no-op path.
        let _ = fp.write_csr(csr, value);
    }

    if rd.num() != 0 {
        regs[rd.num() as usize] = old as i32;
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
        class: InstClass::System,
        inst_size: 4,
        log,
    })
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
        class: InstClass::System,
        inst_size: 4,
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
        class: InstClass::System,
        inst_size: 4,
        log,
    })
}

/// Decode and execute FENCE/FENCE.I instructions (opcode 0x0f).
pub(super) fn decode_execute_fence<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    _regs: &mut [i32; 32],
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
        class: InstClass::Fence,
        inst_size: 4,
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
        class: InstClass::Fence,
        inst_size: 4,
        log,
    })
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec;

    use super::*;
    use crate::emu::executor::{LoggingDisabled, LoggingEnabled};
    use lp_emu_core::Memory;
    use lp_riscv_inst::encode;

    #[test]
    fn test_ecall_fast_path() {
        let mut regs = [0i32; 32];
        let mut memory = Memory::with_default_addresses(vec![], vec![]);
        let mut fp = FpRegs::new();

        let inst_word = encode::ecall();
        let result = decode_execute_system::<LoggingDisabled>(
            inst_word,
            0,
            &mut regs,
            &mut memory,
            &mut fp,
        )
        .unwrap();

        assert!(result.syscall);
        assert!(!result.should_halt);
        assert!(result.log.is_none());
    }

    #[test]
    fn test_ebreak_fast_path() {
        let mut regs = [0i32; 32];
        let mut memory = Memory::with_default_addresses(vec![], vec![]);
        let mut fp = FpRegs::new();

        let inst_word = encode::ebreak();
        let result = decode_execute_system::<LoggingDisabled>(
            inst_word,
            0,
            &mut regs,
            &mut memory,
            &mut fp,
        )
        .unwrap();

        assert!(!result.syscall);
        assert!(result.should_halt);
        assert!(result.log.is_none());
    }

    #[test]
    fn test_ecall_logging_path() {
        let mut regs = [0i32; 32];
        let mut memory = Memory::with_default_addresses(vec![], vec![]);
        let mut fp = FpRegs::new();

        let inst_word = encode::ecall();
        let result = decode_execute_system::<LoggingEnabled>(
            inst_word,
            0,
            &mut regs,
            &mut memory,
            &mut fp,
        )
        .unwrap();

        assert!(result.syscall);
        assert!(result.log.is_some());
        if let Some(InstLog::System { kind, .. }) = result.log {
            assert_eq!(kind, SystemKind::Ecall);
        }
    }

    /// Run one CSR instruction against a given FP state, returning `regs[rd]`.
    fn run_csr(inst_word: u32, fp: &mut FpRegs, rd: u8) -> i32 {
        let mut regs = [0i32; 32];
        regs[5] = 0x1234_5678u32 as i32; // x5, used as the rs1 source below
        let mut memory = Memory::with_default_addresses(vec![], vec![]);
        decode_execute_system::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory, fp).unwrap();
        regs[rd as usize]
    }

    #[test]
    fn csrrs_reads_accrued_fflags() {
        let mut fp = FpRegs::new();
        fp.accrue(crate::emu::fp_regs::FFLAG_NV | crate::emu::fp_regs::FFLAG_NX);
        let inst = encode::csrrs(Gpr::new(6), Gpr::new(0), crate::emu::fp_regs::CSR_FFLAGS);
        assert_eq!(run_csr(inst, &mut fp, 6), 0b1_0001);
    }

    #[test]
    fn csrrw_fcsr_round_trips_frm_and_fflags() {
        let mut fp = FpRegs::new();
        fp.set_frm(0b010);
        fp.accrue(crate::emu::fp_regs::FFLAG_OF);
        // x0 as the source writes 0 to fcsr and returns the old value.
        let inst = encode::csrrw(Gpr::new(6), Gpr::new(0), crate::emu::fp_regs::CSR_FCSR);
        let old = run_csr(inst, &mut fp, 6);
        assert_eq!(old, (0b010 << 5) | 0b0_0100);
        assert_eq!(fp.fcsr(), 0);
    }

    #[test]
    fn csrrwi_sets_frm() {
        let mut fp = FpRegs::new();
        let inst = encode::csrrwi(Gpr::new(0), 0b011, crate::emu::fp_regs::CSR_FRM);
        run_csr(inst, &mut fp, 0);
        assert_eq!(fp.frm(), 0b011);
    }

    #[test]
    fn csrrci_clears_selected_fflags() {
        let mut fp = FpRegs::new();
        fp.set_fflags(0b1_1111);
        let inst = encode::csrrci(Gpr::new(6), 0b0_0101, crate::emu::fp_regs::CSR_FFLAGS);
        let old = run_csr(inst, &mut fp, 6);
        assert_eq!(old, 0b1_1111);
        assert_eq!(fp.fflags(), 0b1_1010);
    }

    #[test]
    fn csrrs_with_x0_source_does_not_write() {
        let mut fp = FpRegs::new();
        fp.set_fflags(0b0_1010);
        let inst = encode::csrrs(Gpr::new(6), Gpr::new(0), crate::emu::fp_regs::CSR_FFLAGS);
        run_csr(inst, &mut fp, 6);
        assert_eq!(fp.fflags(), 0b0_1010);
    }

    #[test]
    fn non_fp_csrs_still_read_zero_and_discard_writes() {
        let mut fp = FpRegs::new();
        // `cycle` (0xc00) is not modelled: reads return 0 and the write is a
        // no-op that must not disturb the FP CSRs.
        fp.set_fcsr(0b0110_0011);
        let inst = encode::csrrw(Gpr::new(6), Gpr::new(5), 0xc00);
        assert_eq!(run_csr(inst, &mut fp, 6), 0);
        assert_eq!(fp.fcsr(), 0b0110_0011);
    }
}
