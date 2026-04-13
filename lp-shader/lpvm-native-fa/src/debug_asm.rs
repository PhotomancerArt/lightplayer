//! GLSL/LPIR → annotated RV32 assembly text (host debugging).

use alloc::format;
use alloc::string::String;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::compile::compile_module;
use crate::error::NativeError;
use crate::rv32::debug::LineTable;
use crate::rv32::debug::disasm::{DisasmOptions, disassemble_function};

/// Emit annotated assembly for every function in `ir` (concatenated).
///
/// # Arguments
/// * `ir` - The LPIR module to emit
/// * `sig` - Module signatures containing function metadata
/// * `float_mode` - Floating point mode
/// * `opts` - Disassembly options
/// * `alloc_trace` - When true, print linear-scan allocation trace to stderr for each function (TODO)
pub fn compile_module_asm_text(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    float_mode: lpir::FloatMode,
    opts: DisasmOptions,
    _alloc_trace: bool,
) -> Result<String, NativeError> {
    let options = crate::native_options::NativeCompileOptions {
        float_mode,
        debug_info: true,
        emu_trace_instructions: false,
        alloc_trace: false,
    };

    // Compile module
    let compiled = compile_module(ir, sig, float_mode, options)?;

    // Build a map from function name to LPIR function
    let mut out = String::new();

    for func in &ir.functions {
        // Find the compiled function
        let compiled_func = compiled
            .functions
            .iter()
            .find(|f| f.name == func.name)
            .ok_or_else(|| {
                NativeError::Internal(format!("compiled function {} not found", func.name))
            })?;

        // Build line table from debug_lines
        let table = LineTable::from_debug_lines(&compiled_func.debug_lines);

        // Disassemble function
        let asm = disassemble_function(&compiled_func.code, &table, func, opts);
        out.push_str(&asm);
        out.push('\n');
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use lpir::types::VRegRange;
    use lpir::{IrFunction, IrType, LpirModule, LpirOp, VReg};
    use lps_shared::{FnParam, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};

    use super::*;

    #[test]
    fn compile_module_asm_contains_lpir() {
        let ir = LpirModule {
            imports: vec![],
            functions: vec![IrFunction {
                name: String::from("add"),
                is_entry: true,
                vmctx_vreg: VReg(0),
                param_count: 2,
                return_types: vec![IrType::I32],
                vreg_types: vec![IrType::I32, IrType::I32, IrType::I32, IrType::I32],
                slots: vec![],
                body: vec![
                    LpirOp::Iadd {
                        dst: VReg(3),
                        lhs: VReg(1),
                        rhs: VReg(2),
                    },
                    LpirOp::Return {
                        values: VRegRange { start: 0, count: 1 },
                    },
                ],
                vreg_pool: vec![VReg(3)],
            }],
        };
        let sig = LpsModuleSig {
            functions: vec![LpsFnSig {
                name: String::from("add"),
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
            }],
        };

        // M1: allocator returns NotImplemented, so compilation fails
        let result = compile_module_asm_text(
            &ir,
            &sig,
            lpir::FloatMode::Q32,
            DisasmOptions::default(),
            false,
        );
        assert!(
            matches!(
                result,
                Err(NativeError::FastAlloc(crate::fa_alloc::AllocError::NotImplemented))
            ),
            "M1: expected NotImplemented error, got: {:?}",
            result
        );
    }
}
