## Scope of Phase

Add RV32 instruction encoders for division, remainder, set-less-than, and bitwise operations.

## Code Organization Reminders

- Place encoders in alphabetical order by instruction name
- Use `encode_` prefix consistently
- Add doc comments explaining the instruction
- Test each encoder produces correct bytes

## Implementation Details

### Changes to `isa/rv32/inst.rs`

The file already has `encode_r_type` and `encode_i_type` helpers. Add new encoders as wrappers:

```rust
// Add new funct3 constants near existing ones
const F3_SLT: u32 = 0b010;
const F3_SLTU: u32 = 0b011;
const F3_XOR: u32 = 0b100;
const F3_SRL: u32 = 0b101;  // Also used for divu
const F3_OR: u32 = 0b110;   // Also used for rem
const F3_AND: u32 = 0b111;  // Also used for remu

// Add funct7 for M extension (div/rem)
const F7_MEXT: u32 = 0b0000001;

/// and rd, rs1, rs2
#[inline]
pub fn encode_and(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_AND, rs1, rs2, 0)
}

/// div rd, rs1, rs2 (M extension - signed division)
#[inline]
pub fn encode_div(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SRL, rs1, rs2, F7_MEXT)
}

/// divu rd, rs1, rs2 (M extension - unsigned division)
#[inline]
pub fn encode_divu(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SRLU, rs1, rs2, F7_MEXT)
}

/// rem rd, rs1, rs2 (M extension - signed remainder)
#[inline]
pub fn encode_rem(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_OR, rs1, rs2, F7_MEXT)
}

/// remu rd, rs1, rs2 (M extension - unsigned remainder)
#[inline]
pub fn encode_remu(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_AND, rs1, rs2, F7_MEXT)
}

/// slt rd, rs1, rs2 (set less than, signed)
#[inline]
pub fn encode_slt(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SLT, rs1, rs2, 0)
}

/// sltu rd, rs1, rs2 (set less than, unsigned)
#[inline]
pub fn encode_sltu(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SLTU, rs1, rs2, 0)
}

/// sltiu rd, rs1, imm (set less than immediate, unsigned)
#[inline]
pub fn encode_sltiu(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_SLTU, rs1, imm)
}

/// xori rd, rs1, imm (XOR immediate)
#[inline]
pub fn encode_xori(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_XOR, rs1, imm)
}

/// xor rd, rs1, rs2 (bitwise XOR)
#[inline]
pub fn encode_xor(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_XOR, rs1, rs2, 0)
}
```

**Note**: Need to add `F3_SRLU` constant (same as `F3_SLTU` which is 0b101, used by divu).

### Tests

Add unit tests for each encoder at the bottom of `inst.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_x1_x2_x3() {
        let word = encode_and(1, 2, 3);
        // AND x1, x2, x3 = 0x0011F0B3
        assert_eq!(word, 0x0011F0B3);
    }

    #[test]
    fn encode_div_x4_x5_x6() {
        let word = encode_div(4, 5, 6);
        // DIV x4, x5, x6 = 0x02628233
        assert_eq!(word, 0x02628233);
    }

    #[test]
    fn encode_slt_x7_x8_x9() {
        let word = encode_slt(7, 8, 9);
        // SLT x7, x8, x9 = 0x009443B3
        assert_eq!(word, 0x009443B3);
    }

    // ... similar tests for divu, rem, remu, sltu, sltiu, xori, xor
}
```

## Validate

```bash
cargo test -p lpvm-native -- isa::rv32::inst::tests
cargo check -p lpvm-native
```

All new encoder tests should pass.