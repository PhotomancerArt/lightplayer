# Milestone 2: RV32 Emission

**Goal**: VInst with assigned registers → RV32 machine code bytes.

## Suggested Plan Name

`lpvm-native-m2`

## Scope

### In Scope
- RV32 instruction encoding (R/I/S/B/U/J formats)
- Physical register definitions (x0-x31, a0-a7, s0-s11, etc.)
- VInst → RV32 instruction selection and emission
- Shader ABI: frame setup, prologue/epilogue
- Builtin call emission with relocation records
- Stack slot management (emergency spill paths)

### Explicitly Out of Scope
- No control flow instructions (branches, jumps) - not needed for `op-add.glsl`
- No 64-bit operations (stubbed, panics if hit)
- No ELF container (raw bytes + metadata for now)
- No actual linking (just relocation records)

## Key Decisions

### Instruction Encoding
Pure functions, no state. Study `qbe/rv64/emit.c` pattern.

```rust
// isa/rv32/inst.rs
pub fn encode_r(funct7: u32, rs2: u8, rs1: u8, funct3: u32, rd: u8, opcode: u32) -> u32 {
    (funct7 << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) |
    (funct3 << 12) | ((rd as u32) << 7) | opcode
}

pub fn encode_add(rd: PhysReg, rs1: PhysReg, rs2: PhysReg) -> u32 {
    encode_r(0b0000000, rs2, rs1, 0b000, rd, 0b0110011)
}

pub fn encode_jal(rd: PhysReg, offset: i32) -> u32 {
    encode_j(offset, rd, 0b1101111)
}
```

### Shader ABI
Minimal, designed for LPIR characteristics:

```rust
// Frame layout (grows down from sp):
// [saved ra] [saved s0] [spill slots...] [padding to 16B]
// sp points to bottom of frame throughout function

struct FrameLayout {
    spill_size: u32,    // Determined by regalloc
    saved_regs: Vec<PhysReg>,  // ra, s0 if needed
    total_size: u32,    // Rounded to 16 bytes
}
```

Call convention:
- Args: a0, a1, a2, a3...
- Returns: a0, a1
- Caller-saved: a0-a7, t0-t6
- Callee-saved: s0-s11 (we use s0 as FP if needed)

### Relocation Records
For linking with builtins later:

```rust
pub struct Relocation {
    pub offset: usize,        // Byte offset in code
    pub symbol: String,       // "__lp_lpir_fadd_q32"
    pub kind: RelocKind,      // JalTarget, etc.
}

pub struct CodeWithRelocs {
    pub bytes: Vec<u8>,
    pub relocs: Vec<Relocation>,
}
```

## Deliverables

| File | Contents |
|------|----------|
| `src/isa/rv32/inst.rs` | Instruction encoding functions (R/I/S/B/U/J) |
| `src/isa/rv32/reg.rs` | Physical register definitions, register class mapping |
| `src/isa/rv32/abi.rs` | Frame layout, calling convention, prologue/epilogue |
| `src/isa/rv32/emit.rs` | `EmitContext`, `emit_vinst()` → bytes |
| `src/output/elf.rs` stub | `Reloc` struct, placeholder for M3 |

## Dependencies

- M1 complete (types, VInst, regalloc interface)

## Estimated Scope

- ~900 lines
- 2-3 days
- Complexity: bit manipulation, ABI edge cases

## Validation

Unit tests for encoding:
```rust
#[test]
fn test_encode_add() {
    // add x1, x2, x3
    let inst = encode_add(1, 2, 3);
    assert_eq!(inst, 0x003100b3);  // Verified against riscv64-unknown-elf-as
}
```

Integration test (no execution yet):
```rust
#[test]
fn test_emit_simple() {
    let vinsts = vec![
        VInst::Add32 { dst: v0, src1: v1, src2: v2 },
    ];
    let alloc = GreedyAlloc::new();
    let result = alloc.allocate(&vinsts, &types);
    let bytes = rv32_emit(&vinsts, &result);
    // Verify bytes are valid RV32 (disassemble with objdump)
}
```
