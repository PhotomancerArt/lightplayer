//! LPIR → custom RISC-V backend (`lpvm-native`).
//!
//! Core lowering, register allocation, and ELF emission (no_std + alloc).
//!
//! Enable feature **`emu`** for host-side linking with builtins and emulation via
//! `lp-riscv-emu` (requires `std`).

#![no_std]

extern crate alloc;

pub mod abi2;
pub mod debug_asm;
pub mod error;
pub mod isa;
pub mod lower;
pub mod native_options;
pub mod regalloc;
pub mod types;
pub mod vinst;

#[cfg(feature = "emu")]
pub mod rt_emu;

pub use debug_asm::compile_module_asm_text;
pub use error::{LowerError, NativeError};
pub use isa::{CodeBlob, IsaBackend, Rv32Backend};
pub use lower::{lower_op, lower_ops};
pub use native_options::NativeCompileOptions;
pub use regalloc::{Allocation, GreedyAlloc, RegAlloc, VRegInfo};
pub use types::NativeType;
pub use vinst::{SymbolRef, VInst};

#[cfg(feature = "emu")]
pub use rt_emu::{NativeEmuEngine, NativeEmuInstance, NativeEmuModule};
