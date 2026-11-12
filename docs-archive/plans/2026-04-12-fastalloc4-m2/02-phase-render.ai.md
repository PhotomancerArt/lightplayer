# Phase 2: AllocOutput Rendering

## Scope

Create `fa_alloc/render.rs` with human-readable output formatting for snapshot tests.

## Implementation

### File: `fa_alloc/render.rs`

Structure:
```rust
use crate::fa_alloc::{Alloc, AllocOutput, EditPoint};
use crate::vinst::{VInst, VReg};
use alloc::string::String;
use alloc::vec::Vec;

/// Render AllocOutput as human-readable text for snapshot tests.
pub fn render_alloc_output(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
) -> String {
    let mut lines = Vec::new();
    
    for (inst_idx, inst) in vinsts.iter().enumerate() {
        // Add separator before each instruction (except first)
        if inst_idx > 0 {
            lines.push("; ---------------------------".to_string());
        }
        
        // TODO: Find edits Before this instruction
        // TODO: Render reads (uses)
        // TODO: Render instruction
        // TODO: Render writes (defs)
        // TODO: Find edits After this instruction
    }
    
    lines.join("\n")
}

/// Get allocation for a specific operand.
fn operand_alloc(output: &AllocOutput, inst_idx: usize, operand_idx: usize) -> Alloc {
    output.operand_alloc(inst_idx as u16, operand_idx as u16)
}

/// Format register name from PReg number.
fn format_reg(preg: u8) -> &'static str {
    // TODO: Use gpr::reg_name() or similar
    match preg {
        5 => "t0",
        6 => "t1", 
        7 => "t2",
        10 => "a0",
        11 => "a1",
        // ... etc
        _ => "??",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // TODO: Test render functions
}
```

### Format Specification

```
i0 = IConst32 10
; write: i0 -> t0
; ---------------------------
; read: i0 <- t0
Ret i0
```

Rules:
- Instruction appears first (as-is from VInst)
- Write (def) comments: `; write: {vreg} -> {location}`
- Separator line before next instruction
- Read (use) comments: `; read: {vreg} <- {location}`
- Spill/reload edits shown between instructions

Location format:
- Register: `t0`, `t1`, etc.
- Spill slot: `slot 0`, `slot 1`, etc.

### Steps

1. **VReg formatting**: Extract vreg index from VReg(0) -> "i0"
2. **Location formatting**: Map Alloc::Reg to name, Alloc::Stack to "slot N"
3. **Inst formatting**: Use existing VInst text format or mnemonic
4. **Edit grouping**: Collect edits by EditPoint, render in order
5. **Assembly**: Combine into final string

## Code Organization

- Place `render_alloc_output` at top (entry point)
- Helper functions below (format_reg, format_vreg, etc.)
- Tests at bottom

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native fa_alloc::render::tests
```
