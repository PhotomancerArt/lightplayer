//! RV32IMC decode into the translator's tiny IR.
//!
//! # The contract
//!
//! This decoder mirrors `lp-riscv-emu`'s executors exactly where it matters for
//! identity: the **cost class** each instruction charges, the instruction's
//! **width**, and the operand semantics. Where the two must agree they are held
//! to it by `tests/decoder_agreement.rs`, which decodes both render reference
//! images and the whole compressed encoding space with both.
//!
//! # Refusing is always exact (JD7)
//!
//! [`decode`] returns [`None`] for anything it does not positively recognise,
//! and `None` is the **universal escape**: the block ends before the word and
//! the interpreter runs it. Every unknown goes out the same door — an
//! unsupported extension, a block swept onto data-in-text, a guest that wrote
//! something new. Refusing costs coverage; guessing costs correctness, and
//! there is no third option that keeps byte-identity. P3's `BlockEnd`
//! carries this `None` as its `Undecodable` variant.
//!
//! Consequently this decoder is deliberately **stricter** than the interpreter,
//! and the two are not symmetric:
//!
//! - The interpreter's R-type and I-type executors implement Zba/Zbb/Zbs
//!   (`rol`, `clz`, `bseti`, `sh2add`, …). The ESP32-C6 has none of them, so
//!   this decoder does not carry them; if one ever appears in an image, the
//!   block ends and the interpreter — which does implement them — runs it.
//! - The interpreter tolerates several encodings the base ISA reserves: a
//!   `srli`/`srai` whose `funct7` is neither `0x00` nor `0x20`, and the RVC
//!   shifts with `shamt[5]` set, which it silently masks to five bits. RV32
//!   reserves those. This decoder refuses them.
//!
//! Both directions of that asymmetry are enumerated and asserted in the
//! agreement test, so neither can drift silently.

use lp_emu_core::InstClass;

/// The register-register ALU and M-extension operations the translator emits.
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

/// The register-immediate ALU operations.
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

/// A conditional-branch predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    Lt,
    Ge,
    Ltu,
    Geu,
}

/// The width and signedness of a load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadKind {
    B,
    H,
    W,
    Bu,
    Hu,
}

/// The width of a store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreKind {
    B,
    H,
    W,
}

/// One decoded instruction.
///
/// Compressed encodings are expanded here rather than kept as their own
/// variants: `c.addi4spn` is an `OpImm` against `x2`, `c.jr` is a `Jalr` with
/// `rd = x0`, and so on. The translator then has one shape per operation
/// instead of two, and the width — the thing the expansion would otherwise
/// lose — rides on [`Decoded::width`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inst {
    /// `imm` is the final 32-bit value (already shifted left by 12).
    Lui {
        rd: u8,
        imm: i32,
    },
    Auipc {
        rd: u8,
        imm: i32,
    },
    Jal {
        rd: u8,
        imm: i32,
    },
    Jalr {
        rd: u8,
        rs1: u8,
        imm: i32,
    },
    Branch {
        cond: Cond,
        rs1: u8,
        rs2: u8,
        imm: i32,
    },
    Load {
        kind: LoadKind,
        rd: u8,
        rs1: u8,
        imm: i32,
    },
    Store {
        kind: StoreKind,
        rs1: u8,
        rs2: u8,
        imm: i32,
    },
    OpImm {
        op: OpI,
        rd: u8,
        rs1: u8,
        imm: i32,
    },
    Op {
        op: Op,
        rd: u8,
        rs1: u8,
        rs2: u8,
    },
    /// A plain `fence` — the executor charges [`InstClass::Fence`] and does
    /// nothing else. `fence.i` is **not** this: it publishes guest-written
    /// code, so it is refused and ends the block.
    Fence,
    /// `c.nop`, and the `c.addi x0, imm` hints the executor treats as it.
    Nop,
}

/// An instruction, its width, and what it charges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub inst: Inst,
    /// 2 for a compressed encoding, 4 for a full one.
    pub width: u8,
    /// The class charged.
    ///
    /// For a branch this is [`InstClass::BranchTaken`], because taken versus
    /// not-taken is a property of the operands at run time, not of the
    /// encoding: the translator charges [`InstClass::BranchNotTaken`] itself on
    /// the fall-through path. `lp-riscv-emu`'s block cache makes the same
    /// choice for the same reason.
    pub class: InstClass,
}

impl Decoded {
    /// Does this instruction decide the next `pc` itself?
    ///
    /// A block ends after one of these. Everything else — stores included —
    /// stays inside the block; the M5 measurement behind that is in
    /// `lp_riscv_emu::mach::block`'s module docs.
    #[must_use]
    pub fn is_control(&self) -> bool {
        matches!(
            self.inst,
            Inst::Jal { .. } | Inst::Jalr { .. } | Inst::Branch { .. }
        )
    }
}

/// Sign-extend the low `bits` of `v`.
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

/// The CJ-format offset of `c.j` / `c.jal`.
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

/// The CB-format offset of `c.beqz` / `c.bnez`.
fn cb_off(i: u32) -> i32 {
    let imm = ((i >> 12) & 1) << 8
        | ((i >> 5) & 3) << 6
        | ((i >> 2) & 1) << 5
        | ((i >> 10) & 3) << 3
        | ((i >> 3) & 3) << 1;
    sext(imm, 9)
}

/// `jalr`'s three cost classes, which the C6 model prices differently: a call
/// writes a link register, a return is the `x1`-relative jump with no
/// displacement, and everything else is an indirect jump.
fn jalr_class(rd: u8, rs1: u8, imm: i32) -> InstClass {
    if rd != 0 {
        InstClass::JalrCall
    } else if rs1 == 1 && imm == 0 {
        InstClass::JalrReturn
    } else {
        InstClass::JalrIndirect
    }
}

/// The compressed-encoding register number: `rd'`/`rs'` name `x8`..`x15`.
fn creg(x: u32) -> u8 {
    8 + (x & 7) as u8
}

/// Is this word a `MISC-MEM` encoding — the opcode `fence.i` lives in?
///
/// [`decode`] recognises exactly one member of the class, the plain `fence`
/// with `funct3 == 0`, so every other `MISC-MEM` word ends its block. One of
/// them is `fence.i` (`0x0000_100f`), the guest publishing code it wrote, and
/// the machine has to see that one **out in its own loop** rather than through
/// the escape hatch inside a stay — see [`crate::blocks::Unknown::Leave`],
/// which is what this predicate selects.
///
/// It names the whole opcode rather than the one encoding on purpose. Leaving
/// is always exact and never a guess (JD7); the class is worth one exit a run
/// on `render-basic`; and a reader checking this against the ISA has to check
/// seven bits rather than reason about which reserved `MISC-MEM` words a
/// future `fence` form might use.
#[must_use]
pub const fn is_misc_mem(word: u32) -> bool {
    word & 0x7f == 0x0f
}

/// Decode the instruction at a word the bus fetched.
///
/// A compressed instruction carries its 16 bits in the low half of `word`; the
/// high half is whatever followed it in memory and is ignored. [`None`] is the
/// universal escape — see the module docs.
#[must_use]
pub fn decode(word: u32) -> Option<Decoded> {
    if word & 0b11 != 0b11 {
        return decode_compressed(word & 0xffff);
    }
    let op = word & 0x7f;
    let rd = ((word >> 7) & 0x1f) as u8;
    let rs1 = ((word >> 15) & 0x1f) as u8;
    let rs2 = ((word >> 20) & 0x1f) as u8;
    let f3 = (word >> 12) & 7;
    let f7 = word >> 25;
    let d = |inst, class| {
        Some(Decoded {
            inst,
            width: 4,
            class,
        })
    };
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
            Inst::Jal {
                rd,
                imm: imm_j(word),
            },
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
                // funct3 2 and 3 are reserved.
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
                // 3 is `ld`, 6 is `lwu`, 7 is reserved: all RV64.
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
                // 3 is `sd` (RV64); 4..7 are reserved.
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
                // The shifts carry `shamt` in the low five bits of the
                // immediate, and RV32 requires the remaining seven to be
                // exactly 0 (`slli`, `srli`) or 0x20 (`srai`). Anything else
                // is a reserved encoding or a Zb* operation; both are refused.
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
            // Base ALU (funct7 0 and 0x20) and the M extension (funct7 1).
            // Every other funct7 here is Zba/Zbb/Zbs, which this chip does not
            // have.
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
        // MISC-MEM. A plain `fence` (funct3 0) is charged and otherwise
        // ignored by the executor. `fence.i` (funct3 1) is the guest
        // publishing code it wrote, which invalidates translations — it must
        // reach the machine, so it ends the block.
        0x0f if f3 == 0 => d(Inst::Fence, InstClass::Fence),
        _ => None,
    }
}

/// RVC (the C extension, v2.0 §16). Quadrant and funct3 decide.
fn decode_compressed(i: u32) -> Option<Decoded> {
    let q = i & 3;
    let f3 = (i >> 13) & 7;
    let d = |inst, class| {
        Some(Decoded {
            inst,
            width: 2,
            class,
        })
    };
    // The 6-bit immediate shared by `c.addi`, `c.li`, `c.andi` and `c.lui`,
    // signed and unsigned. Unsigned is the shift amount, and its bit 5 is
    // reserved on RV32.
    let imm6 = || sext(((i >> 7) & 0x20) | ((i >> 2) & 0x1f), 6);
    let uimm6 = || (((i >> 7) & 0x20) | ((i >> 2) & 0x1f)) as i32;
    match (q, f3) {
        // c.addi4spn. `nzuimm == 0` is the reserved all-zero half of quadrant
        // 0, which is also the illegal instruction the RVC spec defines.
        (0, 0) => {
            let nzuimm =
                ((i >> 7) & 0x30) | ((i >> 1) & 0x3c0) | ((i >> 4) & 0x4) | ((i >> 2) & 0x8);
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
        // c.lw / c.sw. Quadrant 0's other funct3 values are the FP and RV64
        // loads and stores, and 100 is reserved.
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
        // c.addi, and c.nop when rd is x0.
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
        // c.jal — RV32 only, and the reason this decoder is not RV64-shaped.
        (1, 1) => d(
            Inst::Jal {
                rd: 1,
                imm: cj_off(i),
            },
            InstClass::JalCall,
        ),
        // c.li. `rd = x0` is a hint.
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
        // c.addi16sp when rd is x2, otherwise c.lui. A zero immediate is
        // reserved in both, and `c.lui x0` is a hint.
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
        // The misc-ALU group: c.srli, c.srai, c.andi and the four
        // register-register forms.
        (1, 4) => {
            let rd = creg(i >> 7);
            match (i >> 10) & 3 {
                0 | 1 => {
                    // `shamt[5]` — bit 12 — is reserved on RV32. The
                    // interpreter masks it away; refusing is exact.
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
                    // funct6 must be 100011: bit 12 set selects c.subw/c.addw,
                    // which are RV64.
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
        // c.j.
        (1, 5) => d(
            Inst::Jal {
                rd: 0,
                imm: cj_off(i),
            },
            InstClass::JalTail,
        ),
        // c.beqz / c.bnez.
        (1, 6) | (1, 7) => d(
            Inst::Branch {
                cond: if f3 == 6 { Cond::Eq } else { Cond::Ne },
                rs1: creg(i >> 7),
                rs2: 0,
                imm: cb_off(i),
            },
            InstClass::BranchTaken,
        ),
        // c.slli. `rd = x0` is a hint and `shamt[5]` is reserved on RV32.
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
        // c.lwsp. `rd = x0` is reserved.
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
        // Four instructions in one funct3, split by funct4 and rs2:
        // c.jr, c.mv, c.jalr, c.add — plus c.ebreak, which is the hart's own
        // and is refused.
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
        // c.swsp.
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

    /// Real encodings lifted from the pinned render images' disassembly, with
    /// the address they were seen at. An immediate decoder is all bit
    /// shuffling and every field is a chance to be off by one; these are the
    /// cases that caught it.
    #[test]
    fn immediates_match_encodings_seen_in_the_pinned_images() {
        // `bne a0, a3, -0x1c` at 0x420804e8: fed51de3
        match decode(0xfed51de3).unwrap().inst {
            Inst::Branch {
                cond: Cond::Ne,
                rs1: 10,
                rs2: 13,
                imm,
            } => assert_eq!(imm, -6),
            other => panic!("{other:?}"),
        }
        // `jalr 0x442(ra)`: 442080e7
        match decode(0x442080e7).unwrap().inst {
            Inst::Jalr { rd: 1, rs1: 1, imm } => assert_eq!(imm, 0x442),
            other => panic!("{other:?}"),
        }
        // `sw a1, 0x10(a2)` compressed: ca0c
        match decode(0xca0c).unwrap().inst {
            Inst::Store {
                kind: StoreKind::W,
                rs1: 12,
                rs2: 11,
                imm,
            } => assert_eq!(imm, 0x10),
            other => panic!("{other:?}"),
        }
        // `lui a4, 0xc` compressed: 6731
        match decode(0x6731).unwrap().inst {
            Inst::Lui { rd: 14, imm } => assert_eq!(imm, 0xc000),
            other => panic!("{other:?}"),
        }
        // `addi s7, a4, 0x351`: 35170b93
        match decode(0x35170b93).unwrap().inst {
            Inst::OpImm {
                op: OpI::Addi,
                rd: 23,
                rs1: 14,
                imm,
            } => assert_eq!(imm, 0x351),
            other => panic!("{other:?}"),
        }
        // `bnez a1, +0xb4` compressed e9d5 at 0x42084c0a -> 0x42084cbe
        match decode(0xe9d5).unwrap().inst {
            Inst::Branch {
                cond: Cond::Ne,
                rs1: 11,
                rs2: 0,
                imm,
            } => assert_eq!(imm, 0xb4),
            other => panic!("{other:?}"),
        }
        // `j 0x4208053c` from 0x42080516: a01d
        match decode(0xa01d).unwrap().inst {
            Inst::Jal { rd: 0, imm } => assert_eq!(imm, 0x26),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_plain_fence_decodes_and_fence_i_ends_the_block() {
        // `fence r, rw`: 0230000f
        assert!(matches!(decode(0x0230000f).unwrap().inst, Inst::Fence));
        // `fence.i` publishes guest-written code and must reach the machine.
        assert!(decode(0x0000100f).is_none());
    }

    /// The classes the C6 cycle model prices apart. Getting one of these wrong
    /// is not a crash, it is a cycle count that drifts by a few parts per
    /// million and fails an oracle three images later.
    #[test]
    fn the_control_transfers_carry_their_own_cost_classes() {
        let class = |w| decode(w).unwrap().class;
        // jal ra, +0 / jal x0, +0
        assert_eq!(class(0x0000_00ef), InstClass::JalCall);
        assert_eq!(class(0x0000_006f), InstClass::JalTail);
        // jalr ra, 0(a0) / jalr x0, 0(ra) / jalr x0, 4(ra)
        assert_eq!(class(0x0005_00e7), InstClass::JalrCall);
        assert_eq!(class(0x0000_8067), InstClass::JalrReturn);
        assert_eq!(class(0x0040_8067), InstClass::JalrIndirect);
        // c.jr ra (`ret`) / c.jr a0 / c.jalr ra
        assert_eq!(class(0x8082), InstClass::JalrReturn);
        assert_eq!(class(0x8502), InstClass::JalrIndirect);
        assert_eq!(class(0x9082), InstClass::JalrCall);
        // A branch always reports the taken class; see `Decoded::class`.
        assert_eq!(class(0x0000_0063), InstClass::BranchTaken);
        assert_eq!(class(0xc101), InstClass::BranchTaken);
    }

    /// The refusals that are policy rather than ignorance. Each one is an
    /// encoding the interpreter accepts, so the block ends and the
    /// interpreter runs it — the `Undecodable` escape doing its job.
    #[test]
    fn the_reserved_and_out_of_scope_encodings_are_refused() {
        assert!(decode(0x0000_0073).is_none(), "ecall");
        assert!(decode(0x3400_2573).is_none(), "csrrs a0, mscratch");
        assert!(decode(0x1000_212f).is_none(), "lr.w");
        assert!(decode(0x0031_00d3).is_none(), "fadd.s");
        assert!(decode(0x6055_1513).is_none(), "clz a0, a0 (Zbb)");
        assert!(decode(0x60a5_9533).is_none(), "rol a0, a1, a0 (Zbb)");
        assert!(decode(0x9002).is_none(), "c.ebreak");
        assert!(decode(0x0000).is_none(), "the illegal all-zero RVC word");
        // RV32C reserves shamt[5]; the interpreter masks it, we refuse.
        assert!(decode(0x1042).is_none(), "c.slli a0, 32");
        assert!(decode(0x9101).is_none(), "c.srli s0, 32");
        // OP-IMM shifts with a funct7 the base ISA does not define.
        assert!(decode(0x0215_1513).is_none(), "slli with funct7 = 1");
        assert!(decode(0x0215_5513).is_none(), "srli with funct7 = 1");
    }

    #[test]
    fn a_compressed_encoding_is_two_bytes_wide_and_a_full_one_is_four() {
        assert_eq!(decode(0x0505).unwrap().width, 2, "c.addi a0, 1");
        assert_eq!(decode(0x0000_0013).unwrap().width, 4, "nop");
    }
}
