//! Compilation orchestration: LPIR → VInst → PInst → bytes.

use alloc::string::String;
use alloc::vec::Vec;

use lpir::{FloatMode, IrFunction, LpirModule};
use lps_shared::LpsFnSig;

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

/// Compile one function: LPIR → VInst → (peephole) → PInst → bytes.
pub fn compile_function(
    session: &mut CompileSession,
    func: &IrFunction,
    ir: &LpirModule,
    fn_sig: &LpsFnSig,
) -> Result<CompiledFunction, NativeError> {
    // 1. Lower LPIR → VInst
    let mut lowered = crate::lower::lower_ops(func, ir, &session.abi, session.float_mode)
        .map_err(NativeError::Lower)?;

    // 2. Peephole optimize
    crate::peephole::optimize(&mut lowered.vinsts);

    // 3. Allocate registers (fastalloc)
    let func_abi = crate::rv32::abi::func_abi_rv32(fn_sig, func.total_param_slots() as usize);
    let pinsts = crate::rv32::alloc::allocate(&lowered.vinsts, &func_abi, func, &lowered.vreg_pool)
        .map_err(NativeError::FastAlloc)?;

    // 4. Emit PInst → bytes
    let mut emitter = crate::rv32::rv32_emit::Rv32Emitter::new();
    for p in &pinsts {
        emitter.emit(p);
    }
    let (code, phys_relocs) = emitter.finish_with_relocs();

    // Convert PhysReloc → NativeReloc
    let relocs = phys_relocs
        .into_iter()
        .map(|r| NativeReloc {
            offset: r.offset,
            symbol: r.symbol,
        })
        .collect();

    // lowered + pinsts + func_abi dropped here
    Ok(CompiledFunction {
        name: func.name.clone(),
        code,
        relocs,
        debug_lines: Vec::new(), // TODO: wire up debug_lines from lowered.vinsts src_op mapping
    })
}

/// Compile all functions in a module.
pub fn compile_module(
    ir: &LpirModule,
    sig: &lps_shared::LpsModuleSig,
    float_mode: FloatMode,
    options: crate::native_options::NativeCompileOptions,
) -> Result<CompiledModule, NativeError> {
    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);
    let mut session = CompileSession::new(module_abi, float_mode, options);

    let sig_map: alloc::collections::BTreeMap<&str, &LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut functions = Vec::with_capacity(ir.functions.len());
    for func in &ir.functions {
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
    }

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
            &LpirModule { imports: vec![], functions: vec![] },
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
        assert!(result.is_ok(), "compile failed: {:?}", result.err());
        let compiled = result.unwrap();
        assert_eq!(compiled.functions.len(), 1);
        assert!(!compiled.functions[0].code.is_empty());
    }
}
