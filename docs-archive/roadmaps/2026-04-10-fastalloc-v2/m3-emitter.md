# M3: Functional Emitter

## Scope of Work

Implement the functional emitter that converts PhysInst to machine code. Pure mechanical encoding - no decisions.

## Files

```
lp-shader/lpvm-native/src/isa/rv32fa/
└── emit.rs                  # NEW: functional emitter
```

## Implementation Details

### 1. Encoding Helpers

Either extract from `rv32/inst.rs` or reimplement:

```rust
//! RISC-V instruction encoding.

use alloc::vec::Vec;
use crate::error::NativeError;

/// Encode R-type instruction.
fn encode_r(
    buf: &mut Vec<u8>,
    opcode: u8,
    rd: u8,
    funct3: u8,
    rs1: u8,
    rs2: u8,
    funct7: u8,
) -> Result<(), NativeError> {
    let inst = ((funct7 as u32) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | (opcode as u32);
    buf.extend_from_slice(&inst.to_le_bytes());
    Ok(())
}

/// Encode I-type instruction.
fn encode_i(
    buf: &mut Vec<u8>,
    opcode: u8,
    rd: u8,
    funct3: u8,
    rs1: u8,
    imm: i32,
) -> Result<(), NativeError> {
    let imm = imm as u32;
    let inst = ((imm & 0xFFF) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | (opcode as u32);
    buf.extend_from_slice(&inst.to_le_bytes());
    Ok(())
}

/// Encode S-type instruction.
fn encode_s(
    buf: &mut Vec<u8>,
    opcode: u8,
    imm: i32,
    funct3: u8,
    rs1: u8,
    rs2: u8,
) -> Result<(), NativeError> {
    let imm = imm as u32;
    let inst = (((imm >> 5) & 0x7F) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((imm & 0x1F) << 7)
        | (opcode as u32);
    buf.extend_from_slice(&inst.to_le_bytes());
    Ok(())
}

/// Generate iconst32 sequence (lui + addi if needed).
fn iconst32_sequence(buf: &mut Vec<u8>, dst: u8, val: i32) -> Result<(), NativeError> {
    if val >= -2048 && val < 2048 {
        // Fits in 12-bit signed immediate: addi dst, x0, val
        encode_i(buf, 0x13, dst, 0, 0, val)?;
    } else {
        // Need lui + addi
        let upper = (val as i32 + 0x800) >> 12;
        let lower = val - (upper << 12);
        encode_u(buf, 0x37, dst, upper)?;  // lui dst, upper
        encode_i(buf, 0x13, dst, 0, dst, lower)?;  // addi dst, dst, lower
    }
    Ok(())
}

fn encode_u(buf: &mut Vec<u8>, opcode: u8, rd: u8, imm: i32) -> Result<(), NativeError> {
    let inst = (((imm as u32) & 0xFFFFF) << 12) | ((rd as u32) << 7) | (opcode as u32);
    buf.extend_from_slice(&inst.to_le_bytes());
    Ok(())
}
```

### 2. Main Emitter

```rust
//! Functional emitter: PhysInst[] -> machine code bytes.

use alloc::vec::Vec;
use crate::error::NativeError;
use crate::isa::rv32fa::inst::{PhysInst, PhysReg};
use crate::isa::rv32fa::abi::{SP_REG, FP_REG, RA_REG};

/// Emit PhysInst sequence to bytes.
pub fn emit(physinsts: &[PhysInst]) -> Result<Vec<u8>, NativeError> {
    let mut buf = Vec::new();
    for inst in physinsts {
        emit_inst(&mut buf, inst)?;
    }
    Ok(buf)
}

fn emit_inst(buf: &mut Vec<u8>, inst: &PhysInst) -> Result<(), NativeError> {
    match inst {
        PhysInst::FrameSetup { spill_slots } => {
            let frame_size = 16 + spill_slots * 4;  // ra, fp + spills
            // addi sp, sp, -frame_size
            encode_i(buf, 0x13, SP_REG, 0, SP_REG, -(frame_size as i32))?;
            // sw ra, [sp+frame_size-4]
            encode_s(buf, 0x23, (frame_size - 4) as i32, 0x02, SP_REG, RA_REG)?;
            // sw fp, [sp+frame_size-8]
            encode_s(buf, 0x23, (frame_size - 8) as i32, 0x02, SP_REG, FP_REG)?;
            // addi fp, sp, frame_size
            encode_i(buf, 0x13, FP_REG, 0, SP_REG, frame_size as i32)?;
        }

        PhysInst::FrameTeardown { spill_slots } => {
            let frame_size = 16 + spill_slots * 4;
            // lw ra, [fp-4]
            encode_i(buf, 0x03, RA_REG, 0x02, FP_REG, -4)?;
            // lw fp, [fp-8]
            encode_i(buf, 0x03, FP_REG, 0x02, FP_REG, -8)?;
            // addi sp, fp, -frame_size
            encode_i(buf, 0x13, SP_REG, 0, FP_REG, -(frame_size as i32))?;
            // ret (jalr x0, ra, 0)
            encode_i(buf, 0x67, 0, 0x00, RA_REG, 0)?;
        }

        PhysInst::LoadImm { dst, val } => {
            iconst32_sequence(buf, *dst, *val)?;
        }

        PhysInst::Add32 { dst, src1, src2 } => {
            encode_r(buf, 0x33, *dst, 0, *src1, *src2, 0x00)?;
        }

        PhysInst::Sub32 { dst, src1, src2 } => {
            encode_r(buf, 0x33, *dst, 0, *src1, *src2, 0x20)?;
        }

        PhysInst::Mul32 { dst, src1, src2 } => {
            encode_r(buf, 0x33, *dst, 0, *src1, *src2, 0x01)?;
        }

        // ... all other arithmetic

        PhysInst::Load32 { dst, base, offset } => {
            encode_i(buf, 0x03, *dst, 0x02, *base, *offset)?;
        }

        PhysInst::Store32 { src, base, offset } => {
            encode_s(buf, 0x23, *offset, 0x02, *base, *src)?;
        }

        PhysInst::Mov32 { dst, src } => {
            encode_i(buf, 0x13, *dst, 0, *src, 0)?;  // addi dst, src, 0
        }

        PhysInst::Call { target } => {
            // Simplified: for now, emit placeholder or use auipc+jalr
            // Full implementation needs symbol resolution
            todo!("Call encoding");
        }

        PhysInst::Ret => {
            encode_i(buf, 0x67, 0, 0x00, RA_REG, 0)?;  // jalr x0, ra, 0
        }

        _ => {
            return Err(NativeError::FastallocInternal(
                alloc::format!("Unimplemented PhysInst: {:?}", inst)
            ));
        }
    }
    Ok(())
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_add32() {
        let insts = vec![PhysInst::Add32 { dst: 10, src1: 11, src2: 12 }];
        let bytes = emit(&insts).unwrap();
        // add a0, a1, a2 -> 0x00c58533
        assert_eq!(bytes, vec![0xb3, 0x05, 0xc5, 0x00]);
    }

    #[test]
    fn test_emit_loadimm_small() {
        let insts = vec![PhysInst::LoadImm { dst: 10, val: 42 }];
        let bytes = emit(&insts).unwrap();
        // addi a0, x0, 42 -> 0x02a00513
        assert_eq!(bytes, vec![0x13, 0x05, 0xa0, 0x02]);
    }

    #[test]
    fn test_emit_loadimm_large() {
        let insts = vec![PhysInst::LoadImm { dst: 10, val: 0x12345 }];
        let bytes = emit(&insts).unwrap();
        // Should be lui + addi (8 bytes)
        assert_eq!(bytes.len(), 8);
    }

    #[test]
    fn test_emit_load32() {
        let insts = vec![PhysInst::Load32 { dst: 10, base: 11, offset: 4 }];
        let bytes = emit(&insts).unwrap();
        // lw a0, 4(a1) -> 0x0045a503
        assert_eq!(bytes, vec![0x83, 0xa5, 0x45, 0x00]);
    }
}
```

## Validate

```bash
cd lp-shader/lpvm-native
cargo test -p lpvm-native --lib -- rv32fa::emit
```

All emitter tests should pass.
