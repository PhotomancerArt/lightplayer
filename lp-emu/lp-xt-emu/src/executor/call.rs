//! Windowed and non-windowed call executors (`CALL{0,4,8,12}`, `CALLX{...}`).
//!
//! A windowed CALL does NOT rotate the window itself — it stages the return
//! address (with the call-increment in the top two bits) into `a[4*inc]` of the
//! *caller's* window and records `PS.CALLINC`. The callee's `ENTRY` performs the
//! rotation, so `a[4*inc]` becomes the callee's `a0` and the caller's argument
//! registers `a[4*inc + 2..]` become the callee's `a2..`.
//!
//! A non-windowed CALL0/CALLX0 writes `a0` and nothing else: `PS.CALLINC` is
//! not its to touch (see `exec_call`).

use lp_emu_core::bus::Bus;
use lp_xt_inst::{CallOp, CallxOp, Inst};

use super::Exec;
use crate::emu::Flow;
use crate::error::Trap;
use crate::trace::Tracer;

impl<B: Bus> Exec<'_, B> {
    pub(super) fn exec_call<T: Tracer + ?Sized>(
        &mut self,
        inst: &Inst,
        pc: u32,
        tracer: &mut T,
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
            // CALL0 / CALLX0: non-windowed. a0 = return address, no rotation,
            // and **PS.CALLINC is left alone**: the RM's CALL0/CALLX0 pages
            // list no write to it; only CALL4/8/12 and CALLX4/8/12 set it.
            //
            // The write that used to be here (`ps_callinc = 0`) was M4 P4b's
            // finding. xtensa-lx-rt's `_UserExceptionVector` reaches its
            // handler through `call0 __naked_user_exception` and only then
            // does `rsr a0, PS`, so an interrupt that lands between a CALLn
            // and its callee's ENTRY had its CALLINC zeroed before the
            // handler could save it; `rfe` then re-ran that ENTRY with
            // CALLINC = 0, which rotates by nothing and writes the callee's
            // SP into the *caller's* `a1` — and the caller's next `retw`
            // reloaded its own caller from the wrong save area. On the
            // classic that killed every project load a few seconds in, in
            // `_WindowUnderflow8` with `a1 = 0` (lp-emu-esp32v3's README,
            // "The window, across a context save").
            self.wreg(0, ret, tracer);
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
