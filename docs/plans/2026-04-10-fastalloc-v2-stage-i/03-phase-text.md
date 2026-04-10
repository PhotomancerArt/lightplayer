# Phase 3: PhysInst Text Parser and Formatter

## Scope

Implement `parse()` and `format()` for PhysInst using standard RISC-V assembly syntax.

## Implementation

Create `rv32fa/debug/physinst.rs`:

```rust
//! PhysInst parser and formatter using standard RISC-V assembly syntax.

use crate::isa::rv32fa::abi::parse_reg;
use crate::isa::rv32fa::inst::{PhysInst, PhysReg};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

pub fn format(inst: &PhysInst) -> String {
    match inst {
        // Frame operations
        PhysInst::FrameSetup { spill_slots } => format!("FrameSetup {}", spill_slots),
        PhysInst::FrameTeardown { spill_slots } => format!("FrameTeardown {}", spill_slots),

        // Arithmetic
        PhysInst::Add { dst, src1, src2 } => format!("add {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Sub { dst, src1, src2 } => format!("sub {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Mul { dst, src1, src2 } => format!("mul {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Div { dst, src1, src2 } => format!("div {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Divu { dst, src1, src2 } => format!("divu {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Rem { dst, src1, src2 } => format!("rem {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Remu { dst, src1, src2 } => format!("remu {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),

        // Logical
        PhysInst::And { dst, src1, src2 } => format!("and {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Or { dst, src1, src2 } => format!("or {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Xor { dst, src1, src2 } => format!("xor {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),

        // Shifts
        PhysInst::Sll { dst, src1, src2 } => format!("sll {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Srl { dst, src1, src2 } => format!("srl {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Sra { dst, src1, src2 } => format!("sra {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),

        // Unary
        PhysInst::Neg { dst, src } => format!("neg {}, {}", reg(*dst), reg(*src)),
        PhysInst::Not { dst, src } => format!("not {}, {}", reg(*dst), reg(*src)),
        PhysInst::Mv { dst, src } => format!("mv {}, {}", reg(*dst), reg(*src)),

        // Comparison
        PhysInst::Slt { dst, src1, src2 } => format!("slt {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Sltu { dst, src1, src2 } => format!("sltu {}, {}, {}", reg(*dst), reg(*src1), reg(*src2)),
        PhysInst::Seqz { dst, src } => format!("seqz {}, {}", reg(*dst), reg(*src)),
        PhysInst::Snez { dst, src } => format!("snez {}, {}", reg(*dst), reg(*src)),
        PhysInst::Sltz { dst, src } => format!("sltz {}, {}", reg(*dst), reg(*src)),
        PhysInst::Sgtz { dst, src } => format!("sgtz {}, {}", reg(*dst), reg(*src)),

        // Immediate
        PhysInst::Li { dst, imm } => format!("li {}, {}", reg(*dst), imm),
        PhysInst::Addi { dst, src, imm } => format!("addi {}, {}, {}", reg(*dst), reg(*src), imm),

        // Memory
        PhysInst::Lw { dst, base, offset } => format!("lw {}, {}({})", reg(*dst), offset, reg(*base)),
        PhysInst::Sw { src, base, offset } => format!("sw {}, {}({})", reg(*src), offset, reg(*base)),

        // Stack
        PhysInst::SlotAddr { dst, slot } => format!("SlotAddr {}, {}", reg(*dst), slot),

        // Block memory
        PhysInst::MemcpyWords { dst, src, size } => format!("MemcpyWords {}, {}, {}", reg(*dst), reg(*src), size),

        // Control flow
        PhysInst::Call { target } => format!("call {}", target),
        PhysInst::Ret => "ret".to_string(),
        PhysInst::Beq { src1, src2, target } => format!("beq {}, {}, @{}", reg(*src1), reg(*src2), target),
        PhysInst::Bne { src1, src2, target } => format!("bne {}, {}, @{}", reg(*src1), reg(*src2), target),
        PhysInst::Blt { src1, src2, target } => format!("blt {}, {}, @{}", reg(*src1), reg(*src2), target),
        PhysInst::Bge { src1, src2, target } => format!("bge {}, {}, @{}", reg(*src1), reg(*src2), target),
        PhysInst::J { target } => format!("j @{}", target),
    }
}

fn reg(r: PhysReg) -> &'static str {
    crate::isa::rv32fa::abi::reg_name(r)
}

pub fn format_block(inst: &[PhysInst]) -> String {
    inst.iter().map(format).collect::<Vec<_>>().join("\n")
}

pub fn parse(input: &str) -> Result<Vec<PhysInst>, ParseError> {
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
