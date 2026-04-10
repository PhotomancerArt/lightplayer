//! GLSL/LPIR → annotated RV32 assembly text (host debugging).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::IrModule;
use lps_shared::{LpsFnSig, LpsModuleSig, LpsType};

use crate::abi::ModuleAbi;
use crate::error::NativeError;
use crate::isa::rv32::debug::LineTable;
use crate::isa::rv32::debug::disasm::{DisasmOptions, disassemble_function};
use crate::isa::rv32::emit::emit_function_bytes;

/// Emit annotated assembly for every function in `ir` (concatenated).
///
/// # Arguments
/// * `ir` - The LPIR module to emit
/// * `sig` - Module signatures containing function metadata
/// * `float_mode` - Floating point mode
/// * `opts` - Disassembly options
/// * `alloc_trace` - When true, print linear-scan allocation trace to stderr for each function
pub fn compile_module_asm_text(
    ir: &IrModule,
    sig: &LpsModuleSig,
    float_mode: lpir::FloatMode,
    opts: DisasmOptions,
    alloc_trace: bool,
) -> Result<String, NativeError> {
    // Build a map from function name to signature
    let sig_map: BTreeMap<&str, &LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);

    let mut out = String::new();
    for func in &ir.functions {
        // Get signature or use default (void -> void)
        let default_sig = LpsFnSig {
            name: func.name.clone(),
            return_type: LpsType::Void,
            parameters: Vec::new(),
        };
        let fn_sig = sig_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or(&default_sig);
        let emitted =
            emit_function_bytes(func, ir, &module_abi, fn_sig, float_mode, true, alloc_trace)?;
        let table = LineTable::from_debug_lines(&emitted.debug_lines);
        out.push_str(&disassemble_function(&emitted.code, &table, func, opts));
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use lpir::types::VRegRange;
    use lpir::{IrFunction, IrModule, IrType, Op, VReg};
    use lps_shared::{FnParam, LpsFnSig, LpsModuleSig, ParamQualifier};

    use super::*;
    use crate::isa::rv32::debug::disasm::DisasmOptions;

    #[test]
    fn compile_module_asm_contains_lpir() {
        let ir = IrModule {
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
                    Op::Iadd {
                        dst: VReg(3),
                        lhs: VReg(1),
                        rhs: VReg(2),
                    },
                    Op::Return {
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
        let s = compile_module_asm_text(
            &ir,
            &sig,
            lpir::FloatMode::Q32,
            DisasmOptions::default(),
            false,
        )
        .expect("asm");
        assert!(s.contains(".globl\tadd"));
        assert!(s.contains("(0)"));
        assert!(s.contains("iadd"));
    }
}
