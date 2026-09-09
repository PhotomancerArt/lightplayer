//! spike: RV32IMC decoder into the translator's tiny IR.
//!
//! Mirrors `lp-riscv-emu`'s executors exactly where it matters for identity:
//! which encodings are *rejected* (the executor would raise an illegal
//! instruction, so the region must end before them), which cost class each
//! instruction charges, and the operand semantics. Anything not listed here is
//! a region boundary — refusing is always exact.

use lp_emu_core::InstClass;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpI {
    Addi,
    Slti,
    Sltiu,
    Xori,
    Ori,
    Andi,
    Slli,
    Srli,
    Srai,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    Lt,
    Ge,
    Ltu,
    Geu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadKind {
    B,
    H,
    W,
    Bu,
    Hu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreKind {
    B,
    H,
    W,
}

#[derive(Clone, Copy, Debug)]
pub enum Inst {
    /// `imm` is the final 32-bit value (already shifted).
    Lui { rd: u8, imm: i32 },
    Auipc { rd: u8, imm: i32 },
    Jal { rd: u8, imm: i32 },
    Jalr { rd: u8, rs1: u8, imm: i32 },
    Branch { cond: Cond, rs1: u8, rs2: u8, imm: i32 },
    Load { kind: LoadKind, rd: u8, rs1: u8, imm: i32 },
    Store { kind: StoreKind, rs1: u8, rs2: u8, imm: i32 },
    OpImm { op: OpI, rd: u8, rs1: u8, imm: i32 },
    Op { op: Op, rd: u8, rs1: u8, rs2: u8 },
    /// A plain `fence` — the executor charges `Fence` and does nothing.
    Fence,
    /// `c.nop` (and the `c.addi x0` hints the executor treats as it).
    Nop,
}

#[derive(Clone, Copy, Debug)]
pub struct Decoded {
    pub inst: Inst,
    pub width: u8,
    /// The class charged. For a branch this is `BranchTaken`; the translator
    /// charges `BranchNotTaken` on the fall-through path.
    pub class: InstClass,
}

impl Decoded {
    pub fn is_control(&self) -> bool {
        matches!(
            self.inst,
            Inst::Jal { .. } | Inst::Jalr { .. } | Inst::Branch { .. }
        )
    }
}

fn sext(v: u32, bits: u32) -> i32 {
    ((v << (32 - bits)) as i32) >> (32 - bits)
}

fn imm_i(w: u32) -> i32 {
    (w as i32) >> 20
}

fn imm_s(w: u32) -> i32 {
    sext(((w >> 25) << 5) | ((w >> 7) & 0x1f), 12)
}

fn imm_b(w: u32) -> i32 {
    let imm = ((w >> 31) & 1) << 12
        | ((w >> 7) & 1) << 11
        | ((w >> 25) & 0x3f) << 5
        | ((w >> 8) & 0xf) << 1;
    sext(imm, 13)
}

fn imm_j(w: u32) -> i32 {
    let imm = ((w >> 31) & 1) << 20
        | ((w >> 12) & 0xff) << 12
        | ((w >> 20) & 1) << 11
        | ((w >> 21) & 0x3ff) << 1;
    sext(imm, 21)
}

fn cj_off(i: u32) -> i32 {
    let imm = ((i >> 12) & 1) << 11
        | ((i >> 8) & 1) << 10
        | ((i >> 9) & 3) << 8
        | ((i >> 6) & 1) << 7
        | ((i >> 7) & 1) << 6
        | ((i >> 2) & 1) << 5
        | ((i >> 11) & 1) << 4
        | ((i >> 3) & 7) << 1;
    sext(imm, 12)
}

fn cb_off(i: u32) -> i32 {
    let imm = ((i >> 12) & 1) << 8
        | ((i >> 5) & 3) << 6
        | ((i >> 2) & 1) << 5
        | ((i >> 10) & 3) << 3
        | ((i >> 3) & 3) << 1;
    sext(imm, 9)
}

fn jalr_class(rd: u8, rs1: u8, imm: i32) -> InstClass {
    if rd != 0 {
        InstClass::JalrCall
    } else if rs1 == 1 && imm == 0 {
        InstClass::JalrReturn
    } else {
        InstClass::JalrIndirect
    }
}

/// Decode the instruction at a word the bus fetched (compressed ones carry
/// their 16 bits in the low half).
pub fn decode(word: u32) -> Option<Decoded> {
    if word & 0b11 != 0b11 {
        return decode_c(word & 0xffff);
    }
    let op = word & 0x7f;
    let rd = ((word >> 7) & 0x1f) as u8;
    let rs1 = ((word >> 15) & 0x1f) as u8;
    let rs2 = ((word >> 20) & 0x1f) as u8;
    let f3 = (word >> 12) & 7;
    let f7 = word >> 25;
    let d = |inst, class| Some(Decoded { inst, width: 4, class });
    match op {
        0x37 => d(
            Inst::Lui {
                rd,
                imm: (word & 0xffff_f000) as i32,
            },
            InstClass::Lui,
        ),
        0x17 => d(
            Inst::Auipc {
                rd,
                imm: (word & 0xffff_f000) as i32,
            },
            InstClass::Auipc,
        ),
        0x6f => d(
            Inst::Jal { rd, imm: imm_j(word) },
            if rd != 0 {
                InstClass::JalCall
            } else {
                InstClass::JalTail
            },
        ),
        0x67 => {
            if f3 != 0 {
                return None;
            }
            let imm = imm_i(word);
            d(Inst::Jalr { rd, rs1, imm }, jalr_class(rd, rs1, imm))
        }
        0x63 => {
            let cond = match f3 {
                0 => Cond::Eq,
                1 => Cond::Ne,
                4 => Cond::Lt,
                5 => Cond::Ge,
                6 => Cond::Ltu,
                7 => Cond::Geu,
                _ => return None,
            };
            d(
                Inst::Branch {
                    cond,
                    rs1,
                    rs2,
                    imm: imm_b(word),
                },
                InstClass::BranchTaken,
            )
        }
        0x03 => {
            let kind = match f3 {
                0 => LoadKind::B,
                1 => LoadKind::H,
                2 => LoadKind::W,
                4 => LoadKind::Bu,
                5 => LoadKind::Hu,
                _ => return None,
            };
            d(
                Inst::Load {
                    kind,
                    rd,
                    rs1,
                    imm: imm_i(word),
                },
                InstClass::Load,
            )
        }
        0x23 => {
            let kind = match f3 {
                0 => StoreKind::B,
                1 => StoreKind::H,
                2 => StoreKind::W,
                _ => return None,
            };
            d(
                Inst::Store {
                    kind,
                    rs1,
                    rs2,
                    imm: imm_s(word),
                },
                InstClass::Store,
            )
        }
        0x13 => {
            let imm = imm_i(word);
            let op = match f3 {
                0 => OpI::Addi,
                2 => OpI::Slti,
                3 => OpI::Sltiu,
                4 => OpI::Xori,
                6 => OpI::Ori,
                7 => OpI::Andi,
                1 => {
                    if f7 != 0 {
                        return None;
                    }
                    OpI::Slli
                }
                5 => match f7 {
                    0 => OpI::Srli,
                    0x20 => OpI::Srai,
                    _ => return None,
                },
                _ => return None,
            };
            let imm = match op {
                OpI::Slli | OpI::Srli | OpI::Srai => imm & 0x1f,
                _ => imm,
            };
            d(Inst::OpImm { op, rd, rs1, imm }, InstClass::Alu)
        }
        0x33 => {
            let (op, class) = match (f7, f3) {
                (0, 0) => (Op::Add, InstClass::Alu),
                (0, 1) => (Op::Sll, InstClass::Alu),
                (0, 2) => (Op::Slt, InstClass::Alu),
                (0, 3) => (Op::Sltu, InstClass::Alu),
                (0, 4) => (Op::Xor, InstClass::Alu),
                (0, 5) => (Op::Srl, InstClass::Alu),
                (0, 6) => (Op::Or, InstClass::Alu),
                (0, 7) => (Op::And, InstClass::Alu),
                (0x20, 0) => (Op::Sub, InstClass::Alu),
                (0x20, 5) => (Op::Sra, InstClass::Alu),
                (1, 0) => (Op::Mul, InstClass::Mul),
                (1, 1) => (Op::Mulh, InstClass::Mul),
                (1, 2) => (Op::Mulhsu, InstClass::Mul),
                (1, 3) => (Op::Mulhu, InstClass::Mul),
                (1, 4) => (Op::Div, InstClass::DivRem),
                (1, 5) => (Op::Divu, InstClass::DivRem),
                (1, 6) => (Op::Rem, InstClass::DivRem),
                (1, 7) => (Op::Remu, InstClass::DivRem),
                _ => return None,
            };
            d(Inst::Op { op, rd, rs1, rs2 }, class)
        }
        // MISC-MEM: a plain `fence` (funct3 0) is charged and otherwise
        // ignored by the executor; `fence.i` (funct3 1) flushes the block
        // cache and is a region boundary.
        0x0f if f3 == 0 => d(Inst::Fence, InstClass::Fence),
        _ => None,
    }
}

fn creg(x: u32) -> u8 {
    8 + (x & 7) as u8
}

fn decode_c(i: u32) -> Option<Decoded> {
    let q = i & 3;
    let f3 = (i >> 13) & 7;
    let d = |inst, class| Some(Decoded { inst, width: 2, class });
    let imm6 = || sext(((i >> 7) & 0x20) | ((i >> 2) & 0x1f), 6);
    let uimm6 = || (((i >> 7) & 0x20) | ((i >> 2) & 0x1f)) as i32;
    match (q, f3) {
        (0, 0) => {
            let nzuimm = ((i >> 7) & 0x30) | ((i >> 1) & 0x3c0) | ((i >> 4) & 0x4) | ((i >> 2) & 0x8);
            if nzuimm == 0 {
                return None;
            }
            d(
                Inst::OpImm {
                    op: OpI::Addi,
                    rd: creg(i >> 2),
                    rs1: 2,
                    imm: nzuimm as i32,
                },
                InstClass::Alu,
            )
        }
        (0, 2) | (0, 6) => {
            let uimm = (((i >> 7) & 0x38) | ((i >> 4) & 0x4) | ((i << 1) & 0x40)) as i32;
            if f3 == 2 {
                d(
                    Inst::Load {
                        kind: LoadKind::W,
                        rd: creg(i >> 2),
                        rs1: creg(i >> 7),
                        imm: uimm,
                    },
                    InstClass::Load,
                )
            } else {
                d(
                    Inst::Store {
                        kind: StoreKind::W,
                        rs1: creg(i >> 7),
                        rs2: creg(i >> 2),
                        imm: uimm,
                    },
                    InstClass::Store,
                )
            }
        }
        (1, 0) => {
            let rd = ((i >> 7) & 0x1f) as u8;
            if rd == 0 {
                return d(Inst::Nop, InstClass::Alu);
            }
            d(
                Inst::OpImm {
                    op: OpI::Addi,
                    rd,
                    rs1: rd,
                    imm: imm6(),
                },
                InstClass::Alu,
            )
        }
        (1, 1) => d(
            Inst::Jal {
                rd: 1,
                imm: cj_off(i),
            },
            InstClass::JalCall,
        ),
        (1, 2) => {
            let rd = ((i >> 7) & 0x1f) as u8;
            if rd == 0 {
                return None;
            }
            d(
                Inst::OpImm {
                    op: OpI::Addi,
                    rd,
                    rs1: 0,
                    imm: imm6(),
                },
                InstClass::Alu,
            )
        }
        (1, 3) => {
            let rd = ((i >> 7) & 0x1f) as u8;
            if rd == 2 {
                let nzimm = sext(
                    ((i >> 3) & 0x200)
                        | ((i >> 2) & 0x10)
                        | ((i << 1) & 0x40)
                        | ((i << 4) & 0x180)
                        | ((i << 3) & 0x20),
                    10,
                );
                if nzimm == 0 {
                    return None;
                }
                d(
                    Inst::OpImm {
                        op: OpI::Addi,
                        rd: 2,
                        rs1: 2,
                        imm: nzimm,
                    },
                    InstClass::Alu,
                )
            } else {
                if rd == 0 {
                    return None;
                }
                let nzimm = imm6();
                if nzimm == 0 {
                    return None;
                }
                d(
                    Inst::Lui {
                        rd,
                        imm: nzimm << 12,
                    },
                    InstClass::Lui,
                )
            }
        }
        (1, 4) => {
            let rd = creg(i >> 7);
            match (i >> 10) & 3 {
                0 | 1 => {
                    let sh = uimm6();
                    if sh >= 32 {
                        return None;
                    }
                    d(
                        Inst::OpImm {
                            op: if (i >> 10) & 3 == 0 {
                                OpI::Srli
                            } else {
                                OpI::Srai
                            },
                            rd,
                            rs1: rd,
                            imm: sh,
                        },
                        InstClass::Alu,
                    )
                }
                2 => d(
                    Inst::OpImm {
                        op: OpI::Andi,
                        rd,
                        rs1: rd,
                        imm: imm6(),
                    },
                    InstClass::Alu,
                ),
                _ => {
                    if (i >> 10) & 0x3f != 0b100011 {
                        return None;
                    }
                    let op = match (i >> 5) & 3 {
                        0 => Op::Sub,
                        1 => Op::Xor,
                        2 => Op::Or,
                        _ => Op::And,
                    };
                    d(
                        Inst::Op {
                            op,
                            rd,
                            rs1: rd,
                            rs2: creg(i >> 2),
                        },
                        InstClass::Alu,
                    )
                }
            }
        }
        (1, 5) => d(
            Inst::Jal {
                rd: 0,
                imm: cj_off(i),
            },
            InstClass::JalTail,
        ),
        (1, 6) | (1, 7) => d(
            Inst::Branch {
                cond: if f3 == 6 { Cond::Eq } else { Cond::Ne },
                rs1: creg(i >> 7),
                rs2: 0,
                imm: cb_off(i),
            },
            InstClass::BranchTaken,
        ),
        (2, 0) => {
            let rd = ((i >> 7) & 0x1f) as u8;
            let sh = uimm6();
            if rd == 0 || sh >= 32 {
                return None;
            }
            d(
                Inst::OpImm {
                    op: OpI::Slli,
                    rd,
                    rs1: rd,
                    imm: sh,
                },
                InstClass::Alu,
            )
        }
        (2, 2) => {
            let rd = ((i >> 7) & 0x1f) as u8;
            if rd == 0 {
                return None;
            }
            let uimm = (((i >> 7) & 0x20) | ((i >> 2) & 0x1c) | ((i << 4) & 0xc0)) as i32;
            d(
                Inst::Load {
                    kind: LoadKind::W,
                    rd,
                    rs1: 2,
                    imm: uimm,
                },
                InstClass::Load,
            )
        }
        (2, 4) => {
            let rd = ((i >> 7) & 0x1f) as u8;
            let rs2 = ((i >> 2) & 0x1f) as u8;
            let f4 = (i >> 12) & 0xf;
            match (f4, rs2) {
                (8, 0) if rd != 0 => d(
                    Inst::Jalr {
                        rd: 0,
                        rs1: rd,
                        imm: 0,
                    },
                    if rd == 1 {
                        InstClass::JalrReturn
                    } else {
                        InstClass::JalrIndirect
                    },
                ),
                (8, _) if rd != 0 => d(
                    Inst::Op {
                        op: Op::Add,
                        rd,
                        rs1: 0,
                        rs2,
                    },
                    InstClass::Alu,
                ),
                (9, 0) if rd != 0 => d(
                    Inst::Jalr {
                        rd: 1,
                        rs1: rd,
                        imm: 0,
                    },
                    InstClass::JalrCall,
                ),
                (9, _) if rd != 0 => d(
                    Inst::Op {
                        op: Op::Add,
                        rd,
                        rs1: rd,
                        rs2,
                    },
                    InstClass::Alu,
                ),
                _ => None,
            }
        }
        (2, 6) => {
            let uimm = (((i >> 7) & 0x3c) | ((i >> 1) & 0xc0)) as i32;
            d(
                Inst::Store {
                    kind: StoreKind::W,
                    rs1: 2,
                    rs2: ((i >> 2) & 0x1f) as u8,
                    imm: uimm,
                },
                InstClass::Store,
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immediates_match_the_spec_examples() {
        // `bne a0, a3, -0x1c` at 0x420804e8: fed51de3
        let d = decode(0xfed51de3).unwrap();
        match d.inst {
            Inst::Branch { cond: Cond::Ne, rs1: 10, rs2: 13, imm } => assert_eq!(imm, -6),
            other => panic!("{other:?}"),
        }
        // `jalr 0x442(ra)`: 442080e7
        match decode(0x442080e7).unwrap().inst {
            Inst::Jalr { rd: 1, rs1: 1, imm } => assert_eq!(imm, 0x442),
            other => panic!("{other:?}"),
        }
        // `sw a1, 0x10(a2)` compressed: ca0c
        match decode(0xca0c).unwrap().inst {
            Inst::Store { kind: StoreKind::W, rs1: 12, rs2: 11, imm } => assert_eq!(imm, 0x10),
            other => panic!("{other:?}"),
        }
        // `lui a4, 0xc` compressed: 6731
        match decode(0x6731).unwrap().inst {
            Inst::Lui { rd: 14, imm } => assert_eq!(imm, 0xc000),
            other => panic!("{other:?}"),
        }
        // `addi s7, a4, 0x351`: 35170b93
        match decode(0x35170b93).unwrap().inst {
            Inst::OpImm { op: OpI::Addi, rd: 23, rs1: 14, imm } => assert_eq!(imm, 0x351),
            other => panic!("{other:?}"),
        }
        // `fence r, rw`: 0230000f is a plain fence
        assert!(matches!(decode(0x0230000f).unwrap().inst, Inst::Fence));
        // fence.i is refused
        assert!(decode(0x0000100f).is_none());
        // `bnez a1, +0xb4` compressed e9d5 at 0x42084c0a -> 0x42084cbe
        match decode(0xe9d5).unwrap().inst {
            Inst::Branch { cond: Cond::Ne, rs1: 11, rs2: 0, imm } => assert_eq!(imm, 0xb4),
            other => panic!("{other:?}"),
        }
        // `j 0x4208053c` from 0x42080516: a01d
        match decode(0xa01d).unwrap().inst {
            Inst::Jal { rd: 0, imm } => assert_eq!(imm, 0x26),
            other => panic!("{other:?}"),
        }
    }
}
