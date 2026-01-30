//! Instruction execution logic.

extern crate alloc;

use super::super::{
    decoder::decode_instruction, error::EmulatorError, executor::execute_instruction,
    memory::Memory,
};
use super::state::Riscv32Emulator;
use super::types::{PanicInfo, StepResult, SyscallInfo};
use crate::{Gpr, Inst};
use alloc::{format, string::String, vec::Vec};

impl Riscv32Emulator {
    /// Execute a single instruction.
    pub fn step(&mut self) -> Result<StepResult, EmulatorError> {
        // Check instruction limit
        if self.instruction_count >= self.max_instructions {
            return Err(EmulatorError::InstructionLimitExceeded {
                limit: self.max_instructions,
                executed: self.instruction_count,
                pc: self.pc,
                regs: self.regs,
            });
        }

        // Fetch instruction
        let inst_word = self.memory.fetch_instruction(self.pc).map_err(|mut e| {
            match &mut e {
                EmulatorError::InvalidMemoryAccess {
                    regs: err_regs,
                    pc: err_pc,
                    ..
                } => {
                    *err_regs = self.regs;
                    *err_pc = self.pc;
                }
                EmulatorError::UnalignedAccess {
                    regs: err_regs,
                    pc: err_pc,
                    ..
                } => {
                    *err_regs = self.regs;
                    *err_pc = self.pc;
                }
                _ => {}
            }
            e
        })?;

        // Check if compressed instruction (bits [1:0] != 0b11)
        let is_compressed = (inst_word & 0x3) != 0x3;

        // Decode instruction
        let decoded =
            decode_instruction(inst_word).map_err(|reason| EmulatorError::InvalidInstruction {
                pc: self.pc,
                instruction: inst_word,
                reason,
                regs: self.regs,
            })?;

        // Increment instruction count before execution (for cycle counting)
        self.instruction_count += 1;

        // Check if this is a trap BEFORE executing the instruction
        // For EBREAK instructions, we need to check if the current PC is a trap location
        let is_trap_before_execution = if let Inst::Ebreak = decoded {
            // Traps are stored as absolute addresses, compare directly with PC
            self.traps
                .binary_search_by_key(&self.pc, |(addr, _)| *addr)
                .is_ok()
        } else {
            false
        };

        // Execute instruction
        let exec_result = execute_instruction(decoded, self.pc, &mut self.regs, &mut self.memory)?;

        // Update PC (2 bytes for compressed, 4 for standard)
        let pc_increment = if is_compressed { 2 } else { 4 };
        self.pc = exec_result
            .new_pc
            .unwrap_or(self.pc.wrapping_add(pc_increment));

        // Log instruction with cycle count
        let log_with_cycle = exec_result.log.set_cycle(self.instruction_count);
        self.log_instruction(log_with_cycle);

        // Handle special cases
        if exec_result.should_halt {
            if is_trap_before_execution {
                // This was a trap - find the trap code using the original PC (before PC update)
                let original_pc = self.pc.saturating_sub(pc_increment);
                let index = self
                    .traps
                    .binary_search_by_key(&original_pc, |(addr, _)| *addr)
                    .expect("Trap should be found since is_trap_before_execution was true");
                let trap_code = self.traps[index].1;
                Ok(StepResult::Trap(trap_code))
            } else {
                // Regular ebreak (not a trap)
                Ok(StepResult::Halted)
            }
        } else if exec_result.syscall {
            // Extract syscall info from registers
            let syscall_info = SyscallInfo {
                number: self.regs[Gpr::A7.num() as usize],
                args: [
                    self.regs[Gpr::A0.num() as usize],
                    self.regs[Gpr::A1.num() as usize],
                    self.regs[Gpr::A2.num() as usize],
                    self.regs[Gpr::A3.num() as usize],
                    self.regs[Gpr::A4.num() as usize],
                    self.regs[Gpr::A5.num() as usize],
                    self.regs[Gpr::A6.num() as usize],
                ],
            };

            // Check if this is a panic syscall (SYSCALL_PANIC = 1)
            if syscall_info.number == 1 {
                // Extract panic information from syscall args
                // args[0] = message pointer (as i32, cast to u32)
                // args[1] = message length
                // args[2] = file pointer (as i32, 0 if unavailable)
                // args[3] = file length
                // args[4] = line number
                let msg_ptr = syscall_info.args[0] as u32;
                let msg_len = syscall_info.args[1] as usize;
                let file_ptr = syscall_info.args[2] as u32;
                let file_len = syscall_info.args[3] as usize;
                let line = syscall_info.args[4] as u32;

                // Debug: print syscall args
                crate::debug!(
                    "Panic syscall detected: msg_ptr=0x{:x}, msg_len={}, file_ptr=0x{:x}, file_len={}, line={}",
                    msg_ptr,
                    msg_len,
                    file_ptr,
                    file_len,
                    line
                );

                // Read panic message from memory
                let message =
                    read_memory_string(&self.memory, msg_ptr, msg_len).unwrap_or_else(|_| {
                        format!("<failed to read panic message from 0x{msg_ptr:x}>")
                    });

                // Read file name from memory (if pointer is not null)
                let file = if file_ptr != 0 && file_len > 0 {
                    match read_memory_string(&self.memory, file_ptr, file_len) {
                        Ok(f) => {
                            crate::debug!("Read file name from memory: '{}'", f);
                            Some(f)
                        }
                        Err(e) => {
                            crate::debug!("Failed to read file name from 0x{:x}: {}", file_ptr, e);
                            None
                        }
                    }
                } else {
                    crate::debug!("File pointer is null or file_len is 0, skipping file read");
                    None
                };

                // Create panic info
                let panic_info = PanicInfo {
                    message,
                    file,
                    line: if line != 0 { Some(line) } else { None },
                    pc: self.pc,
                };

                Ok(StepResult::Panic(panic_info))
            } else if syscall_info.number == 2 {
                // SYSCALL_WRITE: Write string to host (always prints)
                // args[0] = pointer to string (as i32, cast to u32)
                // args[1] = length of string
                let msg_ptr = syscall_info.args[0] as u32;
                let msg_len = syscall_info.args[1] as usize;

                // Read string from memory and print it
                match read_memory_string(&self.memory, msg_ptr, msg_len) {
                    Ok(s) => {
                        #[cfg(feature = "std")]
                        {
                            use std::io::Write;
                            let _ = std::io::stderr().write_all(s.as_bytes());
                            let _ = std::io::stderr().flush();
                        }
                    }
                    Err(e) => {
                        crate::debug!(
                            "Failed to read write syscall string from 0x{:x}: {}",
                            msg_ptr,
                            e
                        );
                    }
                }

                // Return success (0 in a0)
                self.regs[Gpr::A0.num() as usize] = 0;
                Ok(StepResult::Continue)
            } else if syscall_info.number == 3 {
                // SYSCALL_DEBUG: Debug output (delegates to debug! macro)
                // args[0] = pointer to string (as i32, cast to u32)
                // args[1] = length of string
                let msg_ptr = syscall_info.args[0] as u32;
                let msg_len = syscall_info.args[1] as usize;

                // Read string from memory and delegate to debug! macro
                match read_memory_string(&self.memory, msg_ptr, msg_len) {
                    Ok(s) => {
                        crate::debug!("{}", s);
                    }
                    Err(e) => {
                        crate::debug!(
                            "Failed to read debug syscall string from 0x{:x}: {}",
                            msg_ptr,
                            e
                        );
                    }
                }

                // Return success (0 in a0)
                self.regs[Gpr::A0.num() as usize] = 0;
                Ok(StepResult::Continue)
            } else {
                Ok(StepResult::Syscall(syscall_info))
            }
        } else {
            Ok(StepResult::Continue)
        }
    }
}

/// Read a string from emulator memory.
///
/// # Arguments
/// * `memory` - Reference to emulator memory
/// * `ptr` - Pointer to string in memory (as u32)
/// * `len` - Length of string in bytes
///
/// # Returns
/// * `Ok(String)` - Successfully read string
/// * `Err(String)` - Error message if memory access fails
fn read_memory_string(memory: &Memory, ptr: u32, len: usize) -> Result<String, String> {
    // Limit maximum string length to prevent excessive memory reads
    const MAX_STRING_LEN: usize = 1024;
    let len = len.min(MAX_STRING_LEN);

    if len == 0 {
        return Ok(String::new());
    }

    // Read bytes from memory
    let mut bytes = Vec::with_capacity(len);
    for i in 0..len {
        match memory.read_u8(ptr.wrapping_add(i as u32)) {
            Ok(byte) => bytes.push(byte),
            Err(e) => {
                return Err(format!(
                    "Failed to read byte at 0x{:x}: {}",
                    ptr + i as u32,
                    e
                ));
            }
        }
    }

    // Convert to UTF-8 string, handling invalid UTF-8 gracefully
    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(e) => {
            // If UTF-8 conversion fails, use lossy conversion
            let valid_bytes = e.as_bytes();
            Ok(String::from_utf8_lossy(valid_bytes).into_owned())
        }
    }
}
