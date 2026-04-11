//! Annotated RV32 disassembly using [`lp_riscv_inst::format_instruction`].

use alloc::format;
use alloc::string::String;

use lp_riscv_inst::format_instruction;
use lpir::{IrFunction, LpirOp, VRegRange};

use super::LineTable;

/// 1-based text column where `(` for the LPIR op index starts (monospace).
const LPIR_COMMENT_INDEX_PAREN_COL: usize = 60;

/// Options for text output.
#[derive(Clone, Copy, Debug, Default)]
pub struct DisasmOptions {
    /// Prefix each line with a 4-digit hex offset (function-local).
    pub show_hex_offset: bool,
}

fn format_lpir_op(op: &LpirOp, func: &IrFunction) -> String {
    match op {
        LpirOp::Iadd { dst, lhs, rhs } => {
            format!("v{} = iadd v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        LpirOp::Isub { dst, lhs, rhs } => {
            format!("v{} = isub v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        LpirOp::Imul { dst, lhs, rhs } => {
            format!("v{} = imul v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        LpirOp::IconstI32 { dst, value } => format!("v{} = iconst {}", dst.0, value),
        LpirOp::Copy { dst, src } => format!("v{} = copy v{}", dst.0, src.0),
        LpirOp::Return { values } => format_return(*values, func),
        LpirOp::Fadd { dst, lhs, rhs } => {
            format!("v{} = fadd v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        LpirOp::Fsub { dst, lhs, rhs } => {
            format!("v{} = fsub v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        LpirOp::Fmul { dst, lhs, rhs } => {
            format!("v{} = fmul v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        _ => format!("{op:?}"),
    }
}

fn format_return(values: VRegRange, func: &IrFunction) -> String {
    if values.count == 0 {
        return String::from("return");
    }
    let slice = func.pool_slice(values);
    if slice.is_empty() {
        return format!("return /* pool {values:?} */");
    }
    let mut s = String::from("return ");
    for (i, v) in slice.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&format!("{v}"));
    }
    s
}

fn lpir_op_description(func: &IrFunction, src_op: u32) -> String {
    let i = src_op as usize;
    if i >= func.body.len() {
        return String::from("<out of range>");
    }
    format_lpir_op(&func.body[i], func)
}

/// Disassemble one function's code with LPIR source comments.
pub fn disassemble_function(
    code: &[u8],
    line_table: &LineTable,
    func: &IrFunction,
    opts: DisasmOptions,
) -> String {
    let mut out = String::new();
    let name = func.name.as_str();
    out.push_str(&format!("    .globl\t{name}\n"));
    out.push_str(&format!("    .type\t{name}, @function\n"));
    out.push_str(&format!("{name}:\n"));

    let mut offset = 0u32;
    let mut prev_src_op: Option<u32> = None;
    while offset as usize + 4 <= code.len() {
        let chunk = &code[offset as usize..offset as usize + 4];
        let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let asm = format_instruction(word);

        let mut prefix = String::new();
        if opts.show_hex_offset {
            prefix.push_str(&format!("{offset:04x}: "));
        } else {
            prefix.push_str("    ");
        }

        if let Some(src_op) = line_table.src_op_at_offset(offset) {
            if prev_src_op != Some(src_op) {
                let desc = lpir_op_description(func, src_op);
                out.push('\n');
                out.push_str("# ");
                out.push_str(&desc);
                let used = 2 + desc.len();
                let before_paren = LPIR_COMMENT_INDEX_PAREN_COL.saturating_sub(1);
                if used < before_paren {
                    out.extend(core::iter::repeat(' ').take(before_paren - used));
                }
                out.push_str(&format!("({src_op})\n"));
            }
            prev_src_op = Some(src_op);
            out.push_str(&format!("{prefix}{asm}\n"));
        } else {
            prev_src_op = None;
            out.push_str(&format!("{prefix}{asm}\n"));
        }

        offset += 4;
    }

    out.push_str(&format!("    .size\t{name}, .-{name}\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::rv32::debug::LineTable;
    use alloc::string::String;
    use alloc::vec;
    use lpir::types::VRegRange;
    use lpir::{IrFunction, IrType, LpirOp, VReg};

    #[test]
    fn disassemble_shows_lpir_comment() {
        let func = IrFunction {
            name: String::from("add"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 2,
            return_types: vec![IrType::I32],
            vreg_types: vec![IrType::I32; 4],
            slots: vec![],
            body: vec![
                LpirOp::Iadd {
                    dst: VReg(3),
                    lhs: VReg(1),
                    rhs: VReg(2),
                },
                LpirOp::Return {
                    values: VRegRange { start: 3, count: 1 },
                },
            ],
            vreg_pool: vec![],
        };
        // nop encoded as addi x0,x0,0 — any valid word works for the test
        let code = [0x13u8, 0x00, 0x00, 0x00];
        let table = LineTable::from_debug_lines(&[(0, Some(0u32))]);
        let s = disassemble_function(&code, &table, &func, DisasmOptions::default());
        assert!(s.contains("add:"));
        assert!(s.contains("# v3 = iadd v1, v2"));
        assert!(s.contains("(0)"));
    }
}
