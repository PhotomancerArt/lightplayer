# Phase 4: Annotated Disassembly

## Scope

Create `disasm.rs` module that produces human-readable RV32 assembly with LPIR source annotations.

## Code Organization Reminders

- Use `lp_riscv_inst::decode_instruction()` and `format_instruction()` for decoding
- Annotate each instruction with `# LPIR[n]: <op>` comments
- Include function labels and `.globl`/`.type` directives
- Keep format close to GNU assembler syntax where practical

## Implementation Details

### Create `isa/rv32/debug/disasm.rs`

```rust
//! Annotated disassembly for RV32 code.
//!
//! Produces human-readable assembly with LPIR source annotations.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lp_riscv_inst::{decode_instruction, format_instruction};
use lpir::{IrFunction, IrModule, Op};

use super::{LineEntry, LineTable};

/// Disassemble RV32 code with LPIR source annotations.
///
/// Output format:
/// ```text
///     .globl  <func_name>
///     .type   <func_name>, @function
/// <func_name>:
///     addi    sp, sp, -16             # prologue
///     # LPIR[2]: v3 = fadd v1, v2
///     lui     a3, %hi(__lp_lpir_fadd_q32)
///     ...
/// ```
pub fn disassemble_function(
    code: &[u8],
    line_table: &LineTable,
    module: &IrModule,
    function: &IrFunction,
) -> String {
    let mut output = String::new();
    
    // Function header directives
    let func_name = &function.name;
    output.push_str(&format!("    .globl\t{}\n", func_name));
    output.push_str(&format!("    .type\t{}, @function\n", func_name));
    output.push_str(&format!("{}:\n", func_name));
    
    // Disassemble each 4-byte instruction
    let mut offset = 0u32;
    while (offset as usize + 4) <= code.len() {
        let inst_bytes = &code[offset as usize..offset as usize + 4];
        let inst_word = u32::from_le_bytes([
            inst_bytes[0], inst_bytes[1], inst_bytes[2], inst_bytes[3]
        ]);
        
        // Format instruction
        let asm = format_instruction(inst_word);
        
        // Look up source annotation
        let annotation = if let Some(entry) = line_table.lookup(offset) {
            format_lpir_annotation(entry, module, function)
        } else {
            String::new()
        };
        
        // Output line
        if annotation.is_empty() {
            output.push_str(&format!("    {:<32}\n", asm));
        } else {
            output.push_str(&format!("    {:<32}# {}\n", asm, annotation));
        }
        
        offset += 4;
    }
    
    // Function size directive
    output.push_str(&format!("    .size\t{}, .-{}\n", func_name, func_name));
    
    output
}

/// Format LPIR annotation for a LineEntry.
///
/// Returns string like "LPIR[5]: v3 = add v1, v2"
fn format_lpir_annotation(entry: &LineEntry, module: &IrModule, function: &IrFunction) -> String {
    let op_idx = entry.src_op as usize;
    
    if op_idx >= function.body.len() {
        return format!("LPIR[{}]: <invalid>", entry.src_op);
    }
    
    let op = &function.body[op_idx];
    let op_str = format_op(op);
    
    format!("LPIR[{}]: {}", entry.src_op, op_str)
}

/// Format an LPIR operation for display.
fn format_op(op: &Op) -> String {
    match op {
        Op::Iadd { dst, lhs, rhs } => {
            format!("v{} = add v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        Op::Isub { dst, lhs, rhs } => {
            format!("v{} = sub v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        Op::Imul { dst, lhs, rhs } => {
            format!("v{} = mul v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        Op::Fadd { dst, lhs, rhs, .. } => {
            format!("v{} = fadd v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        Op::Fsub { dst, lhs, rhs, .. } => {
            format!("v{} = fsub v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        Op::Fmul { dst, lhs, rhs, .. } => {
            format!("v{} = fmul v{}, v{}", dst.0, lhs.0, rhs.0)
        }
        Op::Call { dst, target, args, .. } => {
            let arg_str = args.iter()
                .map(|v| format!("v{}", v.0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("v{} = call @{}({})", dst.0, target, arg_str)
        }
        Op::Load { dst, src, offset, ty } => {
            format!("v{} = load v{}+{} {:?}", dst.0, src.0, offset, ty)
        }
        Op::Store { dst, src, offset, ty } => {
            format!("store v{}, v{}+{} {:?}", src.0, dst.0, offset, ty)
        }
        Op::Return { values } => {
            if values.count == 0 {
                "return".to_string()
            } else {
                let val_str = (values.start..values.start + values.count)
                    .map(|i| format!("v{}", i))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("return {}", val_str)
            }
        }
        Op::ConstI32 { dst, value } => {
            format!("v{} = const {}", dst.0, value)
        }
        Op::ConstF32 { dst, value, .. } => {
            format!("v{} = const {}", dst.0, value)
        }
        Op::Cast { dst, src, from, to } => {
            format!("v{} = cast v{} {:?} -> {:?}", dst.0, src.0, from, to)
        }
        Op::Select { dst, cond, then_val, else_val, .. } => {
            format!("v{} = select v{}, v{}, v{}", dst.0, cond.0, then_val.0, else_val.0)
        }
        Op::Phi { dst, .. } => {
            format!("v{} = phi ...", dst.0)
        }
        Op::Jump { target_block } => {
            format!("jump @block{}", target_block.0)
        }
        Op::Branch { cond, true_block, false_block } => {
            format!("br v{}, @block{}, @block{}", cond.0, true_block.0, false_block.0)
        }
        Op::Builtin { dst, name, args, .. } => {
            let arg_str = args.iter()
                .map(|v| format!("v{}", v.0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("v{} = builtin @{}({})", dst.0, name, arg_str)
        }
        _ => format!("{:?}", op),
    }
}

/// Simple hex dump for debugging (optional)
pub fn hex_dump(code: &[u8]) -> String {
    let mut output = String::new();
    for (i, chunk) in code.chunks(4).enumerate() {
        let addr = i * 4;
        let word = u32::from_le_bytes([
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
            chunk.get(3).copied().unwrap_or(0),
        ]);
        output.push_str(&format!("{:04x}: {:08x}\n", addr, word));
    }
    output
}
```

### Export in `debug/mod.rs`

```rust
pub mod disasm;
pub use disasm::{disassemble_function, hex_dump};
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lpir::{IrFunction, IrModule, VReg};
    
    fn simple_add_function() -> (IrModule, IrFunction) {
        let module = IrModule {
            imports: vec![],
            functions: vec![],
        };
        let func = IrFunction {
            name: "add".to_string(),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 2,
            return_types: vec![lpir::IrType::I32],
            vreg_types: vec![lpir::IrType::I32; 4],
            slots: vec![],
            body: vec![
                Op::Iadd { dst: VReg(3), lhs: VReg(1), rhs: VReg(2) },
                Op::Return { values: lpir::VRegRange { start: 3, count: 1 } },
            ],
            vreg_pool: vec![],
        };
        (module, func)
    }

    #[test]
    fn disassemble_simple_function() {
        // Encode: addi sp, sp, -16 (prologue)
        let code = vec![
            0x13, 0x01, 0x01, 0xfe,  // addi sp, sp, -16
            0x33, 0x06, 0x32, 0x00,  // add a2, a3, a4 (placeholder)
            0x13, 0x01, 0x01, 0x01,  // addi sp, sp, 16 (epilogue)
        ];
        
        let line_table = LineTable::from_pairs(&[
            (0, None),      // prologue
            (4, Some(0)),   // LPIR[0]: add
            (8, None),      // epilogue
        ]);
        
        let (module, func) = simple_add_function();
        let asm = disassemble_function(&code, &line_table, &module, &func);
        
        // Check that output contains expected elements
        assert!(asm.contains(".globl\tadd"));
        assert!(asm.contains("add:"));
        assert!(asm.contains("LPIR[0]:"));
        assert!(asm.contains(".size\tadd"));
    }
}
```

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib disasm
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```
