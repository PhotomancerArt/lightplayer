# Phase 2: PhysInst Enum

## Scope

Define the `PhysInst` enum mirroring all VInst variants.

## Implementation

Create `rv32fa/inst.rs`:

```rust
//! Physical-register instructions.
//!
//! Every field that was VReg in VInst is now PhysReg (u8).

use crate::vinst::{IcmpCond, SymbolRef};

pub type PhysReg = u8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysInst {
    // Frame operations (prologue/epilogue)
    FrameSetup { spill_slots: u32 },
    FrameTeardown { spill_slots: u32 },

    // Arithmetic - R-type instructions
    Add { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Sub { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Mul { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Div { dst: PhysReg, src1: PhysReg, src2: PhysReg },      // Signed division
    Divu { dst: PhysReg, src1: PhysReg, src2: PhysReg },     // Unsigned division
    Rem { dst: PhysReg, src1: PhysReg, src2: PhysReg },      // Signed remainder
    Remu { dst: PhysReg, src1: PhysReg, src2: PhysReg },     // Unsigned remainder

    // Logical
    And { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Or { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Xor { dst: PhysReg, src1: PhysReg, src2: PhysReg },

    // Shifts
    Sll { dst: PhysReg, src1: PhysReg, src2: PhysReg },     // Shift left logical
    Srl { dst: PhysReg, src1: PhysReg, src2: PhysReg },     // Shift right logical
    Sra { dst: PhysReg, src1: PhysReg, src2: PhysReg },     // Shift right arithmetic

    // Unary
    Neg { dst: PhysReg, src: PhysReg },                     // Negate
    Not { dst: PhysReg, src: PhysReg },                     // Bitwise not
    Mv { dst: PhysReg, src: PhysReg },                      // Move

    // Comparison - results in 0 or 1
    Slt { dst: PhysReg, src1: PhysReg, src2: PhysReg },     // Set less than (signed)
    Sltu { dst: PhysReg, src1: PhysReg, src2: PhysReg },    // Set less than unsigned
    Seqz { dst: PhysReg, src: PhysReg },                    // Set if equal zero
    Snez { dst: PhysReg, src: PhysReg },                   // Set if not equal zero
    Sltz { dst: PhysReg, src: PhysReg },                   // Set if less than zero
    Sgtz { dst: PhysReg, src: PhysReg },                   // Set if greater than zero

    // Immediate operations
    Li { dst: PhysReg, imm: i32 },                        // Load immediate (pseudoinstruction)
    Addi { dst: PhysReg, src: PhysReg, imm: i32 },        // Add immediate

    // Memory
    Lw { dst: PhysReg, base: PhysReg, offset: i32 },      // Load word
    Sw { src: PhysReg, base: PhysReg, offset: i32 },      // Store word

    // Stack slot
    SlotAddr { dst: PhysReg, slot: u32 },                 // Get address of stack slot

    // Block memory
    MemcpyWords { dst: PhysReg, src: PhysReg, size: u32 }, // Copy size bytes (multiple of 4)

    // Control flow
    Call { target: SymbolRef },                           // Call function
    Ret,                                                 // Return

    // Branches (for future control flow support)
    Beq { src1: PhysReg, src2: PhysReg, target: u32 },   // Branch if equal
    Bne { src1: PhysReg, src2: PhysReg, target: u32 },   // Branch if not equal
    Blt { src1: PhysReg, src2: PhysReg, target: u32 },   // Branch if less than
    Bge { src1: PhysReg, src2: PhysReg, target: u32 },   // Branch if greater/equal
    J { target: u32 },                                   // Unconditional jump
}

impl PhysInst {
    /// Human-readable mnemonic for debugging.
    pub fn mnemonic(&self) -> &'static str {
        match self {
            PhysInst::FrameSetup { .. } => "FrameSetup",
            PhysInst::FrameTeardown { .. } => "FrameTeardown",
            PhysInst::Add { .. } => "add",
            PhysInst::Sub { .. } => "sub",
            PhysInst::Mul { .. } => "mul",
            PhysInst::Div { .. } => "div",
            PhysInst::Divu { .. } => "divu",
            PhysInst::Rem { .. } => "rem",
            PhysInst::Remu { .. } => "remu",
            PhysInst::And { .. } => "and",
            PhysInst::Or { .. } => "or",
            PhysInst::Xor { .. } => "xor",
            PhysInst::Sll { .. } => "sll",
            PhysInst::Srl { .. } => "srl",
            PhysInst::Sra { .. } => "sra",
            PhysInst::Neg { .. } => "neg",
            PhysInst::Not { .. } => "not",
            PhysInst::Mv { .. } => "mv",
            PhysInst::Slt { .. } => "slt",
            PhysInst::Sltu { .. } => "sltu",
            PhysInst::Seqz { .. } => "seqz",
            PhysInst::Snez { .. } => "snez",
            PhysInst::Sltz { .. } => "sltz",
            PhysInst::Sgtz { .. } => "sgtz",
            PhysInst::Li { .. } => "li",
            PhysInst::Addi { .. } => "addi",
            PhysInst::Lw { .. } => "lw",
            PhysInst::Sw { .. } => "sw",
            PhysInst::SlotAddr { .. } => "SlotAddr",
            PhysInst::MemcpyWords { .. } => "MemcpyWords",
            PhysInst::Call { .. } => "call",
            PhysInst::Ret => "ret",
            PhysInst::Beq { .. } => "beq",
            PhysInst::Bne { .. } => "bne",
            PhysInst::Blt { .. } => "blt",
            PhysInst::Bge { .. } => "bge",
            PhysInst::J { .. } => "j",
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
