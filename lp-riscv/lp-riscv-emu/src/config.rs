//! Compile-time configuration for lp-riscv-emu.

/// Maximum number of instruction log entries to retain in the rolling buffer.
/// When exceeded, oldest entries are removed (FIFO).
/// Set high enough to capture full test execution including final memcpy loops.
pub const INSTRUCTION_LOG_BUFFER_SIZE: usize = 5000;

/// Default number of recent instruction logs to display in debug output.
/// Set to the buffer size to show complete history.
pub const INSTRUCTION_LOG_DISPLAY_COUNT: usize = INSTRUCTION_LOG_BUFFER_SIZE;
