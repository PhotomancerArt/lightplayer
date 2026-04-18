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
pub mod types;
pub mod vinst;

#[cfg(feature = "emu")]
pub mod rt_emu;

#[cfg(target_arch = "riscv32")]
pub mod rt_jit;
pub mod isa;

pub use abi::ModuleAbi;
pub use isa::IsaTarget;
pub use compile::{
    compile_function, compile_module, CompileSession, CompiledFunction, CompiledModule, NativeReloc,
};
pub use debug_asm::compile_module_asm_text;
pub use emit::{emit_lowered_with_alloc, emit_vinsts, EmittedCode};
pub use error::{LowerError, NativeError};
pub use link::{link_elf, link_jit, LinkedJitImage};
pub use lower::{lower_lpir_op, lower_ops, LoopRegion, LoweredFunction};
pub use native_options::NativeCompileOptions;
pub use types::NativeType;
pub use vinst::{
    pack_src_op, unpack_src_op, IcmpCond, IrVReg, LabelId, ModuleSymbols, SymbolId, VInst, VReg,
    VRegSlice, SRC_OP_NONE,
};

#[cfg(feature = "emu")]
pub use rt_emu::{NativeEmuEngine, NativeEmuInstance, NativeEmuModule};

#[cfg(target_arch = "riscv32")]
pub use rt_jit::{
    BuiltinTable, NativeJitDirectCall, NativeJitEngine, NativeJitInstance, NativeJitModule,
};
