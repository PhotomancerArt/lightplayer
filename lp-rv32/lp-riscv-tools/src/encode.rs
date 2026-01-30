//! RISC-V 32-bit instruction encoding.
//!
//! This module provides functions to encode RISC-V instructions
//! into their 32-bit binary representation.

use super::regs::Gpr;

/// Encode an R-type instruction.
///
/// Format: `opcode rd rs1 rs2 funct3 funct7`
fn encode_r(opcode: u8, rd: Gpr, rs1: Gpr, rs2: Gpr, funct3: u8, funct7: u8) -> u32 {
    use super::format::TypeR;
    let func = ((funct7 as u16) << 3) | (funct3 as u16);
    TypeR {
        rd: rd.num(),
        rs1: rs1.num(),
        rs2: rs2.num(),
        func,
    }
    .to_riscv(opcode)
}

/// Encode an I-type instruction.
///
/// Format: `opcode rd rs1 imm[11:0] funct3`
fn encode_i(opcode: u8, rd: Gpr, rs1: Gpr, imm: i32, funct3: u8) -> u32 {
    use super::format::TypeI;
    TypeI {
        rd: rd.num(),
        rs1: rs1.num(),
        imm,
        func: funct3,
    }
    .to_riscv(opcode)
}

/// Encode an S-type instruction.
///
/// Format: `opcode imm[4:0] rs1 rs2 imm[11:5] funct3`
fn encode_s(opcode: u8, rs1: Gpr, rs2: Gpr, imm: i32, funct3: u8) -> u32 {
    use super::format::TypeS;
    TypeS {
        rs1: rs1.num(),
        rs2: rs2.num(),
        imm,
        func: funct3,
    }
    .to_riscv(opcode)
}

/// Encode a U-type instruction.
///
/// Format: `opcode rd imm[31:12]`
fn encode_u(opcode: u8, rd: Gpr, imm: u32) -> u32 {
    use super::format::TypeU;
    TypeU {
        rd: rd.num(),
        imm: imm as i32,
    }
    .to_riscv(opcode)
}

/// Encode a J-type instruction.
///
/// Format: `opcode rd imm[20|10:1|11|19:12]`
fn encode_j(opcode: u8, rd: Gpr, imm: i32) -> u32 {
    use super::format::TypeJ;
    TypeJ { rd: rd.num(), imm }.to_riscv(opcode)
}

/// Encode a B-type instruction.
///
/// Format: `opcode imm[12|10:5] rs1 rs2 imm[4:1|11] funct3`
fn encode_b(opcode: u8, rs1: Gpr, rs2: Gpr, imm: i32, funct3: u8) -> u32 {
    use super::format::TypeB;
    TypeB {
        rs1: rs1.num(),
        rs2: rs2.num(),
        imm,
        func: funct3,
    }
    .to_riscv(opcode)
}

// Arithmetic instructions

/// ADD: rd = rs1 + rs2
pub fn add(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x0, 0x0)
}

/// SUB: rd = rs1 - rs2
pub fn sub(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x0, 0x20)
}

/// MUL: rd = rs1 * rs2 (M extension)
pub fn mul(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x0, 0x01)
}

/// MULH: rd = high 32 bits of (rs1 * rs2) (signed, M extension)
pub fn mulh(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x1, 0x01)
}

/// MULHSU: rd = high 32 bits of (rs1 * rs2) (signed * unsigned, M extension)
pub fn mulhsu(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x2, 0x01)
}

/// MULHU: rd = high 32 bits of (rs1 * rs2) (unsigned, M extension)
pub fn mulhu(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x3, 0x01)
}

/// DIV: rd = rs1 / rs2 (signed, M extension)
pub fn div(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x4, 0x01)
}

/// DIVU: rd = rs1 / rs2 (unsigned, M extension)
pub fn divu(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x5, 0x01)
}

/// REM: rd = rs1 % rs2 (signed, M extension)
pub fn rem(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x6, 0x01)
}

/// REMU: rd = rs1 % rs2 (unsigned, M extension)
pub fn remu(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x7, 0x01)
}

/// ADDI: rd = rs1 + imm
pub fn addi(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x0)
}

// Load/Store instructions

/// LB: rd = sign_extend(mem[rs1 + imm][7:0])
pub fn lb(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x03, rd, rs1, imm, 0x0)
}

/// LH: rd = sign_extend(mem[rs1 + imm][15:0])
pub fn lh(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x03, rd, rs1, imm, 0x1)
}

/// LW: rd = mem[rs1 + imm]
pub fn lw(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x03, rd, rs1, imm, 0x2)
}

/// LBU: rd = zero_extend(mem[rs1 + imm][7:0])
pub fn lbu(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x03, rd, rs1, imm, 0x4)
}

/// LHU: rd = zero_extend(mem[rs1 + imm][15:0])
pub fn lhu(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x03, rd, rs1, imm, 0x5)
}

/// SB: mem[rs1 + imm][7:0] = rs2[7:0]
pub fn sb(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_s(0x23, rs1, rs2, imm, 0x0)
}

/// SH: mem[rs1 + imm][15:0] = rs2[15:0]
pub fn sh(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_s(0x23, rs1, rs2, imm, 0x1)
}

/// SW: mem[rs1 + imm] = rs2
pub fn sw(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_s(0x23, rs1, rs2, imm, 0x2)
}

// Control flow instructions

/// JAL: rd = pc + 4; pc = pc + imm
pub fn jal(rd: Gpr, imm: i32) -> u32 {
    encode_j(0x6f, rd, imm)
}

/// JALR: rd = pc + 4; pc = rs1 + imm
pub fn jalr(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x67, rd, rs1, imm, 0x0)
}

/// BEQ: if rs1 == rs2, pc = pc + imm
pub fn beq(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x0)
}

/// BNE: if rs1 != rs2, pc = pc + imm
pub fn bne(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x1)
}

/// BLT: if rs1 < rs2 (signed), pc = pc + imm
pub fn blt(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x4)
}

/// BGE: if rs1 >= rs2 (signed), pc = pc + imm
pub fn bge(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x5)
}

/// BLTU: if rs1 < rs2 (unsigned), pc = pc + imm
pub fn bltu(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x6)
}

/// BGEU: if rs1 >= rs2 (unsigned), pc = pc + imm
pub fn bgeu(rs1: Gpr, rs2: Gpr, imm: i32) -> u32 {
    encode_b(0x63, rs1, rs2, imm, 0x7)
}

// Comparison instructions

/// SLT: rd = (rs1 < rs2) ? 1 : 0 (signed)
pub fn slt(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x2, 0x0)
}

/// SLTI: rd = (rs1 < imm) ? 1 : 0 (signed)
pub fn slti(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x2)
}

/// SLTU: rd = (rs1 < rs2) ? 1 : 0 (unsigned)
pub fn sltu(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x3, 0x0)
}

/// SLTIU: rd = (rs1 < imm) ? 1 : 0 (unsigned)
pub fn sltiu(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x3)
}

/// XORI: rd = rs1 ^ imm
pub fn xori(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x4)
}

// Logical instructions

/// AND: rd = rs1 & rs2
pub fn and(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x7, 0x0)
}

/// ANDI: rd = rs1 & imm
pub fn andi(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x7)
}

/// OR: rd = rs1 | rs2
pub fn or(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x6, 0x0)
}

/// ORI: rd = rs1 | imm
pub fn ori(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i(0x13, rd, rs1, imm, 0x6)
}

/// XOR: rd = rs1 ^ rs2
pub fn xor(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x4, 0x0)
}

// Shift instructions

/// SLL: rd = rs1 << rs2 (logical left shift)
pub fn sll(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x1, 0x0)
}

/// SLLI: rd = rs1 << imm (logical left shift immediate)
/// Note: imm[11:5] must be 0, only imm[4:0] is used for shift amount
pub fn slli(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    // For SLLI, imm[11:5] must be 0, so we only use imm[4:0]
    let imm = imm & 0x1f; // Mask to 5 bits
    encode_i(0x13, rd, rs1, imm, 0x1)
}

/// SRL: rd = rs1 >> rs2 (logical right shift)
pub fn srl(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x5, 0x0)
}

/// SRLI: rd = rs1 >> imm (logical right shift immediate)
/// Note: imm[11:5] must be 0, only imm[4:0] is used for shift amount
pub fn srli(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    // For SRLI, imm[11:5] must be 0, so we only use imm[4:0]
    let imm = imm & 0x1f; // Mask to 5 bits
    encode_i(0x13, rd, rs1, imm, 0x5)
}

/// SRA: rd = rs1 >> rs2 (arithmetic right shift)
pub fn sra(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x5, 0x20)
}

/// SRAI: rd = rs1 >> imm (arithmetic right shift immediate)
/// Note: imm[11:5] must be 0x20, only imm[4:0] is used for shift amount
pub fn srai(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    // For SRAI, imm[11:5] must be 0x20, so we encode it specially
    // imm[4:0] is the shift amount
    let imm_lo = imm & 0x1f; // bits [4:0]
    let imm_hi = 0x20; // bits [11:5] must be 0x20
    encode_i_with_imm_hi(0x13, rd, rs1, imm_lo, imm_hi, 0x5)
}

/// Encode an I-type instruction with explicit imm[11:5] (for SRAI)
fn encode_i_with_imm_hi(opcode: u8, rd: Gpr, rs1: Gpr, imm_lo: i32, imm_hi: u8, funct3: u8) -> u32 {
    let opcode = opcode as u32;
    let rd = rd.num() as u32;
    let funct3 = funct3 as u32;
    let rs1 = rs1.num() as u32;
    let imm_lo = (imm_lo as u32) & 0x1f; // bits [4:0]
    let imm_hi = imm_hi as u32; // bits [11:5]

    opcode | (rd << 7) | (funct3 << 12) | (rs1 << 15) | (imm_lo << 20) | (imm_hi << 25)
}

/// Encode an I-type instruction with funct6 encoding (for Zbs/Zbb instructions)
/// funct6 is in bits [31:26], imm[5:0] is in bits [25:20]
fn encode_i_with_funct6(opcode: u8, rd: Gpr, rs1: Gpr, imm: i32, funct6: u8, funct3: u8) -> u32 {
    let opcode = opcode as u32;
    let rd = rd.num() as u32;
    let funct3 = funct3 as u32;
    let rs1 = rs1.num() as u32;
    let imm_5_0 = (imm as u32) & 0x3f; // bits [5:0]
    let funct6_u32 = funct6 as u32;

    opcode | (rd << 7) | (funct3 << 12) | (rs1 << 15) | (imm_5_0 << 20) | (funct6_u32 << 26)
}

/// Encode an I-type instruction with funct12 encoding (for CLZ, CTZ, etc.)
fn encode_i_with_funct12(opcode: u8, rd: Gpr, rs1: Gpr, funct12: u16, funct3: u8) -> u32 {
    let opcode = opcode as u32;
    let rd = rd.num() as u32;
    let funct3 = funct3 as u32;
    let rs1 = rs1.num() as u32;
    let funct12_u32 = funct12 as u32;

    opcode | (rd << 7) | (funct3 << 12) | (rs1 << 15) | (funct12_u32 << 20)
}

// Zbs: Single-bit instructions (immediate)
pub fn bclri(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i_with_funct6(0x13, rd, rs1, imm, 0b010010, 0x1)
}

pub fn bseti(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i_with_funct6(0x13, rd, rs1, imm, 0b001010, 0x1)
}

pub fn binvi(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i_with_funct6(0x13, rd, rs1, imm, 0b011010, 0x1)
}

pub fn bexti(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i_with_funct6(0x13, rd, rs1, imm, 0b010010, 0x5)
}

// Zbs: Single-bit instructions (register)
pub fn bclr(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x1, 0x24)
}

pub fn bset(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x1, 0x14)
}

pub fn binv(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x1, 0x34)
}

pub fn bext(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x5, 0x24)
}

// Zbb: Count operations
pub fn clz(rd: Gpr, rs1: Gpr) -> u32 {
    encode_i_with_funct12(0x13, rd, rs1, 0x600, 0x1)
}

pub fn ctz(rd: Gpr, rs1: Gpr) -> u32 {
    encode_i_with_funct12(0x13, rd, rs1, 0x601, 0x1)
}

pub fn cpop(rd: Gpr, rs1: Gpr) -> u32 {
    encode_i_with_funct12(0x13, rd, rs1, 0x602, 0x1)
}

// Zbb: Sign/zero extend
pub fn sextb(rd: Gpr, rs1: Gpr) -> u32 {
    encode_i_with_funct12(0x13, rd, rs1, 0x604, 0x1)
}

pub fn sexth(rd: Gpr, rs1: Gpr) -> u32 {
    encode_i_with_funct12(0x13, rd, rs1, 0x605, 0x1)
}

pub fn zexth(rd: Gpr, rs1: Gpr) -> u32 {
    encode_i_with_funct12(0x13, rd, rs1, 0x080, 0x4)
}

// Zbb: Rotate instructions
pub fn rori(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i_with_funct6(0x13, rd, rs1, imm, 0b011000, 0x5)
}

pub fn rol(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x1, 0x30)
}

pub fn ror(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x5, 0x30)
}

// Zbb: Byte reverse
pub fn rev8(rd: Gpr, rs1: Gpr) -> u32 {
    encode_i_with_funct12(0x13, rd, rs1, 0x6b8, 0x5)
}

pub fn brev8(rd: Gpr, rs1: Gpr) -> u32 {
    encode_i_with_funct12(0x13, rd, rs1, 0x687, 0x5)
}

pub fn orcb(rd: Gpr, rs1: Gpr) -> u32 {
    encode_i_with_funct12(0x13, rd, rs1, 0x287, 0x5)
}

// Zbb: Min/Max
pub fn min(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x4, 0x05)
}

pub fn minu(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x5, 0x05)
}

pub fn max(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x6, 0x05)
}

pub fn maxu(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x7, 0x05)
}

// Zbb: Logical operations
pub fn andn(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x7, 0x20)
}

pub fn orn(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x6, 0x20)
}

pub fn xnor(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x4, 0x20)
}

// Zba: Address generation
pub fn sh1add(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x2, 0x10)
}

pub fn sh2add(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x4, 0x10)
}

pub fn sh3add(rd: Gpr, rs1: Gpr, rs2: Gpr) -> u32 {
    encode_r(0x33, rd, rs1, rs2, 0x6, 0x10)
}

pub fn slli_uw(rd: Gpr, rs1: Gpr, imm: i32) -> u32 {
    encode_i_with_funct6(0x13, rd, rs1, imm, 0b000010, 0x1)
}

// Immediate generation

/// LUI: rd = imm
///
/// `imm` is the sign-extended and shifted immediate value (already in final form).
/// This function extracts the upper 20 bits to encode in the instruction.
pub fn lui(rd: Gpr, imm: i32) -> u32 {
    // Extract the upper 20 bits from the i32 value
    // The immediate is stored in bits [31:12] of the instruction
    let imm_u32 = imm as u32;
    encode_u(0x37, rd, imm_u32)
}

/// AUIPC: rd = pc + imm
///
/// `imm` is the sign-extended and shifted immediate value (already in final form).
/// This function extracts the upper 20 bits to encode in the instruction.
pub fn auipc(rd: Gpr, imm: i32) -> u32 {
    // Extract the upper 20 bits from the i32 value
    // The immediate is stored in bits [31:12] of the instruction
    let imm_u32 = imm as u32;
    encode_u(0x17, rd, imm_u32)
}

// System instructions

/// ECALL: Environment call (syscall)
/// Encoding: opcode=0x73, funct3=0, rs1=0, rd=0, imm=0
pub fn ecall() -> u32 {
    0x00000073
}

/// EBREAK: Environment break (halt/debug breakpoint)
/// Encoding: opcode=0x73, funct3=0, rs1=0, rd=0, imm=1
pub fn ebreak() -> u32 {
    0x00100073
}

/// FENCE.I: Instruction cache synchronization
/// Encoding: opcode=0x0f, funct3=0x1, rs1=0, rd=0, imm[11:0]=0x001
pub fn fence_i() -> u32 {
    0x0010100f
}

/// Encode CSRRW instruction: rd = CSR; CSR = rs1
/// Format: opcode=0x73, funct3=0b001, rd, rs1, csr[11:0]
pub fn csrrw(rd: Gpr, rs1: Gpr, csr: u16) -> u32 {
    encode_i(0x73, rd, rs1, csr as i32, 0b001)
}

/// Encode CSRRS instruction: rd = CSR; CSR = CSR | rs1
/// Format: opcode=0x73, funct3=0b010, rd, rs1, csr[11:0]
pub fn csrrs(rd: Gpr, rs1: Gpr, csr: u16) -> u32 {
    encode_i(0x73, rd, rs1, csr as i32, 0b010)
}

/// Encode CSRRC instruction: rd = CSR; CSR = CSR & ~rs1
/// Format: opcode=0x73, funct3=0b011, rd, rs1, csr[11:0]
pub fn csrrc(rd: Gpr, rs1: Gpr, csr: u16) -> u32 {
    encode_i(0x73, rd, rs1, csr as i32, 0b011)
}

/// Encode CSRRWI instruction: rd = CSR; CSR = imm
/// Format: opcode=0x73, funct3=0b101, rd, imm[4:0], csr[11:0]
pub fn csrrwi(rd: Gpr, imm: i32, csr: u16) -> u32 {
    encode_i(0x73, rd, Gpr::new((imm & 0x1f) as u8), csr as i32, 0b101)
}

/// Encode CSRRSI instruction: rd = CSR; CSR = CSR | imm
/// Format: opcode=0x73, funct3=0b110, rd, imm[4:0], csr[11:0]
pub fn csrrsi(rd: Gpr, imm: i32, csr: u16) -> u32 {
    encode_i(0x73, rd, Gpr::new((imm & 0x1f) as u8), csr as i32, 0b110)
}

/// Encode CSRRCI instruction: rd = CSR; CSR = CSR & ~imm
/// Format: opcode=0x73, funct3=0b111, rd, imm[4:0], csr[11:0]
pub fn csrrci(rd: Gpr, imm: i32, csr: u16) -> u32 {
    encode_i(0x73, rd, Gpr::new((imm & 0x1f) as u8), csr as i32, 0b111)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        // add a0, a1, a2
        // Expected: 0x00c58533
        let inst = add(Gpr::A0, Gpr::A1, Gpr::A2);
        assert_eq!(inst, 0x00c58533);
    }

    #[test]
    fn test_sub() {
        // sub a0, a1, a2
        // Expected: 0x40c58533
        let inst = sub(Gpr::A0, Gpr::A1, Gpr::A2);
        assert_eq!(inst, 0x40c58533);
    }

    #[test]
    fn test_mul() {
        // mul a0, a1, a2
        // Expected: 0x02c58533
        let inst = mul(Gpr::A0, Gpr::A1, Gpr::A2);
        assert_eq!(inst, 0x02c58533);
    }

    #[test]
    fn test_addi() {
        // addi a0, a1, 5
        // Expected: 0x00558513
        let inst = addi(Gpr::A0, Gpr::A1, 5);
        assert_eq!(inst, 0x00558513);
    }

    #[test]
    fn test_addi_negative() {
        // addi a0, a1, -5
        // Expected: 0xffb58513
        let inst = addi(Gpr::A0, Gpr::A1, -5);
        assert_eq!(inst, 0xffb58513);
    }

    #[test]
    fn test_lui() {
        // lui a0, 0x12345
        // Expected: 0x12345537
        let inst = lui(Gpr::A0, 0x12345000);
        assert_eq!(inst, 0x12345537);
    }

    #[test]
    fn test_jalr() {
        // jalr zero, ra, 0
        // Expected: 0x00008067
        let inst = jalr(Gpr::Zero, Gpr::Ra, 0);
        assert_eq!(inst, 0x00008067);
    }

    #[test]
    fn test_lw() {
        // lw a0, 4(a1)
        // Expected: 0x0045a503
        let inst = lw(Gpr::A0, Gpr::A1, 4);
        assert_eq!(inst, 0x0045a503);
    }

    #[test]
    fn test_sw() {
        // sw a0, 4(a1)
        // Expected: 0x00a5a223
        let inst = sw(Gpr::A1, Gpr::A0, 4);
        assert_eq!(inst, 0x00a5a223);
    }

    #[test]
    fn test_beq() {
        // beq a0, a1, 8
        // Expected: 0x00b50463 (imm[4:1] = 4 for imm=8)
        let inst = beq(Gpr::A0, Gpr::A1, 8);
        assert_eq!(inst, 0x00b50463);
    }

    #[test]
    fn test_auipc_zero() {
        // auipc t0, 0
        // Expected: 0x00000297 (opcode=0x17, rd=5, imm=0)
        let inst = auipc(Gpr::T0, 0);
        assert_eq!(inst, 0x00000297);
    }

    #[test]
    fn test_auipc_positive_small() {
        // auipc t0, 0x1000 (final value, already shifted)
        // encode_u extracts bits [31:12] = 0x00001
        // Expected: 0x00001297
        let inst = auipc(Gpr::T0, 0x1000);
        assert_eq!(inst, 0x00001297);
    }

    #[test]
    fn test_auipc_positive_large() {
        // auipc t0, 0x12345000 (final value, already shifted)
        // encode_u extracts bits [31:12] = 0x12345
        // Expected: 0x12345297
        let inst = auipc(Gpr::T0, 0x12345000);
        assert_eq!(inst, 0x12345297);
    }

    #[test]
    fn test_auipc_negative() {
        // auipc t0, 0xfffff000 (final value, sign-extended and shifted)
        // encode_u extracts bits [31:12] = 0xfffff
        // Expected: 0xfffff297
        let inst = auipc(Gpr::T0, 0xfffff000u32 as i32);
        assert_eq!(inst, 0xfffff297);
    }

    #[test]
    fn test_auipc_negative_ff000() {
        // auipc t0, 0xff000000 (final value)
        // For 0xff000 in upper 20 bits: instruction has 0xff000 in [31:12]
        // When decoded: (inst & (0xfffff << 12)) as i32 = 0xff000000 as i32
        // encode_u extracts bits [31:12] = 0xff000
        // Expected: 0xff000297
        let inst = auipc(Gpr::T0, 0xff000000u32 as i32);
        assert_eq!(inst, 0xff000297);
    }

    #[test]
    fn test_auipc_max_positive() {
        // auipc t0, 0x7ffff000 (final value, max positive 20-bit value shifted)
        // encode_u extracts bits [31:12] = 0x7ffff
        // Expected: 0x7ffff297
        let inst = auipc(Gpr::T0, 0x7ffff000);
        assert_eq!(inst, 0x7ffff297);
    }

    #[test]
    fn test_auipc_all_registers() {
        // Test that rd encoding works for all registers
        let inst_zero = auipc(Gpr::Zero, 0x12345000);
        assert_eq!(inst_zero, 0x12345017); // rd=0: 0x17 | (0 << 7) | (0x12345 << 12)

        let inst_ra = auipc(Gpr::Ra, 0x12345000);
        assert_eq!(inst_ra, 0x12345097); // rd=1: 0x17 | (1 << 7) | (0x12345 << 12) = 0x17 | 0x80 | 0x12345000

        let inst_t0 = auipc(Gpr::T0, 0x12345000);
        assert_eq!(inst_t0, 0x12345297); // rd=5: 0x17 | (5 << 7) | (0x12345 << 12) = 0x17 | 0x280 | 0x12345000
    }

    #[test]
    fn test_fence_i() {
        // FENCE.I encoding: opcode=0x0f, funct3=0x1, imm=0x001, rs1=0, rd=0
        // Expected: 0x0010100f (per RISC-V spec: imm[11:0]=0x001)
        let inst = fence_i();
        assert_eq!(inst, 0x0010100f);
    }
}
