//! LPIR → Cranelift codegen for RV32 object emission.
//!
//! **Primary API** (feature `riscv32-object`): [`object_bytes_from_ir`] emits a relocatable RV32
//! object from LPIR; [`link_object_with_builtins`] links it against the builtins ELF. Run the
//! result in the emulator via `lpvm-emu`.
//!
//! This crate is the machine-generated reference compiler for the hand-built `lpvm-native`
//! backend: both lower the same LPIR to RV32, and filetests diff them (`rv32c` vs `rv32n`).
//!
//! The former in-process host JIT (`CraneliftEngine` / `jit_from_ir`) was removed: it exhibited
//! non-deterministic heap corruption when multiple `JITModule` instances lived in one process
//! (see `docs/bugs/2026-03-30-jit-filetest-segfault.md`), and `lpvm-wasm`'s wasmtime engine is
//! the host execution backend. Do not add host-execution paths here.

#![no_std]

#[macro_use]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod builtins;
mod compile_options;
mod emit;
pub mod error;
mod generated_builtin_abi;
mod module_lower;
mod process_sync;
mod q32_emit;

#[cfg(feature = "riscv32-object")]
mod object_link;
#[cfg(feature = "riscv32-object")]
mod object_module;

pub use compile_options::{CompileOptions, MemoryStrategy};
pub use emit::{signature_for_ir_func, signature_uses_struct_return};
pub use error::{CompileError, CompilerError};
pub use lpir::FloatMode;
pub use lps_shared::path_resolve::PathError;
pub use lps_shared::{
    FnParam, LayoutRules, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier, StructMember,
};
#[cfg(feature = "riscv32-object")]
pub use object_module::object_bytes_from_ir;

/// Back-compat alias; prefer [`ParamQualifier`].
pub type GlslParamQualifier = ParamQualifier;
/// Back-compat alias for a single formal parameter; prefer [`FnParam`].
pub type LpsSig = FnParam;
pub use lps_q32::q32_options::{AddSubMode, DivMode, MulMode, Q32Options};
pub use lpvm::{
    CallError, CallResult, GlslReturn, LpsValueQ32, decode_q32_return, flatten_q32_arg,
};
#[cfg(feature = "riscv32-object")]
pub use object_link::link_object_with_builtins;

/// Options-only tests: run under `--no-default-features`.
#[cfg(test)]
mod tests_options {
    use super::{
        AddSubMode, CompileOptions, DivMode, FloatMode, MemoryStrategy, MulMode, Q32Options,
    };

    #[test]
    fn compile_options_default() {
        let opts = CompileOptions::default();
        assert_eq!(opts.float_mode, FloatMode::Q32);
        assert_eq!(opts.q32_options, Q32Options::default());
        assert_eq!(opts.memory_strategy, MemoryStrategy::Default);
        assert_eq!(opts.max_errors, None);
    }

    #[test]
    fn q32_options_default_is_fast() {
        let q = Q32Options::default();
        assert_eq!(q.add_sub, AddSubMode::Wrapping);
        assert_eq!(q.mul, MulMode::Wrapping);
        assert_eq!(q.div, DivMode::Reciprocal);
    }
}
