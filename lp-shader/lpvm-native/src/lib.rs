//! LPIR → custom RISC-V backend (`lpvm-native`).
//!
//! M1: [`VInst`], lowering, RV32 ABI, greedy regalloc. M2: RV32 encoding, emission, ELF `.o` output
//! ([`crate::isa::rv32::emit::emit_module_elf`]).

#![no_std]

extern crate alloc;

pub mod engine;
pub mod error;
pub mod instance;
pub mod isa;
pub mod lower;
pub mod module;
pub mod regalloc;
pub mod types;
pub mod vinst;

pub use engine::{NativeCompileOptions, NativeEngine};
pub use error::{LowerError, NativeError};
pub use instance::NativeInstance;
pub use isa::{CodeBlob, IsaBackend, Rv32Backend};
pub use lower::{lower_op, lower_ops};
pub use module::NativeModule;
pub use regalloc::{Allocation, GreedyAlloc, RegAlloc, VRegInfo};
pub use types::NativeType;
pub use vinst::{SymbolRef, VInst};
