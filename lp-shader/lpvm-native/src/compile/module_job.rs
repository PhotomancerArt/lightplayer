use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;

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
    ir: LpirModule,
    stage: NativeCompileStage,
    session: Option<CompileSession>,
    functions: Vec<FunctionCompileState>,
    completed_functions: Vec<crate::CompiledFunction>,
    float_mode: FloatMode,
    options: NativeCompileOptions,
    isa: IsaTarget,
    sig_map: VecMap<String, LpsFnSig>,
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
        let mut sig_map = VecMap::new();
        for function in sig.functions.iter().cloned() {
            sig_map.insert(function.name.clone(), function);
        }
        Self {
            ir,
            stage: NativeCompileStage::SetupModule,
            session: None,
            functions: Vec::new(),
            completed_functions: Vec::with_capacity(function_count),
            float_mode,
            options,
            isa,
            sig_map,
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
                self.run_function_stage(|func, session| {
                    compile_function_fold_constants(func, session);
                    Ok(())
                })
            }
            NativeCompileStage::CompileFunctionLower => {
                let ir = self.ir.clone();
                self.run_function_stage(|func, session| {
                    compile_function_lower_stage(func, &ir, session)?;
                    Ok(())
                })
            }
            NativeCompileStage::CompileFunctionPeephole => {
                self.run_function_stage(|func, _session| {
                    compile_function_peephole(func)?;
                    Ok(())
                })
            }
            NativeCompileStage::CompileFunctionRegalloc => {
                self.run_function_stage(|func, session| {
                    compile_function_regalloc_stage(func, session)?;
                    Ok(())
                })
            }
            NativeCompileStage::CompileFunctionEmit => self.run_function_stage(|func, session| {
                compile_function_emit_stage(func, session)?;
                Ok(())
            }),
            NativeCompileStage::CompileFunctionDebug => {
                let ir = self.ir.clone();
                self.run_function_stage(|func, session| {
                    compile_function_debug_sections(func, &ir, session)?;
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
        let sig = LpsModuleSig {
            functions: self.sig_map.values().cloned().collect(),
            ..Default::default()
        };
        let module_abi = ModuleAbi::from_ir_and_sig(self.isa, &self.ir, &sig);
        let session =
            CompileSession::new(module_abi, self.isa, self.float_mode, self.options.clone());
        let mut functions = Vec::with_capacity(self.ir.functions.len());
        for (index, func) in self.ir.functions.values().enumerate() {
            let fn_sig = self
                .sig_map
                .get(func.name.as_str())
                .cloned()
                .unwrap_or_else(|| LpsFnSig {
                    name: func.name.clone(),
                    return_type: LpsType::Void,
                    parameters: Vec::new(),
                    kind: LpsFnKind::UserDefined,
                });
            let func_abi = compile_function_func_abi(&session, func, &fn_sig);
            let _ = fn_sig;
            functions.push(FunctionCompileState::new(index, func.clone(), func_abi));
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

    fn run_function_stage(
        &mut self,
        f: impl FnOnce(&mut FunctionCompileState, &mut CompileSession) -> Result<(), NativeError>,
    ) -> NativeCompileStepResult {
        let Some(index) = self.next_incomplete_function_slot() else {
            self.stage = NativeCompileStage::AssembleModule;
            return NativeCompileStepResult::Pending;
        };
        let Some(session) = self.session.as_mut() else {
            self.stage = NativeCompileStage::Done;
            return NativeCompileStepResult::Failed(NativeError::Internal(alloc::format!(
                "compile job missing session in stage {:?}",
                self.stage
            )));
        };
        let func = &mut self.functions[index];
        match f(func, session) {
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
