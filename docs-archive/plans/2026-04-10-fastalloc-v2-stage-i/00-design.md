# M1: Core Types - Design

## Scope of Work

Create the `isa/rv32fa/` directory with:
1. Copied ABI definitions from `rv32/abi.rs`
2. `PInst` enum mirroring all VInst variants with `PReg` (u8)
3. `PInst` text parser/formatter using role-specific register names (a0, s0, t0)
4. Module wiring in `isa/mod.rs`

## File Structure

```
lp-shader/lpvm-native/src/isa/
├── rv32/                    # UNCHANGED - existing pipeline
│   ├── abi.rs
│   ├── emit.rs
│   ├── inst.rs
│   ├── lower.rs
│   └── mod.rs
│
└── rv32fa/                  # NEW
    ├── mod.rs               # Module exports
    ├── abi.rs               # Copied from rv32/abi.rs
    ├── inst.rs              # PInst enum
    └── debug/
        ├── mod.rs           # Debug module exports
        └── pinst.rs         # Parser and formatter
```

## Conceptual Architecture

```
VInst (virtual registers)
   |
   | lower/alloc
   v
PInst (physical registers)
   |
   | emit
   v
bytes (machine code)
```

PInst is a parallel IR to VInst:
- Same operations, same semantics
- VReg → PReg (u8)
- IConst32 → LoadImm (rematerialized)
- Added: FrameSetup, FrameTeardown (prologue/epilogue)

## Main Components

### 1. abi.rs (copied from rv32/abi.rs)

ABI definitions for RV32. Includes:
- `ARG_REGS`, `RET_REGS`: Register lists
- `FP_REG`, `SP_REG`, `RA_REG`: Special register indices
- `callee_saved_int()`, `caller_saved_int()`: Register sets
- `reg_name()`: Register name for debugging
- `parse_reg()`: Parse "a0" -> 10, "s0" -> 8, etc.

### 2. inst.rs

```rust
pub type PReg = u8;

pub enum PInst {
    // Frame operations
    FrameSetup { spill_slots: u32 },
    FrameTeardown { spill_slots: u32 },

    // Arithmetic
    Add { dst: PReg, src1: PReg, src2: PReg },
    Sub { dst: PReg, src1: PReg, src2: PReg },
    // ... all other arithmetic (Mul, And, etc.)
    // ... Div*, Rem*

    // Unary
    Neg { dst: PReg, src: PReg },
    Not { dst: PReg, src: PReg },
    Mv { dst: PReg, src: PReg },

    // Comparison
    Slt { dst: PReg, src1: PReg, src2: PReg },
    Seqz { dst: PReg, src: PReg },

    // Memory
    Lw { dst: PReg, base: PReg, offset: i32 },
    Sw { src: PReg, base: PReg, offset: i32 },
    MemcpyWords { dst: PReg, src: PReg, size: u32 },
    SlotAddr { dst: PReg, slot: u32 },

    // Immediate (rematerialized)
    Li { dst: PReg, imm: i32 },

    // Control
    Call { target: SymbolRef },
    Ret,
}
```

### 3. debug/pinst.rs

Parser and formatter using **standard RISC-V assembly syntax**:

**Format:**
```asm
add   a0, a1, a2        # a0 = a1 + a2
sub   a0, a1, a2        # a0 = a1 - a2
mul   a0, a1, a2        # a0 = a1 * a2
li    a0, 42            # a0 = 42 (LoadImm)
lw    a0, 4(a1)         # a0 = mem[a1 + 4]
sw    a0, 4(a1)         # mem[a1 + 4] = a0
mv    a0, a1            # a0 = a1
neg   a0, a1            # a0 = -a1
seqz  a0, a1            # a0 = (a1 == 0)
snez  a0, a1            # a0 = (a1 != 0)
sltz  a0, a1            # a0 = (a1 < 0)
sgtz  a0, a1            # a0 = (a1 > 0)
call  mod               # call function
ret                     # return
FrameSetup 4            # prologue with 4 spill slots
FrameTeardown 4         # epilogue
```

**Standard RISC-V mnemonics used:**
- `add`, `sub`, `mul`, `div`, `rem` (and unsigned variants with 'u' suffix)
- `and`, `or`, `xor`, `sll`, `srl`, `sra`
- `addi` for immediate (but we use `li` for LoadImm)
- `lw`/`sw` for Load32/Store32
- `mv` for Mov32, `neg` for Neg32, `not` for Bnot32
- `beq`, `bne`, `blt`, `bge`, `bltu`, `bgeu` for branches
- `j` for Br, `call` for Call, `ret` for Ret
- `li` (load immediate) pseudoinstruction

**Register naming:** Standard RISC-V ABI names
- `a0`-`a7`: x10-x17 (arguments/returns)
- `s0`-`s11`: x8-x9, x18-x27 (callee-saved)
- `t0`-`t6`: x5-x7, x28-x31 (caller-saved/temporaries)
- `ra`: x1 (return address)
- `sp`: x2 (stack pointer)
- `fp`/`s0`: x8 (frame pointer)

**Functions:**
- `parse(input: &str) -> Result<Vec<PInst>, ParseError>`
- `format(pinsts: &[PInst]) -> String`
- `parse_reg(name: &str) -> Result<PReg, ParseError>`: "a0" -> 10
- `reg_name(reg: PReg) -> &'static str`: 10 -> "a0"

## Key Design Decisions

1. **Copy ABI, don't re-export**: Clean separation per roadmap
2. **Role-specific register names**: a0, s0, t0 instead of r0-r31 - more readable
3. **Complete PInst enum**: All variants now, no uncertainty
4. **LoadImm replaces IConst32**: Explicit rematerialization at emit time
5. **Frame operations explicit**: FrameSetup/Teardown are PInst, not VInst

## Differences from VInst

| Aspect | VInst | PInst |
|--------|-------|-----|
| Registers | `VReg` (virtual) | `PReg` (u8, physical) |
| Constants | `IConst32 { dst, val }` | `Li { dst, imm }` |
| Frame | Not represented | `FrameSetup`, `FrameTeardown` |
| Text format | `i0 = Add32 i1, i2` | `add a0, a1, a2` (RISC-V asm) |

## Tests

- Parse/formatter roundtrip for each variant
- Invalid register names rejected
- Invalid instruction format returns error

## Validate

```bash
cargo test -p lpvm-native --lib -- rv32fa::debug::pinst
cargo check -p lpvm-native --lib
```
