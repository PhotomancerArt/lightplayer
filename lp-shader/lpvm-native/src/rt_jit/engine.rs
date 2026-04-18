//! [`LpvmEngine`] for RV32 JIT (linked in firmware, no ELF).

use alloc::sync::Arc;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory};

use crate::error::NativeError;
use crate::isa::IsaTarget;
use crate::native_options::NativeCompileOptions;

use super::builtins::BuiltinTable;
use super::compiler::compile_module_jit;
use super::host_memory::NativeHostMemory;
use super::module::{NativeJitModule, NativeJitModuleInner};

/// Compiles LPIR to a single in-memory RV32 image with patched builtin calls.
pub struct NativeJitEngine {
    builtin_table: Arc<BuiltinTable>,
    memory: NativeHostMemory,
    options: NativeCompileOptions,
}

impl NativeJitEngine {
    #[must_use]
    pub fn new(builtin_table: Arc<BuiltinTable>, options: NativeCompileOptions) -> Self {
        Self {
            builtin_table,
            memory: NativeHostMemory::new(),
            options,
        }
    }

    #[must_use]
    pub fn builtin_table(&self) -> &BuiltinTable {
        &self.builtin_table
    }
}

impl LpvmEngine for NativeJitEngine {
    type Module = NativeJitModule;
    type Error = NativeError;

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let (buffer, entry_offsets, _debug_info) = compile_module_jit(
            ir,
            meta,
            &self.builtin_table,
            self.options.float_mode,
            self.options.alloc_trace,
            IsaTarget::Rv32imac,
        )?;
        Ok(NativeJitModule {
            inner: Arc::new(NativeJitModuleInner {
                ir: ir.clone(),
                meta: meta.clone(),
                buffer,
                entry_offsets,
                options: self.options.clone(),
            }),
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.memory
    }
}
