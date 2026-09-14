//! The integer core: the ALU, the immediates, the shifts and `SAR`, the
//! multiplies and divides, the barriers and `nop`.
//!
//! Every arm reproduces `lp-xt-emu/src/executor/{arith,imm,misc}.rs` — the
//! executors the interpreter runs — and nothing else; where wasm and the
//! executor differ on an edge (`INT_MIN / -1`, a zero divisor, a shift
//! amount over 31) the arm says which and does what the executor does.

use lp_emu_core::InstClass;
use lp_xt_inst::{AluRrr, AluRs, AluRt, Inst, NullaryNarrowOp, NullaryOp, ShiftSetOp};
use wasm_encoder::{BlockType, Instruction as I, ValType};

use super::{Emitter, L_SAR, L_T};
use crate::decode::Decoded;

impl Emitter<'_> {
    /// One integer-core instruction. Straight-line: the caller charges its
    /// cost and applies the loop-back.
    pub(crate) fn alu(&mut self, k: usize, pc: u32, d: &Decoded) {
        match d.inst {
            Inst::Rrr(op, rd, rs, rt) => self.rrr(k, pc, d, op, rd.num(), rs.num(), rt.num()),
            Inst::Rt(op, rd, rt) => {
                let (rd, rt) = (rd.num(), rt.num());
                match op {
                    AluRt::Neg => {
                        self.i(I::I32Const(0));
                        self.get(rt);
                        self.i(I::I32Sub);
                    }
                    // `unsigned_abs`: `t < 0 ? 0 - t : t`.
                    AluRt::Abs => {
                        self.i(I::I32Const(0));
                        self.get(rt);
                        self.i(I::I32Sub);
                        self.get(rt);
                        self.get(rt);
                        self.i(I::I32Const(0));
                        self.i(I::I32LtS);
                        self.i(I::Select);
                    }
                    // Both take `sar & 31`, which is what wasm's shift does.
                    AluRt::Sra => {
                        self.get(rt);
                        self.i(I::LocalGet(L_SAR));
                        self.i(I::I32ShrS);
                    }
                    AluRt::Srl => {
                        self.get(rt);
                        self.i(I::LocalGet(L_SAR));
                        self.i(I::I32ShrU);
                    }
                    AluRt::Nsau => {
                        self.get(rt);
                        self.i(I::I32Clz);
                    }
                    // Leading redundant sign bits: `clz(t < 0 ? !t : t)`,
                    // then `saturating_sub(1).min(31)` — with `clz <= 32`
                    // that is `c - (c != 0)`.
                    AluRt::Nsa => {
                        self.get(rt);
                        self.get(rt);
                        self.i(I::I32Const(31));
                        self.i(I::I32ShrS);
                        self.i(I::I32Xor);
                        self.i(I::I32Clz);
                        self.i(I::LocalTee(L_T));
                        self.i(I::LocalGet(L_T));
                        self.i(I::I32Const(0));
                        self.i(I::I32Ne);
                        self.i(I::I32Sub);
                    }
                }
                self.set(rd);
            }
            Inst::Rs(op, rd, rs) => {
                let (rd, rs) = (rd.num(), rs.num());
                match op {
                    // `s << ((32 - sar) & 31)`; wasm masks the count.
                    AluRs::Sll => {
                        self.get(rs);
                        self.i(I::I32Const(32));
                        self.i(I::LocalGet(L_SAR));
                        self.i(I::I32Sub);
                        self.i(I::I32Shl);
                    }
                    AluRs::Movsp => unreachable!("movsp is a refusal (`refusal_of`)"),
                }
                self.set(rd);
            }
            Inst::ShiftSet(op, rs) => {
                let rs = rs.num();
                match op {
                    ShiftSetOp::Ssr => {
                        self.get(rs);
                        self.i(I::I32Const(31));
                        self.i(I::I32And);
                    }
                    ShiftSetOp::Ssl => {
                        self.i(I::I32Const(32));
                        self.get(rs);
                        self.i(I::I32Const(31));
                        self.i(I::I32And);
                        self.i(I::I32Sub);
                    }
                    ShiftSetOp::Ssa8l => {
                        self.get(rs);
                        self.i(I::I32Const(3));
                        self.i(I::I32And);
                        self.i(I::I32Const(3));
                        self.i(I::I32Shl);
                    }
                    ShiftSetOp::Ssa8b => {
                        self.i(I::I32Const(32));
                        self.get(rs);
                        self.i(I::I32Const(3));
                        self.i(I::I32And);
                        self.i(I::I32Const(3));
                        self.i(I::I32Shl);
                        self.i(I::I32Sub);
                    }
                }
                self.i(I::LocalSet(L_SAR));
            }
            Inst::Ssai(imm) => {
                self.i(I::I32Const(i32::from(imm)));
                self.i(I::LocalSet(L_SAR));
            }
            Inst::Slli(rd, rs, sa) => {
                self.get(rs.num());
                self.i(I::I32Const(i32::from(sa & 31)));
                self.i(I::I32Shl);
                self.set(rd.num());
            }
            Inst::Srli(rd, rt, sa) => {
                self.get(rt.num());
                self.i(I::I32Const(i32::from(sa & 31)));
                self.i(I::I32ShrU);
                self.set(rd.num());
            }
            Inst::Srai(rd, rt, sa) => {
                self.get(rt.num());
                self.i(I::I32Const(i32::from(sa & 31)));
                self.i(I::I32ShrS);
                self.set(rd.num());
            }
            // `(t >> shift) & ((1 << mask) - 1)`, mask in 1..=16.
            Inst::Extui(rd, rt, shift, mask) => {
                self.get(rt.num());
                self.i(I::I32Const(i32::from(shift & 31)));
                self.i(I::I32ShrU);
                self.i(I::I32Const(((1u32 << mask) - 1) as i32));
                self.i(I::I32And);
                self.set(rd.num());
            }
            // Replicate bit `bit` upward: `(s << (31 - bit)) >>s (31 - bit)`.
            Inst::Sext(rd, rs, bit) => {
                let sh = 31 - i32::from(bit);
                self.get(rs.num());
                self.i(I::I32Const(sh));
                self.i(I::I32Shl);
                self.i(I::I32Const(sh));
                self.i(I::I32ShrS);
                self.set(rd.num());
            }
            Inst::MovN(rt, rs) => {
                self.get(rs.num());
                self.set(rt.num());
            }
            Inst::AddN(rd, rs, rt) => {
                self.get(rs.num());
                self.get(rt.num());
                self.i(I::I32Add);
                self.set(rd.num());
            }
            Inst::AddiN(rd, rs, imm) => {
                self.get(rs.num());
                self.i(I::I32Const(imm));
                self.i(I::I32Add);
                self.set(rd.num());
            }
            Inst::Addi(rt, rs, imm) | Inst::Addmi(rt, rs, imm) => {
                self.get(rs.num());
                self.i(I::I32Const(imm));
                self.i(I::I32Add);
                self.set(rt.num());
            }
            Inst::Movi(rt, imm) | Inst::MoviN(rt, imm) => {
                self.i(I::I32Const(imm));
                self.set(rt.num());
            }
            // No architectural effect. `memw`, `extw` and the syncs are
            // System class, and the interpreter polls (c) after a
            // System-class instruction: with a yield pending from an
            // earlier load, that poll is this instruction's.
            Inst::Nullary(NullaryOp::Nop) | Inst::NullaryN(NullaryNarrowOp::NopN) => {}
            Inst::Nullary(
                NullaryOp::Memw
                | NullaryOp::Extw
                | NullaryOp::Rsync
                | NullaryOp::Esync
                | NullaryOp::Dsync,
            ) => {
                let next = pc.wrapping_add(u32::from(d.width));
                let cost = self.cost(InstClass::System);
                self.loop_commit();
                self.pending_poll(k, next, cost);
            }
            other => unreachable!("the integer core has no arm for {other:?}"),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "one instruction's three operands and the block context"
    )]
    fn rrr(&mut self, k: usize, pc: u32, d: &Decoded, op: AluRrr, rd: u8, rs: u8, rt: u8) {
        match op {
            AluRrr::And | AluRrr::Or | AluRrr::Xor | AluRrr::Add | AluRrr::Sub | AluRrr::Mull => {
                self.get(rs);
                self.get(rt);
                self.i(match op {
                    AluRrr::And => I::I32And,
                    AluRrr::Or => I::I32Or,
                    AluRrr::Xor => I::I32Xor,
                    AluRrr::Add => I::I32Add,
                    AluRrr::Sub => I::I32Sub,
                    _ => I::I32Mul,
                });
            }
            AluRrr::Addx2 | AluRrr::Addx4 | AluRrr::Addx8 => {
                self.get(rs);
                self.i(I::I32Const(match op {
                    AluRrr::Addx2 => 1,
                    AluRrr::Addx4 => 2,
                    _ => 3,
                }));
                self.i(I::I32Shl);
                self.get(rt);
                self.i(I::I32Add);
            }
            AluRrr::Subx2 | AluRrr::Subx4 | AluRrr::Subx8 => {
                self.get(rs);
                self.i(I::I32Const(match op {
                    AluRrr::Subx2 => 1,
                    AluRrr::Subx4 => 2,
                    _ => 3,
                }));
                self.i(I::I32Shl);
                self.get(rt);
                self.i(I::I32Sub);
            }
            // `((s << 32) | t) >> (sar & 63)`; wasm's i64 shift masks by 64.
            AluRrr::Src => {
                self.get(rs);
                self.i(I::I64ExtendI32U);
                self.i(I::I64Const(32));
                self.i(I::I64Shl);
                self.get(rt);
                self.i(I::I64ExtendI32U);
                self.i(I::I64Or);
                self.i(I::LocalGet(L_SAR));
                self.i(I::I64ExtendI32U);
                self.i(I::I64ShrU);
                self.i(I::I32WrapI64);
            }
            AluRrr::Muluh | AluRrr::Mulsh => {
                let ext = if op == AluRrr::Muluh {
                    I::I64ExtendI32U
                } else {
                    I::I64ExtendI32S
                };
                self.get(rs);
                self.i(ext.clone());
                self.get(rt);
                self.i(ext);
                self.i(I::I64Mul);
                self.i(I::I64Const(32));
                self.i(if op == AluRrr::Muluh {
                    I::I64ShrU
                } else {
                    I::I64ShrS
                });
                self.i(I::I32WrapI64);
            }
            AluRrr::Mul16u => {
                self.get(rs);
                self.i(I::I32Const(0xffff));
                self.i(I::I32And);
                self.get(rt);
                self.i(I::I32Const(0xffff));
                self.i(I::I32And);
                self.i(I::I32Mul);
            }
            AluRrr::Mul16s => {
                self.get(rs);
                self.i(I::I32Extend16S);
                self.get(rt);
                self.i(I::I32Extend16S);
                self.i(I::I32Mul);
            }
            // A zero divisor is `IntegerDivideByZero` on the hart: **refuse
            // and escape** — the interpreter runs the instruction and traps
            // with the exact `EPC1`, and the straight-on check leaves. wasm
            // also traps on `INT_MIN / -1`, where the executor's
            // `wrapping_div` gives `INT_MIN` (and `wrapping_rem` gives 0).
            AluRrr::Quou | AluRrr::Quos | AluRrr::Remu | AluRrr::Rems => {
                let next = pc.wrapping_add(u32::from(d.width));
                self.get(rt);
                self.i(I::I32Eqz);
                self.i(I::If(BlockType::Empty));
                self.extra += 1;
                self.escaped_insts += 1;
                self.escape(k, pc, d);
                self.escape_straight_on(k, next);
                // The hart never retires a zero-divisor divide straight on.
                self.i(I::Unreachable);
                self.extra -= 1;
                self.i(I::End);
                match op {
                    AluRrr::Quou => {
                        self.get(rs);
                        self.get(rt);
                        self.i(I::I32DivU);
                    }
                    AluRrr::Remu => {
                        self.get(rs);
                        self.get(rt);
                        self.i(I::I32RemU);
                    }
                    AluRrr::Quos => {
                        self.get(rt);
                        self.i(I::I32Const(-1));
                        self.i(I::I32Eq);
                        self.i(I::If(BlockType::Result(ValType::I32)));
                        self.i(I::I32Const(0));
                        self.get(rs);
                        self.i(I::I32Sub);
                        self.i(I::Else);
                        self.get(rs);
                        self.get(rt);
                        self.i(I::I32DivS);
                        self.i(I::End);
                    }
                    _ => {
                        self.get(rt);
                        self.i(I::I32Const(-1));
                        self.i(I::I32Eq);
                        self.i(I::If(BlockType::Result(ValType::I32)));
                        self.i(I::I32Const(0));
                        self.i(I::Else);
                        self.get(rs);
                        self.get(rt);
                        self.i(I::I32RemS);
                        self.i(I::End);
                    }
                }
            }
            AluRrr::Min | AluRrr::Max | AluRrr::Minu | AluRrr::Maxu => {
                self.get(rs);
                self.get(rt);
                self.get(rs);
                self.get(rt);
                self.i(match op {
                    AluRrr::Min => I::I32LtS,
                    AluRrr::Max => I::I32GtS,
                    AluRrr::Minu => I::I32LtU,
                    _ => I::I32GtU,
                });
                self.i(I::Select);
            }
            // `rd = cond(t) ? s : rd`.
            AluRrr::Moveqz | AluRrr::Movnez | AluRrr::Movltz | AluRrr::Movgez => {
                self.get(rs);
                self.get(rd);
                self.get(rt);
                match op {
                    AluRrr::Moveqz => self.i(I::I32Eqz),
                    AluRrr::Movnez => {
                        self.i(I::I32Const(0));
                        self.i(I::I32Ne);
                    }
                    AluRrr::Movltz => {
                        self.i(I::I32Const(0));
                        self.i(I::I32LtS);
                    }
                    _ => {
                        self.i(I::I32Const(0));
                        self.i(I::I32GeS);
                    }
                }
                self.i(I::Select);
            }
        }
        self.set(rd);
    }
}
