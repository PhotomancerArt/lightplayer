//! Instruction decoder for RISC-V 32-bit instructions.
//!
//! This module re-exports the decoder from lpc-codegen to maintain
//! backward compatibility.

pub use lp_riscv_inst::decode_instruction;
