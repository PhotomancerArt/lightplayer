//! Immediate-move / immediate-add executors.

use lp_emu_core::bus::Bus;
use lp_xt_inst::Inst;

use super::Exec;
use crate::emu::Flow;
use crate::error::Trap;
use crate::trace::Tracer;

impl<B: Bus> Exec<'_, B> {
    pub(super) fn exec_imm<T: Tracer + ?Sized>(
        &mut self,
        inst: &Inst,
        tracer: &mut T,
    ) -> Result<Flow, Trap> {
        match *inst {
            Inst::Movi(rt, imm) => {
                self.wreg(rt.num(), imm as u32, tracer);
            }
            Inst::MoviN(rt, imm) => {
                self.wreg(rt.num(), imm as u32, tracer);
            }
            Inst::Addi(rt, rs, imm) => {
                let v = self.rreg(rs.num()).wrapping_add(imm as u32);
                self.wreg(rt.num(), v, tracer);
            }
            Inst::AddiN(rd, rs, imm) => {
                let v = self.rreg(rs.num()).wrapping_add(imm as u32);
                self.wreg(rd.num(), v, tracer);
            }
            Inst::Addmi(rt, rs, imm) => {
                let v = self.rreg(rs.num()).wrapping_add(imm as u32);
                self.wreg(rt.num(), v, tracer);
            }
            _ => unreachable!("exec_imm got {inst:?}"),
        }
        Ok(Flow::Next)
    }
}
