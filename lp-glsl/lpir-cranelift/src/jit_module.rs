//! Host JIT module: finalized code, GLSL metadata, signatures.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use cranelift_codegen::ir::{Signature, types};
use cranelift_codegen::isa::{CallConv, OwnedTargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::FuncId;
use lpir::module::IrModule;
use lpir::{FloatMode, GlslModuleMeta};

use crate::compile_options::CompileOptions;
use crate::error::{CompileError, CompilerError};
use crate::module_lower::{LpirFuncEmitOrder, lower_lpir_into_module};
use crate::process_sync;

#[cfg(not(feature = "std"))]
use crate::jit_memory::AllocJitMemoryProvider;

/// Finalized JIT shader module with GLSL metadata for typed calls.
pub struct JitModule {
    pub(crate) inner: JITModule,
    pub(crate) glsl_meta: GlslModuleMeta,
    pub(crate) func_names: Vec<String>,
    pub(crate) func_ids: Vec<FuncId>,
    pub(crate) name_to_index: BTreeMap<String, usize>,
    pub(crate) signatures: BTreeMap<String, Signature>,
    /// Scalar return words per function (LPIR), even when the ABI uses StructReturn (empty `returns`).
    pub(crate) logical_return_words: BTreeMap<String, usize>,
    pub(crate) ir_param_counts: Vec<u16>,
    pub(crate) call_conv: CallConv,
    pub(crate) pointer_type: types::Type,
    pub(crate) float_mode: FloatMode,
}

// SAFETY: Finalized JIT code is immutable after `build_jit_module` returns. `JITModule` is not
// mutated on the post-compile call path; `NodeRuntime: Send + Sync` needs this for the engine.
unsafe impl Send for JitModule {}
unsafe impl Sync for JitModule {}

impl JitModule {
    /// Raw finalized code pointer for a function index (same order as source [`IrModule::functions`]).
    pub fn finalized_ptr_by_index(&self, index: usize) -> *const u8 {
        self.inner.get_finalized_function(self.func_ids[index])
    }

    /// Raw finalized code pointer by GLSL / LPIR function name.
    pub fn finalized_ptr(&self, name: &str) -> Option<*const u8> {
        let i = *self.name_to_index.get(name)?;
        Some(self.finalized_ptr_by_index(i))
    }

    /// Cranelift signature recorded for `name`.
    pub fn signature(&self, name: &str) -> Option<&Signature> {
        self.signatures.get(name)
    }

    pub fn call_conv(&self) -> CallConv {
        self.call_conv
    }

    pub fn pointer_type(&self) -> types::Type {
        self.pointer_type
    }

    pub fn float_mode(&self) -> FloatMode {
        self.float_mode
    }

    pub fn glsl_meta(&self) -> &GlslModuleMeta {
        &self.glsl_meta
    }

    /// LPIR function names in module order (same indices as [`Self::finalized_ptr_by_index`]).
    pub fn func_names(&self) -> &[String] {
        &self.func_names
    }
}

#[cfg(feature = "std")]
fn build_isa(flags: settings::Flags) -> Result<OwnedTargetIsa, CompilerError> {
    cranelift_native::builder()
        .map_err(|m| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "native ISA detection: {m}"
            )))
        })?
        .finish(flags)
        .map_err(|e| CompilerError::Codegen(CompileError::cranelift(alloc::format!("ISA: {e}"))))
}

#[cfg(not(feature = "std"))]
fn build_isa(flags: settings::Flags) -> Result<OwnedTargetIsa, CompilerError> {
    use cranelift_codegen::isa;
    use target_lexicon::Triple;

    let triple: Triple = "riscv32imac-unknown-none-elf".parse().map_err(|e| {
        CompilerError::Codegen(CompileError::cranelift(alloc::format!("parse triple: {e}")))
    })?;
    isa::lookup(triple)
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!("ISA lookup: {e}")))
        })?
        .finish(flags)
        .map_err(|e| CompilerError::Codegen(CompileError::cranelift(alloc::format!("ISA: {e}"))))
}

pub(crate) fn build_jit_module(
    ir: &IrModule,
    glsl_meta: GlslModuleMeta,
    options: CompileOptions,
) -> Result<JitModule, CompilerError> {
    let _codegen_guard = process_sync::codegen_guard();

    let mut flag_builder = settings::builder();
    flag_builder
        .set("regalloc_algorithm", "single_pass")
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "regalloc_algorithm: {e}"
            )))
        })?;
    flag_builder.set("is_pic", "false").map_err(|e| {
        CompilerError::Codegen(CompileError::cranelift(alloc::format!("is_pic: {e}")))
    })?;
    let flags = settings::Flags::new(flag_builder);

    let isa = build_isa(flags)?;

    let mut jit_builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    jit_builder.symbol_lookup_fn(crate::builtins::symbol_lookup_fn());

    #[cfg(not(feature = "std"))]
    jit_builder.memory_provider(alloc::boxed::Box::new(AllocJitMemoryProvider::new()));

    let mut jit_module = JITModule::new(jit_builder);

    let lowered = lower_lpir_into_module(&mut jit_module, ir, options, LpirFuncEmitOrder::Source)?;

    jit_module.finalize_definitions().map_err(|e| {
        CompilerError::Codegen(CompileError::cranelift(alloc::format!(
            "finalize_definitions: {e}"
        )))
    })?;

    Ok(JitModule {
        inner: jit_module,
        glsl_meta,
        func_names: lowered.func_names,
        func_ids: lowered.func_ids,
        name_to_index: lowered.name_to_index,
        signatures: lowered.signatures,
        logical_return_words: lowered.logical_return_words,
        ir_param_counts: lowered.ir_param_counts,
        call_conv: lowered.call_conv,
        pointer_type: lowered.pointer_type,
        float_mode: lowered.float_mode,
    })
}
