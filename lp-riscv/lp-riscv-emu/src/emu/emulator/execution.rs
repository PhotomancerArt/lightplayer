//! Instruction execution logic.

extern crate alloc;

use super::super::{
    decoder::decode_instruction, error::EmulatorError, executor::execute_instruction,
    memory::Memory,
};
use super::state::Riscv32Emulator;
use super::types::{PanicInfo, StepResult, SyscallInfo};
use alloc::{format, string::String, vec, vec::Vec};
use log;
use lp_riscv_emu_shared::SERIAL_ERROR_INVALID_POINTER;
use lp_riscv_inst::{Gpr, Inst};

impl Riscv32Emulator {
    /// Execute a single instruction (internal, no fuel check).
    ///
    /// This is the hot path function used by run() loops.
    /// Fuel checking happens in the calling loop, not here.
    #[inline(always)]
    pub(super) fn step_inner(&mut self) -> Result<StepResult, EmulatorError> {
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
        let exec_result = execute_instruction(
            decoded,
            inst_word,
            self.pc,
            &mut self.regs,
            &mut self.memory,
            self.log_level,
        )?;

        // Update PC (2 bytes for compressed, 4 for standard)
        let pc_increment = if is_compressed { 2 } else { 4 };
        self.pc = exec_result
            .new_pc
            .unwrap_or(self.pc.wrapping_add(pc_increment));

        // Log instruction with cycle count (only if logging is enabled)
        if let Some(log) = exec_result.log {
            let log_with_cycle = log.set_cycle(self.instruction_count);
            self.log_instruction(log_with_cycle);
        }

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
            if syscall_info.number == lp_riscv_emu_shared::SYSCALL_PANIC {
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
                log::debug!(
                    "Panic syscall detected: msg_ptr=0x{msg_ptr:x}, msg_len={msg_len}, file_ptr=0x{file_ptr:x}, file_len={file_len}, line={line}"
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
                            log::debug!("Read file name from memory: '{f}'");
                            Some(f)
                        }
                        Err(_e) => {
                            log::debug!("Failed to read file name from 0x{file_ptr:x}: {_e}");
                            None
                        }
                    }
                } else {
                    log::debug!("File pointer is null or file_len is 0, skipping file read");
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
            } else if syscall_info.number == lp_riscv_emu_shared::SYSCALL_WRITE {
                // SYSCALL_WRITE: Write string to host (always prints)
                // args[0] = pointer to string (as i32, cast to u32)
                // args[1] = length of string
                let msg_ptr = syscall_info.args[0] as u32;
                let msg_len = syscall_info.args[1] as usize;

                // Read string from memory and print it
                match read_memory_string(&self.memory, msg_ptr, msg_len) {
                    Ok(_s) => {
                        #[cfg(feature = "std")]
                        {
                            use std::io::Write;
                            let _ = std::io::stderr().write_all(_s.as_bytes());
                            let _ = std::io::stderr().flush();
                        }
                    }
                    Err(_e) => {
                        log::debug!("Failed to read write syscall string from 0x{msg_ptr:x}: {_e}");
                    }
                }

                // Return success (0 in a0)
                self.regs[Gpr::A0.num() as usize] = 0;
                Ok(StepResult::Continue)
            } else if syscall_info.number == lp_riscv_emu_shared::SYSCALL_LOG {
                // SYSCALL_LOG: Log message with level (filtered by RUST_LOG)
                // args[0] = level (u8 as i32: 0=error, 1=warn, 2=info, 3=debug)
                // args[1] = module_path pointer (as i32, cast to u32)
                // args[2] = module_path length (as i32)
                // args[3] = message pointer (as i32, cast to u32)
                // args[4] = message length (as i32)
                let level_val = syscall_info.args[0];
                let module_path_ptr = syscall_info.args[1] as u32;
                let module_path_len = syscall_info.args[2] as usize;
                let msg_ptr = syscall_info.args[3] as u32;
                let msg_len = syscall_info.args[4] as usize;

                // Read module path and message from memory
                match (
                    read_memory_string(&self.memory, module_path_ptr, module_path_len),
                    read_memory_string(&self.memory, msg_ptr, msg_len),
                ) {
                    (Ok(module_path), Ok(msg)) => {
                        // Convert syscall level to log::Level
                        if let Some(level) = lp_riscv_emu_shared::syscall_to_level(level_val) {
                            // Create a log record and call log::log!()
                            // This will respect RUST_LOG filtering via env_logger
                            log::log!(target: &module_path, level, "{msg}");
                        }
                    }
                    _ => {
                        // Failed to read strings - log error
                        log::warn!("Failed to read log syscall strings");
                    }
                }

                // Return success (0 in a0)
                self.regs[Gpr::A0.num() as usize] = 0;
                Ok(StepResult::Continue)
            } else if syscall_info.number == lp_riscv_emu_shared::SYSCALL_YIELD {
                // SYSCALL_YIELD: Yield control back to host
                // No arguments, no return value
                // Just return Syscall result so host can handle it
                Ok(StepResult::Syscall(syscall_info))
            } else if syscall_info.number == lp_riscv_emu_shared::SYSCALL_SERIAL_WRITE {
                // SYSCALL_SERIAL_WRITE: Write bytes to serial output buffer
                // args[0] = pointer to data (as i32, cast to u32)
                // args[1] = length of data
                // Returns: a0 = bytes written (or negative error code)
                let ptr = syscall_info.args[0] as u32;
                let len = syscall_info.args[1] as usize;

                // Validate length (prevent excessive reads)
                const MAX_WRITE_LEN: usize = 64 * 1024; // 64KB max per write
                let len = len.min(MAX_WRITE_LEN);

                // Read data from memory
                let mut data = Vec::with_capacity(len);
                let mut read_ok = true;
                for i in 0..len {
                    match self.memory.read_u8(ptr.wrapping_add(i as u32)) {
                        Ok(byte) => data.push(byte),
                        Err(_) => {
                            read_ok = false;
                            break;
                        }
                    }
                }

                if !read_ok {
                    // Invalid pointer - return error
                    self.regs[Gpr::A0.num() as usize] = SERIAL_ERROR_INVALID_POINTER;
                    Ok(StepResult::Continue)
                } else {
                    let serial = self.get_or_create_serial_host();
                    log::trace!(
                        "SYSCALL_SERIAL_WRITE: Writing {} bytes to serial",
                        data.len()
                    );
                    let result = serial.guest_write(&data);
                    log::trace!("SYSCALL_SERIAL_WRITE: guest_write returned {result}");
                    self.regs[Gpr::A0.num() as usize] = result;
                    Ok(StepResult::Continue)
                }
            } else if syscall_info.number == lp_riscv_emu_shared::SYSCALL_SERIAL_READ {
                // SYSCALL_SERIAL_READ: Read bytes from serial input buffer
                // args[0] = pointer to buffer (as i32, cast to u32)
                // args[1] = max length to read
                // Returns: a0 = bytes read (or negative error code)
                let ptr = syscall_info.args[0] as u32;
                let max_len = syscall_info.args[1] as usize;

                // Validate max_len
                const MAX_READ_LEN: usize = 64 * 1024; // 64KB max per read
                let max_len = max_len.min(MAX_READ_LEN);

                // Allocate buffer for reading
                let mut buffer = vec![0u8; max_len];
                let serial = self.get_or_create_serial_host();
                let bytes_read = serial.guest_read(&mut buffer);

                log::trace!(
                    "SYSCALL_SERIAL_READ: max_len={}, bytes_read={}, buffer[0..10]={:?}",
                    max_len,
                    bytes_read,
                    &buffer[..buffer.len().min(10)]
                );

                if bytes_read < 0 {
                    // Error
                    self.regs[Gpr::A0.num() as usize] = bytes_read;
                    Ok(StepResult::Continue)
                } else if bytes_read == 0 {
                    // No data
                    log::trace!("SYSCALL_SERIAL_READ: No data available, returning 0");
                    self.regs[Gpr::A0.num() as usize] = 0;
                    Ok(StepResult::Continue)
                } else {
                    // Write to memory
                    let bytes_read = bytes_read as usize;
                    let mut write_ok = true;
                    for (i, &byte) in buffer[..bytes_read].iter().enumerate() {
                        match self
                            .memory
                            .write_byte(ptr.wrapping_add(i as u32), byte as i8)
                        {
                            Ok(_) => {}
                            Err(_) => {
                                write_ok = false;
                                break;
                            }
                        }
                    }

                    if !write_ok {
                        self.regs[Gpr::A0.num() as usize] = SERIAL_ERROR_INVALID_POINTER;
                        Ok(StepResult::Continue)
                    } else {
                        self.regs[Gpr::A0.num() as usize] = bytes_read as i32;
                        Ok(StepResult::Continue)
                    }
                }
            } else if syscall_info.number == lp_riscv_emu_shared::SYSCALL_SERIAL_HAS_DATA {
                // SYSCALL_SERIAL_HAS_DATA: Check if serial input has data
                // Returns: a0 = 1 if data available, 0 otherwise
                let has_data = self
                    .serial_host
                    .as_ref()
                    .map(|s| s.has_data())
                    .unwrap_or(false);

                self.regs[Gpr::A0.num() as usize] = if has_data { 1 } else { 0 };
                Ok(StepResult::Continue)
            } else if syscall_info.number == lp_riscv_emu_shared::SYSCALL_TIME_MS {
                // SYSCALL_TIME_MS: Get elapsed milliseconds since emulator start
                // Returns: a0 = elapsed milliseconds (u32)
                #[cfg(feature = "std")]
                {
                    self.init_start_time_if_needed();
                    let elapsed = self.elapsed_ms();
                    self.regs[Gpr::A0.num() as usize] = elapsed as i32;
                }
                #[cfg(not(feature = "std"))]
                {
                    // Return 0 if std feature not enabled
                    self.regs[Gpr::A0.num() as usize] = 0;
                }

                Ok(StepResult::Continue)
            } else {
                Ok(StepResult::Syscall(syscall_info))
            }
        } else {
            Ok(StepResult::Continue)
        }
    }

    /// Execute a single instruction.
    ///
    /// This is the public API for single-step debugging.
    /// For running multiple instructions efficiently, use `run()` or `run_fuel()`.
    pub fn step(&mut self) -> Result<StepResult, EmulatorError> {
        // No fuel check - fuel is per-run, not global
        self.step_inner()
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
