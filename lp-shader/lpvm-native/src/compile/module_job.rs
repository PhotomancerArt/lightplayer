use alloc::string::String;
use alloc::vec::Vec;

use lpir::{FloatMode, LpirModule};
use lps_shared::{LpsFnKind, LpsFnSig, LpsModuleSig, LpsType};

use super::function_job::FunctionCompileState;
use super::stages::{NativeCompileBudget, NativeCompileStage, NativeCompileStepResult};
use crate::CompiledModule;
use crate::abi::ModuleAbi;
use crate::compile::{
    CompileSession, compile_function_debug_sections, compile_function_emit_stage,
    compile_function_finalize, compile_function_fold_constants, compile_function_func_abi,
    compile_function_lower_stage, compile_function_peephole, compile_function_regalloc_stage,
};
use crate::error::NativeError;
use crate::isa::IsaTarget;
use crate::native_options::NativeCompileOptions;

pub struct NativeCompileJob {
    /// The job's own module. Stages borrow from it directly — const fold
    /// mutates functions in place — so no per-stage or per-function IR
    /// copies are made (they used to triple IR residency during compile).
    ir: LpirModule,
    stage: NativeCompileStage,
    session: Option<CompileSession>,
    functions: Vec<FunctionCompileState>,
    completed_functions: Vec<crate::CompiledFunction>,
    float_mode: FloatMode,
    options: NativeCompileOptions,
    isa: IsaTarget,
    sig: LpsModuleSig,
}

impl NativeCompileJob {
    pub fn new(
        ir: LpirModule,
        sig: LpsModuleSig,
        float_mode: FloatMode,
        options: NativeCompileOptions,
        isa: IsaTarget,
    ) -> Self {
        let function_count = ir.functions.len();
        Self {
            ir,
            stage: NativeCompileStage::SetupModule,
            session: None,
            functions: Vec::new(),
            completed_functions: Vec::with_capacity(function_count),
            float_mode,
            options,
            isa,
            sig,
        }
    }

    pub fn step(&mut self, budget: NativeCompileBudget) -> NativeCompileStepResult {
        let max_steps = budget.stage_limit();
        let mut steps = 0;
        loop {
            let result = self.step_one();
            steps += 1;
            if matches!(result, NativeCompileStepResult::Pending) && steps < max_steps {
                continue;
            }
            return result;
        }
    }

    pub fn stage(&self) -> NativeCompileStage {
        self.stage
    }

    pub fn current_function_index(&self) -> Option<usize> {
        self.functions
            .iter()
            .find(|func| !func.finished)
            .map(|func| func.index)
    }

    fn step_one(&mut self) -> NativeCompileStepResult {
        match self.stage {
            NativeCompileStage::SetupModule => match self.setup_module() {
                Ok(()) => {
                    self.stage = if self.functions.is_empty() {
                        NativeCompileStage::AssembleModule
                    } else {
                        NativeCompileStage::CompileFunctionConstFold
                    };
                    NativeCompileStepResult::Pending
                }
                Err(err) => {
                    self.stage = NativeCompileStage::Done;
                    NativeCompileStepResult::Failed(err)
                }
            },
            NativeCompileStage::CompileFunctionConstFold => {
                self.run_function_stage(|ir, func, _session| {
                    let body = ir.functions.get_mut(&func.func_id).ok_or_else(|| {
                        NativeError::Internal(alloc::format!(
                            "const fold stage missing function {} in module",
                            func.name
                        ))
                    })?;
                    compile_function_fold_constants(body);
                    Ok(())
                })
            }
            NativeCompileStage::CompileFunctionLower => {
                self.run_function_stage(|ir, func, session| {
                    let body = ir.functions.get(&func.func_id).ok_or_else(|| {
                        NativeError::Internal(alloc::format!(
                            "lower stage missing function {} in module",
                            func.name
                        ))
                    })?;
                    compile_function_lower_stage(func, body, ir, session)?;
                    Ok(())
                })
            }
            NativeCompileStage::CompileFunctionPeephole => {
                self.run_function_stage(|_ir, func, _session| {
                    compile_function_peephole(func)?;
                    Ok(())
                })
            }
            NativeCompileStage::CompileFunctionRegalloc => {
                self.run_function_stage(|_ir, func, session| {
                    compile_function_regalloc_stage(func, session)?;
                    Ok(())
                })
            }
            NativeCompileStage::CompileFunctionEmit => {
                self.run_function_stage(|_ir, func, session| {
                    compile_function_emit_stage(func, session)?;
                    Ok(())
                })
            }
            NativeCompileStage::CompileFunctionDebug => {
                self.run_function_stage(|ir, func, session| {
                    let body = ir.functions.get(&func.func_id).ok_or_else(|| {
                        NativeError::Internal(alloc::format!(
                            "debug stage missing function {} in module",
                            func.name
                        ))
                    })?;
                    compile_function_debug_sections(func, body, ir, session)?;
                    let compiled = compile_function_finalize(func)?;
                    func.compiled = Some(compiled);
                    Ok(())
                })
            }
            NativeCompileStage::AssembleModule => match self.assemble_module() {
                Ok(module) => {
                    self.stage = NativeCompileStage::Done;
                    NativeCompileStepResult::Finished(module)
                }
                Err(err) => {
                    self.stage = NativeCompileStage::Done;
                    NativeCompileStepResult::Failed(err)
                }
            },
            NativeCompileStage::Done => NativeCompileStepResult::Failed(NativeError::Internal(
                String::from("compile job already finished"),
            )),
        }
    }

    fn setup_module(&mut self) -> Result<(), NativeError> {
        if self.options.stage_trace {
            log::info!(
                "[native-compile] stage=SetupModule functions={}",
                self.ir.functions.len()
            );
        } else {
            log::debug!(
                "[native-fa] NativeCompileJob: building ABI for {} functions",
                self.ir.functions.len()
            );
        }
        let module_abi = ModuleAbi::from_ir_and_sig(self.isa, &self.ir, &self.sig);
        let session =
            CompileSession::new(module_abi, self.isa, self.float_mode, self.options.clone());
        let mut functions = Vec::with_capacity(self.ir.functions.len());
        for (index, (func_id, func)) in self.ir.functions.iter().enumerate() {
            let fn_sig = self.sig.functions.iter().find(|s| s.name == func.name);
            let func_abi = match fn_sig {
                Some(sig) => compile_function_func_abi(&session, func, sig),
                None => {
                    let default_sig = LpsFnSig {
                        name: func.name.clone(),
                        return_type: LpsType::Void,
                        parameters: Vec::new(),
                        kind: LpsFnKind::UserDefined,
                    };
                    compile_function_func_abi(&session, func, &default_sig)
                }
            };
            functions.push(FunctionCompileState::new(
                index,
                *func_id,
                func.name.clone(),
                func_abi,
            ));
        }
        self.functions = functions;
        self.session = Some(session);
        Ok(())
    }

    fn assemble_module(&mut self) -> Result<CompiledModule, NativeError> {
        if self.options.stage_trace {
            log::info!(
                "[native-compile] stage=AssembleModule completed_functions={}",
                self.completed_functions.len()
            );
        }
        let symbols = self
            .session
            .take()
            .map(|session| session.symbols)
            .unwrap_or_default();
        Ok(CompiledModule {
            functions: core::mem::take(&mut self.completed_functions),
            symbols,
        })
    }

    /// Run one per-function stage. The closure receives disjoint borrows of
    /// the job's module, the function's compile state, and the session —
    /// avoiding the whole-module clones this job used to make per stage
    /// step just to satisfy the borrow checker.
    fn run_function_stage(
        &mut self,
        f: impl FnOnce(
            &mut LpirModule,
            &mut FunctionCompileState,
            &mut CompileSession,
        ) -> Result<(), NativeError>,
    ) -> NativeCompileStepResult {
        let Some(index) = self.next_incomplete_function_slot() else {
            self.stage = NativeCompileStage::AssembleModule;
            return NativeCompileStepResult::Pending;
        };
        if self.session.is_none() {
            self.stage = NativeCompileStage::Done;
            return NativeCompileStepResult::Failed(NativeError::Internal(alloc::format!(
                "compile job missing session in stage {:?}",
                self.stage
            )));
        }
        let ir = &mut self.ir;
        let session = self.session.as_mut().expect("session checked above");
        let func = &mut self.functions[index];
        match f(ir, func, session) {
            Ok(()) => {
                self.after_successful_stage(index);
                NativeCompileStepResult::Pending
            }
            Err(err) => {
                self.stage = NativeCompileStage::Done;
                NativeCompileStepResult::Failed(err)
            }
        }
    }

    fn next_incomplete_function_slot(&self) -> Option<usize> {
        self.functions.iter().position(|func| !func.finished)
    }

    fn after_successful_stage(&mut self, index: usize) {
        if self.options.stage_trace {
            log::info!(
                "[native-compile] function={} stage={:?} done",
                self.functions[index].index,
                self.stage
            );
        }
        match self.stage {
            NativeCompileStage::CompileFunctionConstFold => {
                self.stage = NativeCompileStage::CompileFunctionLower;
            }
            NativeCompileStage::CompileFunctionLower => {
                self.stage = NativeCompileStage::CompileFunctionPeephole;
            }
            NativeCompileStage::CompileFunctionPeephole => {
                self.stage = NativeCompileStage::CompileFunctionRegalloc;
            }
            NativeCompileStage::CompileFunctionRegalloc => {
                self.stage = NativeCompileStage::CompileFunctionEmit;
            }
            NativeCompileStage::CompileFunctionEmit => {
                self.stage = NativeCompileStage::CompileFunctionDebug;
            }
            NativeCompileStage::CompileFunctionDebug => {
                let func = &mut self.functions[index];
                let compiled = func
                    .compiled
                    .take()
                    .expect("compile debug stage produced compiled function");
                func.release_intermediates();
                func.finished = true;
                self.completed_functions.push(compiled);
                if self.next_incomplete_function_slot().is_some() {
                    self.stage = NativeCompileStage::CompileFunctionConstFold;
                } else {
                    self.stage = NativeCompileStage::AssembleModule;
                }
            }
            _ => {}
        }
    }
}
