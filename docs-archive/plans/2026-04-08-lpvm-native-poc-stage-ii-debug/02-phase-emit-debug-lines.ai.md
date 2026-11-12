# Phase 2: EmitContext Debug Line Recording

## Scope

Update `EmitContext` to record `(offset, src_op)` pairs during instruction emission when debug tracking is enabled.

## Code Organization Reminders

- Add `debug_info: bool` to `NativeCompileOptions` (separate from this phase, will be done)
- Store `debug_lines: Vec<(u32, Option<u32>)>` in `EmitContext`
- Record on every `push_u32()` call
- Keep overhead minimal when disabled

## Implementation Details

### Update `engine.rs` - Add debug_info option

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeCompileOptions {
    pub float_mode: lpir::FloatMode,
    pub debug_info: bool,  // NEW
}

impl Default for NativeCompileOptions {
    fn default() -> Self {
        Self {
            float_mode: lpir::FloatMode::Q32,
            debug_info: false,  // Disabled by default
        }
    }
}
```

### Update `isa/rv32/emit.rs` - Add debug tracking

Add field to `EmitContext`:

```rust
pub struct EmitContext {
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    frame_size: i32,
    is_leaf: bool,
    // NEW: Debug tracking
    current_src_op: Option<u32>,  // Tracks src_op of current VInst
    pub debug_lines: Vec<(u32, Option<u32>)>,  // (offset, src_op)
}
```

Update constructor:

```rust
pub fn new(is_leaf: bool) -> Self {
    Self {
        code: Vec::new(),
        relocs: Vec::new(),
        frame_size: 16,
        is_leaf,
        current_src_op: None,
        debug_lines: Vec::new(),
    }
}
```

Update `push_u32()` to record debug info:

```rust
fn push_u32(&mut self, w: u32) {
    let offset = self.code.len() as u32;
    self.code.extend_from_slice(&w.to_le_bytes());
    // Record debug line if we have a source op
    if self.current_src_op.is_some() {
        self.debug_lines.push((offset, self.current_src_op));
    }
}
```

### Update `emit_vinst()` to set current_src_op

```rust
pub fn emit_vinst(&mut self, inst: &VInst, alloc: &Allocation) -> Result<(), NativeError> {
    // Set the current source op from the VInst
    self.current_src_op = inst.src_op();
    
    match inst {
        VInst::Add32 { dst, src1, src2, src_op: _ } => {
            let rd = Self::phys(alloc, *dst)? as u32;
            let rs1 = Self::phys(alloc, *src1)? as u32;
            let rs2 = Self::phys(alloc, *src2)? as u32;
            self.push_u32(encode_add(rd, rs1, rs2));
        }
        // ... other cases
    }
    
    // Clear current_src_op after emitting (optional, but cleaner)
    self.current_src_op = None;
    Ok(())
}
```

Note: Multi-instruction sequences (like `iconst32_sequence`) will all record the same `src_op`, which is correct - they all came from the same LPIR operation.

## Tests

```rust
#[test]
fn emit_tracks_debug_lines() {
    let mut ctx = EmitContext::new(true);
    ctx.current_src_op = Some(5);
    ctx.push_u32(0x12345678);
    ctx.push_u32(0x9abcdef0);
    
    assert_eq!(ctx.code.len(), 8);
    assert_eq!(ctx.debug_lines.len(), 2);
    assert_eq!(ctx.debug_lines[0], (0, Some(5)));
    assert_eq!(ctx.debug_lines[1], (4, Some(5)));
}

#[test]
fn emit_vinst_records_src_op() {
    use crate::regalloc::GreedyAlloc;
    
    let inst = VInst::Add32 {
        dst: VReg(1),
        src1: VReg(2),
        src2: VReg(3),
        src_op: Some(7),
    };
    
    let mut ctx = EmitContext::new(true);
    let alloc = Allocation {
        vreg_to_phys: vec![None, Some(PhysReg(10)), Some(PhysReg(11)), Some(PhysReg(12))],
    };
    
    ctx.emit_vinst(&inst, &alloc).expect("emit");
    
    // Should have recorded (0, Some(7))
    assert_eq!(ctx.debug_lines.len(), 1);
    assert_eq!(ctx.debug_lines[0], (0, Some(7)));
}
```

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib emit_debug_lines
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```
