# Phase 1: Instruction Encoding

## Scope

Implement R/I/S/B/U/J instruction encoding functions in `isa/rv32/inst.rs`. These are pure functions that take register numbers and immediates, returning 32-bit encoded instructions.

## Code Organization

- Place core format encoders first (`encode_r_type`, `encode_i_type`, etc.)
- Place convenience wrappers below (`encode_add`, `encode_lw`, etc.)
- Tests at bottom of file

## Implementation Details

```rust
//! RV32 instruction encoding (adapted from Cranelift fork).
//! Pure functions: register numbers + immediates -> 32-bit opcode.

/// R-type: | funct7 | rs2 | rs1 | funct3 | rd | opcode |
/// Bits:   | 31-25  |24-20|19-15| 14-12  |11-7|  6-0   |
pub fn encode_r_type(opcode: u32, rd: u32, funct3: u32, rs1: u32, rs2: u32, funct7: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

/// I-type: | imm[11:0] | rs1 | funct3 | rd | opcode |
pub fn encode_i_type(opcode: u32, rd: u32, funct3: u32, rs1: u32, imm: i32) -> u32 {
    let imm = imm as u32 & 0xFFF;
    (imm << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

/// S-type: | imm[11:5] | rs2 | rs1 | funct3 | imm[4:0] | opcode |
pub fn encode_s_type(opcode: u32, funct3: u32, rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    let imm_hi = (imm >> 5) & 0x7F;
    let imm_lo = imm & 0x1F;
    (imm_hi << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (imm_lo << 7) | opcode
}

/// U-type: | imm[31:12] | rd | opcode |
pub fn encode_u_type(opcode: u32, rd: u32, imm: i32) -> u32 {
    (imm as u32 & 0xFFFFF000) | (rd << 7) | opcode
}

/// J-type: | imm[20|10:1|11|19:12] | rd | opcode |
pub fn encode_j_type(opcode: u32, rd: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    let imm_20 = (imm >> 20) & 0x1;
    let imm_10_1 = (imm >> 1) & 0x3FF;
    let imm_11 = (imm >> 11) & 0x1;
    let imm_19_12 = (imm >> 12) & 0xFF;
    let imm_bits = (imm_20 << 19) | (imm_10_1 << 9) | (imm_11 << 8) | imm_19_12;
    (imm_bits << 12) | (rd << 7) | opcode
}

// RV32I opcodes
const OP_OP: u32 = 0b0110011;      // R-type ALU
const OP_OP_IMM: u32 = 0b0010011;  // I-type ALU
const OP_LOAD: u32 = 0b0000011;    // Loads
const OP_STORE: u32 = 0b0100011;   // Stores
const OP_LUI: u32 = 0b0110111;     // LUI
const OP_AUIPC: u32 = 0b0010111;   // AUIPC
const OP_JAL: u32 = 0b1101111;     // JAL
const OP_JALR: u32 = 0b1100111;    // JALR
const OP_BRANCH: u32 = 0b1100011;  // Branches

// funct3 values
const F3_ADD: u32 = 0b000;
const F3_SL: u32 = 0b001;  // shift left (for shift immediates)
const F3_SR: u32 = 0b101;  // shift right
const F3_LW: u32 = 0b010;  // load/store word

// funct7 values
const F7_ADD: u32 = 0b0000000;
const F7_SUB: u32 = 0b0100000;

/// add rd, rs1, rs2
pub fn encode_add(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_ADD, rs1, rs2, F7_ADD)
}

/// sub rd, rs1, rs2
pub fn encode_sub(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_ADD, rs1, rs2, F7_SUB)
}

/// lw rd, offset(rs1)
pub fn encode_lw(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD, rd, F3_LW, rs1, offset)
}

/// sw rs2, offset(rs1)
pub fn encode_sw(rs2: u32, rs1: u32, offset: i32) -> u32 {
    encode_s_type(OP_STORE, F3_LW, rs1, rs2, offset)
}

/// addi rd, rs1, imm
pub fn encode_addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_ADD, rs1, imm)
}

/// auipc rd, imm
pub fn encode_auipc(rd: u32, imm: i32) -> u32 {
    encode_u_type(OP_AUIPC, rd, imm)
}

/// jalr rd, rs1, offset
pub fn encode_jalr(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_JALR, rd, 0, rs1, offset)
}

/// ret (pseudo-op for jalr x0, x1, 0)
pub fn encode_ret() -> u32 {
    encode_jalr(0, 1, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encode_add() {
        // add x1, x2, x3
        // = 0000000_00011_00010_000_00001_0110011
        // = 0x003100b3
        assert_eq!(encode_add(1, 2, 3), 0x003100b3);
    }
    
    #[test]
    fn test_encode_sub() {
        // sub x1, x2, x3
        // = 0100000_00011_00010_000_00001_0110011
        // = 0x403100b3
        assert_eq!(encode_sub(1, 2, 3), 0x403100b3);
    }
    
    #[test]
    fn test_encode_lw_sw() {
        // lw x5, 4(x6)
        let lw = encode_lw(5, 6, 4);
        // sw x5, 4(x6)
        let sw = encode_sw(5, 6, 4);
        
        // sw imm[4:0] = rd field for lw
        // Verify both use same effective addressing
        assert_eq!(lw & 0x1F, 5);       // rd = x5
        assert_eq!(sw & 0x1F, 4 << 7);  // imm[4:0] positioned differently
    }
    
    #[test]
    fn test_encode_auipc_jalr() {
        // auipc x1, 0
        assert_eq!(encode_auipc(1, 0), 0x00000097);
        // jalr x1, x1, 0
        assert_eq!(encode_jalr(1, 1, 0), 0x000080e7);
        // ret = jalr x0, x1, 0
        assert_eq!(encode_ret(), 0x00008067);
    }
}
```

## Tests to Write

1. `test_encode_add` — Verify `add x1, x2, x3` = `0x003100b3`
2. `test_encode_sub` — Verify `sub x1, x2, x3` = `0x403100b3`
3. `test_encode_lw_sw` — Verify load/store encode correctly
4. `test_encode_auipc_jalr` — Call sequence opcodes
5. `test_encode_ret` — Pseudo-op expands to `jalr x0, x1, 0`

## Validate

```bash
cargo test -p lpvm-native --lib inst::tests
cargo check -p lpvm-native
```
