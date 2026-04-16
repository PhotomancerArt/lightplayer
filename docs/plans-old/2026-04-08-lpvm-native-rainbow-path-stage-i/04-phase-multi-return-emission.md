## Scope of Phase

Implement multi-value return emission in prologue/epilogue. Handle both Direct (a0-a3) and Sret (pointer in a0) cases.

## Code Organization Reminders

- Update `emit.rs` prologue/epilogue to use FrameLayout
- Add multi-return value movement
- Keep sret setup in caller (for now)

## Implementation Details

### Updated EmitContext

```rust
pub struct EmitContext {
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    pub debug_lines: Vec<(u32, Option<u32>)>,
    frame_layout: FrameLayout,
    is_leaf: bool,
    debug_info: bool,
    current_src_op: Option<u32>,
}

impl EmitContext {
    pub fn new(frame_layout: FrameLayout, is_leaf: bool, debug_info: bool) -> Self {
        Self {
            code: Vec::new(),
            relocs: Vec::new(),
            debug_lines: Vec::new(),
            frame_layout,
            is_leaf,
            debug_info,
            current_src_op: None,
        }
    }
}
```

### Updated prologue

```rust
pub fn emit_prologue(&mut self) {
    let sp = SP as u32;
    let s0 = S0 as u32;
    let ra = RA as u32;
    
    // Adjust SP
    self.push_u32(encode_addi(sp, sp, -(self.frame_layout.total_size as i32)));
    
    // Save s0 (frame pointer)
    self.push_u32(encode_sw(s0, sp, self.frame_layout.s0_save_offset));
    
    // Save ra (if non-leaf)
    if self.frame_layout.saved_ra {
        self.push_u32(encode_sw(ra, sp, self.frame_layout.ra_save_offset));
    }
    
    // Establish frame pointer: s0 = sp
    self.push_u32(encode_addi(s0, sp, 0));
}
```

### Multi-return emission

```rust
pub fn emit_return(&mut self, return_vals: &[VReg], alloc: &Allocation, ret_class: &ReturnClass) {
    match ret_class {
        ReturnClass::Direct { regs } => {
            // Move each return value to its register
            for (i, vreg) in return_vals.iter().enumerate() {
                if i >= regs.len() {
                    break; // Should not happen if classification is correct
                }
                let src_reg = Self::phys(alloc, *vreg)? as u32;
                let dst_reg = regs[i] as u32;
                if src_reg != dst_reg {
                    self.push_u32(encode_addi(dst_reg, src_reg, 0));
                }
            }
        }
        ReturnClass::Sret { ptr_reg } => {
            // Values already stored to sret buffer by lowering
            // Return pointer is already in a0
            // (caller set up the pointer before call)
        }
    }
    
    self.emit_epilogue();
}
```

### Epilogue

```rust
pub fn emit_epilogue(&mut self) {
    let sp = SP as u32;
    let s0 = S0 as u32;
    let ra = RA as u32;
    
    // Restore s0
    self.push_u32(encode_lw(s0, sp, self.frame_layout.s0_save_offset));
    
    // Restore ra (if non-leaf)
    if self.frame_layout.saved_ra {
        self.push_u32(encode_lw(ra, sp, self.frame_layout.ra_save_offset));
    }
    
    // Adjust SP back
    self.push_u32(encode_addi(sp, sp, self.frame_layout.total_size as i32));
    
    // Return
    self.push_u32(encode_ret());
}
```

## Tests to Write

```rust
#[test]
fn emit_vec4_return() {
    let func = func_with_return(vec![IrType::Vec4]);
    let ret_class = ReturnClass::from_types(&func.return_types);
    assert!(matches!(ret_class, ReturnClass::Direct { regs } if regs.len() == 4));
    
    let emitted = emit_function_bytes(&func, FloatMode::Q32, false).expect("emit");
    // Should have moves to a0, a1, a2, a3
    assert!(emitted.code.len() > 0);
}

#[test]
fn emit_mat4_sret_return() {
    let func = func_with_return(vec![IrType::Mat4]);
    let ret_class = ReturnClass::from_types(&func.return_types);
    assert!(matches!(ret_class, ReturnClass::Sret { .. }));
    
    let emitted = emit_function_bytes(&func, FloatMode::Q32, false).expect("emit");
    // Sret: no return register moves, just pointer in a0
}
```

## Validate

```bash
cargo test -p lpvm-native emit::tests::emit_return
cargo test -p lpvm-native --lib
```
