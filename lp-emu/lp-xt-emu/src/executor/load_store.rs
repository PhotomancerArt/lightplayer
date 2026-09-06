//! Load / store executors, including the `l32r` PC-relative literal load.

use lp_xt_inst::{Inst, LoadOp, StoreOp};

use crate::emu::{Emulator, Flow};
use crate::error::Trap;
use crate::trace::{TraceEvent, Tracer};

impl Emulator {
    pub(super) fn exec_load_store(
        &mut self,
        inst: &Inst,
        pc: u32,
        tracer: &mut dyn Tracer,
    ) -> Result<Flow, Trap> {
        match *inst {
            Inst::Load(op, rt, rs, off) => {
                let addr = self.rreg(rs.num()).wrapping_add(off);
                let v = match op {
                    LoadOp::L8ui => self.mem.read_u8(addr)? as u32,
                    LoadOp::L16ui => self.mem.read_u16(addr)? as u32,
                    LoadOp::L16si => self.mem.read_u16(addr)? as i16 as i32 as u32,
                    LoadOp::L32i => self.mem.read_u32(addr)?,
                };
                self.wreg(rt.num(), v, tracer);
            }
            Inst::L32iN(rt, rs, off) => {
                let addr = self.rreg(rs.num()).wrapping_add(off);
                let v = self.mem.read_u32(addr)?;
                self.wreg(rt.num(), v, tracer);
            }
            Inst::Store(op, rt, rs, off) => {
                let addr = self.rreg(rs.num()).wrapping_add(off);
                let v = self.rreg(rt.num());
                let nbytes = match op {
                    StoreOp::S8i => {
                        self.mem.write_u8(addr, v as u8)?;
                        1
                    }
                    StoreOp::S16i => {
                        self.mem.write_u16(addr, v as u16)?;
                        2
                    }
                    StoreOp::S32i => {
                        self.mem.write_u32(addr, v)?;
                        4
                    }
                };
                tracer.event(TraceEvent::MemWrite {
                    addr,
                    value: v,
                    nbytes,
                });
            }
            Inst::S32iN(rt, rs, off) => {
                let addr = self.rreg(rs.num()).wrapping_add(off);
                let v = self.rreg(rt.num());
                self.mem.write_u32(addr, v)?;
                tracer.event(TraceEvent::MemWrite {
                    addr,
                    value: v,
                    nbytes: 4,
                });
            }
            Inst::L32r(rt, field) => {
                // Target = ((PC + 3) & !3) + ((imm16 - 0x1_0000) << 2).
                //
                // The 16-bit field is ONE-EXTENDED, not sign-extended: it always
                // denotes a negative word offset in -65536..=-1, so the whole
                // -262144..=-4 byte range is reachable. Sign-extending instead
                // (`field as i16`) mis-executes every field >= 0x8000's
                // counterpart — fields 0x0000..0x7fff would become *forward*
                // offsets. Matches `lp_xt_inst::disasm::l32r_target` (which is
                // verified against objdump) and `xt_mini_emit::imm::L32rDisp`.
                let base = pc.wrapping_add(3) & !3;
                let off = (((field as i32) - 0x1_0000) << 2) as u32;
                let addr = base.wrapping_add(off);
                let v = self.mem.read_u32(addr)?;
                self.wreg(rt.num(), v, tracer);
            }
            _ => unreachable!("exec_load_store got {inst:?}"),
        }
        Ok(Flow::Next)
    }
}
