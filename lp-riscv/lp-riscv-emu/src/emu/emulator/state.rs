//! Core state and initialization for the RISC-V 32-bit emulator.

extern crate alloc;

use super::super::{
    cycle_model::CycleModel, executor::ExecutionResult, logging::LogLevel, memory::Memory,
};
use crate::serial::host_serial::HostSerial;
use crate::time::TimeMode;
#[cfg(feature = "std")]
use alloc::boxed::Box;
#[cfg(feature = "std")]
use alloc::string::String;
use alloc::vec::Vec;
use cranelift_codegen::ir::TrapCode;

#[cfg(feature = "std")]
use std::path::PathBuf;
#[cfg(feature = "std")]
use std::time::Instant;

#[cfg(feature = "std")]
use crate::profile::{Collector, Gate, HaltReason, ProfileSession, SessionMetadata};

/// Default RAM start address (0x80000000, matching embive's RAM_OFFSET).
pub const DEFAULT_RAM_START: u32 = 0x80000000;

pub use super::super::memory::DEFAULT_SHARED_START;

/// Result of running one driven frame (host workload driver).
#[cfg(feature = "std")]
pub enum FrameOutcome {
    /// Guest yielded back to host (idle/scheduler block).
    Yielded,
    /// Profile gate requested stop.
    ProfileStop,
    /// Session or guest stopped for another reason (OOM, EBREAK, trap, …).
    Halted(HaltReason),
}

#[cfg(feature = "std")]
impl core::fmt::Debug for FrameOutcome {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FrameOutcome::Yielded => f.write_str("Yielded"),
            FrameOutcome::ProfileStop => f.write_str("ProfileStop"),
            FrameOutcome::Halted(HaltReason::Oom { size }) => {
                f.debug_struct("Halted").field("oom_size", size).finish()
            }
            FrameOutcome::Halted(HaltReason::ProfileStop) => f.write_str("Halted(ProfileStop)"),
        }
    }
}

/// RISC-V 32-bit emulator state.
pub struct Riscv32Emulator {
    pub(super) regs: [i32; 32],
    pub(super) pc: u32,
    pub(super) memory: Memory,
    pub(super) instruction_count: u64,
    pub(super) cycle_count: u64,
    pub(super) cycle_model: CycleModel,
    pub(super) log_level: LogLevel,
    pub(super) log_buffer: Vec<super::super::logging::InstLog>,
    pub(super) traps: Vec<(u32, TrapCode)>, // sorted by offset (offset, trap_code) pairs
    /// Serial host for bidirectional communication, lazy allocation
    pub(super) serial_host: Option<HostSerial>,
    /// Start time for elapsed time calculation (only when std feature enabled)
    #[cfg(feature = "std")]
    pub(super) start_time: Option<Instant>,
    /// Time mode for controlling time advancement
    pub(super) time_mode: TimeMode,
    /// Unified profiling session (alloc and future collectors); `std` only.
    #[cfg(feature = "std")]
    pub(super) profile_session: Option<ProfileSession>,
    /// Set when a perf-event syscall handled with `std` profiling observes
    /// [`ProfileSession::pending_halt_reason`]. Cleared when surfaced as [`StepResult::ProfileStop`].
    pub(super) profile_stop_pending: bool,
    /// Trace directory root for this session (`std` only).
    #[cfg(feature = "std")]
    pub(super) profile_trace_dir: Option<PathBuf>,
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
            cycle_count: 0,
            cycle_model: CycleModel::default(),
            log_level: LogLevel::None,
            log_buffer: Vec::new(),
            traps: trap_list,
            serial_host: None,
            #[cfg(feature = "std")]
            start_time: None,
            time_mode: TimeMode::RealTime,
            #[cfg(feature = "std")]
            profile_session: None,
            #[cfg(feature = "std")]
            profile_trace_dir: None,
            profile_stop_pending: false,
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

    /// Build an emulator from a pre-built [`Memory`] (e.g. with a shared region) and trap list.
    pub fn from_memory(memory: Memory, traps: &[(u32, TrapCode)]) -> Self {
        let mut trap_list: Vec<(u32, TrapCode)> = traps.to_vec();
        trap_list.sort_by_key(|(offset, _)| *offset);

        Self {
            regs: [0; 32],
            pc: 0,
            memory,
            instruction_count: 0,
            cycle_count: 0,
            cycle_model: CycleModel::default(),
            log_level: LogLevel::None,
            log_buffer: Vec::new(),
            traps: trap_list,
            serial_host: None,
            #[cfg(feature = "std")]
            start_time: None,
            time_mode: TimeMode::RealTime,
            #[cfg(feature = "std")]
            profile_session: None,
            #[cfg(feature = "std")]
            profile_trace_dir: None,
            profile_stop_pending: false,
        }
    }

    /// Set the logging level.
    pub fn with_log_level(mut self, level: LogLevel) -> Self {
        self.log_level = level;
        self
    }

    pub fn with_cycle_model(mut self, model: CycleModel) -> Self {
        self.cycle_model = model;
        self
    }

    /// Allow misaligned memory access (matches embedded targets like ESP32).
    pub fn with_allow_unaligned_access(mut self, allow: bool) -> Self {
        self.memory.set_allow_unaligned_access(allow);
        self
    }

    /// Attach a profiling session (creates `trace_dir`, writes `meta.json`, enables collectors).
    #[cfg(feature = "std")]
    pub fn with_profile_session(
        mut self,
        trace_dir: PathBuf,
        metadata: &SessionMetadata,
        collectors: Vec<Box<dyn Collector>>,
    ) -> std::io::Result<Self> {
        self.profile_trace_dir = Some(trace_dir.clone());
        self.profile_session = Some(ProfileSession::new(trace_dir, metadata, collectors)?);
        Ok(self)
    }

    /// Take the active profiling session after emitting `profile:end` at the current cycle.
    ///
    /// The emulator's session slot is cleared. Call [`crate::profile::ProfileSession::finish`] or
    /// [`crate::profile::ProfileSession::finish_with_symbolizer`] on the returned value to flush
    /// collectors and write `report.txt`.
    #[cfg(feature = "std")]
    pub fn take_profile_session(&mut self) -> Option<ProfileSession> {
        let mut session = self.profile_session.take()?;
        session.end(self.cycle_count);
        Some(session)
    }

    /// Finish the profiling session (flush collectors, write `report.txt`).
    ///
    /// Returns per-collector event counts in session order.
    #[cfg(feature = "std")]
    pub fn finish_profile_session(&mut self) -> std::io::Result<Vec<(String, u64)>> {
        match self.take_profile_session() {
            Some(mut s) => s.finish(),
            None => Ok(Vec::new()),
        }
    }

    /// Install the profile mode gate (requires [`Self::with_profile_session`]).
    #[cfg(feature = "std")]
    pub fn set_profile_gate(&mut self, gate: Box<dyn Gate>) {
        self.profile_session
            .as_mut()
            .expect("set_profile_gate requires an active profile session")
            .set_gate(gate);
    }

    /// Get the number of instructions executed so far.
    pub fn get_instruction_count(&self) -> u64 {
        self.instruction_count
    }

    /// Guest steps accumulated with the active [`CycleModel`] (see [`Self::cycle_model`]).
    pub fn get_cycle_count(&self) -> u64 {
        self.cycle_count
    }

    /// Post-`decode_execute` cycle accounting (shared by run loops and single-step execution).
    #[inline(always)]
    pub(super) fn after_execute(&mut self, pc: u32, exec_result: &ExecutionResult) {
        let class = exec_result.class;
        let cost = self.cycle_model.cycles_for(class);
        self.cycle_count += cost as u64;
        #[cfg(feature = "std")]
        {
            if let Some(profile) = self.profile_session.as_mut() {
                let target_pc = exec_result
                    .new_pc
                    .unwrap_or(pc.wrapping_add(exec_result.inst_size as u32));
                profile.dispatch_instruction(pc, target_pc, class, cost as u32);
            }
        }
        #[cfg(not(feature = "std"))]
        let _ = pc;
    }

    pub fn cycle_model(&self) -> CycleModel {
        self.cycle_model
    }

    pub fn set_cycle_model(&mut self, model: CycleModel) {
        self.cycle_model = model;
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
                    Ok(n) => {
                        if n > 0 {
                            log::trace!(
                                "Riscv32Emulator::drain_serial_output: Read {n} bytes from host_read"
                            );
                            result.extend_from_slice(&buf[..n]);
                        } else {
                            log::trace!(
                                "Riscv32Emulator::drain_serial_output: host_read returned 0, breaking"
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        log::warn!("Riscv32Emulator::drain_serial_output: host_read error: {e:?}");
                        break;
                    }
                }
            }
            log::trace!(
                "Riscv32Emulator::drain_serial_output: Total drained {} bytes",
                result.len()
            );
            result
        } else {
            log::trace!("Riscv32Emulator::drain_serial_output: No serial_host, returning empty");
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

    /// Set the time mode
    pub fn with_time_mode(mut self, mode: TimeMode) -> Self {
        self.time_mode = mode;
        self
    }

    /// Set the time mode (mutating)
    pub fn set_time_mode(&mut self, mode: TimeMode) {
        self.time_mode = mode;
    }

    /// Advance simulated time (only works in Simulated mode)
    ///
    /// # Arguments
    /// * `ms` - Milliseconds to advance
    pub fn advance_time(&mut self, ms: u32) {
        if let TimeMode::Simulated(ref mut current) = self.time_mode {
            *current = current.saturating_add(ms);
        }
        // Ignore if in RealTime mode
    }

    /// Test-only: force [`Self::profile_stop_pending`] (e.g. profile gate stop without a guest ECALL).
    #[cfg(test)]
    pub fn set_profile_stop_pending_for_test(&mut self, pending: bool) {
        self.profile_stop_pending = pending;
    }

    /// Drive the guest until it yields, the profile session requests stop, or an error path
    /// maps to [`FrameOutcome::Halted`].
    ///
    /// `max_steps` caps work per call: if it is reached before yield or profile stop, returns
    /// [`FrameOutcome::Yielded`] so the caller can re-tick clocks or adjust budgets.
    #[cfg(feature = "std")]
    pub fn run_until_yield_or_stop(&mut self, max_steps: u64) -> FrameOutcome {
        use super::types::StepResult;
        use lp_riscv_emu_shared::SYSCALL_YIELD;

        let mut steps = 0u64;
        loop {
            if steps >= max_steps {
                return FrameOutcome::Yielded;
            }
            match self.step() {
                Ok(StepResult::Continue) => steps += 1,
                Ok(StepResult::Syscall(info)) if info.number == SYSCALL_YIELD => {
                    return FrameOutcome::Yielded;
                }
                Ok(StepResult::ProfileStop) => return FrameOutcome::ProfileStop,
                Ok(StepResult::Halted) => {
                    return FrameOutcome::Halted(HaltReason::Oom { size: 0 });
                }
                Ok(StepResult::Oom(info)) => {
                    return FrameOutcome::Halted(HaltReason::Oom { size: info.size });
                }
                Ok(StepResult::Trap(_) | StepResult::Panic(_) | StepResult::FuelExhausted(_)) => {
                    return FrameOutcome::Halted(HaltReason::Oom { size: 0 });
                }
                Ok(StepResult::Syscall(_)) => steps += 1,
                Err(_) => return FrameOutcome::Halted(HaltReason::Oom { size: 0 }),
            }
        }
    }

    /// Get elapsed milliseconds based on current time mode
    ///
    /// Returns 0 if start time not initialized (RealTime mode) or std feature disabled.
    #[cfg_attr(
        not(feature = "std"),
        allow(
            dead_code,
            reason = "SYSCALL_TIME_MS returns 0 without std; still used by unit tests and std builds"
        )
    )]
    pub(super) fn elapsed_ms(&self) -> u32 {
        match self.time_mode {
            TimeMode::Simulated(current) => current,
            TimeMode::RealTime => {
                #[cfg(feature = "std")]
                {
                    if let Some(start) = self.start_time {
                        start.elapsed().as_millis() as u32
                    } else {
                        0
                    }
                }
                #[cfg(not(feature = "std"))]
                {
                    0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::TimeMode;
    use alloc::vec;

    #[test]
    fn step_returns_profile_stop_when_stop_pending_set() {
        use super::super::types::StepResult;

        let code = vec![0x13, 0x00, 0x00, 0x00]; // addi x0, x0, 0
        let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
        emu.set_profile_stop_pending_for_test(true);
        match emu.step() {
            Ok(StepResult::ProfileStop) => {}
            Ok(other) => panic!("expected ProfileStop, got {other:?}"),
            Err(e) => panic!("unexpected err: {e:?}"),
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn run_until_yield_or_stop_returns_profile_stop() {
        use super::super::types::StepResult;

        let code = vec![
            0x13, 0x00, 0x00, 0x00, // nop
            0x13, 0x00, 0x00, 0x00, // nop (PC advances here after first guest step)
        ];
        let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
        emu.set_profile_stop_pending_for_test(true);
        let out = emu.run_until_yield_or_stop(10_000);
        assert!(
            matches!(out, FrameOutcome::ProfileStop),
            "expected ProfileStop, got {out:?}"
        );
        match emu.step() {
            Ok(StepResult::Continue) => {}
            Ok(other) => panic!("expected Continue after pending cleared, got {other:?}"),
            Err(e) => panic!("{e:?}"),
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn run_until_yield_or_stop_max_steps_zero_yields_immediately() {
        let mut emu = Riscv32Emulator::new(vec![], vec![]);
        let out = emu.run_until_yield_or_stop(0);
        assert!(
            matches!(out, FrameOutcome::Yielded),
            "expected Yielded, got {out:?}"
        );
    }

    #[test]
    fn test_simulated_time_mode() {
        let mut emu = Riscv32Emulator::new(vec![], vec![]).with_time_mode(TimeMode::Simulated(0));

        assert_eq!(emu.elapsed_ms(), 0);

        emu.advance_time(100);
        assert_eq!(emu.elapsed_ms(), 100);

        emu.advance_time(50);
        assert_eq!(emu.elapsed_ms(), 150);
    }

    #[test]
    fn test_realtime_mode_ignores_advance() {
        let mut emu = Riscv32Emulator::new(vec![], vec![]).with_time_mode(TimeMode::RealTime);

        // advance_time should be ignored in RealTime mode
        let initial = emu.elapsed_ms();
        emu.advance_time(100);
        // In RealTime mode, elapsed_ms should not jump by 100 immediately
        // (it might increase slightly due to real time passing, but not by 100)
        let after = emu.elapsed_ms();
        assert!(
            after < initial + 100,
            "RealTime mode should ignore advance_time"
        );
    }

    #[test]
    fn test_set_time_mode() {
        let mut emu = Riscv32Emulator::new(vec![], vec![]);
        // Default should be RealTime (can't check time_mode directly, but can check behavior)

        emu.set_time_mode(TimeMode::Simulated(42));
        assert_eq!(emu.elapsed_ms(), 42);

        emu.set_time_mode(TimeMode::RealTime);
        // Can't assert exact value in RealTime, but should not be 42
        let elapsed = emu.elapsed_ms();
        // In RealTime mode without initialization, should be 0
        assert_eq!(elapsed, 0);
    }
}
