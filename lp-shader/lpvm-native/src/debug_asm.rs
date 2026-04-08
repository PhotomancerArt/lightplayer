//! GLSL/LPIR → annotated RV32 assembly text (host debugging).

use alloc::string::String;

use lpir::IrModule;

use crate::error::NativeError;
use crate::isa::rv32::debug::LineTable;
use crate::isa::rv32::debug::disasm::{DisasmOptions, disassemble_function};
use crate::isa::rv32::emit::emit_function_bytes;

/// Emit annotated assembly for every function in `ir` (concatenated).
pub fn compile_module_asm_text(
    ir: &IrModule,
    float_mode: lpir::FloatMode,
    opts: DisasmOptions,
) -> Result<String, NativeError> {
    let mut out = String::new();
    for func in &ir.functions {
        let emitted = emit_function_bytes(func, float_mode, true)?;
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
        let s = compile_module_asm_text(&ir, lpir::FloatMode::Q32, DisasmOptions::default())
            .expect("asm");
        assert!(s.contains(".globl\tadd"));
        assert!(s.contains("LPIR[0]:"));
        assert!(s.contains("iadd"));
    }
}
