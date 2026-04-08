//! RV32I/M instruction encoding (32-bit little-endian).

use alloc::vec::Vec;

/// R-type: opcode | rd | funct3 | rs1 | rs2 | funct7
#[inline]
pub fn encode_r_type(opcode: u32, rd: u32, funct3: u32, rs1: u32, rs2: u32, funct7: u32) -> u32 {
    (opcode & 0x7f)
        | ((rd & 0x1f) << 7)
        | ((funct3 & 7) << 12)
        | ((rs1 & 0x1f) << 15)
        | ((rs2 & 0x1f) << 20)
        | ((funct7 & 0x7f) << 25)
}

/// I-type: imm[11:0] | rs1 | funct3 | rd | opcode
#[inline]
pub fn encode_i_type(opcode: u32, rd: u32, funct3: u32, rs1: u32, imm: i32) -> u32 {
    let imm = imm as u32 & 0xfff;
    imm << 20 | ((rs1 & 0x1f) << 15) | ((funct3 & 7) << 12) | ((rd & 0x1f) << 7) | (opcode & 0x7f)
}

/// S-type: imm[11:5] | rs2 | rs1 | funct3 | imm[4:0] | opcode
#[inline]
pub fn encode_s_type(opcode: u32, funct3: u32, rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    let imm_lo = imm & 0x1f;
    let imm_hi = (imm >> 5) & 0x7f;
    (opcode & 0x7f)
        | ((imm_lo & 0x1f) << 7)
        | ((funct3 & 7) << 12)
        | ((rs1 & 0x1f) << 15)
        | ((rs2 & 0x1f) << 20)
        | (imm_hi << 25)
}

/// U-type: imm[31:12] | rd | opcode
#[inline]
pub fn encode_u_type(opcode: u32, rd: u32, imm_hi20: u32) -> u32 {
    (imm_hi20 & 0xfffff) << 12 | ((rd & 0x1f) << 7) | (opcode & 0x7f)
}

const OP_OP: u32 = 0b0110011;
const OP_OP_IMM: u32 = 0b0010011;
const OP_LOAD: u32 = 0b0000011;
const OP_STORE: u32 = 0b0100011;
const OP_LUI: u32 = 0b0110111;
const OP_AUIPC: u32 = 0b0010111;
const OP_JALR: u32 = 0b1100111;

const F3_ADD: u32 = 0;
const F3_LW: u32 = 0b010;

const F7_ADD: u32 = 0;
const F7_SUB: u32 = 0b0100000;
const F7_MUL: u32 = 0b0000001;

/// add rd, rs1, rs2
#[inline]
pub fn encode_add(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_ADD, rs1, rs2, F7_ADD)
}

/// sub rd, rs1, rs2
#[inline]
pub fn encode_sub(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_ADD, rs1, rs2, F7_SUB)
}

/// mul rd, rs1, rs2 (M extension)
#[inline]
pub fn encode_mul(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_ADD, rs1, rs2, F7_MUL)
}

/// addi rd, rs1, imm
#[inline]
pub fn encode_addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_ADD, rs1, imm)
}

/// lui rd, imm — `imm` is the high 20 bits (will be placed in imm[31:12]).
#[inline]
pub fn encode_lui(rd: u32, imm_hi20: u32) -> u32 {
    encode_u_type(OP_LUI, rd, imm_hi20 & 0xfffff)
}

/// auipc rd, imm — same immediate layout as lui.
#[inline]
pub fn encode_auipc(rd: u32, imm_hi20: u32) -> u32 {
    encode_u_type(OP_AUIPC, rd, imm_hi20 & 0xfffff)
}

/// lw rd, offset(rs1)
#[inline]
pub fn encode_lw(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD, rd, F3_LW, rs1, offset)
}

/// sw rs2, offset(rs1)
#[inline]
pub fn encode_sw(rs2: u32, rs1: u32, offset: i32) -> u32 {
    encode_s_type(OP_STORE, F3_LW, rs1, rs2, offset)
}

/// jalr rd, rs1, offset
#[inline]
pub fn encode_jalr(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_JALR, rd, 0, rs1, offset)
}

/// ret = jalr x0, x1, 0
#[inline]
pub fn encode_ret() -> u32 {
    encode_jalr(0, 1, 0)
}

/// Encode instructions to load a signed 32-bit constant into `rd` (lui+addi or addi from x0).
pub fn iconst32_sequence(rd: u32, value: i32) -> Vec<u32> {
    let mut out = Vec::new();
    if (-2048..2048).contains(&value) {
        out.push(encode_addi(rd, 0, value));
        return out;
    }
    let v = value as u32;
    let mut upper = (v >> 12) as i32;
    if (v & 0x800) != 0 {
        upper = upper.wrapping_add(1);
    }
    out.push(encode_lui(rd, upper as u32 & 0xfffff));
    let lower = ((v & 0xfff) as i32) << 20 >> 20;
    if lower != 0 {
        out.push(encode_addi(rd, rd, lower));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_add_x1_x2_x3() {
        assert_eq!(encode_add(1, 2, 3), 0x003100b3);
    }

    #[test]
    fn encode_sub_x1_x2_x3() {
        assert_eq!(encode_sub(1, 2, 3), 0x403100b3);
    }

    #[test]
    fn encode_auipc_jalr_ret() {
        assert_eq!(encode_auipc(1, 0), 0x00000097);
        assert_eq!(encode_jalr(1, 1, 0), 0x000080e7);
        assert_eq!(encode_ret(), 0x00008067);
    }
}
