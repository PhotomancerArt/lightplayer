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
        let entry_info = build_entry_info(ir, meta, IsaTarget::native())?;
        let (buffer, entry_offsets) = compile_module_jit(
            ir,
            meta,
            &self.builtin_table,
            &self.options,
            IsaTarget::native(),
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

    /// Q32 always; F32 exactly when this build linked a float lowering for the
    /// ISA — [`IsaTarget::f32_lowering`] is the float-capability seam, and
    /// `Unsupported` is what it answers for an Xtensa build without
    /// `float-f32` (and would answer for any future FPU-less target).
    ///
    /// Reading the seam rather than `cfg!(feature = "float-f32")` is what
    /// keeps this answer true per *target* instead of per *build*: the host
    /// links both ISAs, and rv32's soft-float path is available there even
    /// though no rv32 board ships it.
    fn supports_float_mode(&self, mode: lpir::FloatMode) -> bool {
        match mode {
            lpir::FloatMode::Q32 => true,
            lpir::FloatMode::F32 => {
                IsaTarget::native().f32_lowering() != crate::isa::F32Lowering::Unsupported
            }
        }
    }

    fn compile_with_params(
        &self,
        ir: &LpirModule,
        meta: &LpsModuleSig,
        params: &lpvm::LpvmCompileParams,
    ) -> Result<Self::Module, Self::Error> {
        let mut opts = self.options.clone();
        opts.config = params.config.clone();
        opts.float_mode = params.float_mode;
        let entry_info = build_entry_info(ir, meta, IsaTarget::native())?;
        let (buffer, entry_offsets) =
            compile_module_jit(ir, meta, &self.builtin_table, &opts, IsaTarget::native())?;
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
        params: lpvm::LpvmCompileParams,
    ) -> Result<BoxedLpvmCompileJob<'a, Self::Module, Self::Error>, (LpirModule, LpsModuleSig)>
    {
        Ok(Box::new(NativeJitCompileJob::new(
            ir,
            meta,
            Arc::clone(&self.builtin_table),
            self.options.clone(),
            params,
            IsaTarget::native(),
        )))
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.memory
    }
}
