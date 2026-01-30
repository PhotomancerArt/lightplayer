//! RISC-V instruction format types.
//!
//! This module provides format structs for different RISC-V instruction types,
//! following the embive approach for efficient decoding.

/// R-Type Instruction Format
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct TypeR {
    /// Destination Register
    pub rd: u8,
    /// Source Register 1
    pub rs1: u8,
    /// Source Register 2
    pub rs2: u8,
    /// Function Type (funct3 + funct7)
    pub func: u16,
}

impl TypeR {
    #[inline(always)]
    pub fn from_riscv(inst: u32) -> Self {
        TypeR {
            rd: ((inst >> 7) & 0b1_1111) as u8,
            rs1: ((inst >> 15) & 0b1_1111) as u8,
            rs2: ((inst >> 20) & 0b1_1111) as u8,
            func: ((((inst >> 25) & 0b111_1111) << 3) | ((inst >> 12) & 0b111)) as u16,
        }
    }

    /// Encode to RISC-V instruction word (without opcode)
    /// Caller must add the opcode (typically 0x33)
    #[inline(always)]
    pub fn to_riscv_no_opcode(self) -> u32 {
        let funct3 = (self.func & 0x7) as u32;
        let funct7 = ((self.func >> 3) & 0x7f) as u32;
        ((self.rd as u32) << 7)
            | (funct3 << 12)
            | ((self.rs1 as u32) << 15)
            | ((self.rs2 as u32) << 20)
            | (funct7 << 25)
    }

    /// Encode to RISC-V instruction word with opcode
    #[inline(always)]
    pub fn to_riscv(self, opcode: u8) -> u32 {
        self.to_riscv_no_opcode() | (opcode as u32)
    }
}

/// I-Type Instruction Format
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct TypeI {
    /// Destination Register
    pub rd: u8,
    /// Source Register 1
    pub rs1: u8,
    /// Immediate Value (sign-extended)
    pub imm: i32,
    /// Function Type (funct3)
    pub func: u8,
}

impl TypeI {
    #[inline(always)]
    pub fn from_riscv(inst: u32) -> Self {
        TypeI {
            rd: ((inst >> 7) & 0b1_1111) as u8,
            rs1: ((inst >> 15) & 0b1_1111) as u8,
            func: ((inst >> 12) & 0b111) as u8,
            imm: ((inst & (0b1111_1111_1111 << 20)) as i32 >> 20),
        }
    }

    /// Encode to RISC-V instruction word (without opcode)
    /// Caller must add the opcode (e.g., 0x13, 0x03, 0x67)
    #[inline(always)]
    pub fn to_riscv_no_opcode(self) -> u32 {
        let imm_12bit = (self.imm as u32) & 0xfff; // Extract 12-bit immediate
        ((self.rd as u32) << 7)
            | ((self.func as u32) << 12)
            | ((self.rs1 as u32) << 15)
            | (imm_12bit << 20)
    }

    /// Encode to RISC-V instruction word with opcode
    #[inline(always)]
    pub fn to_riscv(self, opcode: u8) -> u32 {
        self.to_riscv_no_opcode() | (opcode as u32)
    }
}

/// S-Type Instruction Format
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct TypeS {
    /// Source Register 1
    pub rs1: u8,
    /// Source Register 2
    pub rs2: u8,
    /// Immediate Value (sign-extended)
    pub imm: i32,
    /// Function Type (funct3)
    pub func: u8,
}

impl TypeS {
    #[inline(always)]
    pub fn from_riscv(inst: u32) -> Self {
        TypeS {
            rs1: ((inst >> 15) & 0b1_1111) as u8,
            rs2: ((inst >> 20) & 0b1_1111) as u8,
            func: ((inst >> 12) & 0b111) as u8,
            imm: (((inst & (0b111_1111 << 25)) | ((inst & (0b1_1111 << 7)) << 13)) as i32 >> 20),
        }
    }

    /// Encode to RISC-V instruction word (without opcode)
    /// Caller must add the opcode (typically 0x23)
    #[inline(always)]
    pub fn to_riscv_no_opcode(self) -> u32 {
        let imm_12bit = (self.imm as u32) & 0xfff; // Extract 12-bit immediate
        let imm_lo = imm_12bit & 0x1f; // bits [4:0]
        let imm_hi = (imm_12bit >> 5) & 0x7f; // bits [11:5]
        (imm_lo << 7)
            | ((self.func as u32) << 12)
            | ((self.rs1 as u32) << 15)
            | ((self.rs2 as u32) << 20)
            | (imm_hi << 25)
    }

    /// Encode to RISC-V instruction word with opcode
    #[inline(always)]
    pub fn to_riscv(self, opcode: u8) -> u32 {
        self.to_riscv_no_opcode() | (opcode as u32)
    }
}

/// B-Type Instruction Format
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct TypeB {
    /// Source Register 1
    pub rs1: u8,
    /// Source Register 2
    pub rs2: u8,
    /// Immediate Value (sign-extended)
    pub imm: i32,
    /// Function Type (funct3)
    pub func: u8,
}

impl TypeB {
    #[inline(always)]
    pub fn from_riscv(inst: u32) -> Self {
        let imm_12 = ((inst >> 31) & 0b1) as i32;
        let imm_10_5 = ((inst >> 25) & 0b11_1111) as i32;
        let imm_4_1 = ((inst >> 8) & 0b1111) as i32;
        let imm_11 = ((inst >> 7) & 0b1) as i32;
        let imm = (imm_12 << 12) | (imm_11 << 11) | (imm_10_5 << 5) | (imm_4_1 << 1);
        let imm_sign_extended = if (imm & 0x1000) != 0 {
            imm | (-8192i32) // 0xffffe000
        } else {
            imm
        };
        TypeB {
            rs1: ((inst >> 15) & 0b1_1111) as u8,
            rs2: ((inst >> 20) & 0b1_1111) as u8,
            func: ((inst >> 12) & 0b111) as u8,
            imm: imm_sign_extended,
        }
    }

    /// Encode to RISC-V instruction word (without opcode)
    /// Caller must add the opcode (typically 0x63)
    #[inline(always)]
    pub fn to_riscv_no_opcode(self) -> u32 {
        let imm_12bit = (self.imm as u32) & 0x1fff; // Extract 13-bit immediate (sign-extended)
        let imm_12 = (imm_12bit >> 12) & 0x1;
        let imm_11 = (imm_12bit >> 11) & 0x1;
        let imm_10_5 = (imm_12bit >> 5) & 0x3f;
        let imm_4_1 = (imm_12bit >> 1) & 0xf;
        (imm_4_1 << 8)
            | ((self.func as u32) << 12)
            | ((self.rs1 as u32) << 15)
            | ((self.rs2 as u32) << 20)
            | (imm_10_5 << 25)
            | (imm_11 << 7)
            | (imm_12 << 31)
    }

    /// Encode to RISC-V instruction word with opcode
    #[inline(always)]
    pub fn to_riscv(self, opcode: u8) -> u32 {
        self.to_riscv_no_opcode() | (opcode as u32)
    }
}

/// J-Type Instruction Format
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct TypeJ {
    /// Destination Register
    pub rd: u8,
    /// Immediate Value (sign-extended)
    pub imm: i32,
}

impl TypeJ {
    #[inline(always)]
    pub fn from_riscv(inst: u32) -> Self {
        let imm_20 = ((inst >> 31) & 0b1) as i32;
        let imm_10_1 = ((inst >> 21) & 0b11_1111_1111) as i32;
        let imm_11 = ((inst >> 20) & 0b1) as i32;
        let imm_19_12 = ((inst >> 12) & 0b1111_1111) as i32;
        let imm = (imm_20 << 20) | (imm_19_12 << 12) | (imm_11 << 11) | (imm_10_1 << 1);
        let imm_sign_extended = if (imm & 0x100000) != 0 {
            imm | (-2097152i32) // 0xffe00000
        } else {
            imm
        };
        TypeJ {
            rd: ((inst >> 7) & 0b1_1111) as u8,
            imm: imm_sign_extended,
        }
    }

    /// Encode to RISC-V instruction word (without opcode)
    /// Caller must add the opcode (typically 0x6f)
    #[inline(always)]
    pub fn to_riscv_no_opcode(self) -> u32 {
        let imm_21bit = (self.imm as u32) & 0x1fffff; // Extract 21-bit immediate (sign-extended)
        let imm_20 = (imm_21bit >> 20) & 0x1;
        let imm_10_1 = (imm_21bit >> 1) & 0x3ff;
        let imm_11 = (imm_21bit >> 11) & 0x1;
        let imm_19_12 = (imm_21bit >> 12) & 0xff;
        ((self.rd as u32) << 7)
            | (imm_20 << 31)
            | (imm_10_1 << 21)
            | (imm_11 << 20)
            | (imm_19_12 << 12)
    }

    /// Encode to RISC-V instruction word with opcode
    #[inline(always)]
    pub fn to_riscv(self, opcode: u8) -> u32 {
        self.to_riscv_no_opcode() | (opcode as u32)
    }
}

/// U-Type Instruction Format
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct TypeU {
    /// Destination Register
    pub rd: u8,
    /// Immediate Value (sign-extended and shifted)
    pub imm: i32,
}

impl TypeU {
    #[inline(always)]
    pub fn from_riscv(inst: u32) -> Self {
        TypeU {
            rd: ((inst >> 7) & 0b1_1111) as u8,
            imm: (inst & (0b1111_1111_1111_1111_1111 << 12)) as i32,
        }
    }

    /// Encode to RISC-V instruction word (without opcode)
    /// Caller must add the opcode (typically 0x37 for LUI, 0x17 for AUIPC)
    #[inline(always)]
    pub fn to_riscv_no_opcode(self) -> u32 {
        let imm_u32 = self.imm as u32;
        let imm_hi = (imm_u32 >> 12) & 0xfffff; // Extract bits [31:12]
        ((self.rd as u32) << 7) | (imm_hi << 12)
    }

    /// Encode to RISC-V instruction word with opcode
    #[inline(always)]
    pub fn to_riscv(self, opcode: u8) -> u32 {
        self.to_riscv_no_opcode() | (opcode as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test round-trip for R-type instructions
    #[test]
    fn test_type_r_round_trip() {
        let test_cases = [
            (0x33, 1, 2, 3, 0x00),   // add
            (0x33, 1, 2, 3, 0x20),   // sub (funct7=0x20)
            (0x33, 5, 10, 15, 0x01), // mul
        ];

        for (opcode, rd, rs1, rs2, funct7) in test_cases.iter() {
            let funct3 = 0x0;
            let func = ((funct7 << 3) | funct3) as u16;
            let inst_word = *opcode
                | ((*rd as u32) << 7)
                | (funct3 << 12)
                | ((*rs1 as u32) << 15)
                | ((*rs2 as u32) << 20)
                | (funct7 << 25);

            let decoded = TypeR::from_riscv(inst_word);
            assert_eq!(decoded.rd, *rd);
            assert_eq!(decoded.rs1, *rs1);
            assert_eq!(decoded.rs2, *rs2);
            assert_eq!(decoded.func, func);

            let encoded = decoded.to_riscv_no_opcode() | *opcode;
            assert_eq!(
                encoded, inst_word,
                "Round-trip failed for R-type: opcode=0x{opcode:02x}, rd={rd}, rs1={rs1}, rs2={rs2}, \
                 funct7=0x{funct7:02x}"
            );
        }
    }

    /// Test round-trip for I-type instructions
    #[test]
    fn test_type_i_round_trip() {
        let test_cases: [(u32, u8, u8, i32, u8); 4] = [
            (0x13, 1, 2, 5, 0x0),  // addi with positive immediate
            (0x13, 1, 2, -5, 0x0), // addi with negative immediate
            (0x03, 1, 2, 4, 0x2),  // lw
            (0x67, 1, 2, 0, 0x0),  // jalr
        ];

        for (opcode, rd, rs1, imm, funct3) in test_cases.iter() {
            let imm_12bit = (*imm as u32) & 0xfff;
            let inst_word = *opcode
                | ((*rd as u32) << 7)
                | ((*funct3 as u32) << 12)
                | ((*rs1 as u32) << 15)
                | (imm_12bit << 20);

            let decoded = TypeI::from_riscv(inst_word);
            assert_eq!(decoded.rd, *rd);
            assert_eq!(decoded.rs1, *rs1);
            assert_eq!(decoded.func, *funct3);
            assert_eq!(
                decoded.imm, *imm,
                "Immediate mismatch: expected {imm}, got {}",
                decoded.imm
            );

            let encoded = decoded.to_riscv_no_opcode() | *opcode;
            assert_eq!(
                encoded, inst_word,
                "Round-trip failed for I-type: opcode=0x{opcode:02x}, rd={rd}, rs1={rs1}, imm={imm}, \
                 funct3=0x{funct3:x}"
            );
        }
    }

    /// Test round-trip for S-type instructions
    #[test]
    fn test_type_s_round_trip() {
        let test_cases: [(u32, u8, u8, i32, u8); 2] = [
            (0x23, 1, 2, 4, 0x2),  // sw with positive offset
            (0x23, 1, 2, -4, 0x2), // sw with negative offset
        ];

        for (opcode, rs1, rs2, imm, funct3) in test_cases.iter() {
            let imm_12bit = (*imm as u32) & 0xfff;
            let imm_lo = imm_12bit & 0x1f;
            let imm_hi = (imm_12bit >> 5) & 0x7f;
            let inst_word = *opcode
                | (imm_lo << 7)
                | ((*funct3 as u32) << 12)
                | ((*rs1 as u32) << 15)
                | ((*rs2 as u32) << 20)
                | (imm_hi << 25);

            let decoded = TypeS::from_riscv(inst_word);
            assert_eq!(decoded.rs1, *rs1);
            assert_eq!(decoded.rs2, *rs2);
            assert_eq!(decoded.func, *funct3);
            assert_eq!(
                decoded.imm, *imm,
                "Immediate mismatch: expected {imm}, got {}",
                decoded.imm
            );

            let encoded = decoded.to_riscv_no_opcode() | *opcode;
            assert_eq!(
                encoded, inst_word,
                "Round-trip failed for S-type: opcode=0x{opcode:02x}, rs1={rs1}, rs2={rs2}, imm={imm}, \
                 funct3=0x{funct3:x}"
            );
        }
    }

    /// Test round-trip for B-type instructions
    #[test]
    fn test_type_b_round_trip() {
        let test_cases: [(u32, u8, u8, i32, u8); 3] = [
            (0x63, 1, 2, 8, 0x0),  // beq with positive offset
            (0x63, 1, 2, -8, 0x0), // beq with negative offset
            (0x63, 1, 2, 16, 0x1), // bne
        ];

        for (opcode, rs1, rs2, imm, funct3) in test_cases.iter() {
            let imm_13bit = (*imm as u32) & 0x1fff;
            let imm_12 = (imm_13bit >> 12) & 0x1;
            let imm_11 = (imm_13bit >> 11) & 0x1;
            let imm_10_5 = (imm_13bit >> 5) & 0x3f;
            let imm_4_1 = (imm_13bit >> 1) & 0xf;
            let inst_word = *opcode
                | (imm_4_1 << 8)
                | ((*funct3 as u32) << 12)
                | ((*rs1 as u32) << 15)
                | ((*rs2 as u32) << 20)
                | (imm_10_5 << 25)
                | (imm_11 << 7)
                | (imm_12 << 31);

            let decoded = TypeB::from_riscv(inst_word);
            assert_eq!(decoded.rs1, *rs1);
            assert_eq!(decoded.rs2, *rs2);
            assert_eq!(decoded.func, *funct3);
            assert_eq!(
                decoded.imm, *imm,
                "Immediate mismatch: expected {imm}, got {}",
                decoded.imm
            );

            let encoded = decoded.to_riscv_no_opcode() | *opcode;
            assert_eq!(
                encoded, inst_word,
                "Round-trip failed for B-type: opcode=0x{opcode:02x}, rs1={rs1}, rs2={rs2}, imm={imm}, \
                 funct3=0x{funct3:x}"
            );
        }
    }

    /// Test round-trip for J-type instructions
    #[test]
    fn test_type_j_round_trip() {
        let test_cases: [(u32, u8, i32); 3] = [
            (0x6f, 1, 16),   // jal with positive offset
            (0x6f, 1, -16),  // jal with negative offset
            (0x6f, 0, 2048), // jal with large positive offset
        ];

        for (opcode, rd, imm) in test_cases.iter() {
            let imm_21bit = (*imm as u32) & 0x1fffff;
            let imm_20 = (imm_21bit >> 20) & 0x1;
            let imm_10_1 = (imm_21bit >> 1) & 0x3ff;
            let imm_11 = (imm_21bit >> 11) & 0x1;
            let imm_19_12 = (imm_21bit >> 12) & 0xff;
            let inst_word = *opcode
                | ((*rd as u32) << 7)
                | (imm_20 << 31)
                | (imm_10_1 << 21)
                | (imm_11 << 20)
                | (imm_19_12 << 12);

            let decoded = TypeJ::from_riscv(inst_word);
            assert_eq!(decoded.rd, *rd);
            assert_eq!(
                decoded.imm, *imm,
                "Immediate mismatch: expected {imm}, got {}",
                decoded.imm
            );

            let encoded = decoded.to_riscv_no_opcode() | *opcode;
            assert_eq!(
                encoded, inst_word,
                "Round-trip failed for J-type: opcode=0x{opcode:02x}, rd={rd}, imm={imm}"
            );
        }
    }

    /// Test round-trip for U-type instructions
    #[test]
    fn test_type_u_round_trip() {
        let test_cases: [(u32, u8, i32); 5] = [
            (0x37, 1, 0x00000000),           // lui with zero
            (0x37, 1, 0x12345000),           // lui with positive
            (0x17, 1, 0x00000000),           // auipc with zero
            (0x17, 1, 0xfffff000u32 as i32), // auipc with negative (sign-extended)
            (0x17, 5, 0xff000000u32 as i32), // auipc with 0xff000 in upper bits
        ];

        for (opcode, rd, imm) in test_cases.iter() {
            let imm_u32 = *imm as u32;
            let imm_hi = (imm_u32 >> 12) & 0xfffff;
            let inst_word = *opcode | ((*rd as u32) << 7) | (imm_hi << 12);

            let decoded = TypeU::from_riscv(inst_word);
            assert_eq!(decoded.rd, *rd);
            assert_eq!(
                decoded.imm, *imm,
                "Immediate mismatch: expected 0x{imm_u32:08x}, got 0x{:08x}",
                decoded.imm as u32
            );

            let encoded = decoded.to_riscv_no_opcode() | *opcode;
            assert_eq!(
                encoded, inst_word,
                "Round-trip failed for U-type: opcode=0x{opcode:02x}, rd={rd}, imm=0x{imm_u32:08x}"
            );
        }
    }

    /// Test U-type with various immediate values
    #[test]
    fn test_type_u_various_immediates() {
        let test_cases: [i32; 7] = [
            0x00000000,
            0x00001000,
            0x12345000,
            0x7ffff000,           // Max positive
            0x80000000u32 as i32, // Min negative (bit 31 set)
            0xfffff000u32 as i32, // Max negative (all upper bits set)
            0xff000000u32 as i32, // 0xff000 in upper bits
        ];

        for imm in test_cases.iter() {
            let rd = 5;
            let opcode = 0x17; // AUIPC
            let imm_u32 = *imm as u32;
            let imm_hi = (imm_u32 >> 12) & 0xfffff;
            let inst_word = opcode | ((rd as u32) << 7) | (imm_hi << 12);

            let decoded = TypeU::from_riscv(inst_word);
            assert_eq!(
                decoded.imm, *imm,
                "Immediate mismatch for 0x{imm_u32:08x}: expected 0x{imm_u32:08x}, got 0x{:08x}",
                decoded.imm as u32
            );

            let encoded = decoded.to_riscv_no_opcode() | opcode;
            assert_eq!(
                encoded, inst_word,
                "Round-trip failed for U-type with imm=0x{imm_u32:08x}"
            );
        }
    }
}
