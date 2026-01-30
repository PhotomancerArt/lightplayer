//! Debug formatting and logging methods.

extern crate alloc;

use super::super::logging::{InstLog, LogLevel, SystemKind};
use super::state::Riscv32Emulator;
use crate::Gpr;
use alloc::{format, string::String, vec::Vec};
use core::fmt::Write;

impl Riscv32Emulator {
    /// Get captured log entries.
    pub fn get_logs(&self) -> &[InstLog] {
        &self.log_buffer
    }

    /// Format all captured logs as a string.
    pub fn format_logs(&self) -> String {
        let mut result = String::new();
        for log in &self.log_buffer {
            result.push_str(&format!("{log}\n"));
        }
        result
    }

    /// Clear captured log messages.
    pub fn clear_logs(&mut self) {
        self.log_buffer.clear();
    }

    /// Log an instruction based on the current log level.
    pub fn log_instruction(&mut self, log: InstLog) {
        match self.log_level {
            LogLevel::None => {}
            LogLevel::Errors => {
                // Only log on errors (handled elsewhere)
            }
            LogLevel::Instructions | LogLevel::Verbose => {
                // Implement rolling buffer: if buffer reaches 100, remove oldest
                if self.log_buffer.len() >= 100 {
                    self.log_buffer.remove(0);
                }
                self.log_buffer.push(log);
            }
        }
    }

    /// Dump the current emulator state as a human-readable string.
    pub fn dump_state(&self) -> String {
        let mut result = String::new();
        result.push_str(&format!("PC: 0x{:08x}\n", self.pc));
        result.push_str(&format!(
            "Instructions executed: {}\n",
            self.instruction_count
        ));
        result.push_str("\nRegisters:\n");

        // Named registers first
        let named_regs = [
            (Gpr::Zero, "zero"),
            (Gpr::Ra, "ra"),
            (Gpr::Sp, "sp"),
            (Gpr::Gp, "gp"),
            (Gpr::Tp, "tp"),
            (Gpr::T0, "t0"),
            (Gpr::T1, "t1"),
            (Gpr::T2, "t2"),
            (Gpr::S0, "s0"),
            (Gpr::S1, "s1"),
            (Gpr::A0, "a0"),
            (Gpr::A1, "a1"),
            (Gpr::A2, "a2"),
            (Gpr::A3, "a3"),
            (Gpr::A4, "a4"),
            (Gpr::A5, "a5"),
            (Gpr::A6, "a6"),
            (Gpr::A7, "a7"),
        ];

        for (reg, name) in &named_regs {
            let value = self.get_register(*reg);
            if value != 0 || *reg == Gpr::Zero {
                result.push_str(&format!(
                    "  {} (x{}) = 0x{:08x} ({})\n",
                    name,
                    reg.num(),
                    value as u32,
                    value
                ));
            }
        }

        // Other registers
        for i in 18..32 {
            let reg = Gpr::new(i);
            let value = self.get_register(reg);
            if value != 0 {
                result.push_str(&format!("  x{} = 0x{:08x} ({})\n", i, value as u32, value));
            }
        }

        result
    }

    /// Helper function to format instructions while skipping long zero runs
    fn format_instructions_with_zero_skip<F>(
        result: &mut String,
        instructions: &[(u32, u32, String)],
        marker_fn: F,
    ) where
        F: Fn(&u32) -> &str,
    {
        const MAX_ZERO_RUN: usize = 16;

        let mut zero_run_start: Option<usize> = None;

        for (idx, (pc, inst_word, disasm)) in instructions.iter().enumerate() {
            if *inst_word == 0 {
                // Track zero runs
                if zero_run_start.is_none() {
                    zero_run_start = Some(idx);
                }
            } else {
                // Non-zero instruction - flush any pending zero run
                if let Some(zero_start) = zero_run_start.take() {
                    let zero_count = idx - zero_start;
                    if zero_count > MAX_ZERO_RUN {
                        // Summarize long zero runs
                        result
                            .push_str(&format!("  ... ({zero_count} zero instructions skipped)\n"));
                    } else {
                        // Show short zero runs
                        for i in 0..zero_count {
                            let zero_pc = instructions[zero_start + i].0;
                            let marker = marker_fn(&zero_pc);
                            result.push_str(&format!(
                                "{}{:3}: 0x{:08x}: {}\n",
                                marker,
                                zero_start + i,
                                zero_pc,
                                instructions[zero_start + i].2
                            ));
                        }
                    }
                }

                // Format non-zero instruction
                let marker = marker_fn(pc);
                result.push_str(&format!("{marker}{idx:3}: 0x{pc:08x}: {disasm}\n"));
            }
        }

        // Flush any remaining zero run at the end
        if let Some(zero_start) = zero_run_start {
            let zero_count = instructions.len() - zero_start;
            if zero_count > MAX_ZERO_RUN {
                result.push_str(&format!("  ... ({zero_count} zero instructions skipped)\n"));
            } else {
                for i in 0..zero_count {
                    let zero_pc = instructions[zero_start + i].0;
                    let marker = marker_fn(&zero_pc);
                    result.push_str(&format!(
                        "{}{:3}: 0x{:08x}: {}\n",
                        marker,
                        zero_start + i,
                        zero_pc,
                        instructions[zero_start + i].2
                    ));
                }
            }
        }
    }

    /// Format debug information including disassembly and execution logs.
    ///
    /// # Arguments
    ///
    /// * `highlight_pc` - Optional PC to highlight in disassembly (for errors or current position)
    /// * `log_count` - Number of recent logs to show (default 20)
    pub fn format_debug_info(&self, highlight_pc: Option<u32>, log_count: usize) -> String {
        let mut result = String::new();
        let code = self.memory.code();

        // Show disassembly when there's a PC to highlight (error or current position)
        if let Some(error_pc) = highlight_pc {
            // Disassemble all instructions
            let mut instructions = Vec::new();
            for i in (0..code.len()).step_by(4) {
                if i + 4 <= code.len() {
                    let inst_word =
                        u32::from_le_bytes([code[i], code[i + 1], code[i + 2], code[i + 3]]);
                    let pc = i as u32;
                    // Use proper disassembly formatting
                    let disasm = crate::inst::format_instruction(inst_word);
                    instructions.push((pc, inst_word, disasm));
                }
            }

            // Show disassembly
            result.push_str("Disassembly:\n");
            if instructions.len() > 50 {
                // Find the index of the highlighted instruction
                let fail_idx = instructions
                    .iter()
                    .position(|(pc, _, _)| *pc == error_pc)
                    .unwrap_or(0);
                let start = fail_idx.saturating_sub(10);
                let end = (fail_idx + 11).min(instructions.len());

                if start > 0 {
                    result.push_str("  ...\n");
                }

                for (idx, (pc, _inst_word, disasm)) in instructions[start..end].iter().enumerate() {
                    let actual_idx = start + idx;
                    let marker = if *pc == error_pc { ">>> " } else { "    " };
                    result.push_str(&format!("{marker}{actual_idx:3}: 0x{pc:08x}: {disasm}\n"));
                }

                if end < instructions.len() {
                    result.push_str("  ...\n");
                }
            } else {
                // Show all instructions, skipping long zero runs
                let error_pc = error_pc; // Capture for closure
                Self::format_instructions_with_zero_skip(&mut result, &instructions, move |pc| {
                    if *pc == error_pc { ">>> " } else { "    " }
                });
            }
        }
        // Skip disassembly when there's no error PC - execution logs are more useful

        // Show logs in chronological order (oldest first, matching execution order)
        if !self.log_buffer.is_empty() {
            result.push_str("\nExecution history:\n");
            // Show the last log_count entries in chronological order (oldest first)
            let start = self.log_buffer.len().saturating_sub(log_count);
            let logs_to_show = &self.log_buffer[start..];

            // Calculate maximum instruction width for proper column alignment
            let max_inst_width = logs_to_show
                .iter()
                .map(|log| {
                    let disassembly = crate::inst::format_instruction(log.instruction());
                    disassembly.len()
                })
                .max()
                .unwrap_or(0);

            // Format each log with proper column alignment
            for log in logs_to_show {
                let cycle = log.cycle();
                let pc = log.pc();
                let instruction = log.instruction();
                let disassembly = crate::inst::format_instruction(instruction);

                // Format: [cycle] 0xPC: instruction (padded) ; comment
                // Writing to String never fails, so unwrap is safe
                write!(
                    result,
                    "[{cycle:4}] 0x{pc:08x}: {disassembly:max_inst_width$}"
                )
                .unwrap();

                // Format comment
                match log {
                    InstLog::Arithmetic {
                        rd,
                        rs1_val,
                        rs2_val,
                        rd_old,
                        rd_new,
                        ..
                    } => {
                        write!(result, " ; {rd}: {rd_old} -> {rd_new}").unwrap();
                        if let Some(rs2_val) = rs2_val {
                            write!(result, " (rs1={rs1_val}, rs2={rs2_val})").unwrap();
                        } else {
                            write!(result, " (rs1={rs1_val})").unwrap();
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
                            result,
                            " ; {rd}: {rd_old} -> {rd_new} (mem[0x{addr:08x}] = {mem_val}) (rs1={rs1_val})"
                        )
                        .unwrap();
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
                            result,
                            " ; mem[0x{addr:08x}]: {mem_old} -> {mem_new} (rs1={rs1_val}, rs2={rs2_val})"
                        )
                        .unwrap();
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
                                    result,
                                    " ; branch taken: 0x{pc:08x} -> 0x{target:08x} (rs1={rs1_val}, rs2={rs2_val})"
                                )
                                .unwrap();
                            } else {
                                write!(result, " ; branch taken (rs1={rs1_val}, rs2={rs2_val})")
                                    .unwrap();
                            }
                        } else {
                            write!(result, " ; branch not taken (rs1={rs1_val}, rs2={rs2_val})")
                                .unwrap();
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
                                result,
                                " ; rd: {rd_old} -> {rd_new} jump: 0x{pc:08x} -> 0x{target_pc:08x}"
                            )
                            .unwrap();
                        } else {
                            write!(result, " ; jump: 0x{pc:08x} -> 0x{target_pc:08x}").unwrap();
                        }
                    }
                    InstLog::Immediate {
                        rd, rd_old, rd_new, ..
                    } => {
                        write!(result, " ; {rd}: {rd_old} -> {rd_new}").unwrap();
                    }
                    InstLog::System { kind, .. } => match kind {
                        SystemKind::Ecall => write!(result, " ; syscall").unwrap(),
                        SystemKind::Ebreak => write!(result, " ; breakpoint").unwrap(),
                    },
                }
                result.push('\n');
            }
        }

        result
    }
}
