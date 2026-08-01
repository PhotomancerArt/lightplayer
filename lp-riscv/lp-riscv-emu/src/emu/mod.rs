pub mod abi_helper;
mod decoder;
pub mod emulator;
pub mod error;
mod executor;
pub mod fp_regs;
pub mod logging;

#[cfg(feature = "std")]
pub use emulator::FrameOutcome;
pub use emulator::{DEFAULT_CALL_INSTRUCTION_LIMIT, Riscv32Emulator};
pub use error::{EmulatorError, trap_code_from_cranelift};
pub use fp_regs::{FpRegs, RoundingMode};
pub use logging::InstLog;
