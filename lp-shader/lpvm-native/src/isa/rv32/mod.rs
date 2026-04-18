//! RV32 ISA-specific code: encoding, GPR, ABI, emission.

pub mod abi;
pub mod debug;
pub mod emit;
pub mod encode;
pub mod gpr;
pub mod link;

// Re-exports from emit module
pub use emit::{EmittedCode, NativeReloc, emit_function};
