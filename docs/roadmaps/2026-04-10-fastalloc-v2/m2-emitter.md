# M2: Functional Emitter

## Scope of Work

Implement the functional emitter that converts PhysInst to machine code. This is purely mechanical - no decisions, just pattern matching and encoding.

## File

```
lp-shader/lpvm-native/src/isa/rv32fa/
├── emit.rs                  # NEW: functional emitter
```

## Implementation Details

### 1. Extract Encoding Helpers

Copy or extract the encoding functions from `rv32/inst.rs`:

```rust
// encode.rs or inline in emit.rs
fn encode_r(buf: &mut Vec<u8>, opcode: u8, rd: u8, funct3: u8, rs1: u8, rs2: u8, funct7: u8);
fn encode_i(buf: &mut Vec<u8>, opcode: u8, rd: u8, funct3: u8, rs1: u8, imm: i32);
fn encode_s(buf: &mut Vec<u8>, opcode: u8, imm: i32, funct3: u8, rs1: u8, rs2: u8);
fn iconst32_sequence(buf: &mut Vec<u8>, dst: PhysReg, val: i32);  // lui + addi if needed
```

### 2. Implement `emit.rs`

```rust
//! Functional emitter: PhysInst[] -> machine code bytes.

use alloc::vec::Vec;
use crate::error::NativeError;
use crate::isa::rv32fa::inst::{PhysInst, PhysReg};

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
            // addi sp, sp, -frame_size
            // sw ra, [sp+frame_size-4]
            // sw fp, [sp+frame_size-8]
            // addi fp, sp, frame_size
        }

        PhysInst::FrameTeardown { spill_slots } => {
            // lw ra, [fp-4]
            // lw fp, [fp-8]
            // addi sp, fp, -frame_size
            // ret (jalr x0, ra, 0)
        }

        PhysInst::LoadImm { dst, val } => {
            iconst32_sequence(buf, *dst, *val)?;
        }

        PhysInst::Add32 { dst, src1, src2 } => {
            encode_r(buf, 0x33, *dst, 0, *src1, *src2, 0x00)?;
        }

        // ... all other arithmetic

        PhysInst::Load32 { dst, base, offset } => {
            encode_i(buf, 0x03, *dst, 0x02, *base, *offset)?;
        }

        PhysInst::Store32 { src, base, offset } => {
            encode_s(buf, 0x23, *offset, 0x02, *base, *src)?;
        }

        // ... all other variants

        PhysInst::Call { target } => {
            // auipc t1, %pcrel_hi(target)
            // jalr ra, t1, %pcrel_lo(target)
        }

        PhysInst::Ret => {
            encode_i(buf, 0x67, 0, 0x00, 1, 0)?;  // jalr x0, ra, 0
        }
    }
    Ok(())
}
```

### 3. Unit Tests

Test each PhysInst variant encodes to expected bytes:

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
        // Should be lui + addi sequence
        assert!(bytes.len() > 4);
    }
}
```

## Validate

```bash
cd lp-shader/lpvm-native
cargo test -p lpvm-native --lib -- rv32fa::emit
```

All emitter tests should pass.
