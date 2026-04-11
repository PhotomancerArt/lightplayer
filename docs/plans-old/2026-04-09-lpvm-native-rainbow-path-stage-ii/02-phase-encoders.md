# Phase 2: RV32 Branch Encoders

## Scope of Phase

Add instruction encoders for RV32 branch and jump instructions.

## Code Organization Reminders

- Use the generic `encode_b_type` helper (add if missing)
- Place new encoders after arithmetic encoders, before memory encoders
- Add unit tests for each new encoder

## Implementation Details

### 1. Add B-type encoder helper

In `isa/rv32/inst.rs`, add if not present:

```rust
/// B-type: imm[12|10:5] | rs2 | rs1 | funct3 | imm[4:1|11] | opcode
#[inline]
pub fn encode_b_type(opcode: u32, funct3: u32, rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    let imm_12 = (imm >> 12) & 1;
    let imm_10_5 = (imm >> 5) & 0x3f;
    let imm_4_1 = (imm >> 1) & 0xf;
    let imm_11 = (imm >> 11) & 1;
    
    (opcode & 0x7f)
        | ((imm_11 << 7) | (imm_4_1 << 8) | (imm_10_5 << 25) | (imm_12 << 31))
        | ((funct3 & 7) << 12)
        | ((rs1 & 0x1f) << 15)
        | ((rs2 & 0x1f) << 20)
}
```

Note: B-type immediates are scaled by 2 (branch targets are 2-byte aligned), but our offsets are in bytes. The hardware shifts left by 1, so we pass byte offset directly and the range is ±4KB.

### 2. Add branch encoders

```rust
const OP_BRANCH: u32 = 0b1100011;
const F3_BEQ: u32 = 0b000;
const F3_BNE: u32 = 0b001;

/// beq rs1, rs2, offset (branch if rs1 == rs2)
#[inline]
pub fn encode_beq(rs1: u32, rs2: u32, offset: i32) -> u32 {
    encode_b_type(OP_BRANCH, F3_BEQ, rs1, rs2, offset)
}

/// bne rs1, rs2, offset (branch if rs1 != rs2)
#[inline]
pub fn encode_bne(rs1: u32, rs2: u32, offset: i32) -> u32 {
    encode_b_type(OP_BRANCH, F3_BNE, rs1, rs2, offset)
}

/// jal rd, offset (jump and link)
/// For unconditional jump: jal x0, offset
#[inline]
pub fn encode_jal(rd: u32, offset: i32) -> u32 {
    let imm = offset as u32;
    let imm_20 = (imm >> 20) & 1;
    let imm_10_1 = (imm >> 1) & 0x3ff;
    let imm_11 = (imm >> 11) & 1;
    let imm_19_12 = (imm >> 12) & 0xff;
    
    (OP_JAL & 0x7f)
        | ((rd & 0x1f) << 7)
        | ((imm_19_12) << 12)
        | ((imm_11) << 20)
        | ((imm_10_1) << 21)
        | ((imm_20) << 31)
}

const OP_JAL: u32 = 0b1101111;
```

### 3. Add unit tests

```rust
#[test]
fn encode_beq_forward() {
    // beq x1, x2, +16 (skip 4 instructions forward)
    let instr = encode_beq(1, 2, 16);
    // Verify opcode is BRANCH (0b1100011 = 0x63)
    assert_eq!(instr & 0x7f, 0x63);
}

#[test]
fn encode_bne_backward() {
    // bne x3, x4, -8 (skip 2 instructions backward)
    let instr = encode_bne(3, 4, -8);
    assert_eq!(instr & 0x7f, 0x63);
}

#[test]
fn encode_jal_unconditional() {
    // jal x0, 32 (jump forward 32 bytes, discard link)
    let instr = encode_jal(0, 32);
    // Verify opcode is JAL (0b1101111 = 0x6f)
    assert_eq!(instr & 0x7f, 0x6f);
}
```

## Tests

Run unit tests:
```bash
cargo test -p lpvm-native encode_b
cargo test -p lpvm-native encode_jal
```

## Validate

```bash
cargo test -p lpvm-native
```

Expected: All tests pass.
