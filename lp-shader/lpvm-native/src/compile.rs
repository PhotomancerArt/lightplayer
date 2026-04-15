//! Compilation orchestration: LPIR → VInst → machine code.

use alloc::string::String;
use alloc::vec::Vec;

use lpir::{FloatMode, IrFunction, LpirModule};
use lps_shared::LpsFnSig;
use lpvm::FunctionDebugInfo;

use crate::abi::ModuleAbi;
use crate::error::NativeError;
use crate::vinst::ModuleSymbols;

/// Relocation entry for a call site.
#[derive(Clone, Debug)]
pub struct NativeReloc {
    /// Byte offset within the function's code where the auipc instruction is.
    pub offset: usize,
    /// Symbol name to resolve (builtin or function).
    pub symbol: String,
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
    pub debug_lines: Vec<(u32, Option<u32>)>,
    /// Structured debug info with sections.
    pub debug_info: FunctionDebugInfo,
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
    /// Floating point mode.
    pub float_mode: FloatMode,
    /// Compilation options.
    pub options: crate::native_options::NativeCompileOptions,
}

impl CompileSession {
    /// Create a new compile session for a module.
    pub fn new(
        abi: ModuleAbi,
        float_mode: FloatMode,
        options: crate::native_options::NativeCompileOptions,
    ) -> Self {
        Self {
            symbols: ModuleSymbols::default(),
            abi,
            float_mode,
            options,
        }
    }
}

/// Compile one function: LPIR → (const fold) → VInst → (imm fold) → AllocOutput → bytes.
pub fn compile_function(
    session: &mut CompileSession,
    func: &IrFunction,
    ir: &LpirModule,
    fn_sig: &LpsFnSig,
) -> Result<CompiledFunction, NativeError> {
    log::debug!(
        "[native-fa] compile_function: lowering {} ({} ops)",
        func.name,
        func.body.len()
    );

    // Build function ABI (needed for both debug and non-debug paths)
    let func_abi = crate::rv32::abi::func_abi_rv32(fn_sig, func.total_param_slots() as usize);

    // 1-4. Const-fold, lower, optimize, allocate, emit
    let (code, relocs, debug_lines, sections) = {
        let mut func_opt = func.clone();
        let n_folded = lpir::const_fold::fold_constants(&mut func_opt);
        if n_folded > 0 {
            log::debug!(
                "[native-fa] compile_function: folded {} LPIR constants",
                n_folded
            );
        }

        let mut lowered = crate::lower::lower_ops(&func_opt, ir, &session.abi, session.float_mode)
            .map_err(NativeError::Lower)?;
        log::debug!(
            "[native-fa] compile_function: lowered to {} vinsts",
            lowered.vinsts.len()
        );

        crate::opt::fold_immediates(&mut lowered);

        log::debug!("[native-fa] compile_function: emitting code...");
        let emitted =
            crate::emit::emit_lowered_ex(&lowered, &func_abi, session.abi.max_callee_sret_bytes())?;
        log::debug!(
            "[native-fa] compile_function: emitted {} bytes",
            emitted.code.len()
        );

        let code = emitted.code;
        let relocs = emitted.relocs;
        let debug_lines = emitted.debug_lines;

        let sections = crate::debug::sections::build_debug_sections(
            &func_opt,
            ir,
            &lowered,
            &code,
            &emitted.alloc_output,
            &func_abi,
            &lowered.symbols,
        );

        (code, relocs, debug_lines, sections)
    };

    let debug_info = FunctionDebugInfo::new(&func.name)
        .with_inst_count(code.len() / 4)
        .with_sections(sections);

    Ok(CompiledFunction {
        name: func.name.clone(),
        code,
        relocs,
        debug_lines,
        debug_info,
    })
}

/// Compile all functions in a module.
pub fn compile_module(
    ir: &LpirModule,
    sig: &lps_shared::LpsModuleSig,
    float_mode: FloatMode,
    options: crate::native_options::NativeCompileOptions,
) -> Result<CompiledModule, NativeError> {
    log::debug!(
        "[native-fa] compile_module: building ABI for {} functions",
        ir.functions.len()
    );
    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);
    let mut session = CompileSession::new(module_abi, float_mode, options);

    let sig_map: alloc::collections::BTreeMap<&str, &LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut functions = Vec::with_capacity(ir.functions.len());
    for (idx, func) in ir.functions.iter().enumerate() {
        log::debug!(
            "[native-fa] compile_module: compiling function {}/{}: {}",
            idx + 1,
            ir.functions.len(),
            func.name
        );
        let default_sig = LpsFnSig {
            name: func.name.clone(),
            return_type: lps_shared::LpsType::Void,
            parameters: Vec::new(),
        };
        let fn_sig = sig_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or(&default_sig);
        let compiled = compile_function(&mut session, func, ir, fn_sig)?;
        functions.push(compiled);
        log::debug!(
            "[native-fa] compile_module: function {} complete",
            func.name
        );
    }

    log::debug!(
        "[native-fa] compile_module: all {} functions compiled",
        functions.len()
    );
    Ok(CompiledModule {
        functions,
        symbols: session.symbols,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use lpir::{IrFunction, IrType, LpirModule, LpirOp, VReg, types::VRegRange};
    use lps_shared::{LpsFnSig, LpsModuleSig, LpsType};

    #[test]
    fn test_compile_session_new() {
        let abi = ModuleAbi::from_ir_and_sig(
            &LpirModule {
                imports: vec![],
                functions: vec![],
            },
            &LpsModuleSig { functions: vec![] },
        );
        let session = CompileSession::new(abi, lpir::FloatMode::Q32, Default::default());
        assert!(session.symbols.names.is_empty());
    }

    #[test]
    fn test_compile_module_empty() {
        let ir = LpirModule {
            imports: vec![],
            functions: vec![],
        };
        let sig = LpsModuleSig { functions: vec![] };
        let result = compile_module(&ir, &sig, lpir::FloatMode::Q32, Default::default());
        // Should succeed with no functions
        let compiled = result.unwrap();
        assert!(compiled.functions.is_empty());
    }

    #[test]
    fn test_compile_simple_iconst() {
        let ir = LpirModule {
            imports: vec![],
            functions: vec![IrFunction {
                name: String::from("test"),
                is_entry: true,
                vmctx_vreg: VReg(0),
                param_count: 0,
                return_types: vec![IrType::I32],
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
                ],
                vreg_pool: vec![VReg(0)],
            }],
        };
        let sig = LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("test"),
                return_type: LpsType::Int,
                parameters: vec![],
            }],
        };
        let result = compile_module(&ir, &sig, lpir::FloatMode::Q32, Default::default());
        assert!(
            result.is_ok(),
            "expected successful compilation, got: {:?}",
            result
        );
        let module = result.unwrap();
        assert_eq!(module.functions.len(), 1, "expected 1 compiled function");
    }
}
