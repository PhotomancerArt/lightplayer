//! Compilation orchestration: LPIR → VInst → PInst → bytes.

use alloc::collections::BTreeMap;
use alloc::format;
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

/// Compile one function: LPIR → VInst → (peephole) → AllocOutput → bytes.
pub fn compile_function(
    session: &mut CompileSession,
    func: &IrFunction,
    ir: &LpirModule,
    fn_sig: &LpsFnSig,
) -> Result<CompiledFunction, NativeError> {
    // 1. Lower LPIR → VInst
    let lowered = crate::lower::lower_ops(func, ir, &session.abi, session.float_mode)
        .map_err(NativeError::Lower)?;

    // 2. Build function ABI
    let func_abi = crate::rv32::abi::func_abi_rv32(fn_sig, func.total_param_slots() as usize);

    // 3. Allocate and emit
    let emitted =
        crate::emit::emit_lowered_ex(&lowered, &func_abi, session.abi.max_callee_sret_bytes())?;

    let code = emitted.code;
    let relocs = emitted.relocs;

    // 4. Build structured debug info
    let mut sections = BTreeMap::new();

    // Interleaved LPIR + VInst + allocations
    let interleaved = crate::fa_alloc::render::render_interleaved(
        func,
        ir,
        &lowered.vinsts,
        &lowered.vreg_pool,
        &emitted.alloc_output,
        &func_abi,
        &lowered.symbols,
    );
    sections.insert("interleaved".into(), interleaved);

    // Disasm section with hex
    let mut disasm = String::new();
    let mut off = 0usize;
    while off + 4 <= code.len() {
        let w = u32::from_le_bytes(code[off..off + 4].try_into().expect("4 bytes"));
        disasm.push_str(&format!(
            "{:04x}\t{:08x}\t{}\n",
            off,
            w,
            lp_riscv_inst::format_instruction(w)
        ));
        off += 4;
    }
    sections.insert("disasm".into(), disasm);

    // Optional: VInst listing
    let mut vinst_text = String::new();
    for inst in &lowered.vinsts {
        vinst_text.push_str(&format!(
            "{} {}\n",
            inst.mnemonic(),
            inst.format_alloc_trace_detail(&lowered.vreg_pool, &lowered.symbols)
        ));
    }
    sections.insert("vinst".into(), vinst_text);

    let debug_info = FunctionDebugInfo::new(&func.name)
        .with_inst_count(code.len() / 4)
        .with_sections(sections);

    Ok(CompiledFunction {
        name: func.name.clone(),
        code,
        relocs,
        debug_lines: emitted.debug_lines,
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
