//! [`PhysInst`](super::inst::PhysInst) → RISC-V machine code (mechanical encoding).

use alloc::string::String;
use alloc::vec::Vec;

use super::abi::{PhysReg, RA_REG, SP_REG};
use super::inst::PhysInst;
use crate::isa::rv32::inst::{
    encode_add, encode_addi, encode_and, encode_auipc, encode_b_type, encode_beq, encode_bne,
    encode_div, encode_divu, encode_jal, encode_jalr, encode_lw, encode_mul, encode_or, encode_rem,
    encode_remu, encode_ret, encode_sll, encode_slt, encode_sltiu, encode_sltu, encode_sra,
    encode_srl, encode_sub, encode_sw, encode_xor, encode_xori, iconst32_sequence,
};

/// Relocation at the `auipc` of an auipc+jalr pair (same as [`crate::isa::rv32::emit::NativeReloc`]).
#[derive(Clone, Debug)]
pub struct PhysReloc {
    pub offset: usize,
    pub symbol: String,
}

pub struct PhysEmitter {
    code: Vec<u8>,
    relocs: Vec<PhysReloc>,
}

impl PhysEmitter {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            relocs: Vec::new(),
        }
    }

    fn push_u32(&mut self, w: u32) {
        self.code.extend_from_slice(&w.to_le_bytes());
    }

    pub fn emit(&mut self, inst: &PhysInst) {
        match inst {
            PhysInst::FrameSetup { spill_slots } => self.emit_frame_setup(*spill_slots),
            PhysInst::FrameTeardown { spill_slots } => self.emit_frame_teardown(*spill_slots),

            PhysInst::Add { dst, src1, src2 } => {
                self.push_u32(encode_add(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Sub { dst, src1, src2 } => {
                self.push_u32(encode_sub(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Mul { dst, src1, src2 } => {
                self.push_u32(encode_mul(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Div { dst, src1, src2 } => {
                self.push_u32(encode_div(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Divu { dst, src1, src2 } => {
                self.push_u32(encode_divu(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Rem { dst, src1, src2 } => {
                self.push_u32(encode_rem(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Remu { dst, src1, src2 } => {
                self.push_u32(encode_remu(*dst as u32, *src1 as u32, *src2 as u32));
            }

            PhysInst::And { dst, src1, src2 } => {
                self.push_u32(encode_and(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Or { dst, src1, src2 } => {
                self.push_u32(encode_or(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Xor { dst, src1, src2 } => {
                self.push_u32(encode_xor(*dst as u32, *src1 as u32, *src2 as u32));
            }

            PhysInst::Sll { dst, src1, src2 } => {
                self.push_u32(encode_sll(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Srl { dst, src1, src2 } => {
                self.push_u32(encode_srl(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Sra { dst, src1, src2 } => {
                self.push_u32(encode_sra(*dst as u32, *src1 as u32, *src2 as u32));
            }

            PhysInst::Neg { dst, src } => {
                self.push_u32(encode_sub(*dst as u32, 0, *src as u32));
            }
            PhysInst::Not { dst, src } => {
                self.push_u32(encode_xori(*dst as u32, *src as u32, -1));
            }
            PhysInst::Mv { dst, src } => {
                self.push_u32(encode_addi(*dst as u32, *src as u32, 0));
            }

            PhysInst::Slt { dst, src1, src2 } => {
                self.push_u32(encode_slt(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Sltu { dst, src1, src2 } => {
                self.push_u32(encode_sltu(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PhysInst::Seqz { dst, src } => {
                self.push_u32(encode_sltiu(*dst as u32, *src as u32, 1));
            }
            PhysInst::Snez { dst, src } => {
                self.push_u32(encode_sltu(*dst as u32, 0, *src as u32));
            }
            PhysInst::Sltz { dst, src } => {
                self.push_u32(encode_slt(*dst as u32, *src as u32, 0));
            }
            PhysInst::Sgtz { dst, src } => {
                self.push_u32(encode_slt(*dst as u32, 0, *src as u32));
            }

            PhysInst::Li { dst, imm } => {
                for w in iconst32_sequence(*dst as u32, *imm) {
                    self.push_u32(w);
                }
            }
            PhysInst::Addi { dst, src, imm } => {
                self.push_u32(encode_addi(*dst as u32, *src as u32, *imm));
            }

            PhysInst::Lw { dst, base, offset } => {
                self.push_u32(encode_lw(*dst as u32, *base as u32, *offset));
            }
            PhysInst::Sw { src, base, offset } => {
                self.push_u32(encode_sw(*src as u32, *base as u32, *offset));
            }

            PhysInst::SlotAddr { dst, slot } => {
                let off = (*slot as i32).saturating_mul(4);
                self.push_u32(encode_addi(*dst as u32, SP_REG as u32, off));
            }

            PhysInst::MemcpyWords { dst, src, size } => self.emit_memcpy_words(*dst, *src, *size),

            PhysInst::Call { target } => {
                let auipc_off = self.code.len();
                self.push_u32(encode_auipc(RA_REG as u32, 0));
                self.push_u32(encode_jalr(RA_REG as u32, RA_REG as u32, 0));
                self.relocs.push(PhysReloc {
                    offset: auipc_off,
                    symbol: target.name.clone(),
                });
            }
            PhysInst::Ret => {
                self.push_u32(encode_ret());
            }

            PhysInst::Beq {
                src1,
                src2,
                target: _,
            } => {
                self.push_u32(encode_beq(*src1 as u32, *src2 as u32, 0));
            }
            PhysInst::Bne {
                src1,
                src2,
                target: _,
            } => {
                self.push_u32(encode_bne(*src1 as u32, *src2 as u32, 0));
            }
            PhysInst::Blt {
                src1,
                src2,
                target: _,
            } => {
                self.push_u32(encode_b_type(0b100, *src1 as u32, *src2 as u32, 0));
            }
            PhysInst::Bge {
                src1,
                src2,
                target: _,
            } => {
                self.push_u32(encode_b_type(0b101, *src1 as u32, *src2 as u32, 0));
            }
            PhysInst::J { target: _ } => {
                self.push_u32(encode_jal(0, 0));
            }
        }
    }

    fn emit_memcpy_words(&mut self, dst: PhysReg, src: PhysReg, size: u32) {
        let mut data_temp: u32 = 29;
        if dst as u32 == data_temp || src as u32 == data_temp {
            data_temp = 30;
        }
        let r_dst = dst as u32;
        let r_src = src as u32;
        let mut off = 0i32;
        let mut left = size;
        while left > 0 {
            self.push_u32(encode_lw(data_temp, r_src, off));
            self.push_u32(encode_sw(data_temp, r_dst, off));
            off = off.saturating_add(4);
            left = left.saturating_sub(4);
        }
    }

    fn emit_frame_setup(&mut self, spill_slots: u32) {
        let spill = (spill_slots as i32).saturating_mul(4);
        let mut frame = 16i32.saturating_add(spill);
        frame = (frame + 15) & !15;
        self.push_u32(encode_addi(SP_REG as u32, SP_REG as u32, -frame));
        self.push_u32(encode_sw(RA_REG as u32, SP_REG as u32, frame - 4));
        self.push_u32(encode_sw(8, SP_REG as u32, frame - 8));
        self.push_u32(encode_addi(8, SP_REG as u32, frame));
    }

    fn emit_frame_teardown(&mut self, spill_slots: u32) {
        let spill = (spill_slots as i32).saturating_mul(4);
        let mut frame = 16i32.saturating_add(spill);
        frame = (frame + 15) & !15;
        self.push_u32(encode_lw(RA_REG as u32, SP_REG as u32, frame - 4));
        self.push_u32(encode_lw(8, SP_REG as u32, frame - 8));
        self.push_u32(encode_addi(SP_REG as u32, SP_REG as u32, frame));
    }

    pub fn finish(self) -> Vec<u8> {
        self.code
    }

    pub fn finish_with_relocs(self) -> (Vec<u8>, Vec<PhysReloc>) {
        (self.code, self.relocs)
    }
}

impl Default for PhysEmitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_add() {
        let mut emitter = PhysEmitter::new();
        emitter.emit(&PhysInst::Add {
            dst: 10,
            src1: 11,
            src2: 12,
        });
        let code = emitter.finish();
        assert_eq!(code, &[0x33, 0x85, 0xC5, 0x00]);
    }

    #[test]
    fn test_emit_li() {
        let mut emitter = PhysEmitter::new();
        emitter.emit(&PhysInst::Li { dst: 10, imm: 42 });
        let code = emitter.finish();
        assert_eq!(code, &[0x13, 0x05, 0xA0, 0x02]);
    }

    #[test]
    fn test_emit_ret() {
        let mut emitter = PhysEmitter::new();
        emitter.emit(&PhysInst::Ret);
        let code = emitter.finish();
        assert_eq!(code, &[0x67, 0x80, 0x00, 0x00]);
    }
}
