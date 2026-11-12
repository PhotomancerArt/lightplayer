# Phase 4: PhysInst-to-Bytes Emitter

## Scope

Create mechanical emitter that converts PhysInst to bytes.

## Implementation

Create `rv32fa/emit.rs`:

```rust
//! PhysInst to machine code emitter.
//!
//! Mechanical translation with no indirection.
//! Extracts encoding logic from isa/rv32/emit.rs.

use crate::isa::rv32fa::inst::{PhysInst, PhysReg};
use alloc::vec::Vec;

pub struct PhysEmitter {
    code: Vec<u8>,
}

impl PhysEmitter {
    pub fn new() -> Self {
        Self { code: Vec::new() }
    }

    pub fn emit(&mut self, inst: &PhysInst) {
        match inst {
            // Frame operations (expand to actual prologue/epilogue)
            PhysInst::FrameSetup { spill_slots } => self.emit_frame_setup(*spill_slots),
            PhysInst::FrameTeardown { spill_slots } => self.emit_frame_teardown(*spill_slots),

            // R-type arithmetic
            PhysInst::Add { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b000, *src1, *src2, 0b0000000),
            PhysInst::Sub { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b000, *src1, *src2, 0b0100000),
            PhysInst::Mul { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b000, *src1, *src2, 0b0000001),
            PhysInst::Div { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b100, *src1, *src2, 0b0000001),
            PhysInst::Divu { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b101, *src1, *src2, 0b0000001),
            PhysInst::Rem { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b110, *src1, *src2, 0b0000001),
            PhysInst::Remu { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b111, *src1, *src2, 0b0000001),

            // Logical
            PhysInst::And { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b111, *src1, *src2, 0b0000000),
            PhysInst::Or { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b110, *src1, *src2, 0b0000000),
            PhysInst::Xor { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b100, *src1, *src2, 0b0000000),

            // Shifts
            PhysInst::Sll { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b001, *src1, *src2, 0b0000000),
            PhysInst::Srl { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b101, *src1, *src2, 0b0000000),
            PhysInst::Sra { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b101, *src1, *src2, 0b0100000),

            // Pseudoinstructions
            PhysInst::Neg { dst, src } => {
                // neg rd, rs = sub rd, x0, rs
                self.emit_r(0b0110011, *dst, 0b000, 0, *src, 0b0100000);
            }
            PhysInst::Not { dst, src } => {
                // not rd, rs = xori rd, rs, -1
                self.emit_i(0b0010011, *dst, 0b100, *src, -1);
            }
            PhysInst::Mv { dst, src } => {
                // mv rd, rs = addi rd, rs, 0
                self.emit_i(0b0010011, *dst, 0b000, *src, 0);
            }

            // Comparison
            PhysInst::Slt { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b010, *src1, *src2, 0b0000000),
            PhysInst::Sltu { dst, src1, src2 } => self.emit_r(0b0110011, *dst, 0b011, *src1, *src2, 0b0000000),
            PhysInst::Seqz { dst, src } => {
                // seqz rd, rs = sltiu rd, rs, 1
                self.emit_i(0b0010011, *dst, 0b011, *src, 1);
            }
            PhysInst::Snez { dst, src } => {
                // snez rd, rs = sltu rd, x0, rs
                self.emit_r(0b0110011, *dst, 0b011, 0, *src, 0b0000000);
            }
            PhysInst::Sltz { dst, src } => {
                // sltz rd, rs = slt rd, rs, x0
                self.emit_r(0b0110011, *dst, 0b010, *src, 0, 0b0000000);
            }
            PhysInst::Sgtz { dst, src } => {
                // sgtz rd, rs = slt rd, x0, rs
                self.emit_r(0b0110011, *dst, 0b010, 0, *src, 0b0000000);
            }

            // Immediate
            PhysInst::Li { dst, imm } => {
                // li is a pseudoinstruction - use lui+addi for large immediates
                // For now, assume small immediates: addi rd, x0, imm
                self.emit_i(0b0010011, *dst, 0b000, 0, *imm);
            }
            PhysInst::Addi { dst, src, imm } => self.emit_i(0b0010011, *dst, 0b000, *src, *imm),

            // Memory
            PhysInst::Lw { dst, base, offset } => self.emit_i(0b0000011, *dst, 0b010, *base, *offset),
            PhysInst::Sw { src, base, offset } => {
                // sw rs2, offset(rs1)
                let offset = (*offset) as u32 as u16;
                let imm_11_5 = (offset >> 5) as u8 & 0x7f;
                let imm_4_0 = offset & 0x1f;
                self.emit_s(0b0100011, imm_4_0, 0b010, *base, *src, imm_11_5);
            }

            // Stack slot (addi)
            PhysInst::SlotAddr { dst, slot } => {
                // dst = sp + slot * 4
                self.emit_i(0b0010011, *dst, 0b000, 2, (*slot as i32) * 4);
            }

            // Block memory (todo!())
            PhysInst::MemcpyWords { .. } => todo!("MemcpyWords emission"),

            // Control flow
            PhysInst::Call { target } => self.emit_call(*target),
            PhysInst::Ret => self.emit_ret(),
            PhysInst::Beq { src1, src2, target } => self.emit_b(0b1100011, *target, 0b000, *src1, *src2),
            PhysInst::Bne { src1, src2, target } => self.emit_b(0b1100011, *target, 0b001, *src1, *src2),
            PhysInst::Blt { src1, src2, target } => self.emit_b(0b1100011, *target, 0b100, *src1, *src2),
            PhysInst::Bge { src1, src2, target } => self.emit_b(0b1100011, *target, 0b101, *src1, *src2),
            PhysInst::J { target } => self.emit_j(*target),
        }
    }

    fn emit_r(&mut self, opcode: u8, rd: PhysReg, funct3: u8, rs1: PhysReg, rs2: PhysReg, funct7: u8) {
        let inst: u32 = ((funct7 as u32) << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((funct3 as u32) << 12)
            | ((rd as u32) << 7)
            | (opcode as u32);
        self.code.extend_from_slice(&inst.to_le_bytes());
    }

    fn emit_i(&mut self, opcode: u8, rd: PhysReg, funct3: u8, rs1: PhysReg, imm: i32) {
        let inst: u32 = ((imm as u32 & 0xfff) << 20)
            | ((rs1 as u32) << 15)
            | ((funct3 as u32) << 12)
            | ((rd as u32) << 7)
            | (opcode as u32);
        self.code.extend_from_slice(&inst.to_le_bytes());
    }

    fn emit_s(&mut self, opcode: u8, imm_4_0: u16, funct3: u8, rs1: PhysReg, rs2: PhysReg, imm_11_5: u8) {
        let inst: u32 = ((imm_11_5 as u32) << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((funct3 as u32) << 12)
            | ((imm_4_0 as u32) << 7)
            | (opcode as u32);
        self.code.extend_from_slice(&inst.to_le_bytes());
    }

    fn emit_b(&mut self, opcode: u8, target: u32, funct3: u8, rs1: PhysReg, rs2: PhysReg) {
        // For now: placeholder branch emission
        // TODO: resolve label offsets
        let _ = target;
        let _ = funct3;
        let _ = rs1;
        let _ = rs2;
        let _ = opcode;
        todo!("branch emission needs label resolution")
    }

    fn emit_j(&mut self, target: u32) {
        // For now: placeholder jump
        let _ = target;
        todo!("jump emission needs label resolution")
    }

    fn emit_frame_setup(&mut self, _spill_slots: u32) {
        // TODO: proper prologue
        let _ = _spill_slots;
    }

    fn emit_frame_teardown(&mut self, _spill_slots: u32) {
        // TODO: proper epilogue
        let _ = _spill_slots;
    }

    fn emit_call(&mut self, target: SymbolRef) {
        // TODO: call sequence
        let _ = target;
    }

    fn emit_ret(&mut self) {
        // jalr x0, 0(ra)
        self.emit_i(0b1100111, 0, 0b000, 1, 0);
    }

    pub fn finish(self) -> Vec<u8> {
        self.code
    }
}

use crate::vinst::SymbolRef;
```

## Notes

- Mechanical translation from PhysInst to RISC-V encoding
- Uses encoding functions from rv32/emit.rs
- Frame operations expand to multiple instructions
- Branches/jumps need label resolution (todo!())

## Validate

```bash
cargo check -p lpvm-native --lib
```
