//! RISC-V 32-bit instruction decoding.
//!
//! This module provides functions to decode RISC-V 32-bit instructions
//! into their structured representation.
//!
//! This implementation uses format-specific decoding (following embive's approach)
//! to only extract the fields needed for each instruction type, avoiding
//! the inefficiency of extracting all immediate formats for every instruction.

use super::{format::*, inst::Inst, regs::Gpr};

/// Decode a 32-bit instruction word into a structured representation.
/// This function handles both 32-bit and 16-bit compressed instructions.
pub fn decode_instruction(inst: u32) -> Result<Inst, alloc::string::String> {
    use alloc::format;

    // Check if this is a compressed instruction (bits [1:0] != 0b11)
    if (inst & 0x3) != 0x3 {
        // It's a 16-bit compressed instruction
        return crate::decode_rvc::decode_compressed(inst as u16);
    }

    let opcode = (inst & 0x7f) as u8;

    match opcode {
        0x33 => {
            // R-type (arithmetic)
            let r = TypeR::from_riscv(inst);
            let rd = Gpr::new(r.rd);
            let rs1 = Gpr::new(r.rs1);
            let rs2 = Gpr::new(r.rs2);
            let funct3 = (r.func & 0x7) as u8;
            let funct7 = ((r.func >> 3) & 0x7f) as u8;
            match (funct3, funct7) {
                (0x0, 0x0) => Ok(Inst::Add { rd, rs1, rs2 }),
                (0x0, 0x20) => Ok(Inst::Sub { rd, rs1, rs2 }),
                // M extension (multiply/divide)
                (0x0, 0x01) => Ok(Inst::Mul { rd, rs1, rs2 }),
                (0x1, 0x01) => Ok(Inst::Mulh { rd, rs1, rs2 }),
                (0x2, 0x01) => Ok(Inst::Mulhsu { rd, rs1, rs2 }),
                (0x3, 0x01) => Ok(Inst::Mulhu { rd, rs1, rs2 }),
                (0x4, 0x01) => Ok(Inst::Div { rd, rs1, rs2 }),
                (0x5, 0x01) => Ok(Inst::Divu { rd, rs1, rs2 }),
                (0x6, 0x01) => Ok(Inst::Rem { rd, rs1, rs2 }),
                (0x7, 0x01) => Ok(Inst::Remu { rd, rs1, rs2 }),
                // Base integer instructions
                (0x2, 0x0) => Ok(Inst::Slt { rd, rs1, rs2 }),
                (0x3, 0x0) => Ok(Inst::Sltu { rd, rs1, rs2 }),
                (0x4, 0x0) => Ok(Inst::Xor { rd, rs1, rs2 }),
                (0x6, 0x0) => Ok(Inst::Or { rd, rs1, rs2 }),
                (0x7, 0x0) => Ok(Inst::And { rd, rs1, rs2 }),
                (0x1, 0x0) => Ok(Inst::Sll { rd, rs1, rs2 }),
                (0x5, 0x0) => Ok(Inst::Srl { rd, rs1, rs2 }),
                (0x5, 0x20) => Ok(Inst::Sra { rd, rs1, rs2 }),
                // Zbb: Rotate instructions
                (0x1, 0x30) => Ok(Inst::Rol { rd, rs1, rs2 }),
                (0x5, 0x30) => Ok(Inst::Ror { rd, rs1, rs2 }),
                // Zbb: Logical operations
                (0x7, 0x20) => Ok(Inst::Andn { rd, rs1, rs2 }),
                (0x6, 0x20) => Ok(Inst::Orn { rd, rs1, rs2 }),
                (0x4, 0x20) => Ok(Inst::Xnor { rd, rs1, rs2 }),
                // Zbb: Min/Max instructions
                (0x4, 0x05) => Ok(Inst::Min { rd, rs1, rs2 }),
                (0x5, 0x05) => Ok(Inst::Minu { rd, rs1, rs2 }),
                (0x6, 0x05) => Ok(Inst::Max { rd, rs1, rs2 }),
                (0x7, 0x05) => Ok(Inst::Maxu { rd, rs1, rs2 }),
                // Zbs: Bit manipulation instructions
                (0x1, 0x24) => Ok(Inst::Bclr { rd, rs1, rs2 }),
                (0x5, 0x24) => Ok(Inst::Bext { rd, rs1, rs2 }),
                (0x1, 0x34) => Ok(Inst::Binv { rd, rs1, rs2 }),
                (0x1, 0x14) => Ok(Inst::Bset { rd, rs1, rs2 }),
                // Zba: Address generation instructions
                (0x2, 0x10) => Ok(Inst::Sh1add { rd, rs1, rs2 }),
                (0x4, 0x10) => Ok(Inst::Sh2add { rd, rs1, rs2 }),
                (0x6, 0x10) => Ok(Inst::Sh3add { rd, rs1, rs2 }),
                _ => Err(format!(
                    "Unknown R-type instruction: funct3=0x{funct3:x}, funct7=0x{funct7:x}"
                )),
            }
        }
        0x13 => {
            // I-type (immediate arithmetic/logical/shift)
            let i = TypeI::from_riscv(inst);
            let rd = Gpr::new(i.rd);
            let rs1 = Gpr::new(i.rs1);
            let funct3 = i.func;
            let imm_i = i.imm;
            let funct7 = ((inst >> 25) & 0x7f) as u8;
            match funct3 {
                0x0 => Ok(Inst::Addi {
                    rd,
                    rs1,
                    imm: imm_i,
                }),
                0x2 => Ok(Inst::Slti {
                    rd,
                    rs1,
                    imm: imm_i,
                }),
                0x3 => Ok(Inst::Sltiu {
                    rd,
                    rs1,
                    imm: imm_i,
                }),
                0x4 => {
                    // XORI and ZEXTH
                    // Check for ZEXTH (funct12=0x080)
                    let funct12 = ((inst >> 20) & 0xfff) as u16;
                    if funct12 == 0x080 {
                        Ok(Inst::Zexth { rd, rs1 })
                    } else {
                        Ok(Inst::Xori {
                            rd,
                            rs1,
                            imm: imm_i,
                        })
                    }
                }
                0x6 => Ok(Inst::Ori {
                    rd,
                    rs1,
                    imm: imm_i,
                }),
                0x7 => Ok(Inst::Andi {
                    rd,
                    rs1,
                    imm: imm_i,
                }),
                0x1 => {
                    // SLLI and other funct3=0x1 instructions
                    // Extract funct6 from bits [31:26] and imm[5:0] from bits [25:20]
                    let funct6 = ((inst >> 26) & 0x3f) as u8;
                    let imm_5_0 = ((inst >> 20) & 0x3f) as u8;

                    match funct6 {
                        0x00 => {
                            // SLLI: funct6=0x00
                            Ok(Inst::Slli {
                                rd,
                                rs1,
                                imm: imm_i,
                            })
                        }
                        0x12 => {
                            // BSETI: funct6=0b001010 (0x12)
                            Ok(Inst::Bseti {
                                rd,
                                rs1,
                                imm: imm_5_0 as i32,
                            })
                        }
                        0x1a => {
                            // BINVI: funct6=0b011010 (0x1a)
                            Ok(Inst::Binvi {
                                rd,
                                rs1,
                                imm: imm_5_0 as i32,
                            })
                        }
                        0x09 => {
                            // BCLRI: funct6=0b010010 (0x09)
                            Ok(Inst::Bclri {
                                rd,
                                rs1,
                                imm: imm_5_0 as i32,
                            })
                        }
                        0x02 => {
                            // SLLIUW: funct6=0b000010 (0x02)
                            Ok(Inst::SlliUw {
                                rd,
                                rs1,
                                imm: imm_5_0 as i32,
                            })
                        }
                        _ => {
                            // Check for funct12 encodings (CLZ, CTZ, CPOP, SEXTB, SEXTH)
                            let funct12 = ((inst >> 20) & 0xfff) as u16;
                            match funct12 {
                                0x600 => Ok(Inst::Clz { rd, rs1 }),
                                0x601 => Ok(Inst::Ctz { rd, rs1 }),
                                0x602 => Ok(Inst::Cpop { rd, rs1 }),
                                0x604 => Ok(Inst::Sextb { rd, rs1 }),
                                0x605 => Ok(Inst::Sexth { rd, rs1 }),
                                _ => Err(format!(
                                    "Unknown I-type instruction: funct3=0x{funct3:x}, funct6=0x{funct6:x}, funct12=0x{funct12:x}"
                                )),
                            }
                        }
                    }
                }
                0x5 => {
                    // SRLI/SRAI and other funct3=0x5 instructions
                    // Extract funct6 from bits [31:26] and imm[5:0] from bits [25:20]
                    let funct6 = ((inst >> 26) & 0x3f) as u8;
                    let imm_5_0 = ((inst >> 20) & 0x3f) as u8;

                    // Check for standard SRLI/SRAI first (funct7 encoding)
                    if funct7 == 0 {
                        Ok(Inst::Srli {
                            rd,
                            rs1,
                            imm: imm_i & 0x1f, // Extract only bits [4:0]
                        })
                    } else if funct7 == 0x20 {
                        // SRAI with funct7 encoding (standard)
                        Ok(Inst::Srai {
                            rd,
                            rs1,
                            imm: imm_i & 0x1f, // Extract only bits [4:0]
                        })
                    } else if funct6 == 0x10 {
                        // SRAI with funct6 encoding (Cranelift style)
                        // funct6=0x10 (0b010000), imm[5:0] contains shift amount
                        // Mask to 5 bits to match RISC-V spec (shift amounts are 5 bits)
                        Ok(Inst::Srai {
                            rd,
                            rs1,
                            imm: (imm_5_0 & 0x1f) as i32,
                        })
                    } else if funct6 == 0x0 {
                        // SRLI with funct6 encoding (Cranelift style)
                        // funct6=0x0 (0b000000), imm[5:0] contains shift amount
                        // Mask to 5 bits to match RISC-V spec (shift amounts are 5 bits)
                        Ok(Inst::Srli {
                            rd,
                            rs1,
                            imm: (imm_5_0 & 0x1f) as i32,
                        })
                    } else {
                        // Check for other funct6 encoded instructions
                        match funct6 {
                            0x18 => {
                                // RORI: funct6=0b011000 (0x18)
                                Ok(Inst::Rori {
                                    rd,
                                    rs1,
                                    imm: imm_5_0 as i32,
                                })
                            }
                            0x09 => {
                                // BEXTI: funct6=0b010010 (0x09)
                                Ok(Inst::Bexti {
                                    rd,
                                    rs1,
                                    imm: imm_5_0 as i32,
                                })
                            }
                            _ => {
                                // Check for funct12 encodings (REV8, ORCB, BREV8)
                                let funct12 = ((inst >> 20) & 0xfff) as u16;
                                match funct12 {
                                    0x6b8 => Ok(Inst::Rev8 { rd, rs1 }),
                                    0x287 => Ok(Inst::Orcb { rd, rs1 }),
                                    0x687 => Ok(Inst::Brev8 { rd, rs1 }),
                                    _ => Err(format!(
                                        "Unknown I-type instruction: funct3=0x{funct3:x}, funct7=0x{funct7:x}, funct6=0x{funct6:x}, funct12=0x{funct12:x}"
                                    )),
                                }
                            }
                        }
                    }
                }
                _ => Err(format!(
                    "Unknown I-type arithmetic instruction: funct3=0x{funct3:x}"
                )),
            }
        }
        0x03 => {
            // I-type (load)
            let i = TypeI::from_riscv(inst);
            let rd = Gpr::new(i.rd);
            let rs1 = Gpr::new(i.rs1);
            match i.func {
                0x0 => Ok(Inst::Lb {
                    rd,
                    rs1,
                    imm: i.imm,
                }),
                0x1 => Ok(Inst::Lh {
                    rd,
                    rs1,
                    imm: i.imm,
                }),
                0x2 => Ok(Inst::Lw {
                    rd,
                    rs1,
                    imm: i.imm,
                }),
                0x3 => Err(format!(
                    "Invalid load instruction: funct3=0x3 (reserved on RV32, LD is RV64-only)"
                )),
                0x4 => Ok(Inst::Lbu {
                    rd,
                    rs1,
                    imm: i.imm,
                }),
                0x5 => Ok(Inst::Lhu {
                    rd,
                    rs1,
                    imm: i.imm,
                }),
                _ => Err(format!("Unknown load instruction: funct3=0x{:x}", i.func)),
            }
        }
        0x23 => {
            // S-type (store)
            let s = TypeS::from_riscv(inst);
            let rs1 = Gpr::new(s.rs1);
            let rs2 = Gpr::new(s.rs2);
            match s.func {
                0x0 => Ok(Inst::Sb {
                    rs1,
                    rs2,
                    imm: s.imm,
                }),
                0x1 => Ok(Inst::Sh {
                    rs1,
                    rs2,
                    imm: s.imm,
                }),
                0x2 => Ok(Inst::Sw {
                    rs1,
                    rs2,
                    imm: s.imm,
                }),
                0x3 => {
                    // FSD: Floating-point store double (RV32: treat as SW)
                    // On RV32, FSD doesn't exist, but treat it as a word store
                    Ok(Inst::Sw {
                        rs1,
                        rs2,
                        imm: s.imm,
                    })
                }
                _ => Err(format!("Unknown store instruction: funct3=0x{:x}", s.func)),
            }
        }
        0x27 => {
            // S-type (floating-point store: FSH, FSW, FSD)
            let s = TypeS::from_riscv(inst);
            let rs1 = Gpr::new(s.rs1);
            let rs2 = Gpr::new(s.rs2);
            match s.func {
                0x1 => {
                    // FSH: Floating-point store halfword (treat as SH)
                    Ok(Inst::Sh {
                        rs1,
                        rs2,
                        imm: s.imm,
                    })
                }
                0x2 => {
                    // FSW: Floating-point store word (treat as SW)
                    Ok(Inst::Sw {
                        rs1,
                        rs2,
                        imm: s.imm,
                    })
                }
                0x3 => {
                    // FSD: Floating-point store double (RV32: treat as SW)
                    Ok(Inst::Sw {
                        rs1,
                        rs2,
                        imm: s.imm,
                    })
                }
                _ => Err(format!(
                    "Unknown floating-point store instruction: funct3=0x{:x}",
                    s.func
                )),
            }
        }
        0x37 => {
            // U-type (lui)
            let u = TypeU::from_riscv(inst);
            let rd = Gpr::new(u.rd);
            Ok(Inst::Lui { rd, imm: u.imm })
        }
        0x17 => {
            // U-type (auipc)
            let u = TypeU::from_riscv(inst);
            let rd = Gpr::new(u.rd);
            Ok(Inst::Auipc { rd, imm: u.imm })
        }
        0x6f => {
            // J-type (jal)
            let j = TypeJ::from_riscv(inst);
            let rd = Gpr::new(j.rd);
            Ok(Inst::Jal { rd, imm: j.imm })
        }
        0x67 => {
            // I-type (jalr)
            let i = TypeI::from_riscv(inst);
            let rd = Gpr::new(i.rd);
            let rs1 = Gpr::new(i.rs1);
            match i.func {
                0x0 => Ok(Inst::Jalr {
                    rd,
                    rs1,
                    imm: i.imm,
                }),
                _ => Err(format!("Unknown jalr instruction: funct3=0x{:x}", i.func)),
            }
        }
        0x63 => {
            // B-type (branch)
            let b = TypeB::from_riscv(inst);
            let rs1 = Gpr::new(b.rs1);
            let rs2 = Gpr::new(b.rs2);
            match b.func {
                0x0 => Ok(Inst::Beq {
                    rs1,
                    rs2,
                    imm: b.imm,
                }),
                0x1 => Ok(Inst::Bne {
                    rs1,
                    rs2,
                    imm: b.imm,
                }),
                0x4 => Ok(Inst::Blt {
                    rs1,
                    rs2,
                    imm: b.imm,
                }),
                0x5 => Ok(Inst::Bge {
                    rs1,
                    rs2,
                    imm: b.imm,
                }),
                0x6 => Ok(Inst::Bltu {
                    rs1,
                    rs2,
                    imm: b.imm,
                }),
                0x7 => Ok(Inst::Bgeu {
                    rs1,
                    rs2,
                    imm: b.imm,
                }),
                _ => Err(format!("Unknown branch instruction: funct3=0x{:x}", b.func)),
            }
        }
        0x0f => {
            // FENCE/FENCE.I instructions
            // Check funct3 to distinguish between FENCE and FENCE.I
            let funct3 = ((inst >> 12) & 0x7) as u8;
            let imm = ((inst >> 20) & 0xfff) as u16;
            let rs1 = ((inst >> 15) & 0x1f) as u8;
            let rd = ((inst >> 7) & 0x1f) as u8;

            if funct3 == 0x1 && imm == 0x001 && rs1 == 0 && rd == 0 {
                // FENCE.I: funct3=0x1, imm[11:0]=0x001, rs1=0, rd=0
                Ok(Inst::FenceI)
            } else {
                // FENCE: funct3=0x0 (or other values, but we treat as FENCE)
                Ok(Inst::Fence)
            }
        }
        0x73 => {
            // System instructions
            if inst == 0x00000073 {
                Ok(Inst::Ecall)
            } else if inst == 0x00100073 {
                Ok(Inst::Ebreak)
            } else {
                // CSR instructions: CSRRW, CSRRS, CSRRC, CSRRWI, CSRRSI, CSRRCI
                // Format: [31:20]=csr, [19:15]=rs1/imm, [14:12]=funct3, [11:7]=rd, [6:0]=opcode(0x73)
                let funct3 = ((inst >> 12) & 0x7) as u8;
                let rd = ((inst >> 7) & 0x1f) as u8;
                let rs1_or_imm = ((inst >> 15) & 0x1f) as u8;
                let csr = ((inst >> 20) & 0xfff) as u16;

                // Decode CSR instruction based on funct3
                // 0b001 = CSRRW, 0b010 = CSRRS, 0b011 = CSRRC
                // 0b101 = CSRRWI, 0b110 = CSRRSI, 0b111 = CSRRCI
                match funct3 {
                    0b001 => Ok(Inst::Csrrw {
                        rd: Gpr::new(rd),
                        rs1: Gpr::new(rs1_or_imm),
                        csr,
                    }),
                    0b010 => Ok(Inst::Csrrs {
                        rd: Gpr::new(rd),
                        rs1: Gpr::new(rs1_or_imm),
                        csr,
                    }),
                    0b011 => Ok(Inst::Csrrc {
                        rd: Gpr::new(rd),
                        rs1: Gpr::new(rs1_or_imm),
                        csr,
                    }),
                    0b101 => Ok(Inst::Csrrwi {
                        rd: Gpr::new(rd),
                        imm: rs1_or_imm as i32,
                        csr,
                    }),
                    0b110 => Ok(Inst::Csrrsi {
                        rd: Gpr::new(rd),
                        imm: rs1_or_imm as i32,
                        csr,
                    }),
                    0b111 => Ok(Inst::Csrrci {
                        rd: Gpr::new(rd),
                        imm: rs1_or_imm as i32,
                        csr,
                    }),
                    _ => Err(format!(
                        "Unknown CSR instruction: funct3=0x{funct3:x}, inst=0x{inst:08x}"
                    )),
                }
            }
        }
        0x2f => {
            // Atomic instructions (A extension)
            let r = TypeR::from_riscv(inst);
            let rd = Gpr::new(r.rd);
            let rs1 = Gpr::new(r.rs1);
            let rs2 = Gpr::new(r.rs2);
            let funct3 = (r.func & 0x7) as u8;
            let funct5 = ((inst >> 27) & 0x1f) as u8;

            if funct3 != 0x2 {
                return Err(format!("Unsupported atomic width: funct3=0x{funct3:x}"));
            }

            match funct5 {
                0x02 => Ok(Inst::LrW { rd, rs1 }),
                0x03 => Ok(Inst::ScW { rd, rs1, rs2 }),
                0x01 => Ok(Inst::AmoswapW { rd, rs1, rs2 }),
                0x00 => Ok(Inst::AmoaddW { rd, rs1, rs2 }),
                0x04 => Ok(Inst::AmoxorW { rd, rs1, rs2 }),
                0x0c => Ok(Inst::AmoandW { rd, rs1, rs2 }),
                0x08 => Ok(Inst::AmoorW { rd, rs1, rs2 }),
                _ => Err(format!("Unknown atomic instruction: funct5=0x{funct5:x}")),
            }
        }
        _ => Err(format!("Unknown opcode: 0x{opcode:02x}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{encode::auipc, inst::Inst};

    /// Test that encoding and decoding AUIPC produces consistent results
    fn test_auipc_round_trip(imm: i32, rd: Gpr) {
        // Encode
        let encoded = auipc(rd, imm);

        // Decode
        let decoded = decode_instruction(encoded).expect("Failed to decode");

        // Verify it's an AUIPC instruction
        match decoded {
            Inst::Auipc {
                rd: decoded_rd,
                imm: decoded_imm,
            } => {
                assert_eq!(
                    decoded_rd, rd,
                    "Register mismatch: expected {rd:?}, got {decoded_rd:?}"
                );

                // The decoded imm is the final i32 value (sign-extended and shifted)
                // It should match the encoded imm
                assert_eq!(
                    decoded_imm, imm,
                    "Immediate mismatch: encoded imm={} (0x{:08x}), decoded imm={} (0x{:08x})",
                    imm, imm as u32, decoded_imm, decoded_imm as u32
                );
            }
            _ => panic!("Expected AUIPC instruction, got {:?}", decoded),
        }
    }

    #[test]
    fn test_auipc_round_trip_zero() {
        test_auipc_round_trip(0, Gpr::T0);
    }

    #[test]
    fn test_auipc_round_trip_positive_small() {
        test_auipc_round_trip(0x1000, Gpr::T0); // 1 << 12
        test_auipc_round_trip(0x123000, Gpr::T0); // 0x123 << 12
        test_auipc_round_trip(0x1234000, Gpr::T0); // 0x1234 << 12
    }

    #[test]
    fn test_auipc_round_trip_positive_large() {
        test_auipc_round_trip(0x12345000, Gpr::T0); // 0x12345 << 12
        test_auipc_round_trip(0x7ffff000, Gpr::T0); // Max positive 20-bit << 12
    }

    #[test]
    fn test_auipc_round_trip_negative() {
        // Negative values: upper 20 bits with sign bit set, already shifted
        test_auipc_round_trip(0x80000000u32 as i32, Gpr::T0); // Min negative 20-bit << 12
        test_auipc_round_trip(0xfffff000u32 as i32, Gpr::T0); // -1 in 20-bit << 12
        test_auipc_round_trip(0xff000000u32 as i32, Gpr::T0); // Specific negative value << 12
    }

    #[test]
    fn test_auipc_round_trip_all_registers() {
        test_auipc_round_trip(0x12345000, Gpr::Zero);
        test_auipc_round_trip(0x12345000, Gpr::Ra);
        test_auipc_round_trip(0x12345000, Gpr::Sp);
        test_auipc_round_trip(0x12345000, Gpr::Gp);
        test_auipc_round_trip(0x12345000, Gpr::Tp);
        test_auipc_round_trip(0x12345000, Gpr::T0);
        test_auipc_round_trip(0x12345000, Gpr::T1);
        test_auipc_round_trip(0x12345000, Gpr::T2);
        test_auipc_round_trip(0x12345000, Gpr::S0);
        test_auipc_round_trip(0x12345000, Gpr::S1);
        test_auipc_round_trip(0x12345000, Gpr::A0);
        test_auipc_round_trip(0x12345000, Gpr::A1);
    }

    #[test]
    fn test_auipc_decode_specific_instructions() {
        // Test decoding specific instruction words
        // auipc t0, 0
        let inst = decode_instruction(0x00000297).expect("Failed to decode");
        match inst {
            Inst::Auipc { rd, imm } => {
                assert_eq!(rd, Gpr::T0);
                assert_eq!(imm, 0x00000);
            }
            _ => panic!("Expected AUIPC, got {:?}", inst),
        }

        // auipc t0, 0xff000 (upper 20 bits)
        // When decoded, this becomes 0xff000000 (sign-extended and shifted)
        let inst = decode_instruction(0xff000297).expect("Failed to decode");
        match inst {
            Inst::Auipc { rd, imm } => {
                assert_eq!(rd, Gpr::T0);
                assert_eq!(imm, 0xff000000u32 as i32);
            }
            _ => panic!("Expected AUIPC, got {:?}", inst),
        }

        // auipc t0, 0xfffff (upper 20 bits)
        // When decoded, this becomes 0xfffff000 (sign-extended and shifted)
        let inst = decode_instruction(0xfffff297).expect("Failed to decode");
        match inst {
            Inst::Auipc { rd, imm } => {
                assert_eq!(rd, Gpr::T0);
                assert_eq!(imm, 0xfffff000u32 as i32);
            }
            _ => panic!("Expected AUIPC, got {:?}", inst),
        }
    }

    #[test]
    fn test_auipc_sign_extension_behavior() {
        // Test that negative immediates are handled correctly
        // When we encode 0xfffff000 (the final value), it extracts 0xfffff as the upper 20 bits
        // When decoded, it should be 0xfffff000 (sign-extended and shifted)
        let encoded = auipc(Gpr::T0, 0xfffff000u32 as i32);
        let decoded = decode_instruction(encoded).expect("Failed to decode");
        match decoded {
            Inst::Auipc { imm, .. } => {
                // The decoded imm should be the sign-extended and shifted value 0xfffff000
                assert_eq!(imm, 0xfffff000u32 as i32);
            }
            _ => panic!("Expected AUIPC"),
        }

        // Same for 0xff000
        let encoded = auipc(Gpr::T0, 0xff000);
        let decoded = decode_instruction(encoded).expect("Failed to decode");
        match decoded {
            Inst::Auipc { imm, .. } => {
                assert_eq!(imm, 0xff000);
            }
            _ => panic!("Expected AUIPC"),
        }
    }

    #[test]
    fn test_fence_i_decode() {
        // FENCE.I: 0x0010100f (per RISC-V spec: imm[11:0]=0x001)
        let inst = decode_instruction(0x0010100f).expect("Failed to decode");
        match inst {
            Inst::FenceI => {}
            _ => panic!("Expected FenceI, got {:?}", inst),
        }
    }

    #[test]
    fn test_fence_i_round_trip() {
        use crate::encode::fence_i;
        // Encode FENCE.I (per RISC-V spec: imm[11:0]=0x001)
        let encoded = fence_i();
        assert_eq!(encoded, 0x0010100f);

        // Decode it back
        let decoded = decode_instruction(encoded).expect("Failed to decode");
        match decoded {
            Inst::FenceI => {}
            _ => panic!("Expected FenceI, got {:?}", decoded),
        }
    }

    /// Test SRLI decoding with standard funct7 encoding
    #[test]
    fn test_srli_decode_standard() {
        use crate::encode::srli;
        use crate::regs::Gpr;

        // Test all shift amounts 0-31
        for shamt in 0..32 {
            let encoded = srli(Gpr::A0, Gpr::A1, shamt);
            let decoded = decode_instruction(encoded).expect("Failed to decode");

            match decoded {
                Inst::Srli { rd, rs1, imm } => {
                    assert_eq!(rd, Gpr::A0, "rd mismatch for shamt={shamt}");
                    assert_eq!(rs1, Gpr::A1, "rs1 mismatch for shamt={shamt}");
                    assert_eq!(
                        imm, shamt,
                        "shift amount mismatch: expected {shamt}, got {imm}"
                    );
                }
                _ => panic!("Expected SRLI, got {:?} for shamt={}", decoded, shamt),
            }
        }
    }

    /// Test SRAI decoding with standard funct7 encoding
    #[test]
    fn test_srai_decode_standard() {
        use crate::encode::srai;
        use crate::regs::Gpr;

        // Test all shift amounts 0-31
        for shamt in 0..32 {
            let encoded = srai(Gpr::A0, Gpr::A1, shamt);
            let decoded = decode_instruction(encoded).expect("Failed to decode");

            match decoded {
                Inst::Srai { rd, rs1, imm } => {
                    assert_eq!(rd, Gpr::A0, "rd mismatch for shamt={shamt}");
                    assert_eq!(rs1, Gpr::A1, "rs1 mismatch for shamt={shamt}");
                    assert_eq!(
                        imm, shamt,
                        "shift amount mismatch: expected {shamt}, got {imm}"
                    );
                }
                _ => panic!("Expected SRAI, got {:?} for shamt={}", decoded, shamt),
            }
        }
    }

    /// Test SRLI/SRAI with specific instruction encodings to verify bit extraction
    #[test]
    fn test_srli_srai_bit_extraction() {
        use crate::regs::Gpr;

        // SRLI a0, a1, 0
        // opcode=0x13, rd=10 (a0), funct3=0x5, rs1=11 (a1), imm[4:0]=0, imm[11:5]=0x00
        // Instruction: 0x0005d513
        let inst = decode_instruction(0x0005d513).expect("Failed to decode");
        match inst {
            Inst::Srli { rd, rs1, imm } => {
                assert_eq!(rd, Gpr::A0);
                assert_eq!(rs1, Gpr::A1);
                assert_eq!(imm, 0, "Expected shift amount 0, got {imm}");
            }
            _ => panic!("Expected SRLI, got {:?}", inst),
        }

        // SRLI a0, a1, 24
        // opcode=0x13, rd=10 (a0), funct3=0x5, rs1=11 (a1), imm[4:0]=24, imm[11:5]=0x00
        // Instruction: 0x0185d513
        let inst = decode_instruction(0x0185d513).expect("Failed to decode");
        match inst {
            Inst::Srli { rd, rs1, imm } => {
                assert_eq!(rd, Gpr::A0);
                assert_eq!(rs1, Gpr::A1);
                assert_eq!(imm, 24, "Expected shift amount 24, got {imm}");
            }
            _ => panic!("Expected SRLI, got {:?}", inst),
        }

        // SRAI a0, a1, 0
        // opcode=0x13, rd=10 (a0), funct3=0x5, rs1=11 (a1), imm[4:0]=0, imm[11:5]=0x20
        // Instruction: 0x4005d513
        let inst = decode_instruction(0x4005d513).expect("Failed to decode");
        match inst {
            Inst::Srai { rd, rs1, imm } => {
                assert_eq!(rd, Gpr::A0);
                assert_eq!(rs1, Gpr::A1);
                assert_eq!(imm, 0, "Expected shift amount 0, got {imm}");
            }
            _ => panic!("Expected SRAI, got {:?}", inst),
        }

        // SRAI a0, a1, 24
        // opcode=0x13, rd=10 (a0), funct3=0x5, rs1=11 (a1), imm[4:0]=24, imm[11:5]=0x20
        // Instruction: 0x4185d513
        let inst = decode_instruction(0x4185d513).expect("Failed to decode");
        match inst {
            Inst::Srai { rd, rs1, imm } => {
                assert_eq!(rd, Gpr::A0);
                assert_eq!(rs1, Gpr::A1);
                assert_eq!(imm, 24, "Expected shift amount 24, got {imm}");
            }
            _ => panic!("Expected SRAI, got {:?}", inst),
        }

        // SRAI a0, a1, 31
        // opcode=0x13, rd=10 (a0), funct3=0x5, rs1=11 (a1), imm[4:0]=31, imm[11:5]=0x20
        // Instruction: 0x41f5d513
        let inst = decode_instruction(0x41f5d513).expect("Failed to decode");
        match inst {
            Inst::Srai { rd, rs1, imm } => {
                assert_eq!(rd, Gpr::A0);
                assert_eq!(rs1, Gpr::A1);
                assert_eq!(imm, 31, "Expected shift amount 31, got {imm}");
            }
            _ => panic!("Expected SRAI, got {:?}", inst),
        }
    }

    /// Test that negative imm_i values (from SRAI) are handled correctly
    #[test]
    fn test_srai_negative_imm_handling() {
        // For SRAI, imm_i will be negative because bits [31:25] = 0x20
        // This sets bit 11 of the immediate, making it negative when sign-extended
        // But imm_i & 0x1f should still correctly extract bits [4:0]

        // SRAI with shamt=0: imm_i should be 0xfffffe00 (negative), but imm_i & 0x1f = 0
        let inst = decode_instruction(0x40055513).expect("Failed to decode");
        match inst {
            Inst::Srai { imm, .. } => {
                assert_eq!(imm, 0, "Expected shift amount 0, got {imm}");
            }
            _ => panic!("Expected SRAI"),
        }

        // SRAI with shamt=24: imm_i should be 0xfffffe18 (negative), but imm_i & 0x1f = 24
        let inst = decode_instruction(0x41855513).expect("Failed to decode");
        match inst {
            Inst::Srai { imm, .. } => {
                assert_eq!(imm, 24, "Expected shift amount 24, got {imm}");
            }
            _ => panic!("Expected SRAI"),
        }
    }

    /// Test round-trip encoding/decoding for SRLI
    #[test]
    fn test_srli_round_trip() {
        use crate::encode::srli;
        use crate::regs::Gpr;

        // Test various shift amounts
        let shift_amounts = [0, 1, 5, 16, 24, 31];

        for &shamt in &shift_amounts {
            let encoded = srli(Gpr::A0, Gpr::A1, shamt);
            let decoded = decode_instruction(encoded).expect("Failed to decode");

            match decoded {
                Inst::Srli { rd, rs1, imm } => {
                    assert_eq!(rd, Gpr::A0, "rd mismatch for shamt={shamt}");
                    assert_eq!(rs1, Gpr::A1, "rs1 mismatch for shamt={shamt}");
                    assert_eq!(
                        imm, shamt,
                        "shift amount mismatch: expected {shamt}, got {imm}"
                    );
                }
                _ => panic!("Expected SRLI, got {:?} for shamt={}", decoded, shamt),
            }
        }
    }

    /// Test round-trip encoding/decoding for SRAI
    #[test]
    fn test_srai_round_trip() {
        use crate::encode::srai;
        use crate::regs::Gpr;

        // Test various shift amounts
        let shift_amounts = [0, 1, 5, 16, 24, 31];

        for &shamt in &shift_amounts {
            let encoded = srai(Gpr::A0, Gpr::A1, shamt);
            let decoded = decode_instruction(encoded).expect("Failed to decode");

            match decoded {
                Inst::Srai { rd, rs1, imm } => {
                    assert_eq!(rd, Gpr::A0, "rd mismatch for shamt={shamt}");
                    assert_eq!(rs1, Gpr::A1, "rs1 mismatch for shamt={shamt}");
                    assert_eq!(
                        imm, shamt,
                        "shift amount mismatch: expected {shamt}, got {imm}"
                    );
                }
                _ => panic!("Expected SRAI, got {:?} for shamt={}", decoded, shamt),
            }
        }
    }

    /// Test that shift amounts > 31 are masked correctly
    #[test]
    fn test_srli_srai_shift_mask() {
        use crate::encode::{srai, srli};
        use crate::regs::Gpr;

        // Test that shift amounts > 31 are masked to 5 bits
        // Note: The encoder should mask these, but let's verify the decoder handles them correctly

        // SRLI with shamt=32 (should be masked to 0)
        let encoded = srli(Gpr::A0, Gpr::A1, 32);
        let decoded = decode_instruction(encoded).expect("Failed to decode");
        match decoded {
            Inst::Srli { imm, .. } => {
                assert_eq!(imm, 0, "Shift amount 32 should be masked to 0");
            }
            _ => panic!("Expected SRLI"),
        }

        // SRAI with shamt=63 (should be masked to 31)
        let encoded = srai(Gpr::A0, Gpr::A1, 63);
        let decoded = decode_instruction(encoded).expect("Failed to decode");
        match decoded {
            Inst::Srai { imm, .. } => {
                assert_eq!(imm, 31, "Shift amount 63 should be masked to 31");
            }
            _ => panic!("Expected SRAI"),
        }
    }
}
