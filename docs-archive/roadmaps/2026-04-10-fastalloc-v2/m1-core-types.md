# M1: Core Types

## Scope of Work

Create the `rv32fa/` directory, copy ABI definitions, and define PhysInst enum. Also add PhysInst parser/formatter for tests.

## Files

```
lp-shader/lpvm-native/src/isa/
└── rv32fa/
    ├── mod.rs                 # NEW: module exports
    ├── abi.rs                 # NEW: ABI definitions (copy)
    ├── inst.rs                # NEW: PhysInst enum
    └── debug/
        ├── mod.rs              # NEW: debug module
        └── physinst.rs         # NEW: PhysInst Display + parser
```

## Implementation Details

### 1. Create `rv32fa/mod.rs`

```rust
//! Fast allocator pipeline for RV32.
//!
//! This pipeline replaces the legacy rv32/ allocator + emitter with:
//! - Backward-walk register allocator producing PhysInst
//! - Functional emitter with no indirection
//! - Debug-first architecture (every stage visible)

pub mod abi;
pub mod alloc;
pub mod debug;
pub mod emit;
pub mod inst;
```

### 2. Copy ABI Definitions to `abi.rs`

Copy from `rv32/abi.rs`:

```rust
//! ABI definitions for RV32 fastalloc pipeline.
//!
//! Copied from rv32/abi.rs - identical ABI.

use crate::abi::{PReg, RegClass, PregSet};

/// Integer argument registers: a0-a7
pub const ARG_REGS: &[PReg] = &[
    PReg { hw: 10, class: RegClass::Int }, // a0
    PReg { hw: 11, class: RegClass::Int }, // a1
    // ... a2-a7
];

/// Integer return registers: a0-a1
pub const RET_REGS: &[PReg] = &[
    PReg { hw: 10, class: RegClass::Int }, // a0
    PReg { hw: 11, class: RegClass::Int }, // a1
];

/// Frame pointer: s0 (x8)
pub const FP_REG: u8 = 8;

/// Stack pointer: sp (x2)
pub const SP_REG: u8 = 2;

/// Return address: ra (x1)
pub const RA_REG: u8 = 1;

/// Callee-saved integer registers: s0-s11 (excluding s1 used for SRET)
pub fn callee_saved_int() -> PregSet {
    // s0-s11 (x8-x9, x18-x27), but s1 (x9) is reserved for SRET
    let mut set = PregSet::new();
    set.add(PReg { hw: 8, class: RegClass::Int });  // s0 (fp)
    // s1 (x9) reserved
    for hw in 18..=27 {
        set.add(PReg { hw, class: RegClass::Int });
    }
    set
}

/// Caller-saved integer registers: t0-t6, a0-a7 (excluding args/rets)
pub fn caller_saved_int() -> PregSet {
    // t0-t6 (x5-x7, x28-x31), a0-a7 (x10-x17)
    let mut set = PregSet::new();
    for hw in [5, 6, 7, 28, 29, 30, 31].iter().copied() {
        set.add(PReg { hw, class: RegClass::Int });
    }
    for hw in 10..=17 {
        set.add(PReg { hw, class: RegClass::Int });
    }
    set
}

/// Allocatable registers (not reserved)
pub fn allocatable_int() -> PregSet {
    // t0-t6, a0-a7, s0, s2-s11
    // Excludes: x0 (zero), x1 (ra), x2 (sp), x3 (gp), x4 (tp), s1 (x9 - SRET)
}

/// Physical register name for debugging.
pub fn reg_name(reg: u8) -> &'static str {
    match reg {
        0 => "x0",
        1 => "ra",
        2 => "sp",
        // ... all 32 registers
        10 => "a0",
        11 => "a1",
        // ... etc
        _ => "???",
    }
}
```

### 3. Define PhysInst in `inst.rs`

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

    // Arithmetic (2 src, 1 dst)
    Add32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Sub32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Mul32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    And32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Or32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Xor32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    Shl32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    ShrS32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    ShrU32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },

    // Division/remainder
    DivS32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    DivU32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    RemS32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },
    RemU32 { dst: PhysReg, src1: PhysReg, src2: PhysReg },

    // Unary
    Neg32 { dst: PhysReg, src: PhysReg },
    Bnot32 { dst: PhysReg, src: PhysReg },
    Mov32 { dst: PhysReg, src: PhysReg },

    // Comparison
    Icmp32 { dst: PhysReg, src1: PhysReg, src2: PhysReg, cond: IcmpCond },
    IeqImm32 { dst: PhysReg, src: PhysReg, imm: i32 },

    // Select - named fields, no ordering confusion
    Select32 {
        dst: PhysReg,
        cond: PhysReg,
        if_true: PhysReg,
        if_false: PhysReg,
    },

    // Memory
    Load32 { dst: PhysReg, base: PhysReg, offset: i32 },
    Store32 { src: PhysReg, base: PhysReg, offset: i32 },
    MemcpyWords { dst_base: PhysReg, src_base: PhysReg, size: u32 },
    SlotAddr { dst: PhysReg, slot: u32 },

    // Immediate
    LoadImm { dst: PhysReg, val: i32 },

    // Control
    Call { target: SymbolRef },
    Ret,
}

impl PhysInst {
    /// Human-readable mnemonic.
    pub fn mnemonic(&self) -> &'static str {
        match self {
            PhysInst::FrameSetup { .. } => "FrameSetup",
            PhysInst::FrameTeardown { .. } => "FrameTeardown",
            PhysInst::Add32 { .. } => "Add32",
            PhysInst::Sub32 { .. } => "Sub32",
            // ... all variants
            PhysInst::Call { .. } => "Call",
            PhysInst::Ret => "Ret",
        }
    }
}
```

### 4. PhysInst Debug in `debug/physinst.rs`

```rust
//! PhysInst text format and parser.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use crate::isa::rv32fa::inst::{PhysInst, PhysReg};
use crate::isa::rv32fa::abi::reg_name;
use crate::vinst::{IcmpCond, SymbolRef};

/// Format PhysInsts for human-readable output.
pub fn format_physinsts(physinsts: &[PhysInst]) -> String {
    let mut lines = Vec::new();
    for inst in physinsts {
        lines.push(fmt_physinst(inst));
    }
    lines.join("\n")
}

fn fmt_physinst(inst: &PhysInst) -> String {
    match inst {
        PhysInst::FrameSetup { spill_slots } => {
            format!("FrameSetup {{ spill_slots: {} }}", spill_slots)
        }
        PhysInst::LoadImm { dst, val } => {
            format!("{} = LoadImm {}", reg_name(*dst), val)
        }
        PhysInst::Add32 { dst, src1, src2 } => {
            format!("{} = Add32 {}, {}", reg_name(*dst), reg_name(*src1), reg_name(*src2))
        }
        // ... all other variants
        PhysInst::Call { target } => format!("Call {}", target.name),
        PhysInst::Ret => "Ret".to_string(),
    }
}

/// Parse PhysInsts from text.
pub fn parse_physinsts(input: &str) -> Result<Vec<PhysInst>, ParseError> {
    let mut insts = Vec::new();
    for (line_num, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let inst = parse_physinst(line, line_num)?;
        insts.push(inst);
    }
    Ok(insts)
}

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

fn parse_physinst(line: &str, line_num: usize) -> Result<PhysInst, ParseError> {
    // Similar structure to VInst parser
    // Handle: "a0 = LoadImm 1"
    // Handle: "Call mod"
    // Handle: "Ret"
}

fn parse_reg(name: &str) -> Result<PhysReg, ParseError> {
    // Map "a0" -> 10, "a1" -> 11, "t0" -> 5, "s0" -> 8, etc.
    match name {
        "x0" | "zero" => Ok(0),
        "ra" => Ok(1),
        "sp" => Ok(2),
        "t0" => Ok(5),
        "a0" => Ok(10),
        "a1" => Ok(11),
        "s0" | "fp" => Ok(8),
        // ... all registers
        _ => Err(ParseError { line: 0, message: format!("Unknown register: {}", name) }),
    }
}
```

### 5. Wire up in `isa/mod.rs`

```rust
pub mod rv32;      // Existing
pub mod rv32fa;    // NEW
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_loadimm() {
        let input = "a0 = LoadImm 42";
        let insts = parse_physinsts(input).unwrap();
        let output = format_physinsts(&insts);
        assert!(output.contains("LoadImm 42"));
    }

    #[test]
    fn test_roundtrip_add32() {
        let input = "a0 = Add32 a1, a2";
        let insts = parse_physinsts(input).unwrap();
        assert_eq!(insts.len(), 1);
        assert!(matches!(insts[0], PhysInst::Add32 { dst: 10, src1: 11, src2: 12 }));
    }
}
```

## Validate

```bash
cd lp-shader/lpvm-native
cargo test -p lpvm-native --lib -- rv32fa::debug::physinst
```
