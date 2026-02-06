//! Logging infrastructure for the RISC-V 32 emu.

use core::fmt;

use lp_riscv_inst::Gpr;

/// Logging verbosity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// No logging.
    None,
    /// Only log errors.
    Errors,
    /// Log each instruction execution.
    Instructions,
}

/// System instruction kind for logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemKind {
    Ecall,
    Ebreak,
}

/// Log entry for a single instruction execution.
#[derive(Debug, Clone)]
pub enum InstLog {
    /// Arithmetic instructions: Add, Sub, Mul, Addi
    Arithmetic {
        cycle: u64,
        pc: u32,
        instruction: u32,
        rd: Gpr,
        rs1_val: i32,
        rs2_val: Option<i32>, // None for Addi, Some for Add/Sub/Mul
        rd_old: i32,
        rd_new: i32,
    },
    /// Load instruction: Lw
    Load {
        cycle: u64,
        pc: u32,
        instruction: u32,
        rd: Gpr,
        rs1_val: i32,
        addr: u32,
        mem_val: i32,
        rd_old: i32,
        rd_new: i32,
    },
    /// Store instruction: Sw
    Store {
        cycle: u64,
        pc: u32,
        instruction: u32,
        rs1_val: i32,
        rs2_val: i32,
        addr: u32,
        mem_old: i32,
        mem_new: i32,
    },
    /// Branch instructions: Beq, Bne, Blt, Bge
    Branch {
        cycle: u64,
        pc: u32,
        instruction: u32,
        rs1_val: i32,
        rs2_val: i32,
        taken: bool,
        target_pc: Option<u32>, // Some if taken
    },
    /// Jump instructions: Jal, Jalr
    Jump {
        cycle: u64,
        pc: u32,
        instruction: u32,
        rd_old: i32,
        rd_new: Option<i32>, // None if rd is x0
        target_pc: u32,
    },
    /// Immediate generation: Lui, Auipc
    Immediate {
        cycle: u64,
        pc: u32,
        instruction: u32,
        rd: Gpr,
        rd_old: i32,
        rd_new: i32,
    },
    /// System instructions: Ecall, Ebreak
    System {
        cycle: u64,
        pc: u32,
        instruction: u32,
        kind: SystemKind,
    },
}

impl InstLog {
    /// Get the cycle count for this log entry.
    pub fn cycle(&self) -> u64 {
        match self {
            InstLog::Arithmetic { cycle, .. }
            | InstLog::Load { cycle, .. }
            | InstLog::Store { cycle, .. }
            | InstLog::Branch { cycle, .. }
            | InstLog::Jump { cycle, .. }
            | InstLog::Immediate { cycle, .. }
            | InstLog::System { cycle, .. } => *cycle,
        }
    }

    /// Get the PC for this log entry.
    pub fn pc(&self) -> u32 {
        match self {
            InstLog::Arithmetic { pc, .. }
            | InstLog::Load { pc, .. }
            | InstLog::Store { pc, .. }
            | InstLog::Branch { pc, .. }
            | InstLog::Jump { pc, .. }
            | InstLog::Immediate { pc, .. }
            | InstLog::System { pc, .. } => *pc,
        }
    }

    /// Get the instruction word for this log entry.
    pub fn instruction(&self) -> u32 {
        match self {
            InstLog::Arithmetic { instruction, .. }
            | InstLog::Load { instruction, .. }
            | InstLog::Store { instruction, .. }
            | InstLog::Branch { instruction, .. }
            | InstLog::Jump { instruction, .. }
            | InstLog::Immediate { instruction, .. }
            | InstLog::System { instruction, .. } => *instruction,
        }
    }

    /// Set the cycle count for this log entry.
    pub fn set_cycle(self, cycle: u64) -> Self {
        match self {
            InstLog::Arithmetic {
                pc,
                instruction,
                rd,
                rs1_val,
                rs2_val,
                rd_old,
                rd_new,
                ..
            } => InstLog::Arithmetic {
                cycle,
                pc,
                instruction,
                rd,
                rs1_val,
                rs2_val,
                rd_old,
                rd_new,
            },
            InstLog::Load {
                pc,
                instruction,
                rd,
                rs1_val,
                addr,
                mem_val,
                rd_old,
                rd_new,
                ..
            } => InstLog::Load {
                cycle,
                pc,
                instruction,
                rd,
                rs1_val,
                addr,
                mem_val,
                rd_old,
                rd_new,
            },
            InstLog::Store {
                pc,
                instruction,
                rs1_val,
                rs2_val,
                addr,
                mem_old,
                mem_new,
                ..
            } => InstLog::Store {
                cycle,
                pc,
                instruction,
                rs1_val,
                rs2_val,
                addr,
                mem_old,
                mem_new,
            },
            InstLog::Branch {
                pc,
                instruction,
                rs1_val,
                rs2_val,
                taken,
                target_pc,
                ..
            } => InstLog::Branch {
                cycle,
                pc,
                instruction,
                rs1_val,
                rs2_val,
                taken,
                target_pc,
            },
            InstLog::Jump {
                pc,
                instruction,
                rd_old,
                rd_new,
                target_pc,
                ..
            } => InstLog::Jump {
                cycle,
                pc,
                instruction,
                rd_old,
                rd_new,
                target_pc,
            },
            InstLog::Immediate {
                pc,
                instruction,
                rd,
                rd_old,
                rd_new,
                ..
            } => InstLog::Immediate {
                cycle,
                pc,
                instruction,
                rd,
                rd_old,
                rd_new,
            },
            InstLog::System {
                pc,
                instruction,
                kind,
                ..
            } => InstLog::System {
                cycle,
                pc,
                instruction,
                kind,
            },
        }
    }
}

impl fmt::Display for InstLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cycle = self.cycle();
        let pc = self.pc();
        let instruction = self.instruction();
        // Use proper disassembly formatting
        let disassembly = lp_riscv_inst::format_instruction(instruction);

        // Print cycle count, address and instruction, padding instruction to align semicolons
        // Format: [cycle] 0xPC: instruction (padded to 30 chars) ; comment
        write!(f, "[{cycle:4}] 0x{pc:08x}: {disassembly:30}")?;

        // Format comment on the same line, separated by semicolon
        match self {
            InstLog::Arithmetic {
                rd,
                rs1_val,
                rs2_val,
                rd_old,
                rd_new,
                ..
            } => {
                write!(f, "; {rd}: {rd_old} -> {rd_new}")?;
                if let Some(rs2_val) = rs2_val {
                    write!(f, " (rs1={rs1_val}, rs2={rs2_val})")?;
                } else {
                    write!(f, " (rs1={rs1_val})")?;
                }
            }
            InstLog::Load {
                rd,
                rs1_val,
                addr,
                mem_val,
                rd_old,
                rd_new,
                ..
            } => {
                write!(
                    f,
                    "; {rd}: {rd_old} -> {rd_new} (mem[0x{addr:08x}] = {mem_val}) (rs1={rs1_val})"
                )?;
            }
            InstLog::Store {
                rs1_val,
                rs2_val,
                addr,
                mem_old,
                mem_new,
                ..
            } => {
                write!(
                    f,
                    "; mem[0x{addr:08x}]: {mem_old} -> {mem_new} (rs1={rs1_val}, rs2={rs2_val})"
                )?;
            }
            InstLog::Branch {
                rs1_val,
                rs2_val,
                taken,
                target_pc,
                ..
            } => {
                if *taken {
                    if let Some(target) = target_pc {
                        write!(
                            f,
                            "; branch taken: 0x{pc:08x} -> 0x{target:08x} (rs1={rs1_val}, rs2={rs2_val})"
                        )?;
                    } else {
                        write!(f, "; branch taken (rs1={rs1_val}, rs2={rs2_val})")?;
                    }
                } else {
                    write!(f, "; branch not taken (rs1={rs1_val}, rs2={rs2_val})")?;
                }
            }
            InstLog::Jump {
                rd_old,
                rd_new,
                target_pc,
                ..
            } => {
                if let Some(rd_new) = rd_new {
                    write!(
                        f,
                        "; rd: {rd_old} -> {rd_new} jump: 0x{pc:08x} -> 0x{target_pc:08x}"
                    )?;
                } else {
                    write!(f, "; jump: 0x{pc:08x} -> 0x{target_pc:08x}")?;
                }
            }
            InstLog::Immediate {
                rd, rd_old, rd_new, ..
            } => {
                write!(f, "; {rd}: {rd_old} -> {rd_new}")?;
            }
            InstLog::System { kind, .. } => match kind {
                SystemKind::Ecall => write!(f, "; syscall")?,
                SystemKind::Ebreak => write!(f, "; breakpoint")?,
            },
        }

        Ok(())
    }
}
