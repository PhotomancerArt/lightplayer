//! Mechanical emission for fastalloc [`super::inst::PInst`] (straight-line path).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::rv32::encode::{
    encode_add, encode_addi, encode_and, encode_auipc, encode_b_type, encode_beq, encode_bne,
    encode_div, encode_divu, encode_jal, encode_jalr, encode_lw, encode_mul, encode_or, encode_rem,
    encode_remu, encode_ret, encode_sll, encode_slt, encode_sltiu, encode_sltu, encode_sra,
    encode_srl, encode_sub, encode_sw, encode_xor, encode_xori, iconst32_sequence,
};
use crate::rv32::gpr::{RA_REG, SP_REG};
use crate::rv32::inst::PInst;
const F3_BLT: u32 = 0b100;
const F3_BGE: u32 = 0b101;

/// Relocation at the `auipc` of an auipc+jalr pair.
#[derive(Clone, Debug)]
pub struct PhysReloc {
    pub offset: usize,
    pub symbol: String,
}

#[derive(Clone, Copy, Debug)]
enum BranchKind {
    Beq,
    Bne,
    Blt,
    Bge,
}

/// Branch fixup: record location of a branch that needs its offset patched.
#[derive(Clone, Debug)]
struct BranchFixup {
    instr_offset: usize,
    label_id: u32,
    src1: u32,
    src2: u32,
    kind: BranchKind,
}

/// Jump fixup: record location of a J instruction that needs its offset patched.
#[derive(Clone, Debug)]
struct JalFixup {
    instr_offset: usize,
    label_id: u32,
}

pub struct Rv32Emitter {
    code: Vec<u8>,
    relocs: Vec<PhysReloc>,
    label_offsets: BTreeMap<u32, usize>, // label id → byte offset
    branch_fixups: Vec<BranchFixup>,
    jal_fixups: Vec<JalFixup>,
}

impl Rv32Emitter {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            relocs: Vec::new(),
            label_offsets: BTreeMap::new(),
            branch_fixups: Vec::new(),
            jal_fixups: Vec::new(),
        }
    }

    fn push_u32(&mut self, w: u32) {
        self.code.extend_from_slice(&w.to_le_bytes());
    }

    pub fn emit(&mut self, inst: &PInst) {
        match inst {
            PInst::FrameSetup { spill_slots } => self.emit_frame_setup(*spill_slots),
            PInst::FrameTeardown { spill_slots } => self.emit_frame_teardown(*spill_slots),

            PInst::Add { dst, src1, src2 } => {
                self.push_u32(encode_add(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Sub { dst, src1, src2 } => {
                self.push_u32(encode_sub(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Mul { dst, src1, src2 } => {
                self.push_u32(encode_mul(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Div { dst, src1, src2 } => {
                self.push_u32(encode_div(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Divu { dst, src1, src2 } => {
                self.push_u32(encode_divu(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Rem { dst, src1, src2 } => {
                self.push_u32(encode_rem(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Remu { dst, src1, src2 } => {
                self.push_u32(encode_remu(*dst as u32, *src1 as u32, *src2 as u32));
            }

            PInst::And { dst, src1, src2 } => {
                self.push_u32(encode_and(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Or { dst, src1, src2 } => {
                self.push_u32(encode_or(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Xor { dst, src1, src2 } => {
                self.push_u32(encode_xor(*dst as u32, *src1 as u32, *src2 as u32));
            }

            PInst::Sll { dst, src1, src2 } => {
                self.push_u32(encode_sll(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Srl { dst, src1, src2 } => {
                self.push_u32(encode_srl(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Sra { dst, src1, src2 } => {
                self.push_u32(encode_sra(*dst as u32, *src1 as u32, *src2 as u32));
            }

            PInst::Neg { dst, src } => {
                self.push_u32(encode_sub(*dst as u32, 0, *src as u32));
            }
            PInst::Not { dst, src } => {
                self.push_u32(encode_xori(*dst as u32, *src as u32, -1));
            }
            PInst::Mv { dst, src } => {
                self.push_u32(encode_addi(*dst as u32, *src as u32, 0));
            }

            PInst::Slt { dst, src1, src2 } => {
                self.push_u32(encode_slt(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Sltu { dst, src1, src2 } => {
                self.push_u32(encode_sltu(*dst as u32, *src1 as u32, *src2 as u32));
            }
            PInst::Seqz { dst, src } => {
                self.push_u32(encode_sltiu(*dst as u32, *src as u32, 1));
            }
            PInst::Snez { dst, src } => {
                self.push_u32(encode_sltu(*dst as u32, 0, *src as u32));
            }
            PInst::Sltz { dst, src } => {
                self.push_u32(encode_slt(*dst as u32, *src as u32, 0));
            }
            PInst::Sgtz { dst, src } => {
                self.push_u32(encode_slt(*dst as u32, 0, *src as u32));
            }

            PInst::Li { dst, imm } => {
                for w in iconst32_sequence(*dst as u32, *imm) {
                    self.push_u32(w);
                }
            }
            PInst::Addi { dst, src, imm } => {
                self.push_u32(encode_addi(*dst as u32, *src as u32, *imm));
            }

            PInst::Lw { dst, base, offset } => {
                self.push_u32(encode_lw(*dst as u32, *base as u32, *offset));
            }
            PInst::Sw { src, base, offset } => {
                self.push_u32(encode_sw(*src as u32, *base as u32, *offset));
            }

            PInst::SlotAddr { dst, slot } => {
                self.push_u32(encode_addi(
                    *dst as u32,
                    SP_REG as u32,
                    (*slot as i32).saturating_mul(4),
                ));
            }

            PInst::MemcpyWords { dst, src, size } => {
                let t_data = 5u32;
                let p_src = 6u32;
                let p_dst = 7u32;
                self.push_u32(encode_addi(p_src, *src as u32, 0));
                self.push_u32(encode_addi(p_dst, *dst as u32, 0));
                let mut remaining = *size as i32;
                while remaining > 0 {
                    let mut local_off = 0i32;
                    while local_off + 4 <= remaining && local_off <= 2047 - 3 {
                        self.push_u32(encode_lw(t_data, p_src, local_off));
                        self.push_u32(encode_sw(t_data, p_dst, local_off));
                        local_off += 4;
                    }
                    if local_off == 0 {
                        panic!("MemcpyWords: could not emit chunk (size alignment?)");
                    }
                    if local_off < remaining {
                        self.push_u32(encode_addi(p_src, p_src, local_off));
                        self.push_u32(encode_addi(p_dst, p_dst, local_off));
                    }
                    remaining -= local_off;
                }
            }

            PInst::Call { target } => {
                let name = target.name.clone();
                let hi = self.relocs.len();
                self.relocs.push(PhysReloc {
                    offset: self.code.len(),
                    symbol: name,
                });
                self.push_u32(encode_auipc(6, 0));
                self.relocs[hi].offset = self.code.len() - 4;
                self.push_u32(encode_jalr(1, 6, 0));
            }

            PInst::Ret => {
                self.push_u32(encode_ret());
            }

            PInst::Beq { src1, src2, target } => {
                self.emit_branch(BranchKind::Beq, *src1 as u32, *src2 as u32, *target);
            }
            PInst::Bne { src1, src2, target } => {
                self.emit_branch(BranchKind::Bne, *src1 as u32, *src2 as u32, *target);
            }
            PInst::Blt { src1, src2, target } => {
                self.emit_branch(BranchKind::Blt, *src1 as u32, *src2 as u32, *target);
            }
            PInst::Bge { src1, src2, target } => {
                self.emit_branch(BranchKind::Bge, *src1 as u32, *src2 as u32, *target);
            }
            PInst::J { target } => {
                if let Some(&label_off) = self.label_offsets.get(target) {
                    let imm = label_off as i32 - self.code.len() as i32;
                    self.push_u32(encode_jal(0, imm));
                } else {
                    self.jal_fixups.push(JalFixup {
                        instr_offset: self.code.len(),
                        label_id: *target,
                    });
                    self.push_u32(0);
                }
            }
            PInst::Label { id } => {
                // Record the byte offset for this label; emit no code
                self.label_offsets.insert(*id, self.code.len());
            }
        }
    }

    fn encode_branch(kind: BranchKind, src1: u32, src2: u32, imm: i32) -> u32 {
        match kind {
            BranchKind::Beq => encode_beq(src1, src2, imm),
            BranchKind::Bne => encode_bne(src1, src2, imm),
            BranchKind::Blt => encode_b_type(F3_BLT, src1, src2, imm),
            BranchKind::Bge => encode_b_type(F3_BGE, src1, src2, imm),
        }
    }

    fn emit_branch(&mut self, kind: BranchKind, src1: u32, src2: u32, target: u32) {
        if let Some(&label_off) = self.label_offsets.get(&target) {
            let imm = label_off as i32 - self.code.len() as i32;
            self.push_u32(Self::encode_branch(kind, src1, src2, imm));
        } else {
            self.branch_fixups.push(BranchFixup {
                instr_offset: self.code.len(),
                label_id: target,
                src1,
                src2,
                kind,
            });
            self.push_u32(0);
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

    /// Apply branch and jump fixups. Must be called after all PInsts are emitted
    /// and all label positions are known.
    pub fn apply_fixups(&mut self) {
        for fixup in &self.branch_fixups {
            let Some(&label_off) = self.label_offsets.get(&fixup.label_id) else {
                continue;
            };
            let imm = label_off as i32 - fixup.instr_offset as i32;
            let encoded = Self::encode_branch(fixup.kind, fixup.src1, fixup.src2, imm);
            let off = fixup.instr_offset;
            if off + 4 <= self.code.len() {
                self.code[off..off + 4].copy_from_slice(&encoded.to_le_bytes());
            }
        }

        for fixup in &self.jal_fixups {
            let Some(&label_off) = self.label_offsets.get(&fixup.label_id) else {
                continue;
            };
            let imm = label_off as i32 - fixup.instr_offset as i32;
            let encoded = encode_jal(0, imm);
            let off = fixup.instr_offset;
            if off + 4 <= self.code.len() {
                self.code[off..off + 4].copy_from_slice(&encoded.to_le_bytes());
            }
        }
    }

    /// Emit all instructions, apply fixups, and return the final code + relocs.
    pub fn finish_with_fixups(mut self) -> (Vec<u8>, Vec<PhysReloc>) {
        self.apply_fixups();
        (self.code, self.relocs)
    }
}

impl Default for Rv32Emitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_add() {
        let mut emitter = Rv32Emitter::new();
        emitter.emit(&PInst::Add {
            dst: 10,
            src1: 11,
            src2: 12,
        });
        let code = emitter.finish();
        assert_eq!(code, &[0x33, 0x85, 0xC5, 0x00]);
    }

    #[test]
    fn test_emit_li() {
        let mut emitter = Rv32Emitter::new();
        emitter.emit(&PInst::Li { dst: 10, imm: 42 });
        let code = emitter.finish();
        assert_eq!(code, &[0x13, 0x05, 0xA0, 0x02]);
    }

    #[test]
    fn test_emit_ret() {
        let mut emitter = Rv32Emitter::new();
        emitter.emit(&PInst::Ret);
        let code = emitter.finish();
        assert_eq!(code, &[0x67, 0x80, 0x00, 0x00]);
    }
}
