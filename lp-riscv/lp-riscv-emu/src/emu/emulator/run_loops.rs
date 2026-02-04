//! High-level run loop methods.

extern crate alloc;

use super::super::{
    error::EmulatorError,
    executor::{LoggingDisabled, LoggingEnabled, decode_execute},
    logging::LogLevel,
    memory::Memory,
};
use super::state::Riscv32Emulator;
use super::types::{PanicInfo, StepResult, SyscallInfo};
use alloc::{format, string::String, vec, vec::Vec};
use lp_riscv_emu_shared::{
    SERIAL_ERROR_INVALID_POINTER, SYSCALL_LOG, SYSCALL_PANIC, SYSCALL_SERIAL_HAS_DATA,
    SYSCALL_SERIAL_READ, SYSCALL_SERIAL_WRITE, SYSCALL_TIME_MS, SYSCALL_WRITE, SYSCALL_YIELD,
    syscall_to_level,
};
use lp_riscv_inst::Gpr;

/// Default fuel for run() function
const DEFAULT_FUEL: u64 = 100_000;

impl Riscv32Emulator {
    /// Internal run loop with tight loop and inline fuel checking.
    ///
    /// This dispatches to fast or logging path based on log_level.
    ///
    /// # Arguments
    /// * `fuel` - Maximum number of instructions to execute before returning FuelExhausted
    ///
    /// # Returns
    /// * `Ok(StepResult::Syscall(info))` - Yield syscall encountered (SYSCALL_YIELD)
    /// * `Ok(StepResult::Halted)` - EBREAK encountered (not a trap)
    /// * `Ok(StepResult::Trap(code))` - Trap encountered
    /// * `Ok(StepResult::Panic(info))` - Panic occurred
    /// * `Ok(StepResult::FuelExhausted(count))` - Fuel exhausted (instructions executed)
    /// * `Err(EmulatorError)` - Error occurred (memory access violation, etc.)
    pub(super) fn run_inner(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        match self.log_level {
            LogLevel::None => self.run_inner_fast(fuel),
            _ => self.run_inner_logging(fuel),
        }
    }

    /// Fast path run loop - zero logging overhead.
    fn run_inner_fast(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        let initial_instruction_count = self.instruction_count;

        loop {
            // Inline fuel check - decrement and check in the loop
            fuel -= 1;
            if fuel == 0 {
                let instructions_executed = self.instruction_count - initial_instruction_count;
                return Ok(StepResult::FuelExhausted(instructions_executed));
            }

            // Fetch instruction
            let inst_word = self.memory.fetch_instruction(self.pc).map_err(|mut e| {
                match &mut e {
                    super::super::error::EmulatorError::InvalidMemoryAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = self.regs;
                        *err_pc = self.pc;
                    }
                    super::super::error::EmulatorError::UnalignedAccess {
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

            // Increment instruction count before execution (for cycle counting)
            self.instruction_count += 1;

            // Execute using fast path (no logging)
            let exec_result = decode_execute::<LoggingDisabled>(
                inst_word,
                self.pc,
                &mut self.regs,
                &mut self.memory,
            )?;

            // Update PC (2 bytes for compressed, 4 for standard)
            let pc_increment = if is_compressed { 2 } else { 4 };
            self.pc = exec_result
                .new_pc
                .unwrap_or(self.pc.wrapping_add(pc_increment));

            // Handle results (no logging)
            if exec_result.should_halt {
                // Check if this is a trap BEFORE executing the instruction
                // For EBREAK instructions, we need to check if the current PC is a trap location
                let is_trap = self
                    .traps
                    .binary_search_by_key(&self.pc.saturating_sub(pc_increment), |(addr, _)| *addr)
                    .is_ok();

                if is_trap {
                    let original_pc = self.pc.saturating_sub(pc_increment);
                    let index = self
                        .traps
                        .binary_search_by_key(&original_pc, |(addr, _)| *addr)
                        .expect("Trap should be found");
                    let trap_code = self.traps[index].1;
                    return Ok(StepResult::Trap(trap_code));
                } else {
                    return Ok(StepResult::Halted);
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

                // Handle syscall
                match self.handle_syscall(syscall_info)? {
                    StepResult::Continue => continue,
                    result => return Ok(result),
                }
            } else {
                // Most common case - continue execution
                continue;
            }
        }
    }

    /// Logging path run loop - full logging support.
    fn run_inner_logging(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        let initial_instruction_count = self.instruction_count;

        loop {
            // Inline fuel check - decrement and check in the loop
            fuel -= 1;
            if fuel == 0 {
                let instructions_executed = self.instruction_count - initial_instruction_count;
                return Ok(StepResult::FuelExhausted(instructions_executed));
            }

            // Fetch instruction
            let inst_word = self.memory.fetch_instruction(self.pc).map_err(|mut e| {
                match &mut e {
                    super::super::error::EmulatorError::InvalidMemoryAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = self.regs;
                        *err_pc = self.pc;
                    }
                    super::super::error::EmulatorError::UnalignedAccess {
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

            // Increment instruction count before execution (for cycle counting)
            self.instruction_count += 1;

            // Execute using logging path
            let exec_result = decode_execute::<LoggingEnabled>(
                inst_word,
                self.pc,
                &mut self.regs,
                &mut self.memory,
            )?;

            // Update PC (2 bytes for compressed, 4 for standard)
            let pc_increment = if is_compressed { 2 } else { 4 };
            self.pc = exec_result
                .new_pc
                .unwrap_or(self.pc.wrapping_add(pc_increment));

            // Handle logging
            if let Some(log) = exec_result.log {
                let log_with_cycle = log.set_cycle(self.instruction_count);
                self.log_instruction(log_with_cycle);
            }

            // Handle results (same as fast path)
            if exec_result.should_halt {
                let is_trap = self
                    .traps
                    .binary_search_by_key(&self.pc.saturating_sub(pc_increment), |(addr, _)| *addr)
                    .is_ok();

                if is_trap {
                    let original_pc = self.pc.saturating_sub(pc_increment);
                    let index = self
                        .traps
                        .binary_search_by_key(&original_pc, |(addr, _)| *addr)
                        .expect("Trap should be found");
                    let trap_code = self.traps[index].1;
                    return Ok(StepResult::Trap(trap_code));
                } else {
                    return Ok(StepResult::Halted);
                }
            } else if exec_result.syscall {
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

                // Handle syscall
                match self.handle_syscall(syscall_info)? {
                    StepResult::Continue => continue,
                    result => return Ok(result),
                }
            } else {
                continue;
            }
        }
    }

    /// Handle a syscall and return the appropriate StepResult.
    ///
    /// This is shared between run_inner_fast, run_inner_logging, and step_inner.
    fn handle_syscall(&mut self, syscall_info: SyscallInfo) -> Result<StepResult, EmulatorError> {
        if syscall_info.number == SYSCALL_PANIC {
            // Extract panic information from syscall args
            let msg_ptr = syscall_info.args[0] as u32;
            let msg_len = syscall_info.args[1] as usize;
            let file_ptr = syscall_info.args[2] as u32;
            let file_len = syscall_info.args[3] as usize;
            let line = syscall_info.args[4] as u32;

            let message = read_memory_string(&self.memory, msg_ptr, msg_len)
                .unwrap_or_else(|_| format!("<failed to read panic message from 0x{msg_ptr:x}>"));

            let file = if file_ptr != 0 && file_len > 0 {
                read_memory_string(&self.memory, file_ptr, file_len).ok()
            } else {
                None
            };

            let panic_info = PanicInfo {
                message,
                file,
                line: if line != 0 { Some(line) } else { None },
                pc: self.pc,
            };

            Ok(StepResult::Panic(panic_info))
        } else if syscall_info.number == SYSCALL_WRITE {
            let msg_ptr = syscall_info.args[0] as u32;
            let msg_len = syscall_info.args[1] as usize;

            if let Ok(s) = read_memory_string(&self.memory, msg_ptr, msg_len) {
                #[cfg(feature = "std")]
                {
                    use std::io::Write;
                    let _ = std::io::stderr().write_all(s.as_bytes());
                    let _ = std::io::stderr().flush();
                }
            }

            self.regs[Gpr::A0.num() as usize] = 0;
            Ok(StepResult::Continue)
        } else if syscall_info.number == SYSCALL_LOG {
            let level_val = syscall_info.args[0];
            let module_path_ptr = syscall_info.args[1] as u32;
            let module_path_len = syscall_info.args[2] as usize;
            let msg_ptr = syscall_info.args[3] as u32;
            let msg_len = syscall_info.args[4] as usize;

            if let (Ok(module_path), Ok(msg)) = (
                read_memory_string(&self.memory, module_path_ptr, module_path_len),
                read_memory_string(&self.memory, msg_ptr, msg_len),
            ) {
                if let Some(level) = syscall_to_level(level_val) {
                    log::log!(target: &module_path, level, "{msg}");
                }
            }

            self.regs[Gpr::A0.num() as usize] = 0;
            Ok(StepResult::Continue)
        } else if syscall_info.number == SYSCALL_YIELD {
            Ok(StepResult::Syscall(syscall_info))
        } else if syscall_info.number == SYSCALL_SERIAL_WRITE {
            let ptr = syscall_info.args[0] as u32;
            let len = syscall_info.args[1] as usize;
            const MAX_WRITE_LEN: usize = 64 * 1024;
            let len = len.min(MAX_WRITE_LEN);

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
                self.regs[Gpr::A0.num() as usize] = SERIAL_ERROR_INVALID_POINTER;
            } else {
                let serial = self.get_or_create_serial_host();
                let result = serial.guest_write(&data);
                self.regs[Gpr::A0.num() as usize] = result;
            }
            Ok(StepResult::Continue)
        } else if syscall_info.number == SYSCALL_SERIAL_READ {
            let ptr = syscall_info.args[0] as u32;
            let max_len = syscall_info.args[1] as usize;
            const MAX_READ_LEN: usize = 64 * 1024;
            let max_len = max_len.min(MAX_READ_LEN);

            let mut buffer = vec![0u8; max_len];
            let serial = self.get_or_create_serial_host();
            let bytes_read = serial.guest_read(&mut buffer);

            if bytes_read < 0 {
                self.regs[Gpr::A0.num() as usize] = bytes_read;
            } else if bytes_read == 0 {
                self.regs[Gpr::A0.num() as usize] = 0;
            } else {
                let bytes_read = bytes_read as usize;
                let mut write_ok = true;
                for (i, &byte) in buffer[..bytes_read].iter().enumerate() {
                    if self
                        .memory
                        .write_byte(ptr.wrapping_add(i as u32), byte as i8)
                        .is_err()
                    {
                        write_ok = false;
                        break;
                    }
                }

                if !write_ok {
                    self.regs[Gpr::A0.num() as usize] = SERIAL_ERROR_INVALID_POINTER;
                } else {
                    self.regs[Gpr::A0.num() as usize] = bytes_read as i32;
                }
            }
            Ok(StepResult::Continue)
        } else if syscall_info.number == SYSCALL_SERIAL_HAS_DATA {
            let has_data = self
                .serial_host
                .as_ref()
                .map(|s| s.has_data())
                .unwrap_or(false);
            self.regs[Gpr::A0.num() as usize] = if has_data { 1 } else { 0 };
            Ok(StepResult::Continue)
        } else if syscall_info.number == SYSCALL_TIME_MS {
            #[cfg(feature = "std")]
            {
                self.init_start_time_if_needed();
                let elapsed = self.elapsed_ms();
                self.regs[Gpr::A0.num() as usize] = elapsed as i32;
            }
            #[cfg(not(feature = "std"))]
            {
                self.regs[Gpr::A0.num() as usize] = 0;
            }
            Ok(StepResult::Continue)
        } else {
            Ok(StepResult::Syscall(syscall_info))
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
    const MAX_STRING_LEN: usize = 1024;
    let len = len.min(MAX_STRING_LEN);

    if len == 0 {
        return Ok(String::new());
    }

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

    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(e) => {
            let valid_bytes = e.as_bytes();
            Ok(String::from_utf8_lossy(valid_bytes).into_owned())
        }
    }
}

impl Riscv32Emulator {
    /// Run the emulator with default fuel until yield, halt, trap, panic, or fuel exhaustion.
    ///
    /// Uses default fuel (100_000 instructions). For custom fuel, use `run_fuel()`.
    ///
    /// # Returns
    /// * `Ok(StepResult::Syscall(info))` - Yield syscall encountered (SYSCALL_YIELD)
    /// * `Ok(StepResult::Halted)` - EBREAK encountered (not a trap)
    /// * `Ok(StepResult::Trap(code))` - Trap encountered
    /// * `Ok(StepResult::Panic(info))` - Panic occurred
    /// * `Ok(StepResult::FuelExhausted(count))` - Fuel exhausted (instructions executed)
    /// * `Err(EmulatorError)` - Error occurred (memory access violation, etc.)
    pub fn run(&mut self) -> Result<StepResult, EmulatorError> {
        self.run_fuel(DEFAULT_FUEL)
    }

    /// Run the emulator with specified fuel until yield, halt, trap, panic, or fuel exhaustion.
    ///
    /// # Arguments
    /// * `fuel` - Maximum number of instructions to execute before returning FuelExhausted
    ///
    /// # Returns
    /// * `Ok(StepResult::Syscall(info))` - Yield syscall encountered (SYSCALL_YIELD)
    /// * `Ok(StepResult::Halted)` - EBREAK encountered (not a trap)
    /// * `Ok(StepResult::Trap(code))` - Trap encountered
    /// * `Ok(StepResult::Panic(info))` - Panic occurred
    /// * `Ok(StepResult::FuelExhausted(count))` - Fuel exhausted (instructions executed)
    /// * `Err(EmulatorError)` - Error occurred (memory access violation, etc.)
    pub fn run_fuel(&mut self, fuel: u64) -> Result<StepResult, EmulatorError> {
        self.run_inner(fuel)
    }

    /// Run until EBREAK is encountered, returning the value in a0.
    pub fn run_until_ebreak(&mut self) -> Result<i32, EmulatorError> {
        loop {
            match self.run()? {
                StepResult::Halted => {
                    return Ok(self.regs[Gpr::A0.num() as usize]);
                }
                StepResult::Trap(code) => {
                    return Err(EmulatorError::Trap {
                        code,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Panic(info) => {
                    return Err(EmulatorError::Panic {
                        info,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::FuelExhausted(_) => {
                    // Continue running - use more fuel
                    continue;
                }
                StepResult::Syscall(_) => {
                    // Treat syscall as error in this context (caller should use run_until_ecall)
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected ECALL in run_until_ebreak"),
                        regs: self.regs,
                    });
                }
                StepResult::Continue => {
                    // run() should not return Continue
                    unreachable!("run() should not return Continue");
                }
            }
        }
    }

    /// Run until ECALL is encountered, returning syscall information.
    pub fn run_until_ecall(&mut self) -> Result<SyscallInfo, EmulatorError> {
        loop {
            match self.run()? {
                StepResult::Syscall(info) => {
                    return Ok(info);
                }
                StepResult::Halted => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected EBREAK in run_until_ecall"),
                        regs: self.regs,
                    });
                }
                StepResult::Trap(_) => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected trap in run_until_ecall"),
                        regs: self.regs,
                    });
                }
                StepResult::Panic(info) => {
                    return Err(EmulatorError::Panic {
                        info,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::FuelExhausted(_) => {
                    // Continue running - use more fuel
                    continue;
                }
                StepResult::Continue => {
                    // run() should not return Continue
                    unreachable!("run() should not return Continue");
                }
            }
        }
    }

    /// Run until a yield syscall is encountered, with a maximum step limit
    ///
    /// Steps the emulator until a yield syscall (SYSCALL_YIELD) is encountered,
    /// or until the maximum number of steps is reached.
    ///
    /// # Arguments
    /// * `max_steps` - Maximum number of steps to execute
    ///
    /// # Returns
    /// * `Ok(SyscallInfo)` - Yield syscall was encountered
    /// * `Err(EmulatorError)` - Error occurred (trap, panic, or max steps exceeded)
    pub fn run_until_yield(&mut self, max_steps: u64) -> Result<SyscallInfo, EmulatorError> {
        loop {
            match self.run_fuel(max_steps)? {
                StepResult::Syscall(info) if info.number == SYSCALL_YIELD => {
                    return Ok(info);
                }
                StepResult::Syscall(_) => {
                    // Other syscall - continue execution (but run() only returns yield syscalls)
                    // This shouldn't happen, but handle it gracefully
                    continue;
                }
                StepResult::Halted => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected EBREAK in run_until_yield"),
                        regs: self.regs,
                    });
                }
                StepResult::Trap(code) => {
                    return Err(EmulatorError::Trap {
                        code,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Panic(info) => {
                    return Err(EmulatorError::Panic {
                        info,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::FuelExhausted(_) => {
                    // Fuel exhausted - this means we hit max_steps
                    return Err(EmulatorError::InstructionLimitExceeded {
                        limit: max_steps,
                        executed: max_steps,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Continue => {
                    // run() should not return Continue
                    unreachable!("run() should not return Continue");
                }
            }
        }
    }
}
