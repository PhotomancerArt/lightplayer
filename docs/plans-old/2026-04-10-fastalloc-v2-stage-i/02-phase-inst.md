# Phase 2: PInst Enum

## Scope

Define the `PInst` enum mirroring all VInst variants.

## Implementation

Create `rv32fa/inst.rs`:

```rust
//! Physical-register instructions.
//!
//! Every field that was VReg in VInst is now PReg (u8).

use crate::vinst::{IcmpCond, SymbolRef};

pub type PReg = u8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PInst {
    // Frame operations (prologue/epilogue)
    FrameSetup { spill_slots: u32 },
    FrameTeardown { spill_slots: u32 },

    // Arithmetic - R-type instructions
    Add { dst: PReg, src1: PReg, src2: PReg },
    Sub { dst: PReg, src1: PReg, src2: PReg },
    Mul { dst: PReg, src1: PReg, src2: PReg },
    Div { dst: PReg, src1: PReg, src2: PReg },      // Signed division
    Divu { dst: PReg, src1: PReg, src2: PReg },     // Unsigned division
    Rem { dst: PReg, src1: PReg, src2: PReg },      // Signed remainder
    Remu { dst: PReg, src1: PReg, src2: PReg },     // Unsigned remainder

    // Logical
    And { dst: PReg, src1: PReg, src2: PReg },
    Or { dst: PReg, src1: PReg, src2: PReg },
    Xor { dst: PReg, src1: PReg, src2: PReg },

    // Shifts
    Sll { dst: PReg, src1: PReg, src2: PReg },     // Shift left logical
    Srl { dst: PReg, src1: PReg, src2: PReg },     // Shift right logical
    Sra { dst: PReg, src1: PReg, src2: PReg },     // Shift right arithmetic

    // Unary
    Neg { dst: PReg, src: PReg },                     // Negate
    Not { dst: PReg, src: PReg },                     // Bitwise not
    Mv { dst: PReg, src: PReg },                      // Move

    // Comparison - results in 0 or 1
    Slt { dst: PReg, src1: PReg, src2: PReg },     // Set less than (signed)
    Sltu { dst: PReg, src1: PReg, src2: PReg },    // Set less than unsigned
    Seqz { dst: PReg, src: PReg },                    // Set if equal zero
    Snez { dst: PReg, src: PReg },                   // Set if not equal zero
    Sltz { dst: PReg, src: PReg },                   // Set if less than zero
    Sgtz { dst: PReg, src: PReg },                   // Set if greater than zero

    // Immediate operations
    Li { dst: PReg, imm: i32 },                        // Load immediate (pseudoinstruction)
    Addi { dst: PReg, src: PReg, imm: i32 },        // Add immediate

    // Memory
    Lw { dst: PReg, base: PReg, offset: i32 },      // Load word
    Sw { src: PReg, base: PReg, offset: i32 },      // Store word

    // Stack slot
    SlotAddr { dst: PReg, slot: u32 },                 // Get address of stack slot

    // Block memory
    MemcpyWords { dst: PReg, src: PReg, size: u32 }, // Copy size bytes (multiple of 4)

    // Control flow
    Call { target: SymbolRef },                           // Call function
    Ret,                                                 // Return

    // Branches (for future control flow support)
    Beq { src1: PReg, src2: PReg, target: u32 },   // Branch if equal
    Bne { src1: PReg, src2: PReg, target: u32 },   // Branch if not equal
    Blt { src1: PReg, src2: PReg, target: u32 },   // Branch if less than
    Bge { src1: PReg, src2: PReg, target: u32 },   // Branch if greater/equal
    J { target: u32 },                                   // Unconditional jump
}

impl PInst {
    /// Human-readable mnemonic for debugging.
    pub fn mnemonic(&self) -> &'static str {
        match self {
            PInst::FrameSetup { .. } => "FrameSetup",
            PInst::FrameTeardown { .. } => "FrameTeardown",
            PInst::Add { .. } => "add",
            PInst::Sub { .. } => "sub",
            PInst::Mul { .. } => "mul",
            PInst::Div { .. } => "div",
            PInst::Divu { .. } => "divu",
            PInst::Rem { .. } => "rem",
            PInst::Remu { .. } => "remu",
            PInst::And { .. } => "and",
            PInst::Or { .. } => "or",
            PInst::Xor { .. } => "xor",
            PInst::Sll { .. } => "sll",
            PInst::Srl { .. } => "srl",
            PInst::Sra { .. } => "sra",
            PInst::Neg { .. } => "neg",
            PInst::Not { .. } => "not",
            PInst::Mv { .. } => "mv",
            PInst::Slt { .. } => "slt",
            PInst::Sltu { .. } => "sltu",
            PInst::Seqz { .. } => "seqz",
            PInst::Snez { .. } => "snez",
            PInst::Sltz { .. } => "sltz",
            PInst::Sgtz { .. } => "sgtz",
            PInst::Li { .. } => "li",
            PInst::Addi { .. } => "addi",
            PInst::Lw { .. } => "lw",
            PInst::Sw { .. } => "sw",
            PInst::SlotAddr { .. } => "SlotAddr",
            PInst::MemcpyWords { .. } => "MemcpyWords",
            PInst::Call { .. } => "call",
            PInst::Ret => "ret",
            PInst::Beq { .. } => "beq",
            PInst::Bne { .. } => "bne",
            PInst::Blt { .. } => "blt",
            PInst::Bge { .. } => "bge",
            PInst::J { .. } => "j",
        }
    }
}
```

## Notes

- Uses standard RISC-V instruction names (add, sub, mul, etc.)
- FrameSetup/Teardown are custom (not standard RISC-V)
- SlotAddr and MemcpyWords are custom (not standard RISC-V)
- Comparison uses standard RISC-V slt/sltu and pseudoinstructions (seqz, snez, etc.)
- Branches use standard RISC-V names but target is LabelId (u32), not offset

## Validate

```bash
cargo check -p lpvm-native --lib
```
