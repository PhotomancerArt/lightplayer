# Phase 3: PInst Text Parser and Formatter

## Scope

Implement `parse()` and `format()` for PInst using standard RISC-V assembly syntax.

## Implementation

Create `rv32fa/debug/pinst.rs`:

```rust
//! PInst parser and formatter using standard RISC-V assembly syntax.

use crate::isa::rv32fa::abi::parse_reg;
use crate::isa::rv32fa::inst::{PInst, PReg};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

pub fn format(inst: &PInst) -> String {
    match inst {
        // Frame operations
        PInst::FrameSetup { spill_slots } => format!("FrameSetup {}", spill_slots),
        PInst::FrameTeardown { spill_slots } => format!("FrameTeardown {}", spill_slots),

        // Arithmetic
        PInst::Add { dst, src1, src2 } => format!("add {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Sub { dst, src1, src2 } => format!("sub {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Mul { dst, src1, src2 } => format!("mul {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Div { dst, src1, src2 } => format!("div {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Divu { dst, src1, src2 } => format!("divu {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Rem { dst, src1, src2 } => format!("rem {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Remu { dst, src1, src2 } => format!("remu {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),

        // Logical
        PInst::And { dst, src1, src2 } => format!("and {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Or { dst, src1, src2 } => format!("or {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Xor { dst, src1, src2 } => format!("xor {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),

        // Shifts
        PInst::Sll { dst, src1, src2 } => format!("sll {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Srl { dst, src1, src2 } => format!("srl {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Sra { dst, src1, src2 } => format!("sra {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),

        // Unary
        PInst::Neg { dst, src } => format!("neg {}, {}", reg(*dst), reg(*src)),
        PInst::Not { dst, src } => format!("not {}, {}", reg(*dst), reg(*src)),
        PInst::Mv { dst, src } => format!("mv {}, {}", reg(*dst), reg(*src)),

        // Comparison
        PInst::Slt { dst, src1, src2 } => format!("slt {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Sltu { dst, src1, src2 } => format!("sltu {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PInst::Seqz { dst, src } => format!("seqz {}, {}", reg(*dst), reg(*src)),
        PInst::Snez { dst, src } => format!("snez {}, {}", reg(*dst), reg(*src)),
        PInst::Sltz { dst, src } => format!("sltz {}, {}", reg(*dst), reg(*src)),
        PInst::Sgtz { dst, src } => format!("sgtz {}, {}", reg(*dst), reg(*src)),

        // Immediate
        PInst::Li { dst, imm } => format!("li {}, {}", reg(*dst), imm),
        PInst::Addi { dst, src, imm } => format!("addi {}, {}, {}", reg(*dst), reg(*src), imm),

        // Memory
        PInst::Lw { dst, base, offset } => format!("lw {}, {}({})", reg(*dst), offset, reg(*base)),
        PInst::Sw { src, base, offset } => format!("sw {}, {}({})", reg(*src), offset, reg(*base)),

        // Stack
        PInst::SlotAddr { dst, slot } => format!("SlotAddr {}, {}", reg(*dst), slot),

        // Block memory
        PInst::MemcpyWords { dst, src, size } => format!("MemcpyWords {}, {}, {}", reg(*dst), reg(*src), size),

        // Control flow
        PInst::Call { target } => format!("call {}", target),
        PInst::Ret => "ret".to_string(),
        PInst::Beq { src1, src2, target } => format!("beq {}, {}, @{}", reg(*src1), reg(*src2), target),
        PInst::Bne { src1, src2, target } => format!("bne {}, {}, @{}", reg(*src1), reg(*src2), target),
        PInst::Blt { src1, src2, target } => format!("blt {}, {}, @{}", reg(*src1), reg(*src2), target),
        PInst::Bge { src1, src2, target } => format!("bge {}, {}, @{}", reg(*src1), reg(*src2), target),
        PInst::J { target } => format!("j @{}", target),
    }
}

fn reg(r: PReg) -> &'static str {
    crate::isa::rv32fa::abi::reg_name(r)
}

pub fn format_block(inst: &[PInst]) -> String {
    inst.iter().map(format).collect::<Vec<_>>().join("\n")
}

pub fn parse(input: &str) -> Result<Vec<PInst>, ParseError> {
    // Implementation follows VInst parser pattern
    // Parse standard RISC-V assembly syntax
    todo!()
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl ParseError {
    pub fn new(line: usize, msg: impl Into<String>) -> Self {
        Self {
            line,
            msg: msg.into(),
        }
    }
}
```

## Notes

- Use standard RISC-V assembly syntax (add a0, a1, a2)
- Target labels in branches use `@N` prefix (j @1)
- FrameSetup/FrameTeardown and SlotAddr are non-standard, use descriptive names

## Validate

```bash
cargo check -p lpvm-native --lib
```
