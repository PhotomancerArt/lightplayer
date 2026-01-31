//! RISC-V 32-bit instruction encoding/decoding utilities.
//!
//! This crate provides:
//! - Instruction encoding and decoding (including compressed RVC instructions)
//! - Register definitions and instruction types
//! - Instruction formatting and disassembly

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

// Debug macro
#[macro_use]
mod debug;

// Instruction utilities
pub mod auipc_imm;
pub mod decode;
pub mod decode_rvc;
pub mod encode;
pub mod format;
pub mod inst;
pub mod register_role;
pub mod regs;

// Re-exports for convenience
pub use decode::decode_instruction;
pub use inst::{Inst, format_instruction};
pub use regs::Gpr;
