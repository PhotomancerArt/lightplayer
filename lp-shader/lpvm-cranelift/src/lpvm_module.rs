//! LPVM trait implementation: CraneliftModule

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use cranelift_codegen::ir::Signature;
use lpir::FloatMode;
use lpir::module::IrModule;
use lps_shared::LpsModuleSig;

use crate::compile_options::CompileOptions;
use crate::direct_call::DirectCall;
use crate::error::CompilerError;
use crate::jit_module::build_jit_module;

/// Compiled Cranelift JIT module implementing [`lpvm::LpvmModule`] (see `lpvm_instance` module).
///
/// This is the new trait-based API for compiled modules. It holds the finalized
/// code pointers and metadata, allowing multiple instances to be created.
///
/// Coexists with [`crate::jit_module::JitModule`] until M7.
#[derive(Clone)]
pub struct CraneliftModule {
    /// Code pointers by function name
    code_ptrs: BTreeMap<String, *const u8>,
    /// Function signatures by name
    signatures: BTreeMap<String, Signature>,
    /// Logical return word counts (for multi-return handling)
    pub(crate) logical_return_words: BTreeMap<String, usize>,
    /// LPIR param counts per function index (same order as [`Self::name_to_index`])
    pub(crate) name_to_index: BTreeMap<String, usize>,
    pub(crate) ir_param_counts: Vec<u16>,
    /// Function metadata from compilation
    metadata: LpsModuleSig,
    /// Float mode used during compilation
    float_mode: FloatMode,
    /// Call convention (needed for DirectCall)
    call_conv: cranelift_codegen::isa::CallConv,
    /// Pointer type (needed for DirectCall)
    pointer_type: cranelift_codegen::ir::types::Type,
}

// SAFETY: Finalized JIT code is immutable. Module is Send+Sync like JitModule.
unsafe impl Send for CraneliftModule {}
unsafe impl Sync for CraneliftModule {}

impl CraneliftModule {
    /// Compile an LPIR module into a Cranelift JIT module.
    pub(crate) fn compile(
        ir: &IrModule,
        meta: &LpsModuleSig,
        options: CompileOptions,
    ) -> Result<Self, CompilerError> {
        // Use existing build_jit_module to do the heavy lifting
        let jit = build_jit_module(ir, meta.clone(), options)?;

        // Extract code pointers and signatures
        let mut code_ptrs = BTreeMap::new();
        let mut signatures = BTreeMap::new();
        let mut logical_return_words = BTreeMap::new();

        for (i, name) in jit.func_names.iter().enumerate() {
            let ptr = jit.inner.get_finalized_function(jit.func_ids[i]);
            code_ptrs.insert(name.clone(), ptr);

            if let Some(sig) = jit.signatures.get(name) {
                signatures.insert(name.clone(), sig.clone());
            }

            if let Some(words) = jit.logical_return_words.get(name) {
                logical_return_words.insert(name.clone(), *words);
            }
        }

        Ok(Self {
            code_ptrs,
            signatures,
            logical_return_words,
            name_to_index: jit.name_to_index,
            ir_param_counts: jit.ir_param_counts,
            metadata: meta.clone(),
            float_mode: jit.float_mode,
            call_conv: jit.call_conv,
            pointer_type: jit.pointer_type,
        })
    }

    pub(crate) fn float_mode(&self) -> FloatMode {
        self.float_mode
    }

    pub(crate) fn metadata(&self) -> &LpsModuleSig {
        &self.metadata
    }

    /// Get a raw code pointer for a function.
    pub fn code_ptr(&self, name: &str) -> Option<*const u8> {
        self.code_ptrs.get(name).copied()
    }

    /// Get the Cranelift signature for a function.
    pub fn signature(&self, name: &str) -> Option<&Signature> {
        self.signatures.get(name)
    }

    /// Get a DirectCall handle for the hot path.
    ///
    /// This is the zero-overhead calling interface for performance-critical
    /// code like the per-pixel render loop. The trait's [`LpvmInstance::call`]
    /// method adds overhead through name lookup and value marshaling.
    pub fn direct_call(&self, name: &str) -> Option<DirectCall> {
        use cranelift_codegen::ir::ArgumentPurpose;

        let sig = self.signatures.get(name)?;
        let ptr = self.code_ptrs.get(name).copied()?;

        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);

        let logical_ret = self
            .logical_return_words
            .get(name)
            .copied()
            .unwrap_or_else(|| sig.returns.len());

        let user_arg_count = if uses_struct_return {
            sig.params.len().saturating_sub(2)
        } else {
            sig.params.len().saturating_sub(1)
        };

        Some(DirectCall {
            func_ptr: ptr,
            call_conv: self.call_conv,
            pointer_type: self.pointer_type,
            arg_i32_count: user_arg_count,
            ret_i32_count: logical_ret,
            uses_struct_return,
        })
    }

    /// Check if a function exists in this module.
    pub fn has_function(&self, name: &str) -> bool {
        self.code_ptrs.contains_key(name)
    }

    /// Get all function names in this module.
    pub fn function_names(&self) -> Vec<&str> {
        self.code_ptrs.keys().map(|s| s.as_str()).collect()
    }
}
