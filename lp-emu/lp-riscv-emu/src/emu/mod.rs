pub mod abi_helper;
mod decoder;
pub mod emulator;
pub mod error;
// `pub(crate)`, not private: the privileged hart (P2) lives elsewhere in
// this crate and steps instructions through `decode_execute` directly,
// handling only SYSTEM-opcode instructions itself.
pub(crate) mod executor;
pub mod fp_regs;
pub mod logging;

#[cfg(feature = "std")]
pub use emulator::FrameOutcome;
pub use emulator::{DEFAULT_CALL_INSTRUCTION_LIMIT, Riscv32Emulator};
pub use error::{EmulatorError, trap_code_from_cranelift};
// Crate-level surface for the privileged hart (P2), which lives outside
// `emu` and steps instructions through `decode_execute` directly, handling
// only SYSTEM-opcode instructions itself. No consumer yet in this phase —
// `execution.rs`/`run_loops.rs` still reach `executor::` directly.
#[allow(
    unused_imports,
    reason = "crate-internal API for the privileged hart landing in M3 P2"
)]
pub(crate) use executor::{
    ExecutionResult, LoggingDisabled, LoggingEnabled, LoggingMode, decode_execute,
};
pub use fp_regs::{FpRegs, RoundingMode};
pub use logging::InstLog;
