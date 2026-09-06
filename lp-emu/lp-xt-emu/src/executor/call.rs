//! Windowed and non-windowed call executors (`CALL{0,4,8,12}`, `CALLX{...}`).
//!
//! A windowed CALL does NOT rotate the window itself — it stages the return
//! address (with the call-increment in the top two bits) into `a[4*inc]` of the
//! *caller's* window and records `PS.CALLINC`. The callee's `ENTRY` performs the
//! rotation, so `a[4*inc]` becomes the callee's `a0` and the caller's argument
//! registers `a[4*inc + 2..]` become the callee's `a2..`.

use lp_xt_inst::{CallOp, CallxOp, Inst};

use crate::emu::{Emulator, Flow};
use crate::error::Trap;
use crate::trace::Tracer;

impl Emulator {
    pub(super) fn exec_call(
        &mut self,
        inst: &Inst,
        pc: u32,
        tracer: &mut dyn Tracer,
    ) -> Result<Flow, Trap> {
        let (inc, target) = match *inst {
            Inst::Call(op, off) => {
                let inc = match op {
                    CallOp::Call0 => 0,
                    CallOp::Call4 => 1,
                    CallOp::Call8 => 2,
                    CallOp::Call12 => 3,
                };
                // Target = (PC & !3) + (offset << 2) + 4.
                let target = (pc & !3).wrapping_add(((off) << 2) as u32).wrapping_add(4);
                (inc, target)
            }
            Inst::Callx(op, rs) => {
                let inc = match op {
                    CallxOp::Callx0 => 0,
                    CallxOp::Callx4 => 1,
                    CallxOp::Callx8 => 2,
                    CallxOp::Callx12 => 3,
                };
                // Read the target BEFORE overwriting the return-address register
                // (for `callx8 a8`, the target and the write land on a8).
                let target = self.rreg(rs.num());
                (inc, target)
            }
            _ => unreachable!("exec_call got {inst:?}"),
        };

        let ret = pc.wrapping_add(3);
        if inc == 0 {
            // CALL0 / CALLX0: non-windowed. a0 = return address, no rotation.
            self.wreg(0, ret, tracer);
            self.cpu.ps_callinc = 0;
        } else {
            // Windowed: stage the mangled return address in a[4*inc], record
            // PS.CALLINC for the callee's ENTRY.
            let mangled = ((inc as u32) << 30) | (ret & 0x3FFF_FFFF);
            self.wreg(4 * inc, mangled, tracer);
            self.cpu.ps_callinc = inc;
        }
        Ok(Flow::Jump(target))
    }
}
