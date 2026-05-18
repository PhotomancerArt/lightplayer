//! [`LpvmEngine`] for RV32 JIT (linked in firmware, no ELF).

use alloc::boxed::Box;
use alloc::sync::Arc;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::{BoxedLpvmCompileJob, LpvmEngine, LpvmMemory};

use crate::error::NativeError;
use crate::isa::IsaTarget;
use crate::native_options::NativeCompileOptions;

use super::builtins::BuiltinTable;
use super::compile_job::NativeJitCompileJob;
use super::compiler::compile_module_jit;
use super::host_memory::NativeHostMemory;
use super::module::{NativeJitModule, NativeJitModuleInner, build_entry_info};

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
        let entry_info = build_entry_info(ir, meta, IsaTarget::Rv32imac)?;
        let (buffer, entry_offsets) = compile_module_jit(
            ir,
            meta,
            &self.builtin_table,
            &self.options,
            IsaTarget::Rv32imac,
        )?;
        Ok(NativeJitModule {
            inner: Arc::new(NativeJitModuleInner {
                meta: meta.clone(),
                buffer,
                entry_offsets,
                entry_info,
                options: self.options.clone(),
            }),
        })
    }

    fn compile_with_config(
        &self,
        ir: &LpirModule,
        meta: &LpsModuleSig,
        config: &lpir::CompilerConfig,
    ) -> Result<Self::Module, Self::Error> {
        let mut opts = self.options.clone();
        opts.config = config.clone();
        let entry_info = build_entry_info(ir, meta, IsaTarget::Rv32imac)?;
        let (buffer, entry_offsets) =
            compile_module_jit(ir, meta, &self.builtin_table, &opts, IsaTarget::Rv32imac)?;
        Ok(NativeJitModule {
            inner: Arc::new(NativeJitModuleInner {
                meta: meta.clone(),
                buffer,
                entry_offsets,
                entry_info,
                options: opts,
            }),
        })
    }

    fn start_compile_job<'a>(
        &'a self,
        ir: LpirModule,
        meta: LpsModuleSig,
        config: lpir::CompilerConfig,
    ) -> Option<BoxedLpvmCompileJob<'a, Self::Module, Self::Error>> {
        Some(Box::new(NativeJitCompileJob::new(
            ir,
            meta,
            Arc::clone(&self.builtin_table),
            self.options.clone(),
            config,
            IsaTarget::Rv32imac,
        )))
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.memory
    }
}
