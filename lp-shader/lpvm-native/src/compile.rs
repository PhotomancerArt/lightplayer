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
    /// ELF / JIT relocation type (see [`crate::isa::IsaTarget::call_reloc_type`]).
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
        #[cfg(feature = "isa-rv32")]
        IsaTarget::Rv32imac => crate::isa::rv32::abi::func_abi_rv32(fn_sig, Some(func)),
        #[cfg(feature = "isa-xt")]
        IsaTarget::Xtensa => crate::isa::xt::abi::func_abi_xt(fn_sig, Some(func)),
    }
}

/// Const-fold a function in place. In the job path `func` is the function
/// inside the job's own `LpirModule`; folding never changes signatures, so
/// later lowering of other functions (which reads callee names only) is
/// unaffected.
pub(crate) fn compile_function_fold_constants(func: &mut IrFunction) {
    let n_folded = lpir::const_fold::fold_constants(func);
    if n_folded > 0 {
        log::debug!(
            "[native-fa] compile_function: folded {n_folded} LPIR constants for {}",
            func.name
        );
    }
}

pub(crate) fn compile_function_lower_stage(
    state: &mut function_job::FunctionCompileState,
    func: &IrFunction,
    ir: &LpirModule,
    session: &CompileSession,
) -> Result<(), NativeError> {
    let lower_opts = LowerOpts {
        float_mode: session.float_mode,
        fuel: session.options.fuel,
    };
    let lowered =
        crate::lower::lower_ops(func, ir, &session.abi, &lower_opts).map_err(NativeError::Lower)?;
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
    let isa = state.func_abi.isa();
    let Some(lowered) = state.lowered.as_mut() else {
        return Err(NativeError::Internal(format!(
            "peephole stage missing lowered function for {}",
            state.name
        )));
    };
    crate::opt::fold_immediates(lowered, isa);
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
    func: &IrFunction,
    ir: &LpirModule,
    session: &CompileSession,
) -> Result<(), NativeError> {
    // Take ownership: nothing reads `state.emitted` after this stage, so the
    // machine code and relocs move into the CompiledFunction instead of being
    // cloned (previously a transient 2x of every function's code).
    let Some(emitted) = state.emitted.take() else {
        return Err(NativeError::Internal(format!(
            "debug stage missing emitted code for {}",
            state.name
        )));
    };
    let (debug_lines, debug_info) = if session.options.debug_info {
        let Some(lowered) = state.lowered.as_ref() else {
            return Err(NativeError::Internal(format!(
                "debug stage missing lowered function for {}",
                state.name
            )));
        };
        let sections = crate::debug::sections::build_debug_sections(
            func,
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
        (Some(emitted.debug_lines), Some(debug_info))
    } else {
        (None, None)
    };
    state.compiled = Some(CompiledFunction {
        name: state.name.clone(),
        code: emitted.code,
        relocs: emitted.relocs,
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
    // Standalone path (host/tests): the caller's module is borrowed, so fold
    // a local copy. The job path folds in place inside its owned module.
    let mut folded = func.clone();
    compile_function_fold_constants(&mut folded);
    let mut state =
        function_job::FunctionCompileState::new(0, lpir::FuncId(0), folded.name.clone(), func_abi);
    compile_function_lower_stage(&mut state, &folded, ir, session)?;
    compile_function_peephole(&mut state)?;
    compile_function_regalloc_stage(&mut state, session)?;
    compile_function_emit_stage(&mut state, session)?;
    compile_function_debug_sections(&mut state, &folded, ir, session)?;
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
    use alloc::string::String;
    use alloc::vec;
    use lp_collection::VecMap;

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
            functions: VecMap::new(),
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
            functions: VecMap::from([(
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

    /// Phase 0 regression: the caller's [`crate::native_options::NativeCompileOptions`] must
    /// reach [`compile_module`] *whole*, not just its `float_mode`.
    /// `rt_jit::compile_module_jit` forwards the same struct into here; if it rebuilt options
    /// from defaults, every other field would silently revert to its default value.
    ///
    /// Keyed on `fuel` because it is the only field outside `float_mode` whose value is visible
    /// in emitted code. `options.config` ([`lpir::CompilerConfig`]) is not a candidate: its
    /// `texture.texel_fetch_bounds` is consumed by the GLSL frontend while lowering to LPIR, so
    /// `compile_module` only ever sees the already-clamped (or already-unclamped) ops, and its
    /// `inline.*` keys have no consumer anywhere — the inliner never merged. Extend this test to
    /// cover `config` the moment one of its keys grows a backend consumer.
    #[test]
    fn compile_module_respects_non_float_mode_options_in_emitted_code() {
        let (ir, sig) = simple_iconst_module();

        let opts_fuel = crate::native_options::NativeCompileOptions {
            fuel: true,
            ..Default::default()
        };
        let opts_no_fuel = crate::native_options::NativeCompileOptions {
            fuel: false,
            ..Default::default()
        };

        let fueled = compile_module(
            &ir,
            &sig,
            lpir::FloatMode::Q32,
            opts_fuel,
            IsaTarget::Rv32imac,
        )
        .expect("fuel: true compile");
        let unfueled = compile_module(
            &ir,
            &sig,
            lpir::FloatMode::Q32,
            opts_no_fuel,
            IsaTarget::Rv32imac,
        )
        .expect("fuel: false compile");

        assert_ne!(
            fueled.functions[0].code, unfueled.functions[0].code,
            "fuel: true adds a function-entry fuel check — code must differ",
        );
        assert!(
            fueled.functions[0].code.len() > unfueled.functions[0].code.len(),
            "the fuel check is extra instructions, so the fueled function must be longer \
             (fuel: true {} bytes, fuel: false {} bytes)",
            fueled.functions[0].code.len(),
            unfueled.functions[0].code.len(),
        );
    }

    fn simple_iconst_module() -> (LpirModule, LpsModuleSig) {
        let ir = LpirModule {
            imports: vec![],
            functions: VecMap::from([(
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
