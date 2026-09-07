//! Arithmetic, logical, shift, and register-move executors.

use lp_xt_inst::{AluRrr, AluRs, AluRt, Inst, ShiftSetOp};

use crate::emu::{Emulator, Flow};
use crate::error::{EXC_INTEGER_DIVIDE_BY_ZERO, Trap, TrapKind};
use crate::trace::Tracer;

impl Emulator {
    pub(super) fn exec_arith(
        &mut self,
        inst: &Inst,
        tracer: &mut dyn Tracer,
    ) -> Result<Flow, Trap> {
        match *inst {
            Inst::Rrr(op, rd, rs, rt) => {
                let s = self.rreg(rs.num());
                let t = self.rreg(rt.num());
                // Divide/remainder by zero raises IntegerDivideByZero on the
                // hardware (EXCCAUSE 6) — model it as the same trap, not a
                // value (dual-run parity pinned by the P3 corpus). The run
                // loop fills in the faulting pc.
                if t == 0
                    && matches!(
                        op,
                        AluRrr::Quou | AluRrr::Quos | AluRrr::Remu | AluRrr::Rems
                    )
                {
                    return Err(Trap {
                        kind: TrapKind::Exception,
                        cause: EXC_INTEGER_DIVIDE_BY_ZERO,
                        pc: 0,
                        vaddr: 0,
                    });
                }
                let d = self.rreg(rd.num());
                let v = alu_rrr(op, s, t, d, self.cpu.sar);
                self.wreg(rd.num(), v, tracer);
            }
            Inst::Rt(op, rd, rt) => {
                let t = self.rreg(rt.num());
                let v = alu_rt(op, t, self.cpu.sar);
                self.wreg(rd.num(), v, tracer);
            }
            Inst::Rs(op, rd, rs) => {
                let s = self.rreg(rs.num());
                let v = match op {
                    // SLL uses SAR, which SSL set to (32 - amount).
                    AluRs::Sll => s.wrapping_shl(32u32.wrapping_sub(self.cpu.sar) & 31),
                    AluRs::Movsp => s, // MOVSP: plain move for our (non-trapping) model.
                };
                self.wreg(rd.num(), v, tracer);
            }
            Inst::ShiftSet(op, rs) => {
                let s = self.rreg(rs.num());
                self.cpu.sar = match op {
                    ShiftSetOp::Ssr => s & 31,
                    ShiftSetOp::Ssl => 32u32.wrapping_sub(s & 31),
                    ShiftSetOp::Ssa8l => (s & 3) * 8,
                    ShiftSetOp::Ssa8b => 32u32.wrapping_sub((s & 3) * 8),
                };
            }
            Inst::Ssai(imm) => {
                self.cpu.sar = imm as u32;
            }
            Inst::Slli(rd, rs, sa) => {
                let v = self.rreg(rs.num()).wrapping_shl(sa as u32 & 31);
                self.wreg(rd.num(), v, tracer);
            }
            Inst::Srli(rd, rt, sa) => {
                let v = self.rreg(rt.num()) >> (sa as u32 & 31);
                self.wreg(rd.num(), v, tracer);
            }
            Inst::Srai(rd, rt, sa) => {
                let v = ((self.rreg(rt.num()) as i32) >> (sa as u32 & 31)) as u32;
                self.wreg(rd.num(), v, tracer);
            }
            Inst::Extui(rd, rt, shift, mask) => {
                let t = self.rreg(rt.num());
                let masked = if mask >= 32 {
                    u32::MAX
                } else {
                    (1u32 << mask) - 1
                };
                let v = (t >> (shift as u32 & 31)) & masked;
                self.wreg(rd.num(), v, tracer);
            }
            Inst::Sext(rd, rs, bit) => {
                // Sign-extend `rs` from bit `bit` (7..=22): replicate bit `bit`.
                let s = self.rreg(rs.num());
                let sh = 31 - (bit as u32);
                let v = (((s << sh) as i32) >> sh) as u32;
                self.wreg(rd.num(), v, tracer);
            }
            Inst::AddN(rd, rs, rt) => {
                let v = self.rreg(rs.num()).wrapping_add(self.rreg(rt.num()));
                self.wreg(rd.num(), v, tracer);
            }
            Inst::MovN(rt, rs) => {
                let v = self.rreg(rs.num());
                self.wreg(rt.num(), v, tracer);
            }
            _ => unreachable!("exec_arith got {inst:?}"),
        }
        Ok(Flow::Next)
    }
}

/// Three-register ALU. `d` is the current value of the destination (for the
/// conditional moves); `sar` for SRC.
fn alu_rrr(op: AluRrr, s: u32, t: u32, d: u32, sar: u32) -> u32 {
    match op {
        AluRrr::And => s & t,
        AluRrr::Or => s | t,
        AluRrr::Xor => s ^ t,
        AluRrr::Add => s.wrapping_add(t),
        AluRrr::Sub => s.wrapping_sub(t),
        AluRrr::Addx2 => (s << 1).wrapping_add(t),
        AluRrr::Addx4 => (s << 2).wrapping_add(t),
        AluRrr::Addx8 => (s << 3).wrapping_add(t),
        AluRrr::Subx2 => (s << 1).wrapping_sub(t),
        AluRrr::Subx4 => (s << 2).wrapping_sub(t),
        AluRrr::Subx8 => (s << 3).wrapping_sub(t),
        AluRrr::Src => {
            let cat = ((s as u64) << 32) | (t as u64);
            (cat >> (sar & 63)) as u32
        }
        AluRrr::Mull => s.wrapping_mul(t),
        AluRrr::Muluh => (((s as u64) * (t as u64)) >> 32) as u32,
        AluRrr::Mulsh => ((((s as i32 as i64) * (t as i32 as i64)) >> 32) as i32) as u32,
        // Zero divisors trap in `exec_arith` before reaching here (the
        // IntegerDivideByZero model); `t != 0` is an invariant of these arms.
        // `wrapping_div`/`wrapping_rem` give INT_MIN / -1 = INT_MIN, rem 0.
        AluRrr::Quou => s / t,
        AluRrr::Quos => ((s as i32).wrapping_div(t as i32)) as u32,
        AluRrr::Remu => s % t,
        AluRrr::Rems => ((s as i32).wrapping_rem(t as i32)) as u32,
        AluRrr::Min => (s as i32).min(t as i32) as u32,
        AluRrr::Max => (s as i32).max(t as i32) as u32,
        AluRrr::Minu => s.min(t),
        AluRrr::Maxu => s.max(t),
        AluRrr::Mul16u => (s & 0xffff).wrapping_mul(t & 0xffff),
        AluRrr::Mul16s => ((s as i16 as i32).wrapping_mul(t as i16 as i32)) as u32,
        AluRrr::Moveqz => {
            if t == 0 {
                s
            } else {
                d
            }
        }
        AluRrr::Movnez => {
            if t != 0 {
                s
            } else {
                d
            }
        }
        AluRrr::Movltz => {
            if (t as i32) < 0 {
                s
            } else {
                d
            }
        }
        AluRrr::Movgez => {
            if (t as i32) >= 0 {
                s
            } else {
                d
            }
        }
    }
}

/// Two-register `op rd, rt` ALU.
fn alu_rt(op: AluRt, t: u32, sar: u32) -> u32 {
    match op {
        AluRt::Neg => 0u32.wrapping_sub(t),
        AluRt::Abs => (t as i32).unsigned_abs(),
        AluRt::Sra => ((t as i32) >> (sar & 31)) as u32,
        AluRt::Srl => t >> (sar & 31),
        AluRt::Nsau => t.leading_zeros(),
        AluRt::Nsa => {
            // Count leading redundant sign bits: (#sign bits) - 1, in 0..=31.
            let redundant = if (t as i32) < 0 {
                (!t).leading_zeros()
            } else {
                t.leading_zeros()
            };
            redundant.saturating_sub(1).min(31)
        }
    }
}
