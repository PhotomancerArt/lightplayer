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
const OP_BRANCH: u32 = 0b1100011;
const OP_JAL: u32 = 0b1101111;

const F3_ADD: u32 = 0;
const F3_BEQ: u32 = 0;
const F3_BNE: u32 = 0b001;
const F3_SLL: u32 = 0b001;
const F3_SLT: u32 = 0b010;
const F3_SLTU: u32 = 0b011;
/// Shared with `div` (M): `xor` uses funct7=0, `div` uses funct7=1.
const F3_XOR_DIV: u32 = 0b100;
/// Shared with `divu` / `srl`.
const F3_SRL_DIVU: u32 = 0b101;
/// Shared with `rem` / `or`.
const F3_OR_REM: u32 = 0b110;
/// Shared with `remu` / `and`.
const F3_AND_REMU: u32 = 0b111;
const F3_LW: u32 = 0b010;
const F3_LB: u32 = 0b000;
const F3_LH: u32 = 0b001;
const F3_LBU: u32 = 0b100;
const F3_LHU: u32 = 0b101;
const F3_SB: u32 = 0b000;
const F3_SH: u32 = 0b001;

const F7_ADD: u32 = 0;
const F7_SUB: u32 = 0b0100000;
const F7_MUL: u32 = 0b0000001;
const F7_MEXT: u32 = 0b0000001;

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

/// and rd, rs1, rs2
#[inline]
pub fn encode_and(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_AND_REMU, rs1, rs2, F7_ADD)
}

/// xor rd, rs1, rs2
#[inline]
pub fn encode_xor(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_XOR_DIV, rs1, rs2, F7_ADD)
}

/// or rd, rs1, rs2
#[inline]
pub fn encode_or(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_OR_REM, rs1, rs2, F7_ADD)
}

/// sll rd, rs1, rs2 (shift amount = rs2[4:0])
#[inline]
pub fn encode_sll(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SLL, rs1, rs2, F7_ADD)
}

/// srl rd, rs1, rs2 (logical; shift amount = rs2[4:0])
#[inline]
pub fn encode_srl(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SRL_DIVU, rs1, rs2, F7_ADD)
}

/// sra rd, rs1, rs2 (arithmetic; shift amount = rs2[4:0])
#[inline]
pub fn encode_sra(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SRL_DIVU, rs1, rs2, F7_SUB)
}

/// div rd, rs1, rs2 (M extension, signed)
#[inline]
pub fn encode_div(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_XOR_DIV, rs1, rs2, F7_MEXT)
}

/// divu rd, rs1, rs2 (M extension, unsigned)
#[inline]
pub fn encode_divu(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SRL_DIVU, rs1, rs2, F7_MEXT)
}

/// rem rd, rs1, rs2 (M extension, signed)
#[inline]
pub fn encode_rem(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_OR_REM, rs1, rs2, F7_MEXT)
}

/// remu rd, rs1, rs2 (M extension, unsigned)
#[inline]
pub fn encode_remu(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_AND_REMU, rs1, rs2, F7_MEXT)
}

/// slt rd, rs1, rs2 (signed less-than)
#[inline]
pub fn encode_slt(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SLT, rs1, rs2, F7_ADD)
}

/// sltu rd, rs1, rs2 (unsigned less-than)
#[inline]
pub fn encode_sltu(rd: u32, rs1: u32, rs2: u32) -> u32 {
    encode_r_type(OP_OP, rd, F3_SLTU, rs1, rs2, F7_ADD)
}

/// slti rd, rs1, imm (signed less-than immediate)
#[inline]
pub fn encode_slti(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_SLT, rs1, imm)
}

/// sltiu rd, rs1, imm
#[inline]
pub fn encode_sltiu(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_SLTU, rs1, imm)
}

/// xori rd, rs1, imm
#[inline]
pub fn encode_xori(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_XOR_DIV, rs1, imm)
}

/// ori rd, rs1, imm
#[inline]
pub fn encode_ori(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_OR_REM, rs1, imm)
}

/// andi rd, rs1, imm
#[inline]
pub fn encode_andi(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_AND_REMU, rs1, imm)
}

/// addi rd, rs1, imm
#[inline]
pub fn encode_addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_ADD, rs1, imm)
}

/// slli rd, rs1, shamt (shift left logical immediate, shamt in 0..31)
#[inline]
pub fn encode_slli(rd: u32, rs1: u32, shamt: u32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_SLL, rs1, (shamt & 0x1f) as i32)
}

/// srli rd, rs1, shamt (shift right logical immediate, shamt in 0..31)
#[inline]
pub fn encode_srli(rd: u32, rs1: u32, shamt: u32) -> u32 {
    encode_i_type(OP_OP_IMM, rd, F3_SRL_DIVU, rs1, (shamt & 0x1f) as i32)
}

/// srai rd, rs1, shamt (shift right arithmetic immediate, shamt in 0..31)
#[inline]
pub fn encode_srai(rd: u32, rs1: u32, shamt: u32) -> u32 {
    let imm = (0b0100000 << 5) | (shamt & 0x1f);
    encode_i_type(OP_OP_IMM, rd, F3_SRL_DIVU, rs1, imm as i32)
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

/// lb rd, offset(rs1)
#[inline]
pub fn encode_lb(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD, rd, F3_LB, rs1, offset)
}

/// lbu rd, offset(rs1)
#[inline]
pub fn encode_lbu(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD, rd, F3_LBU, rs1, offset)
}

/// lh rd, offset(rs1)
#[inline]
pub fn encode_lh(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD, rd, F3_LH, rs1, offset)
}

/// lhu rd, offset(rs1)
#[inline]
pub fn encode_lhu(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD, rd, F3_LHU, rs1, offset)
}

/// sb rs2, offset(rs1)
#[inline]
pub fn encode_sb(rs2: u32, rs1: u32, offset: i32) -> u32 {
    encode_s_type(OP_STORE, F3_SB, rs1, rs2, offset)
}

/// sh rs2, offset(rs1)
#[inline]
pub fn encode_sh(rs2: u32, rs1: u32, offset: i32) -> u32 {
    encode_s_type(OP_STORE, F3_SH, rs1, rs2, offset)
}

/// B-type branch: `imm` is byte offset (must be even, ±4 KiB).
#[inline]
pub fn encode_b_type(funct3: u32, rs1: u32, rs2: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    debug_assert!((imm & 1) == 0, "branch offset must be 2-byte aligned");
    let imm_12 = (imm >> 12) & 1;
    let imm_10_5 = (imm >> 5) & 0x3f;
    let imm_4_1 = (imm >> 1) & 0xf;
    let imm_11 = (imm >> 11) & 1;
    OP_BRANCH
        | (imm_11 << 7)
        | (imm_4_1 << 8)
        | ((funct3 & 7) << 12)
        | ((rs1 & 0x1f) << 15)
        | ((rs2 & 0x1f) << 20)
        | (imm_10_5 << 25)
        | (imm_12 << 31)
}

/// beq rs1, rs2, imm
#[inline]
pub fn encode_beq(rs1: u32, rs2: u32, imm: i32) -> u32 {
    encode_b_type(F3_BEQ, rs1, rs2, imm)
}

/// bne rs1, rs2, imm
#[inline]
pub fn encode_bne(rs1: u32, rs2: u32, imm: i32) -> u32 {
    encode_b_type(F3_BNE, rs1, rs2, imm)
}

/// jal rd, imm — `imm` is byte offset (must be even, ±1 MiB).
#[inline]
pub fn encode_jal(rd: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    debug_assert!((imm & 1) == 0, "jal offset must be 2-byte aligned");
    let imm_20 = (imm >> 20) & 1;
    let imm_10_1 = (imm >> 1) & 0x3ff;
    let imm_11 = (imm >> 11) & 1;
    let imm_19_12 = (imm >> 12) & 0xff;
    (OP_JAL & 0x7f)
        | ((rd & 0x1f) << 7)
        | (imm_19_12 << 12)
        | (imm_11 << 20)
        | (imm_10_1 << 21)
        | (imm_20 << 31)
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
    fn encode_lb_lbu_lh_lhu_sb_sh_smoke() {
        assert_eq!(encode_lb(1, 2, 4), 0x00410083);
        assert_eq!(encode_lbu(1, 2, 4), 0x00414083);
        assert_eq!(encode_lh(1, 2, 4), 0x00411083);
        assert_eq!(encode_lhu(1, 2, 4), 0x00415083);
        assert_eq!(encode_sb(1, 2, 4), 0x00110223);
        assert_eq!(encode_sh(1, 2, 4), 0x00111223);
    }

    #[test]
    fn encode_sub_x1_x2_x3() {
        assert_eq!(encode_sub(1, 2, 3), 0x403100b3);
    }

    #[test]
    fn encode_and_x1_x2_x3() {
        assert_eq!(encode_and(1, 2, 3), 0x003170b3);
    }

    #[test]
    fn encode_div_x4_x5_x6() {
        assert_eq!(encode_div(4, 5, 6), 0x0262c233);
    }

    #[test]
    fn encode_or_sll() {
        assert_eq!(encode_or(1, 2, 3), 0x003160b3);
        assert_eq!(encode_sll(1, 2, 3), 0x003110b3);
    }

    #[test]
    fn encode_slt_x7_x8_x9() {
        assert_eq!(encode_slt(7, 8, 9), 0x009423b3);
    }

    #[test]
    fn encode_auipc_jalr_ret() {
        assert_eq!(encode_auipc(1, 0), 0x00000097);
        assert_eq!(encode_jalr(1, 1, 0), 0x000080e7);
        assert_eq!(encode_ret(), 0x00008067);
    }

    #[test]
    fn encode_beq_bne_jal_smoke() {
        let beq = encode_beq(1, 2, 16);
        assert_eq!(beq & 0x7f, 0x63);
        let bne = encode_bne(3, 4, -8);
        assert_eq!(bne & 0x7f, 0x63);
        let jal = encode_jal(0, 32);
        assert_eq!(jal & 0x7f, 0x6f);
    }
}
