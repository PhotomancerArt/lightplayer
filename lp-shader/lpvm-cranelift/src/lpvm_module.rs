//! LPVM trait implementation: [`CraneliftModule`] wraps the live JIT ([`crate::jit_module::JitModule`])
//! in [`Arc`] so finalized code stays valid and [`Clone`] is cheap.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use cranelift_codegen::ir::Signature;
use lpir::FloatMode;
use lpir::lpir_module::LpirModule;
use lps_shared::LpsModuleSig;

use crate::compile_options::CompileOptions;
use crate::error::CompilerError;
use crate::jit_module::{JitModule, build_jit_module};

/// Compiled Cranelift JIT module implementing [`lpvm::LpvmModule`] (see `lpvm_instance` module).
///
/// Holds an [`Arc`] to the underlying [`JitModule`] so machine code remains mapped for the
/// module's lifetime (including after [`Clone`]).
#[derive(Clone)]
pub struct CraneliftModule(pub(crate) Arc<JitModule>);

// SAFETY: Finalized JIT code is immutable. Same reasoning as `JitModule`.
unsafe impl Send for CraneliftModule {}
unsafe impl Sync for CraneliftModule {}

impl CraneliftModule {
    /// Compile an LPIR module into a Cranelift JIT module.
    pub(crate) fn compile(
        ir: &LpirModule,
        meta: &LpsModuleSig,
        options: CompileOptions,
    ) -> Result<Self, CompilerError> {
        let jit = build_jit_module(ir, meta.clone(), options)?;
        Ok(Self(Arc::new(jit)))
    }

    pub(crate) fn float_mode(&self) -> FloatMode {
        self.0.float_mode()
    }

    pub(crate) fn metadata(&self) -> &LpsModuleSig {
        self.0.glsl_meta()
    }

    pub(crate) fn name_to_index(&self) -> &BTreeMap<String, usize> {
        &self.0.name_to_index
    }

    pub(crate) fn ir_param_counts(&self) -> &[u16] {
        &self.0.ir_param_counts
    }

    pub(crate) fn logical_return_words(&self) -> &BTreeMap<String, usize> {
        &self.0.logical_return_words
    }

    /// Raw finalized code pointer by GLSL / LPIR function name.
    pub fn finalized_ptr(&self, name: &str) -> Option<*const u8> {
        self.0.finalized_ptr(name)
    }

    /// Raw finalized code pointer for a function index (same order as source [`LpirModule::functions`]).
    pub fn finalized_ptr_by_index(&self, index: usize) -> *const u8 {
        self.0.finalized_ptr_by_index(index)
    }

    /// Alias for [`Self::finalized_ptr`] (historical name).
    pub fn code_ptr(&self, name: &str) -> Option<*const u8> {
        self.finalized_ptr(name)
    }

    /// Get the Cranelift signature for a function.
    pub fn signature(&self, name: &str) -> Option<&Signature> {
        self.0.signature(name)
    }

    /// LPIR function names in module order (same indices as [`Self::finalized_ptr_by_index`]).
    pub fn func_names(&self) -> &[String] {
        self.0.func_names()
    }

    /// Check if a function exists in this module.
    pub fn has_function(&self, name: &str) -> bool {
        self.0.name_to_index.contains_key(name)
    }

    /// Get all function names in this module.
    pub fn function_names(&self) -> Vec<&str> {
        self.0.func_names().iter().map(|s| s.as_str()).collect()
    }
}
