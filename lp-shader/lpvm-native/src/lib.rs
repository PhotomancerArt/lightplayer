//! LPIR → custom RISC-V backend (`lpvm-native`): lowering, register allocation, emission.
//!
//! Core lowering, [`regalloc`], and ELF emission are `no_std` + alloc.
//!
//! Enable feature **`emu`** for host-side linking with builtins and emulation via
//! `lp-riscv-emu` (requires `std`).
//!
//! # `any(target_arch = "riscv32", target_arch = "xtensa")` — the JIT-capable target set
//!
//! Everything that exists only because the crate can JIT and run code *on the
//! CPU it was compiled for* is gated on the literal, single-element form
//! `any(target_arch = "riscv32", target_arch = "xtensa")` rather than a bare `target_arch = "riscv32"`.
//! The `any(...)` is redundant today and semantically identical; it exists so
//! the set has one grep-able spelling:
//!
//! ```text
//! rg 'any\(target_arch = "riscv32"\)'
//! ```
//!
//! Every hit is a **capability** gate that gains `, target_arch = "xtensa"`
//! when the ESP32-S3 backport lands — a mechanical insertion, no
//! restructuring. Gates written as a bare `target_arch = "riscv32"` mean the
//! opposite: the code is RV32-**specific** (inline assembly in
//! `rt_jit::call`, [`isa::IsaTarget::native`]'s RV32 answer), and the backport
//! adds a sibling `#[cfg(target_arch = "xtensa")]` arm next to it instead.
//!
//! The same two spellings, with the same meanings, are used in
//! `lp-gfx-lpvm::target_backend` and `lpc-shared::backtrace`.

#![cfg_attr(not(feature = "emu"), no_std)]

#[macro_use]
extern crate alloc;

// Re-export log crate for use within this crate
pub use log;

pub mod abi;
pub mod compile;
pub mod config;
pub mod debug;
// Host debugging tool that compiles against the rv32 reference target
// explicitly (see its `let isa = IsaTarget::Rv32imac`), so it exists only
// when that backend does.
#[cfg(feature = "isa-rv32")]
pub mod debug_asm;
pub mod emit;
pub mod error;
mod exec_addr;
// The Xtensa hardware-risk corpus. Feature-gated because the firmware harness
// wants it and the app build must not pay for it; `no_std`, so the same module
// builds for the device and for the host golden test.
#[cfg(any(test, any(target_arch = "riscv32", target_arch = "xtensa")))]
mod jit_symbol_sizes;
pub mod link;
pub mod lower;
// Native-f32 lowering. Behind `float-f32` for the same reason the ISAs are
// behind `isa-*`: `FloatMode` is matched on a *runtime* value, so LTO cannot
// drop this on its own, and a Fixed-only device image must not pay for it
// (f32 roadmap D2).
#[cfg(feature = "float-f32")]
pub mod lower_f32;
pub mod lower_opts;
pub mod native_options;
pub mod opt;
pub mod regalloc;
pub mod region;
pub mod regset;
pub mod types;
pub mod vinst;
#[cfg(feature = "xt-corpus")]
pub mod xt_corpus;

#[cfg(feature = "emu")]
pub mod rt_emu;

pub mod isa;
#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
pub mod rt_jit;

pub use abi::ModuleAbi;
pub use compile::{
    CompileSession, CompiledFunction, CompiledModule, NativeCompileBudget, NativeCompileJob,
    NativeCompileStage, NativeCompileStepResult, NativeReloc, compile_function, compile_module,
};
#[cfg(feature = "isa-rv32")]
pub use debug_asm::compile_module_asm_text;
pub use emit::{EmittedCode, emit_lowered_with_alloc};
pub use error::{LowerError, NativeError};
pub use isa::IsaTarget;
pub use link::{LinkedJitImage, link_elf, link_jit};
pub use lower::{LoopRegion, LoweredFunction, lower_lpir_op, lower_ops};
pub use lower_opts::LowerOpts;
pub use native_options::NativeCompileOptions;
pub use types::NativeType;
pub use vinst::{
    IcmpCond, IrVReg, LabelId, ModuleSymbols, SRC_OP_NONE, SymbolId, TempVRegs, VInst, VReg,
    VRegSlice, pack_src_op, unpack_src_op,
};

#[cfg(feature = "emu")]
pub use rt_emu::{NativeEmuEngine, NativeEmuInstance, NativeEmuModule};

#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))]
pub use rt_jit::{
    BuiltinTable, NativeJitDirectCall, NativeJitEngine, NativeJitInstance, NativeJitModule,
};
