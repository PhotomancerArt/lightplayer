//! LPIR → custom RISC-V backend (`lpvm-native`): lowering, register allocation, emission.
//!
//! Core lowering, [`regalloc`], and ELF emission are `no_std` + alloc.
//!
//! Enable feature **`emu`** for host-side linking with builtins and emulation via
//! `lp-riscv-emu` (requires `std`).

#![cfg_attr(not(feature = "emu"), no_std)]

#[macro_use]
extern crate alloc;

// Re-export log crate for use within this crate
pub use log;

pub mod abi;
pub mod compile;
pub mod config;
pub mod debug;
pub mod debug_asm;
pub mod emit;
pub mod error;
pub mod link;
pub mod lower;
pub mod native_options;
pub mod opt;
pub mod regalloc;
pub mod region;
pub mod regset;
pub mod rv32;
pub mod types;
pub mod vinst;

#[cfg(feature = "emu")]
pub mod rt_emu;

#[cfg(target_arch = "riscv32")]
pub mod rt_jit;

pub use abi::ModuleAbi;
pub use compile::{
    CompileSession, CompiledFunction, CompiledModule, NativeReloc, compile_function, compile_module,
};
pub use debug_asm::compile_module_asm_text;
pub use emit::{EmittedCode, emit_lowered_with_alloc, emit_vinsts};
pub use error::{LowerError, NativeError};
pub use link::{LinkedJitImage, link_elf, link_jit};
pub use lower::{LoopRegion, LoweredFunction, lower_lpir_op, lower_ops};
pub use native_options::NativeCompileOptions;
pub use types::NativeType;
pub use vinst::{
    IcmpCond, IrVReg, LabelId, ModuleSymbols, SRC_OP_NONE, SymbolId, VInst, VReg, VRegSlice,
    pack_src_op, unpack_src_op,
};

#[cfg(feature = "emu")]
pub use rt_emu::{NativeEmuEngine, NativeEmuInstance, NativeEmuModule};

#[cfg(target_arch = "riscv32")]
pub use rt_jit::{
    BuiltinTable, NativeJitDirectCall, NativeJitEngine, NativeJitInstance, NativeJitModule,
};
