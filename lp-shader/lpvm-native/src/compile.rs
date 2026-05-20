//! Compilation orchestration: LPIR → VInst → machine code.

mod function_job;
mod module_job;
mod stages;

use alloc::string::String;
use alloc::vec::Vec;

use lpir::{FloatMode, IrFunction, LpirModule};
use lps_shared::LpsFnSig;
use lpvm::FunctionDebugInfo;

use crate::LowerOpts;
use crate::abi::FuncAbi;
use crate::abi::ModuleAbi;
use crate::error::NativeError;
use crate::isa::IsaTarget;
use crate::vinst::ModuleSymbols;

pub use module_job::NativeCompileJob;
pub use stages::{NativeCompileBudget, NativeCompileStage, NativeCompileStepResult};

/// Relocation entry for a call site.
#[derive(Clone, Debug)]
pub struct NativeReloc {
    /// Byte offset within the function's code where the auipc instruction is.
    pub offset: usize,
    /// Symbol name to resolve (builtin or function).
    pub symbol: String,
    /// ELF / JIT relocation type (e.g. [`crate::isa::rv32::link::R_RISCV_CALL_PLT`]).
    pub r_type: u32,
}

/// Output of one function's compilation.
#[derive(Clone, Debug)]
pub struct CompiledFunction {
    /// Function name.
    pub name: String,
    /// RISC-V machine code bytes.
    pub code: Vec<u8>,
    /// Relocations for this function.
    pub relocs: Vec<NativeReloc>,
    /// Debug info: (code_offset, optional_src_op).
    pub debug_lines: Option<Vec<(u32, Option<u32>)>>,
    /// Structured debug info with sections.
    pub debug_info: Option<FunctionDebugInfo>,
}

/// Output of a full module compilation.
#[derive(Clone, Debug)]
pub struct CompiledModule {
    /// Compiled functions.
    pub functions: Vec<CompiledFunction>,
    /// Module-level symbol table (for interned strings).
    pub symbols: ModuleSymbols,
}

/// Module-level state shared across function compilations.
pub struct CompileSession {
    /// Interned symbols for calls/imports.
    pub symbols: ModuleSymbols,
    /// Module ABI for param/return locations.
    pub abi: ModuleAbi,
    /// Target ISA for per-function ABI construction.
    pub isa: IsaTarget,
    /// Floating point mode.
    pub float_mode: FloatMode,
    /// Compilation options.
    pub options: crate::native_options::NativeCompileOptions,
}

impl CompileSession {
    /// Create a new compile session for a module.
    pub fn new(
        abi: ModuleAbi,
        isa: IsaTarget,
        float_mode: FloatMode,
        options: crate::native_options::NativeCompileOptions,
    ) -> Self {
        Self {
            symbols: ModuleSymbols::default(),
            abi,
            isa,
            float_mode,
            options,
        }
    }
}

pub(crate) fn compile_function_func_abi(
    session: &CompileSession,
    func: &IrFunction,
    fn_sig: &LpsFnSig,
) -> FuncAbi {
    match session.isa {
        IsaTarget::Rv32imac => crate::isa::rv32::abi::func_abi_rv32(fn_sig, Some(func)),
    }
}

pub(crate) fn compile_function_fold_constants(
    state: &mut function_job::FunctionCompileState,
    _session: &CompileSession,
) {
    let mut func_opt = state
        .original
        .as_ref()
        .expect("const fold stage missing original function")
        .clone();
    let n_folded = lpir::const_fold::fold_constants(&mut func_opt);
    if n_folded > 0 {
        log::debug!(
            "[native-fa] compile_function: folded {n_folded} LPIR constants for {}",
            state.name
        );
    }
    state.optimized = Some(func_opt);
}

pub(crate) fn compile_function_lower_stage(
    state: &mut function_job::FunctionCompileState,
    ir: &LpirModule,
    session: &CompileSession,
) -> Result<(), NativeError> {
    let Some(func_opt) = state.optimized.as_ref() else {
        return Err(NativeError::Internal(format!(
            "lower stage missing optimized function for {}",
            state.name
        )));
    };
    let lower_opts = LowerOpts {
        float_mode: session.float_mode,
        q32: &session.options.config.q32,
    };
    let lowered = crate::lower::lower_ops(func_opt, ir, &session.abi, &lower_opts)
        .map_err(NativeError::Lower)?;
    log::debug!(
        "[native-fa] compile_function: lowered {} to {} vinsts",
        state.name,
        lowered.vinsts.len()
    );
    state.lowered = Some(lowered);
    Ok(())
}

pub(crate) fn compile_function_peephole(
    state: &mut function_job::FunctionCompileState,
) -> Result<(), NativeError> {
    let Some(lowered) = state.lowered.as_mut() else {
        return Err(NativeError::Internal(format!(
            "peephole stage missing lowered function for {}",
            state.name
        )));
    };
    crate::opt::fold_immediates(lowered);
    Ok(())
}

pub(crate) fn compile_function_regalloc_stage(
    state: &mut function_job::FunctionCompileState,
    _session: &CompileSession,
) -> Result<(), NativeError> {
    let Some(lowered) = state.lowered.as_ref() else {
        return Err(NativeError::Internal(format!(
            "regalloc stage missing lowered function for {}",
            state.name
        )));
    };
    let alloc_result =
        crate::regalloc::allocate(lowered, &state.func_abi).map_err(NativeError::RegAlloc)?;
    state.alloc_result = Some(alloc_result);
    Ok(())
}

pub(crate) fn compile_function_emit_stage(
    state: &mut function_job::FunctionCompileState,
    session: &CompileSession,
) -> Result<(), NativeError> {
    let Some(lowered) = state.lowered.as_ref() else {
        return Err(NativeError::Internal(format!(
            "emit stage missing lowered function for {}",
            state.name
        )));
    };
    let Some(alloc_result) = state.alloc_result.take() else {
        return Err(NativeError::Internal(format!(
            "emit stage missing allocation result for {}",
            state.name
        )));
    };
    let emitted = crate::emit::emit_lowered_with_alloc(
        lowered,
        &state.func_abi,
        alloc_result,
        session.abi.max_callee_sret_bytes(),
        session.options.debug_info,
    )?;
    log::debug!(
        "[native-fa] compile_function: emitted {} bytes for {}",
        emitted.code.len(),
        state.name
    );
    state.emitted = Some(emitted);
    Ok(())
}

pub(crate) fn compile_function_debug_sections(
    state: &mut function_job::FunctionCompileState,
    ir: &LpirModule,
    session: &CompileSession,
) -> Result<(), NativeError> {
    let Some(emitted) = state.emitted.as_ref() else {
        return Err(NativeError::Internal(format!(
            "debug stage missing emitted code for {}",
            state.name
        )));
    };
    let (debug_lines, debug_info) = if session.options.debug_info {
        let Some(func_opt) = state.optimized.as_ref() else {
            return Err(NativeError::Internal(format!(
                "debug stage missing optimized function for {}",
                state.name
            )));
        };
        let Some(lowered) = state.lowered.as_ref() else {
            return Err(NativeError::Internal(format!(
                "debug stage missing lowered function for {}",
                state.name
            )));
        };
        let sections = crate::debug::sections::build_debug_sections(
            func_opt,
            ir,
            lowered,
            &emitted.code,
            &emitted.alloc_output,
            &state.func_abi,
            &lowered.symbols,
        );
        let debug_info = FunctionDebugInfo::new(&state.name)
            .with_inst_count(emitted.code.len() / 4)
            .with_sections(sections);
        (Some(emitted.debug_lines.clone()), Some(debug_info))
    } else {
        (None, None)
    };
    state.compiled = Some(CompiledFunction {
        name: state.name.clone(),
        code: emitted.code.clone(),
        relocs: emitted.relocs.clone(),
        debug_lines,
        debug_info,
    });
    Ok(())
}

pub(crate) fn compile_function_finalize(
    state: &mut function_job::FunctionCompileState,
) -> Result<CompiledFunction, NativeError> {
    state.compiled.take().ok_or_else(|| {
        NativeError::Internal(format!(
            "finalize stage missing compiled output for {}",
            state.name
        ))
    })
}

/// Compile one function: LPIR → (const fold) → VInst → (imm fold) → AllocOutput → bytes.
pub fn compile_function(
    session: &mut CompileSession,
    func: &IrFunction,
    ir: &LpirModule,
    fn_sig: &LpsFnSig,
) -> Result<CompiledFunction, NativeError> {
    log::debug!(
        "[native-fa] compile_function: lowering {name} ({ops} ops)",
        name = func.name,
        ops = func.body.len(),
    );

    let func_abi = compile_function_func_abi(session, func, fn_sig);
    let mut state = function_job::FunctionCompileState::new(0, func.clone(), func_abi);
    compile_function_fold_constants(&mut state, session);
    compile_function_lower_stage(&mut state, ir, session)?;
    compile_function_peephole(&mut state)?;
    compile_function_regalloc_stage(&mut state, session)?;
    compile_function_emit_stage(&mut state, session)?;
    compile_function_debug_sections(&mut state, ir, session)?;
    compile_function_finalize(&mut state)
}

/// Compile all functions in a module.
pub fn compile_module(
    ir: &LpirModule,
    sig: &lps_shared::LpsModuleSig,
    float_mode: FloatMode,
    options: crate::native_options::NativeCompileOptions,
    isa: IsaTarget,
) -> Result<CompiledModule, NativeError> {
    let mut job = NativeCompileJob::new(ir.clone(), sig.clone(), float_mode, options, isa);
    loop {
        match job.step(NativeCompileBudget::default()) {
            NativeCompileStepResult::Pending => {}
            NativeCompileStepResult::Finished(module) => return Ok(module),
            NativeCompileStepResult::Failed(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use alloc::vec;

    use lpir::{FuncId, IrFunction, IrType, LpirModule, LpirOp, VReg, types::VRegRange};
    use lps_shared::{LpsFnKind, LpsFnSig, LpsModuleSig, LpsType};

    #[test]
    fn test_compile_session_new() {
        let abi = ModuleAbi::from_ir_and_sig(
            IsaTarget::Rv32imac,
            &LpirModule {
                imports: vec![],
                functions: Default::default(),
            },
            &LpsModuleSig::default(),
        );
        let session = CompileSession::new(
            abi,
            IsaTarget::Rv32imac,
            lpir::FloatMode::Q32,
            Default::default(),
        );
        assert!(session.symbols.names.is_empty());
    }

    #[test]
    fn test_compile_module_empty() {
        let ir = LpirModule {
            imports: vec![],
            functions: BTreeMap::new(),
        };
        let sig = LpsModuleSig::default();
        let result = compile_module(
            &ir,
            &sig,
            lpir::FloatMode::Q32,
            Default::default(),
            IsaTarget::Rv32imac,
        );
        // Should succeed with no functions
        let compiled = result.unwrap();
        assert!(compiled.functions.is_empty());
    }

    #[test]
    fn test_compile_simple_iconst() {
        let ir = LpirModule {
            imports: vec![],
            functions: BTreeMap::from([(
                FuncId(0),
                IrFunction {
                    name: String::from("test"),
                    is_entry: true,
                    vmctx_vreg: VReg(0),
                    param_count: 0,
                    return_types: vec![IrType::I32],
                    sret_arg: None,
                    vreg_types: vec![IrType::I32],
                    slots: vec![],
                    body: vec![
                        LpirOp::IconstI32 {
                            dst: VReg(0),
                            value: 42,
                        },
                        LpirOp::Return {
                            values: VRegRange { start: 0, count: 1 },
                        },
                    ]
                    .into(),
                    vreg_pool: vec![VReg(0)],
                },
            )]),
        };
        let sig = LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("test"),
                return_type: LpsType::Int,
                parameters: vec![],
                kind: LpsFnKind::UserDefined,
            }],
            ..Default::default()
        };
        let result = compile_module(
            &ir,
            &sig,
            lpir::FloatMode::Q32,
            Default::default(),
            IsaTarget::Rv32imac,
        );
        assert!(
            result.is_ok(),
            "expected successful compilation, got: {result:?}",
        );
        let module = result.unwrap();
        assert_eq!(module.functions.len(), 1, "expected 1 compiled function");
    }

    /// Phase 0 regression: [`crate::native_options::NativeCompileOptions::config`] (e.g. Q32 mul
    /// mode) must reach [`compile_module`]. `rt_jit::compile_module_jit` forwards the same
    /// struct into here; if it rebuilt options from defaults, only float_mode would apply.
    #[test]
    fn compile_module_respects_q32_mul_mode_in_emitted_code() {
        use lps_q32::q32_options::{MulMode, Q32Options};
        use lps_shared::{FnParam, ParamQualifier};

        let func = IrFunction {
            name: String::from("q32_fmul"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 2,
            return_types: vec![IrType::F32],
            sret_arg: None,
            vreg_types: vec![IrType::Pointer, IrType::F32, IrType::F32, IrType::F32],
            slots: vec![],
            body: vec![
                LpirOp::Fmul {
                    dst: VReg(3),
                    lhs: VReg(1),
                    rhs: VReg(2),
                },
                LpirOp::Return {
                    values: VRegRange { start: 0, count: 1 },
                },
            ]
            .into(),
            vreg_pool: vec![VReg(3)],
        };
        let ir = LpirModule {
            imports: vec![],
            functions: BTreeMap::from([(FuncId(0), func)]),
        };
        let sig = LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("q32_fmul"),
                return_type: LpsType::Float,
                parameters: vec![
                    FnParam {
                        name: String::from("a"),
                        ty: LpsType::Float,
                        qualifier: ParamQualifier::In,
                    },
                    FnParam {
                        name: String::from("b"),
                        ty: LpsType::Float,
                        qualifier: ParamQualifier::In,
                    },
                ],
                kind: LpsFnKind::UserDefined,
            }],
            ..Default::default()
        };

        let mut opts_sat = crate::native_options::NativeCompileOptions::default();
        opts_sat.float_mode = lpir::FloatMode::Q32;
        opts_sat.config.q32.mul = MulMode::Saturating;

        let mut opts_wrap = crate::native_options::NativeCompileOptions::default();
        opts_wrap.float_mode = lpir::FloatMode::Q32;
        opts_wrap.config.q32 = Q32Options {
            mul: MulMode::Wrapping,
            ..Default::default()
        };

        let sat = compile_module(
            &ir,
            &sig,
            lpir::FloatMode::Q32,
            opts_sat,
            IsaTarget::Rv32imac,
        )
        .expect("saturating mul compile");
        let wrap = compile_module(
            &ir,
            &sig,
            lpir::FloatMode::Q32,
            opts_wrap,
            IsaTarget::Rv32imac,
        )
        .expect("wrapping mul compile");

        assert_ne!(
            sat.functions[0].code, wrap.functions[0].code,
            "saturating fmul lowers to a builtin call; wrapping uses inline mul/mulh — code must differ"
        );
    }

    fn simple_iconst_module() -> (LpirModule, LpsModuleSig) {
        let ir = LpirModule {
            imports: vec![],
            functions: BTreeMap::from([(
                FuncId(0),
                IrFunction {
                    name: String::from("test"),
                    is_entry: true,
                    vmctx_vreg: VReg(0),
                    param_count: 0,
                    return_types: vec![IrType::I32],
                    sret_arg: None,
                    vreg_types: vec![IrType::I32],
                    slots: vec![],
                    body: vec![
                        LpirOp::IconstI32 {
                            dst: VReg(0),
                            value: 42,
                        },
                        LpirOp::Return {
                            values: VRegRange { start: 0, count: 1 },
                        },
                    ]
                    .into(),
                    vreg_pool: vec![VReg(0)],
                },
            )]),
        };
        let sig = LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("test"),
                return_type: LpsType::Int,
                parameters: vec![],
                kind: LpsFnKind::UserDefined,
            }],
            ..Default::default()
        };
        (ir, sig)
    }

    #[test]
    fn native_compile_job_single_step_reaches_finished_module() {
        let (ir, sig) = simple_iconst_module();
        let mut job = NativeCompileJob::new(
            ir.clone(),
            sig.clone(),
            lpir::FloatMode::Q32,
            Default::default(),
            IsaTarget::Rv32imac,
        );
        let mut seen = Vec::new();
        loop {
            seen.push(job.stage());
            match job.step(NativeCompileBudget::single_step()) {
                NativeCompileStepResult::Pending => {}
                NativeCompileStepResult::Finished(module) => {
                    assert_eq!(module.functions.len(), 1);
                    break;
                }
                NativeCompileStepResult::Failed(err) => {
                    panic!("compile job failed unexpectedly: {err}");
                }
            }
        }
        assert_eq!(
            seen,
            vec![
                NativeCompileStage::SetupModule,
                NativeCompileStage::CompileFunctionConstFold,
                NativeCompileStage::CompileFunctionLower,
                NativeCompileStage::CompileFunctionPeephole,
                NativeCompileStage::CompileFunctionRegalloc,
                NativeCompileStage::CompileFunctionEmit,
                NativeCompileStage::CompileFunctionDebug,
                NativeCompileStage::AssembleModule,
            ]
        );
    }

    #[test]
    fn native_compile_job_matches_direct_compile_function_output() {
        let (ir, sig) = simple_iconst_module();
        let func = ir.functions.values().next().expect("one function");
        let module_abi = ModuleAbi::from_ir_and_sig(IsaTarget::Rv32imac, &ir, &sig);
        let mut session = CompileSession::new(
            module_abi,
            IsaTarget::Rv32imac,
            lpir::FloatMode::Q32,
            Default::default(),
        );
        let direct = compile_function(&mut session, func, &ir, &sig.functions[0])
            .expect("direct compile_function");

        let mut job = NativeCompileJob::new(
            ir.clone(),
            sig.clone(),
            lpir::FloatMode::Q32,
            Default::default(),
            IsaTarget::Rv32imac,
        );
        let stepped = loop {
            match job.step(NativeCompileBudget::single_step()) {
                NativeCompileStepResult::Pending => {}
                NativeCompileStepResult::Finished(module) => break module,
                NativeCompileStepResult::Failed(err) => {
                    panic!("compile job failed unexpectedly: {err}");
                }
            }
        };
        let stepped_fn = &stepped.functions[0];
        assert_eq!(stepped_fn.name, direct.name);
        assert_eq!(stepped_fn.code, direct.code);
        assert_eq!(stepped_fn.relocs.len(), direct.relocs.len());
        assert_eq!(stepped_fn.debug_lines, direct.debug_lines);
        match (&stepped_fn.debug_info, &direct.debug_info) {
            (Some(stepped), Some(direct)) => {
                assert_eq!(stepped.inst_count, direct.inst_count);
                assert_eq!(stepped.sections, direct.sections);
            }
            (None, None) => {}
            (lhs, rhs) => panic!("debug_info mismatch: left={lhs:?} right={rhs:?}"),
        }
    }
}
