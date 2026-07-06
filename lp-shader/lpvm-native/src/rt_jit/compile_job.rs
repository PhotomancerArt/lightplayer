use alloc::sync::Arc;

use lpir::{CompilerConfig, LpirModule};
use lps_shared::LpsModuleSig;
use lpvm::{LpvmCompileBudget, LpvmCompileJob, LpvmCompileStepResult};

use crate::compile::{NativeCompileBudget, NativeCompileJob, NativeCompileStepResult};
use crate::error::NativeError;
use crate::isa::IsaTarget;
use crate::native_options::NativeCompileOptions;

use super::builtins::BuiltinTable;
use super::compiler::link_compiled_module_jit;
use super::module::{NativeJitModule, NativeJitModuleInner, build_entry_info};

enum NativeJitCompileStage {
    Backend(NativeCompileJob),
    Done,
}

pub struct NativeJitCompileJob {
    meta: LpsModuleSig,
    builtin_table: Arc<BuiltinTable>,
    options: NativeCompileOptions,
    isa: IsaTarget,
    stage: NativeJitCompileStage,
    entry_info: lp_collection::VecMap<alloc::string::String, super::module::NativeJitEntryInfo>,
}

impl NativeJitCompileJob {
    pub fn new(
        ir: LpirModule,
        meta: LpsModuleSig,
        builtin_table: Arc<BuiltinTable>,
        mut options: NativeCompileOptions,
        config: CompilerConfig,
        isa: IsaTarget,
    ) -> Self {
        options.config = config;
        // Build entry info before handing `ir` to the backend so the module
        // is moved, not cloned — the backend job owns the only IR copy.
        let entry_info = build_entry_info(&ir, &meta, isa)
            .expect("native jit compile job requires matching IR and module signatures");
        let backend =
            NativeCompileJob::new(ir, meta.clone(), options.float_mode, options.clone(), isa);
        Self {
            meta,
            builtin_table,
            options,
            isa,
            stage: NativeJitCompileStage::Backend(backend),
            entry_info,
        }
    }
}

impl LpvmCompileJob for NativeJitCompileJob {
    type Module = NativeJitModule;
    type Error = NativeError;

    fn step(
        &mut self,
        budget: LpvmCompileBudget,
    ) -> LpvmCompileStepResult<Self::Module, Self::Error> {
        match &mut self.stage {
            NativeJitCompileStage::Backend(job) => {
                match job.step(NativeCompileBudget::steps(budget.max_steps)) {
                    NativeCompileStepResult::Pending => LpvmCompileStepResult::Pending,
                    NativeCompileStepResult::Failed(err) => {
                        self.stage = NativeJitCompileStage::Done;
                        LpvmCompileStepResult::Failed(err)
                    }
                    NativeCompileStepResult::Finished(compiled) => {
                        self.stage = NativeJitCompileStage::Done;
                        match link_compiled_module_jit(compiled, &self.builtin_table, self.isa) {
                            Ok((buffer, entry_offsets)) => {
                                LpvmCompileStepResult::Finished(NativeJitModule {
                                    inner: Arc::new(NativeJitModuleInner {
                                        meta: self.meta.clone(),
                                        buffer,
                                        entry_offsets,
                                        entry_info: self.entry_info.clone(),
                                        options: self.options.clone(),
                                    }),
                                })
                            }
                            Err(err) => LpvmCompileStepResult::Failed(err),
                        }
                    }
                }
            }
            NativeJitCompileStage::Done => LpvmCompileStepResult::Failed(NativeError::Internal(
                alloc::format!("compile job already finished"),
            )),
        }
    }
}
