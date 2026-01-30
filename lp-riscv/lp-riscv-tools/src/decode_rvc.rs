//! Decoder for RISC-V Compressed (RVC) instructions.
//!
//! This module provides decoding for 16-bit compressed instructions
//! from the RISC-V "C" extension. Compressed instructions are identified
//! by bits [1:0] != 0b11.

use crate::inst::Inst;
use crate::regs::Gpr;
use alloc::format;
use alloc::string::String;

/// Decode a 16-bit compressed instruction.
pub fn decode_compressed(inst: u16) -> Result<Inst, String> {
    let opcode = inst & 0x3; // bits [1:0]
    let funct3 = (inst >> 13) & 0x7; // bits [15:13]

    match opcode {
        0b00 => decode_c0(inst, funct3),
        0b01 => decode_c1(inst, funct3),
        0b10 => decode_c2(inst, funct3),
        _ => unreachable!(), // 0b11 is not compressed
    }
}

/// Decode quadrant 0 (opcode = 0b00)
fn decode_c0(inst: u16, funct3: u16) -> Result<Inst, String> {
    match funct3 {
        0b000 => decode_c_addi4spn(inst), // C.ADDI4SPN
        0b010 => decode_c_lw(inst),       // C.LW
        0b110 => decode_c_sw(inst),       // C.SW
        _ => Err(format!(
            "Unknown C0 instruction: funct3={funct3:03b}, inst=0x{inst:04x}"
        )),
    }
}

/// Decode quadrant 1 (opcode = 0b01)
fn decode_c1(inst: u16, funct3: u16) -> Result<Inst, String> {
    match funct3 {
        0b000 => decode_c_addi_or_nop(inst),     // C.ADDI / C.NOP
        0b001 => decode_c_jal(inst),             // C.JAL (RV32 only)
        0b010 => decode_c_li(inst),              // C.LI
        0b011 => decode_c_addi16sp_or_lui(inst), // C.ADDI16SP / C.LUI
        0b100 => decode_c_misc_alu(inst), // ALU operations (SRLI, SRAI, ANDI, SUB, XOR, OR, AND)
        0b101 => decode_c_j(inst),        // C.J
        0b110 => decode_c_beqz(inst),     // C.BEQZ
        0b111 => decode_c_bnez(inst),     // C.BNEZ
        _ => unreachable!(),
    }
}

/// Decode quadrant 2 (opcode = 0b10)
fn decode_c2(inst: u16, funct3: u16) -> Result<Inst, String> {
    match funct3 {
        0b000 => decode_c_slli(inst),    // C.SLLI
        0b010 => decode_c_lwsp(inst),    // C.LWSP
        0b100 => decode_c_misc_cr(inst), // C.JR, C.MV, C.JALR, C.ADD
        0b110 => decode_c_swsp(inst),    // C.SWSP
        _ => Err(format!(
            "Unknown C2 instruction: funct3={funct3:03b}, inst=0x{inst:04x}"
        )),
    }
}

// ============================================================================
// Helper functions to decode specific instruction types
// ============================================================================

/// C.ADDI4SPN: rd' = sp + nzuimm
/// Format: CIW-type: nzuimm[5:4|9:6|2|3] rd'
fn decode_c_addi4spn(inst: u16) -> Result<Inst, String> {
    let rd_prime = ((inst >> 2) & 0x7) as u8;
    let rd = compressed_reg(rd_prime);

    // Extract nzuimm: bits [12:5]
    // nzuimm[5:4] = inst[12:11]
    // nzuimm[9:6] = inst[10:7]
    // nzuimm[2] = inst[6]
    // nzuimm[3] = inst[5]
    let nzuimm = ((inst >> 7) & 0x30)   // nzuimm[5:4] from inst[12:11]
        | ((inst >> 1) & 0x3c0)          // nzuimm[9:6] from inst[10:7]
        | ((inst >> 4) & 0x4)            // nzuimm[2] from inst[6]
        | ((inst >> 2) & 0x8); // nzuimm[3] from inst[5]

    if nzuimm == 0 {
        return Err("C.ADDI4SPN with nzuimm=0 is reserved".into());
    }

    Ok(Inst::CAddi4spn {
        rd,
        imm: nzuimm as i32,
    })
}

/// C.LW: rd' = mem[rs1' + uimm]
/// Format: CL-type: uimm[5:3] rs1' uimm[2|6] rd'
fn decode_c_lw(inst: u16) -> Result<Inst, String> {
    let rd_prime = ((inst >> 2) & 0x7) as u8;
    let rs1_prime = ((inst >> 7) & 0x7) as u8;
    let rd = compressed_reg(rd_prime);
    let rs = compressed_reg(rs1_prime);

    // Extract uimm: bits [12:10] and [6:5]
    // uimm[5:3] = inst[12:10]
    // uimm[2|6] = inst[6:5]
    let uimm = ((inst >> 7) & 0x38)   // uimm[5:3] from inst[12:10]
        | ((inst >> 4) & 0x4)          // uimm[2] from inst[6]
        | ((inst << 1) & 0x40); // uimm[6] from inst[5]

    Ok(Inst::CLw {
        rd,
        rs,
        offset: uimm as i32,
    })
}

/// C.SW: mem[rs1' + uimm] = rs2'
/// Format: CS-type: uimm[5:3] rs1' uimm[2|6] rs2'
fn decode_c_sw(inst: u16) -> Result<Inst, String> {
    let rs2_prime = ((inst >> 2) & 0x7) as u8;
    let rs1_prime = ((inst >> 7) & 0x7) as u8;
    let rs1 = compressed_reg(rs1_prime);
    let rs2 = compressed_reg(rs2_prime);

    // Extract uimm (same as C.LW)
    let uimm = ((inst >> 7) & 0x38)   // uimm[5:3] from inst[12:10]
        | ((inst >> 4) & 0x4)          // uimm[2] from inst[6]
        | ((inst << 1) & 0x40); // uimm[6] from inst[5]

    Ok(Inst::CSw {
        rs1,
        rs2,
        offset: uimm as i32,
    })
}

/// C.ADDI / C.NOP: rd = rd + imm (or NOP if rd=x0)
/// Format: CI-type: imm[5] rd imm[4:0]
fn decode_c_addi_or_nop(inst: u16) -> Result<Inst, String> {
    let rd = ((inst >> 7) & 0x1f) as u8;

    if rd == 0 {
        // C.NOP: ADDI x0, x0, 0
        return Ok(Inst::CNop);
    }

    // Extract immediate: imm[5] | imm[4:0]
    let imm = sign_extend(((inst >> 7) & 0x20) | ((inst >> 2) & 0x1f), 6);

    Ok(Inst::CAddi {
        rd: Gpr::new(rd),
        imm,
    })
}

/// C.JAL: ra = pc + 2; pc = pc + offset (RV32 only)
/// Format: CJ-type: offset[11|4|9:8|10|6|7|3:1|5]
fn decode_c_jal(inst: u16) -> Result<Inst, String> {
    let offset = decode_cj_offset(inst);
    Ok(Inst::CJal { offset })
}

/// C.LI: rd = imm
/// Format: CI-type: imm[5] rd imm[4:0]
fn decode_c_li(inst: u16) -> Result<Inst, String> {
    let rd = ((inst >> 7) & 0x1f) as u8;

    if rd == 0 {
        return Err("C.LI with rd=x0 is a hint (nop)".into());
    }

    // Extract immediate: imm[5] | imm[4:0]
    let imm = sign_extend(((inst >> 7) & 0x20) | ((inst >> 2) & 0x1f), 6);

    Ok(Inst::CLi {
        rd: Gpr::new(rd),
        imm,
    })
}

/// C.ADDI16SP / C.LUI
/// Format: CI-type
fn decode_c_addi16sp_or_lui(inst: u16) -> Result<Inst, String> {
    let rd = ((inst >> 7) & 0x1f) as u8;

    if rd == 2 {
        // C.ADDI16SP: sp = sp + nzimm
        // nzimm[9] = inst[12]
        // nzimm[4|6|8:7|5] = inst[6:2]
        let nzimm = sign_extend(
            ((inst >> 3) & 0x200)   // nzimm[9] from inst[12]
                | ((inst >> 2) & 0x10)   // nzimm[4] from inst[6]
                | ((inst << 1) & 0x40)   // nzimm[6] from inst[5]
                | ((inst << 4) & 0x180)  // nzimm[8:7] from inst[4:3]
                | ((inst << 3) & 0x20), // nzimm[5] from inst[2]
            10,
        );

        if nzimm == 0 {
            return Err("C.ADDI16SP with nzimm=0 is reserved".into());
        }

        return Ok(Inst::CAddi16sp { imm: nzimm });
    }

    if rd == 0 {
        return Err("C.LUI with rd=x0 is a hint (nop)".into());
    }

    // C.LUI: rd = nzimm << 12
    // nzimm[17] = inst[12]
    // nzimm[16:12] = inst[6:2]
    let nzimm = sign_extend(((inst >> 7) & 0x20) | ((inst >> 2) & 0x1f), 6);

    if nzimm == 0 {
        return Err("C.LUI with nzimm=0 is reserved".into());
    }

    // Shift left by 12 bits (into upper 20 bits)
    let imm = nzimm << 12;

    Ok(Inst::CLui {
        rd: Gpr::new(rd),
        imm,
    })
}

/// C.MISC_ALU: Various ALU operations
/// Format: CA-type / CB-type
fn decode_c_misc_alu(inst: u16) -> Result<Inst, String> {
    let funct2 = (inst >> 10) & 0x3;
    let rd_prime = ((inst >> 7) & 0x7) as u8;
    let rd = compressed_reg(rd_prime);

    match funct2 {
        0b00 => {
            // C.SRLI: rd' = rd' >> uimm
            let uimm = ((inst >> 7) & 0x20) | ((inst >> 2) & 0x1f);
            Ok(Inst::CSrli {
                rd,
                imm: uimm as i32,
            })
        }
        0b01 => {
            // C.SRAI: rd' = rd' >> uimm (arithmetic)
            let uimm = ((inst >> 7) & 0x20) | ((inst >> 2) & 0x1f);
            Ok(Inst::CSrai {
                rd,
                imm: uimm as i32,
            })
        }
        0b10 => {
            // C.ANDI: rd' = rd' & imm
            let imm = sign_extend(((inst >> 7) & 0x20) | ((inst >> 2) & 0x1f), 6);
            Ok(Inst::CAndi { rd, imm })
        }
        0b11 => {
            // Register-register operations
            let funct6 = (inst >> 10) & 0x3f;
            let rs2_prime = ((inst >> 2) & 0x7) as u8;
            let rs = compressed_reg(rs2_prime);

            match (funct6, (inst >> 5) & 0x3) {
                (0b100011, 0b00) => Ok(Inst::CSub { rd, rs }),
                (0b100011, 0b01) => Ok(Inst::CXor { rd, rs }),
                (0b100011, 0b10) => Ok(Inst::COr { rd, rs }),
                (0b100011, 0b11) => Ok(Inst::CAnd { rd, rs }),
                _ => Err(format!(
                    "Unknown C.MISC_ALU instruction: funct6={funct6:06b}, inst=0x{inst:04x}"
                )),
            }
        }
        _ => unreachable!(),
    }
}

/// C.J: pc = pc + offset
/// Format: CJ-type: offset[11|4|9:8|10|6|7|3:1|5]
fn decode_c_j(inst: u16) -> Result<Inst, String> {
    let offset = decode_cj_offset(inst);
    Ok(Inst::CJ { offset })
}

/// C.BEQZ: if rs1' == 0, pc = pc + offset
/// Format: CB-type: offset[8|4:3] rs1' offset[7:6|2:1|5]
fn decode_c_beqz(inst: u16) -> Result<Inst, String> {
    let rs1_prime = ((inst >> 7) & 0x7) as u8;
    let rs = compressed_reg(rs1_prime);
    let offset = decode_cb_offset(inst);

    Ok(Inst::CBeqz { rs, offset })
}

/// C.BNEZ: if rs1' != 0, pc = pc + offset
/// Format: CB-type: offset[8|4:3] rs1' offset[7:6|2:1|5]
fn decode_c_bnez(inst: u16) -> Result<Inst, String> {
    let rs1_prime = ((inst >> 7) & 0x7) as u8;
    let rs = compressed_reg(rs1_prime);
    let offset = decode_cb_offset(inst);

    Ok(Inst::CBnez { rs, offset })
}

/// C.SLLI: rd = rd << uimm
/// Format: CI-type: uimm[5] rd uimm[4:0]
fn decode_c_slli(inst: u16) -> Result<Inst, String> {
    let rd = ((inst >> 7) & 0x1f) as u8;

    if rd == 0 {
        return Err("C.SLLI with rd=x0 is a hint (nop)".into());
    }

    let uimm = ((inst >> 7) & 0x20) | ((inst >> 2) & 0x1f);

    Ok(Inst::CSlli {
        rd: Gpr::new(rd),
        imm: uimm as i32,
    })
}

/// C.LWSP: rd = mem[sp + uimm]
/// Format: CI-type: uimm[5] rd uimm[4:2|7:6]
fn decode_c_lwsp(inst: u16) -> Result<Inst, String> {
    let rd = ((inst >> 7) & 0x1f) as u8;

    if rd == 0 {
        return Err("C.LWSP with rd=x0 is reserved".into());
    }

    // Extract uimm: uimm[5] = inst[12], uimm[4:2] = inst[6:4], uimm[7:6] = inst[3:2]
    let uimm = ((inst >> 7) & 0x20)   // uimm[5] from inst[12]
        | ((inst >> 2) & 0x1c)         // uimm[4:2] from inst[6:4]
        | ((inst << 4) & 0xc0); // uimm[7:6] from inst[3:2]

    Ok(Inst::CLwsp {
        rd: Gpr::new(rd),
        offset: uimm as i32,
    })
}

/// C.MISC_CR: C.JR, C.MV, C.JALR, C.ADD
/// Format: CR-type
fn decode_c_misc_cr(inst: u16) -> Result<Inst, String> {
    let rd_rs1 = ((inst >> 7) & 0x1f) as u8;
    let rs2 = ((inst >> 2) & 0x1f) as u8;
    let funct4 = (inst >> 12) & 0xf;

    match (funct4, rs2) {
        (0b1000, 0) if rd_rs1 != 0 => {
            // C.JR: pc = rs1
            Ok(Inst::CJr {
                rs: Gpr::new(rd_rs1),
            })
        }
        (0b1000, _) if rd_rs1 != 0 && rs2 != 0 => {
            // C.MV: rd = rs2
            Ok(Inst::CMv {
                rd: Gpr::new(rd_rs1),
                rs: Gpr::new(rs2),
            })
        }
        (0b1001, 0) if rd_rs1 == 0 => {
            // C.EBREAK
            Ok(Inst::CEbreak)
        }
        (0b1001, 0) if rd_rs1 != 0 => {
            // C.JALR: ra = pc + 2; pc = rs1
            Ok(Inst::CJalr {
                rs: Gpr::new(rd_rs1),
            })
        }
        (0b1001, _) if rd_rs1 != 0 && rs2 != 0 => {
            // C.ADD: rd = rd + rs2
            Ok(Inst::CAdd {
                rd: Gpr::new(rd_rs1),
                rs: Gpr::new(rs2),
            })
        }
        _ => Err(format!(
            "Unknown C.MISC_CR instruction: funct4={funct4:04b}, rd_rs1={rd_rs1}, rs2={rs2}, inst=0x{inst:04x}"
        )),
    }
}

/// C.SWSP: mem[sp + uimm] = rs2
/// Format: CSS-type: uimm[5:2|7:6] rs2
fn decode_c_swsp(inst: u16) -> Result<Inst, String> {
    let rs2 = ((inst >> 2) & 0x1f) as u8;

    // Extract uimm: uimm[5:2] = inst[12:9], uimm[7:6] = inst[8:7]
    let uimm = ((inst >> 7) & 0x3c)   // uimm[5:2] from inst[12:9]
        | ((inst >> 1) & 0xc0); // uimm[7:6] from inst[8:7]

    Ok(Inst::CSwsp {
        rs: Gpr::new(rs2),
        offset: uimm as i32,
    })
}

// ============================================================================
// Helper functions
// ============================================================================

/// Map compressed register encoding (3 bits) to full register number (x8-x15)
fn compressed_reg(reg_prime: u8) -> Gpr {
    Gpr::new(reg_prime + 8)
}

/// Sign-extend a value from `bits` bits to i32
fn sign_extend(value: u16, bits: u8) -> i32 {
    let sign_bit = 1 << (bits - 1);
    let mask = (1 << bits) - 1;
    let value = (value & mask) as i32;

    if (value & sign_bit) != 0 {
        value | (!(mask as i32))
    } else {
        value
    }
}

/// Decode CJ-type offset (for C.J and C.JAL)
/// offset[11|4|9:8|10|6|7|3:1|5]
fn decode_cj_offset(inst: u16) -> i32 {
    let offset = ((inst >> 1) & 0x800)   // offset[11] from inst[12]
        | ((inst >> 7) & 0x10)            // offset[4] from inst[11]
        | ((inst >> 1) & 0x300)           // offset[9:8] from inst[10:9]
        | ((inst << 2) & 0x400)           // offset[10] from inst[8]
        | ((inst >> 1) & 0x40)            // offset[6] from inst[7]
        | ((inst << 1) & 0x80)            // offset[7] from inst[6]
        | ((inst >> 2) & 0xe)             // offset[3:1] from inst[5:3]
        | ((inst << 3) & 0x20); // offset[5] from inst[2]

    sign_extend(offset, 12)
}

/// Decode CB-type offset (for C.BEQZ and C.BNEZ)
/// offset[8|4:3] rs1' offset[7:6|2:1|5]
fn decode_cb_offset(inst: u16) -> i32 {
    let offset = ((inst >> 4) & 0x100)   // offset[8] from inst[12]
        | ((inst >> 7) & 0x18)            // offset[4:3] from inst[11:10]
        | ((inst << 1) & 0xc0)            // offset[7:6] from inst[6:5]
        | ((inst >> 2) & 0x6)             // offset[2:1] from inst[4:3]
        | ((inst << 3) & 0x20); // offset[5] from inst[2]

    sign_extend(offset, 9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compressed_reg() {
        assert_eq!(compressed_reg(0).num(), 8);
        assert_eq!(compressed_reg(1).num(), 9);
        assert_eq!(compressed_reg(7).num(), 15);
    }

    #[test]
    fn test_sign_extend() {
        // 6-bit sign extension
        assert_eq!(sign_extend(0b000000, 6), 0);
        assert_eq!(sign_extend(0b000001, 6), 1);
        assert_eq!(sign_extend(0b011111, 6), 31);
        assert_eq!(sign_extend(0b111111, 6), -1);
        assert_eq!(sign_extend(0b100000, 6), -32);
    }

    #[test]
    fn test_decode_c_addi() {
        // C.ADDI x10, 5
        // opcode=01, funct3=000, rd=x10 (10), imm=5
        // Encoding: 001 0 01010 00101 01
        // = 0x0515
        let inst = decode_compressed(0x0515).unwrap();
        match inst {
            Inst::CAddi { rd, imm } => {
                assert_eq!(rd.num(), 10);
                assert_eq!(imm, 5);
            }
            _ => panic!("Expected CAddi, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_c_nop() {
        // C.NOP: ADDI x0, x0, 0
        // opcode=01, funct3=000, rd=x0, imm=0
        // Encoding: 000 0 00000 00000 01 = 0x0001
        let inst = decode_compressed(0x0001).unwrap();
        match inst {
            Inst::CNop => {}
            _ => panic!("Expected CNop, got {:?}", inst),
        }
    }

    #[test]
    fn test_decode_c_li() {
        // C.LI x10, 5
        // opcode=01, funct3=010, rd=x10, imm=5
        // Encoding: 010 0 01010 00101 01
        // = 0x4515
        let inst = decode_compressed(0x4515).unwrap();
        match inst {
            Inst::CLi { rd, imm } => {
                assert_eq!(rd.num(), 10);
                assert_eq!(imm, 5);
            }
            _ => panic!("Expected CLi, got {:?}", inst),
        }
    }
}
