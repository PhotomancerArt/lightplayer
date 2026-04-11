//! LPIR → custom RISC-V backend (`lpvm-native-fa`): fastalloc / straight-line PInst path.
//!
//! This crate is split from `lpvm-native` so the linear-scan emitter and fastalloc pipeline
//! can evolve independently. Core lowering, fastalloc, and ELF emission remain `no_std` + alloc.
//!
//! Enable feature **`emu`** for host-side linking with builtins and emulation via
//! `lp-riscv-emu` (requires `std`).

#![no_std]

#[macro_use]
extern crate alloc;

pub mod abi;
pub mod config;
pub mod debug;
pub mod debug_asm;
pub mod error;
pub mod isa;
pub mod lower;
pub mod native_options;
pub mod peephole;
pub mod regalloc;
pub mod region;
pub mod regset;
pub mod types;
pub mod vinst;

#[cfg(feature = "emu")]
pub mod rt_emu;

#[cfg(target_arch = "riscv32")]
pub mod rt_jit;

pub use abi::ModuleAbi;
pub use debug_asm::compile_module_asm_text;
pub use error::{LowerError, NativeError};
pub use isa::rv32::emit_function_fastalloc_bytes;
pub use isa::{CodeBlob, IsaBackend, Rv32Backend};
pub use lower::{LoopRegion, LoweredFunction, lower_lpir_op, lower_ops};
pub use native_options::NativeCompileOptions;
pub use regalloc::{Allocation, GreedyAlloc, LinearScan, RegAlloc, VRegInfo};
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
