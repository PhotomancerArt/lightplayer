## Scope of Phase

Implement sret emission in `VInst::Ret` - store to buffer instead of register moves when return is classified as Sret.

## Implementation Details

### 1. Update `emit_vinst` signature and Ret handling

```rust
pub fn emit_vinst(
    &mut self, 
    inst: &VInst, 
    alloc: &Allocation,
    abi_info: &AbiInfo,  // NEW
) -> Result<(), NativeError> {
    self.current_src_op = inst.src_op();
    match inst {
        // ... other cases unchanged ...
        
        VInst::Ret { vals, .. } => {
            match &abi_info.return_class {
                ReturnClass::Direct { regs } => {
                    // Existing behavior: move to a0-a3
                    for (i, v) in vals.iter().enumerate() {
                        if i >= regs.len() {
                            return Err(NativeError::TooManyReturns(vals.len()));
                        }
                        let src = self.use_vreg(alloc, *v, Self::TEMP0)? as u32;
                        let dst = regs[i] as u32;
                        if src != dst {
                            self.push_u32(encode_addi(dst, src, 0));
                        }
                    }
                }
                ReturnClass::Sret { ptr_reg } => {
                    // NEW: Store values to sret buffer at a0-relative offsets
                    for (i, v) in vals.iter().enumerate() {
                        let offset = (i * 4) as i32; // 4 bytes per scalar
                        let src = self.use_vreg(alloc, *v, Self::TEMP0)? as u32;
                        let base = *ptr_reg as u32; // a0 contains sret pointer
                        self.push_u32(encode_sw(src, base, offset));
                    }
                }
            }
        }
        
        // ... rest unchanged ...
    }
    self.current_src_op = None;
    Ok(())
}
```

### 2. Update epilogue for sret

For sret functions, the callee returns the pointer in a0 (standard ABI). But since we're storing to a0 as base, we don't need to return anything special - just normal return.

Actually, for sret the return "value" is the pointer, but the callee just returns normally since the caller already has the pointer. No special epilogue needed.

```rust
pub fn emit_epilogue(&mut self, abi_info: &AbiInfo) {
    let sp = SP as u32;
    let s0 = S0 as u32;
    let ra = RA as u32;
    
    // Restore s0
    self.push_u32(encode_lw(s0, sp, self.frame.s0_save_offset));
    
    // Restore ra (if non-leaf)
    if self.frame.saved_ra {
        self.push_u32(encode_lw(ra, sp, self.frame.ra_save_offset));
    }
    
    // Adjust SP back
    self.push_u32(encode_addi(sp, sp, self.frame.size as i32));
    
    // Return (for sret, a0 still has the buffer pointer the caller passed)
    self.push_u32(encode_ret());
}
```

## Tests to Write

```rust
#[test]
fn emit_mat4_sret_stores_to_buffer() {
    use lps_shared::{LpsType, LpsFnSig};
    
    // Create function returning mat4 (16 scalars)
    let func = IrFunction {
        name: String::from("test_mat4"),
        is_entry: true,
        vmctx_vreg: VReg(0),
        param_count: 0,
        return_types: vec![IrType::F32; 16], // 16 floats for mat4
        vreg_types: vec![/* ... */],
        // ... create vinsts that return 16 values ...
    };
    
    let sig = LpsFnSig {
        name: String::from("test_mat4"),
        return_type: LpsType::Mat4,
        parameters: vec![],
    };
    
    let emitted = emit_function_bytes(&func, &sig, FloatMode::Q32, false).expect("emit");
    
    // Disassemble and verify stores to a0-relative addresses
    let asm = disassemble(&emitted.code);
    assert!(asm.contains("sw")); // Stores present
    assert!(asm.contains("a0"));  // Using a0 as base
}
```

## Validate

```bash
cargo test -p lpvm-native emit::tests
cargo test -p lpvm-native --lib
```
