//! Unconditional-jump executors.

use lp_xt_inst::Inst;

use crate::emu::{Emulator, Flow};
use crate::error::Trap;

impl Emulator {
    pub(super) fn exec_jump(&mut self, inst: &Inst, pc: u32) -> Result<Flow, Trap> {
        match *inst {
            // J: target = PC + 4 + offset (18-bit signed byte offset).
            Inst::J(off) => Ok(Flow::Jump(pc.wrapping_add(4).wrapping_add(off as u32))),
            // JX: jump to the register value.
            Inst::Jx(rs) => Ok(Flow::Jump(self.rreg(rs.num()))),
            _ => unreachable!("exec_jump got {inst:?}"),
        }
    }
}
