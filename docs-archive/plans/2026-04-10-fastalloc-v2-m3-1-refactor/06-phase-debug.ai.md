# Phase 6: Update Debug Formatters

## Scope

Update debug formatters to work with the new compact types:
- `debug/vinst.rs` — format VInst with VRegSlice resolution
- `isa/rv32/debug/pinst.rs` — format PInst (uses VReg info from allocation)

## Implementation

### 1. Update `debug/vinst.rs`

```rust
//! Debug formatting for compact VInst.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::vinst::{VInst, VReg, VRegSlice, SymbolId, SRC_OP_NONE, LabelId};
use crate::lower::ModuleSymbols;

/// Format a single VInst for debug output.
/// Requires vreg_pool to resolve VRegSlice contents.
/// Requires symbols to resolve SymbolId names.
pub fn format_vinst(inst: &VInst, pool: &[VReg], symbols: &ModuleSymbols) -> String {
    let op_str = match inst {
        VInst::Add32 { dst, src1, src2, .. } => {
            format!("i{} = Add32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::Sub32 { dst, src1, src2, .. } => {
            format!("i{} = Sub32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::Neg32 { dst, src, .. } => {
            format!("i{} = Neg32 i{}", dst.0, src.0)
        }
        VInst::Mul32 { dst, src1, src2, .. } => {
            format!("i{} = Mul32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::And32 { dst, src1, src2, .. } => {
            format!("i{} = And32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::Or32 { dst, src1, src2, .. } => {
            format!("i{} = Or32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::Xor32 { dst, src1, src2, .. } => {
            format!("i{} = Xor32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::Bnot32 { dst, src, .. } => {
            format!("i{} = Bnot32 i{}", dst.0, src.0)
        }
        VInst::Shl32 { dst, src1, src2, .. } => {
            format!("i{} = Shl32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::ShrS32 { dst, src1, src2, .. } => {
            format!("i{} = ShrS32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::ShrU32 { dst, src1, src2, .. } => {
            format!("i{} = ShrU32 i{}, i{}", dst.0, src1.0, src2.0)
        }
        VInst::DivS32 { dst, lhs, rhs, .. } => {
            format!("i{} = DivS32 i{}, i{}", dst.0, lhs.0, rhs.0)
        }
        VInst::DivU32 { dst, lhs, rhs, .. } => {
            format!("i{} = DivU32 i{}, i{}", dst.0, lhs.0, rhs.0)
        }
        VInst::RemS32 { dst, lhs, rhs, .. } => {
            format!("i{} = RemS32 i{}, i{}", dst.0, lhs.0, rhs.0)
        }
        VInst::RemU32 { dst, lhs, rhs, .. } => {
            format!("i{} = RemU32 i{}, i{}", dst.0, lhs.0, rhs.0)
        }
        VInst::Icmp32 { dst, lhs, rhs, cond, .. } => {
            format!("i{} = Icmp32 {:?} i{}, i{}", dst.0, cond, lhs.0, rhs.0)
        }
        VInst::IeqImm32 { dst, src, imm, .. } => {
            format!("i{} = IeqImm32 i{}, {}", dst.0, src.0, imm)
        }
        VInst::Select32 { dst, cond, if_true, if_false, .. } => {
            format!("i{} = Select32 i{}, i{}, i{}",
                dst.0, cond.0, if_true.0, if_false.0)
        }
        VInst::Br { target, .. } => {
            format!("Br L{}", target)
        }
        VInst::BrIf { cond, target, invert, .. } => {
            let cond_str = if *invert { "== 0" } else { "!= 0" };
            format!("BrIf i{} {}, L{}", cond.0, cond_str, target)
        }
        VInst::Mov32 { dst, src, .. } => {
            format!("i{} = Mov32 i{}", dst.0, src.0)
        }
        VInst::Load32 { dst, base, offset, .. } => {
            format!("i{} = Load32 [i{} + {}]", dst.0, base.0, offset)
        }
        VInst::Store32 { src, base, offset, .. } => {
            format!("Store32 i{}, [i{} + {}]", src.0, base.0, offset)
        }
        VInst::SlotAddr { dst, slot, .. } => {
            format!("i{} = SlotAddr {}", dst.0, slot)
        }
        VInst::MemcpyWords { dst_base, src_base, size, .. } => {
            format!("MemcpyWords [i{}], [i{}], {}", dst_base.0, src_base.0, size)
        }
        VInst::IConst32 { dst, val, .. } => {
            format!("i{} = IConst32 {}", dst.0, val)
        }
        VInst::Call { target, args, rets, callee_uses_sret, .. } => {
            let name = symbols.get(*target).unwrap_or("?");
            let args_str = format_vreg_slice(args, pool);
            let rets_str = format_vreg_slice(rets, pool);
            let sret_str = if *callee_uses_sret { " (sret)" } else { "" };
            if rets.is_empty() {
                format!("Call {}({}){}", name, args_str, sret_str)
            } else {
                format!("[{}] = Call {}({}){}", rets_str, name, args_str, sret_str)
            }
        }
        VInst::Ret { vals, .. } => {
            let vals_str = format_vreg_slice(vals, pool);
            format!("Ret {}", vals_str)
        }
        VInst::Label(id, _) => {
            format!("Label L{}", id)
        }
    };
    
    op_str
}

/// Format a VRegSlice by looking up vregs in pool.
fn format_vreg_slice(slice: &VRegSlice, pool: &[VReg]) -> String {
    if slice.is_empty() {
        return String::new();
    }
    
    let vregs: Vec<String> = slice.iter(pool)
        .map(|v| format!("i{}", v.0))
        .collect();
    
    vregs.join(", ")
}

/// Format a sequence of VInsts.
pub fn format_vinsts(vinsts: &[VInst], pool: &[VReg], symbols: &ModuleSymbols) -> String {
    let mut lines = Vec::new();
    for (i, inst) in vinsts.iter().enumerate() {
        lines.push(format!("{:4}: {}", i, format_vinst(inst, pool, symbols)));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::{VRegSlice, IcmpCond};
    
    fn test_symbols() -> ModuleSymbols {
        let mut s = ModuleSymbols::new();
        s.intern("sin");
        s.intern("cos");
        s
    }
    
    #[test]
    fn test_format_add32() {
        let inst = VInst::Add32 {
            dst: VReg(2),
            src1: VReg(0),
            src2: VReg(1),
            src_op: SRC_OP_NONE,
        };
        let s = format_vinst(&inst, &[], &test_symbols());
        assert_eq!(s, "i2 = Add32 i0, i1");
    }
    
    #[test]
    fn test_format_call() {
        let mut symbols = ModuleSymbols::new();
        let sin_id = symbols.intern("sin");
        
        let pool = vec![VReg(0)];
        let inst = VInst::Call {
            target: sin_id,
            args: VRegSlice::new(0, 1),
            rets: VRegSlice::EMPTY,
            callee_uses_sret: false,
            src_op: SRC_OP_NONE,
        };
        
        let s = format_vinst(&inst, &pool, &symbols);
        assert_eq!(s, "Call sin(i0)");
    }
}
```

### 2. Update `isa/rv32/debug/pinst.rs`

This file formats physical instructions (PInst). The changes are minimal since PInst uses physical registers (PReg), not VReg. Just verify it compiles.

```rust
//! Debug formatting for PInst (physical instructions).
//! 
//! Note: PInst uses PReg (physical registers), not VReg.
//! Minimal changes needed for M3.1.

// ... existing code, verify compilation ...
```

## Validate

```bash
cargo test -p lpvm-native --lib -- debug::vinst
```

Tests should pass.
