//! Conditional-branch executors. All Xtensa PC-relative branches target
//! `PC + 4 + offset` (the `+4` is fixed, independent of instruction width).

use lp_xt_inst::{BrRi, BrRiu, BrRr, BrZ, Inst};

use crate::emu::{Emulator, Flow};
use crate::error::Trap;

impl Emulator {
    pub(super) fn exec_branch(&mut self, inst: &Inst, pc: u32) -> Result<Flow, Trap> {
        let taken = match *inst {
            Inst::BranchRr(op, rs, rt, off) => {
                let a = self.rreg(rs.num());
                let b = self.rreg(rt.num());
                let hit = match op {
                    BrRr::Beq => a == b,
                    BrRr::Bne => a != b,
                    BrRr::Blt => (a as i32) < (b as i32),
                    BrRr::Bge => (a as i32) >= (b as i32),
                    BrRr::Bltu => a < b,
                    BrRr::Bgeu => a >= b,
                    BrRr::Ball => (a & b) == b,
                    BrRr::Bany => (a & b) != 0,
                    BrRr::Bnall => (a & b) != b,
                    BrRr::Bnone => (a & b) == 0,
                    BrRr::Bbc => (a & (1u32 << (b & 31))) == 0,
                    BrRr::Bbs => (a & (1u32 << (b & 31))) != 0,
                };
                (hit, off)
            }
            Inst::BranchRi(op, rs, imm, off) => {
                let a = self.rreg(rs.num()) as i32;
                let hit = match op {
                    BrRi::Beqi => a == imm,
                    BrRi::Bnei => a != imm,
                    BrRi::Blti => a < imm,
                    BrRi::Bgei => a >= imm,
                };
                (hit, off)
            }
            Inst::BranchRiu(op, rs, imm, off) => {
                let a = self.rreg(rs.num());
                let b = imm as u32;
                let hit = match op {
                    BrRiu::Bltui => a < b,
                    BrRiu::Bgeui => a >= b,
                };
                (hit, off)
            }
            Inst::BranchZ(op, rs, off) => {
                let a = self.rreg(rs.num());
                let hit = match op {
                    BrZ::Beqz => a == 0,
                    BrZ::Bnez => a != 0,
                    BrZ::Bltz => (a as i32) < 0,
                    BrZ::Bgez => (a as i32) >= 0,
                };
                (hit, off)
            }
            Inst::BranchBiI(set, rs, bit, off) => {
                let a = self.rreg(rs.num());
                let is_set = (a & (1u32 << (bit & 31))) != 0;
                (is_set == set, off)
            }
            Inst::BranchZN(nez, rs, off) => {
                let a = self.rreg(rs.num());
                let hit = if nez { a != 0 } else { a == 0 };
                (hit, off as i32)
            }
            _ => unreachable!("exec_branch got {inst:?}"),
        };

        let (hit, off) = taken;
        if hit {
            Ok(Flow::Jump(pc.wrapping_add(4).wrapping_add(off as u32)))
        } else {
            Ok(Flow::Next)
        }
    }
}
