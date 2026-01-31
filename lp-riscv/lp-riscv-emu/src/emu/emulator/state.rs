//! Core state and initialization for the RISC-V 32-bit emulator.

extern crate alloc;

use super::super::{logging::LogLevel, memory::Memory};
use crate::serial::host_serial::HostSerial;
use alloc::vec::Vec;
use cranelift_codegen::ir::TrapCode;

#[cfg(feature = "std")]
use std::time::Instant;

/// Default RAM start address (0x80000000, matching embive's RAM_OFFSET).
pub const DEFAULT_RAM_START: u32 = 0x80000000;

/// RISC-V 32-bit emulator state.
pub struct Riscv32Emulator {
    pub(super) regs: [i32; 32],
    pub(super) pc: u32,
    pub(super) memory: Memory,
    pub(super) instruction_count: u64,
    pub(super) max_instructions: u64,
    pub(super) log_level: LogLevel,
    pub(super) log_buffer: Vec<super::super::logging::InstLog>,
    pub(super) traps: Vec<(u32, TrapCode)>, // sorted by offset (offset, trap_code) pairs
    /// Serial host for bidirectional communication, lazy allocation
    pub(super) serial_host: Option<HostSerial>,
    /// Start time for elapsed time calculation (only when std feature enabled)
    #[cfg(feature = "std")]
    pub(super) start_time: Option<Instant>,
}

impl Riscv32Emulator {
    /// Create a new emulator with the given code, RAM, and trap information.
    ///
    /// # Arguments
    ///
    /// * `code` - Code region (instructions)
    /// * `ram` - RAM region (data)
    /// * `traps` - Trap information from compiled code (offset -> TrapCode pairs)
    pub fn with_traps(code: Vec<u8>, ram: Vec<u8>, traps: &[(u32, TrapCode)]) -> Self {
        // Sort traps by offset for efficient binary search lookup
        let mut trap_list: Vec<(u32, TrapCode)> = traps.to_vec();
        trap_list.sort_by_key(|(offset, _)| *offset);

        Self {
            regs: [0; 32],
            pc: 0,
            memory: Memory::with_default_addresses(code, ram),
            instruction_count: 0,
            max_instructions: 100_000,
            log_level: LogLevel::None,
            log_buffer: Vec::new(),
            traps: trap_list,
            serial_host: None,
            #[cfg(feature = "std")]
            start_time: None,
        }
    }

    /// Create a new emulator with the given code and RAM.
    ///
    /// # Arguments
    ///
    /// * `code` - Code region (instructions)
    /// * `ram` - RAM region (data)
    pub fn new(code: Vec<u8>, ram: Vec<u8>) -> Self {
        Self::with_traps(code, ram, &[])
    }

    /// Set the maximum number of instructions to execute.
    pub fn with_max_instructions(mut self, limit: u64) -> Self {
        self.max_instructions = limit;
        self
    }

    /// Set the maximum number of instructions to execute (mutating method).
    pub fn set_max_instructions(&mut self, limit: u64) {
        self.max_instructions = limit;
    }

    /// Set the logging level.
    pub fn with_log_level(mut self, level: LogLevel) -> Self {
        self.log_level = level;
        self
    }

    /// Get the number of instructions executed so far.
    pub fn get_instruction_count(&self) -> u64 {
        self.instruction_count
    }

    /// Drain all bytes from the serial output buffer
    ///
    /// Returns all bytes currently in the output buffer and clears it.
    /// Returns empty vector if buffer is not allocated or empty.
    pub fn drain_serial_output(&mut self) -> Vec<u8> {
        if let Some(serial) = &mut self.serial_host {
            let mut result = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                match serial.host_read(&mut buf) {
                    Ok(n) if n > 0 => {
                        result.extend_from_slice(&buf[..n]);
                    }
                    _ => break,
                }
            }
            result
        } else {
            Vec::new()
        }
    }

    /// Add bytes to the serial input buffer
    ///
    /// Adds bytes to the input buffer, respecting the 128KB limit.
    /// If buffer would exceed limit, drops excess bytes from the end.
    ///
    /// # Arguments
    /// * `data` - Bytes to add to input buffer
    pub fn serial_write(&mut self, data: &[u8]) {
        let serial = self.get_or_create_serial_host();
        let _ = serial.host_write(data); // Ignore errors (drops excess)
    }

    /// Read bytes from the serial output buffer (guest -> host)
    ///
    /// Reads up to `buffer.len()` bytes from the output buffer.
    /// Returns the number of bytes actually read.
    ///
    /// # Arguments
    /// * `buffer` - Buffer to read into
    ///
    /// # Returns
    /// Number of bytes read
    pub fn serial_read(&mut self, buffer: &mut [u8]) -> usize {
        if let Some(serial) = &mut self.serial_host {
            serial.host_read(buffer).unwrap_or(0)
        } else {
            0
        }
    }

    /// Write a line to the serial input buffer (host -> guest)
    ///
    /// Writes a line to the input buffer, appending a newline.
    /// If buffer is full, returns an error.
    ///
    /// # Arguments
    /// * `line` - Line to write (without newline)
    ///
    /// # Returns
    /// * `Ok(usize)` - Bytes written (including newline)
    /// * `Err(SerialError::BufferFull)` - Buffer is full
    pub fn serial_write_line(
        &mut self,
        line: &str,
    ) -> Result<usize, crate::serial::host_serial::SerialError> {
        let serial = self.get_or_create_serial_host();
        serial.host_write_line(line)
    }

    /// Read a line from the serial output buffer (guest -> host)
    ///
    /// Reads a line from the output buffer (until newline or EOF).
    /// Returns the line without the newline character.
    ///
    /// # Returns
    /// Line read (without newline), or empty string if no data available
    pub fn serial_read_line(&mut self) -> alloc::string::String {
        if let Some(serial) = &mut self.serial_host {
            serial.host_read_line()
        } else {
            alloc::string::String::new()
        }
    }

    /// Get or create the serial host
    pub(super) fn get_or_create_serial_host(&mut self) -> &mut HostSerial {
        if self.serial_host.is_none() {
            self.serial_host = Some(HostSerial::new(HostSerial::DEFAULT_BUF_SIZE));
        }
        self.serial_host.as_mut().unwrap()
    }

    /// Initialize start time if not already initialized
    #[cfg(feature = "std")]
    pub(super) fn init_start_time_if_needed(&mut self) {
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
    }

    /// Get elapsed milliseconds since start
    ///
    /// Returns 0 if start time not initialized or std feature disabled.
    #[cfg(feature = "std")]
    pub(super) fn elapsed_ms(&self) -> u32 {
        if let Some(start) = self.start_time {
            start.elapsed().as_millis() as u32
        } else {
            0
        }
    }
}
