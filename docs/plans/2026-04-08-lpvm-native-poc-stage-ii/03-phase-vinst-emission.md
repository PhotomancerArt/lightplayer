# Phase 3: VInst to Machine Code Emission

## Scope

Implement `emit_vinst()` that maps each `VInst` to the corresponding RV32 instructions using allocated physical registers. Handles: Add32, Load, Store, Call, Ret.

## Code Organization

- `emit_vinst()` match statement first
- Helper methods for each variant
- Tests at bottom

## Implementation Details

```rust
use crate::isa::rv32::inst::*;
use crate::regalloc::Allocation;
use crate::vinst::VInst;
use crate::isa::rv32::abi::{self, PReg};

impl EmitContext {
    /// Emit machine code for a single VInst using allocated registers.
    /// 
    /// # Arguments
    /// - `vinst`: The virtual instruction to emit
    /// - `alloc`: Register allocation for this instruction's vregs
    pub fn emit_vinst(&mut self, vinst: &VInst, alloc: &Allocation) {
        match vinst {
            VInst::Add32 { dst, src1, src2 } => {
                let rd = alloc.map[*dst as usize] as u32;
                let rs1 = alloc.map[*src1 as usize] as u32;
                let rs2 = alloc.map[*src2 as usize] as u32;
                let insn = encode_add(rd, rs1, rs2);
                self.push32(insn);
            }
            
            VInst::Mul32 { dst, src1, src2 } => {
                // M extension: mul is R-type with funct7=1
                let rd = alloc.map[*dst as usize] as u32;
                let rs1 = alloc.map[*src1 as usize] as u32;
                let rs2 = alloc.map[*src2 as usize] as u32;
                let insn = encode_mul(rd, rs1, rs2); // funct7=0x01, funct3=0
                self.push32(insn);
            }
            
            VInst::Const32 { dst, value } => {
                let rd = alloc.map[*dst as usize] as u32;
                // Load 32-bit immediate: lui + addi sequence
                let upper = (*value >> 12) as i32;
                let lower = (*value & 0xFFF) as i32;
                // Handle sign of lower 12 bits
                let (lui_imm, addi_imm) = if lower >= 0x800 {
                    (upper + 1, lower - 0x1000)
                } else {
                    (upper, lower)
                };
                self.push32(encode_lui(rd, lui_imm));
                if addi_imm != 0 {
                    self.push32(encode_addi(rd, rd, addi_imm));
                }
            }
            
            VInst::Load { dst, base, offset } => {
                let rd = alloc.map[*dst as usize] as u32;
                let rs1 = alloc.map[*base as usize] as u32;
                let insn = encode_lw(rd, rs1, *offset);
                self.push32(insn);
            }
            
            VInst::Store { src, base, offset } => {
                let rs2 = alloc.map[*src as usize] as u32;
                let rs1 = alloc.map[*base as usize] as u32;
                let insn = encode_sw(rs2, rs1, *offset);
                self.push32(insn);
            }
            
            VInst::Call { symbol, returns } => {
                // auipc+jalr sequence with relocation
                self.emit_call(symbol);
                // If returns is false, we don't expect a return value
                // (handled in ABI contract, ra is clobbered)
            }
            
            VInst::Ret { val } => {
                if let Some(vreg) = val {
                    // Move return value to a0 (x10)
                    let rs = alloc.map[*vreg as usize] as u32;
                    let a0 = abi::A0 as u32;
                    if rs != a0 {
                        // addi a0, rs, 0 (mv pseudoinstruction)
                        self.push32(encode_addi(a0, rs, 0));
                    }
                }
                // Fall through to epilogue (caller emits ret)
            }
            
            VInst::Nop => {
                // addi x0, x0, 0 (nop)
                self.push32(encode_addi(0, 0, 0));
            }
            
            _ => {
                // TODO: Other VInst variants for future milestones
                unimplemented!("VInst variant not yet supported: {:?}", vinst);
            }
        }
    }
}

/// M extension multiply
fn encode_mul(rd: u32, rs1: u32, rs2: u32) -> u32 {
    // mul: funct7=0x01, funct3=0x000
    encode_r_type(0b0110011, rd, 0b000, rs1, rs2, 0b0000001)
}

/// LUI instruction
fn encode_lui(rd: u32, imm: i32) -> u32 {
    encode_u_type(0b0110111, rd, imm << 12)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::*;
    use crate::regalloc::greedy::GreedyAlloc;
    
    #[test]
    fn test_emit_add32() {
        let mut ctx = EmitContext::new(/*is_leaf=*/ true);
        let alloc = Allocation {
            map: vec![8, 9, 10], // v0->x8, v1->x9, v2->x10
        };
        
        // v2 = v0 + v1
        ctx.emit_vinst(&VInst::Add32 { dst: 2, src1: 0, src2: 1 }, &alloc);
        
        assert_eq!(ctx.code.len(), 4); // One 32-bit instruction
        // Decode and verify: add x10, x8, x9
        let encoded = u32::from_le_bytes([ctx.code[0], ctx.code[1], ctx.code[2], ctx.code[3]]);
        assert_eq!(encoded, encode_add(10, 8, 9));
    }
    
    #[test]
    fn test_emit_const32_small() {
        let mut ctx = EmitContext::new(/*is_leaf=*/ true);
        let alloc = Allocation { map: vec![8] };
        
        // v0 = 0x123 (fits in 12 bits, no lui needed)
        ctx.emit_vinst(&VInst::Const32 { dst: 0, value: 0x123 }, &alloc);
        
        // addi x8, x0, 0x123
        assert_eq!(ctx.code.len(), 4);
    }
    
    #[test]
    fn test_emit_const32_large() {
        let mut ctx = EmitContext::new(/*is_leaf=*/ true);
        let alloc = Allocation { map: vec![8] };
        
        // v0 = 0x12345 (needs lui + addi)
        ctx.emit_vinst(&VInst::Const32 { dst: 0, value: 0x12345 }, &alloc);
        
        // lui + addi = 8 bytes
        assert_eq!(ctx.code.len(), 8);
    }
    
    #[test]
    fn test_emit_call_with_reloc() {
        let mut ctx = EmitContext::new(/*is_leaf=*/ false);
        let alloc = Allocation { map: vec![] };
        
        ctx.emit_vinst(&VInst::Call { symbol: "__lpir_fadd_q32".into(), returns: true }, &alloc);
        
        assert_eq!(ctx.code.len(), 8); // auipc + jalr
        assert_eq!(ctx.relocs.len(), 1);
        assert_eq!(ctx.relocs[0].symbol, "__lpir_fadd_q32");
    }
}
```

## Key Points

- `Allocation::map[vreg_idx]` gives the physical register (x8-x31)
- Constants: `lui + addi` sequence for 32-bit immediates
- Calls: Uses `emit_call()` from Phase 2 (records relocation)
- Ret: Moves value to a0 if present (ABI return register)

## Tests to Write

1. `test_emit_add32` — Simple ALU with allocated registers
2. `test_emit_const32_small` — 12-bit immediate (addi only)
3. `test_emit_const32_large` — 32-bit immediate (lui+addi)
4. `test_emit_call_with_reloc` — Records CallPlt reloc
5. `test_emit_ret_value` — Moves result to a0

## Validate

```bash
cargo test -p lpvm-native --lib emit::tests
cargo check -p lpvm-native
```
