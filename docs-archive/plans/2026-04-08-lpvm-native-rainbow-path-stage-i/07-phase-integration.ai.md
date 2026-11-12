## Scope of Phase

Integrate all components: return classification, frame layout, multi-return emission, spill support. Verify existing filetests pass.

## Code Organization Reminders

- Update `emit_function_bytes` to use new FrameLayout computation
- Ensure backward compatibility with existing tests
- Add integration tests for multi-return

## Implementation Details

### Updated emit_function_bytes

```rust
pub fn emit_function_bytes(
    func: &lpir::IrFunction,
    float_mode: lpir::FloatMode,
    debug_info: bool,
) -> Result<EmittedFunction, NativeError> {
    // 1. Lower to VInsts
    let vinsts = crate::lower::lower_ops(func, float_mode)?;
    
    // 2. Allocate registers
    let alloc = RegAlloc::allocate(&GreedyAlloc, func, &vinsts)?;
    
    // 3. Compute frame layout with spill count
    let frame_layout = FrameLayout::compute(func, alloc.spill_count());
    
    // 4. Classify return
    let ret_class = ReturnClass::from_types(&func.return_types);
    
    // 5. Emit
    let is_leaf = !vinsts.iter().any(|v| v.is_call());
    let mut ctx = EmitContext::new(frame_layout, is_leaf, debug_info);
    
    ctx.emit_prologue();
    
    for v in &vinsts {
        ctx.emit_vinst(v, &alloc)?;
    }
    
    // Extract return values from vreg_pool
    let return_vals = func.return_vregs_from_pool();
    ctx.emit_return(&return_vals, &alloc, &ret_class)?;
    
    Ok(EmittedFunction {
        code: ctx.code,
        relocs: ctx.relocs,
        debug_lines: ctx.debug_lines,
    })
}
```

### Backward compatibility

Ensure existing filetests still work:
- `op-add.glsl` (single scalar return)
- Other existing `rv32lp.q32` tests

Any test that was working before should continue to work. New features (multi-return, spills) should only activate when needed.

## Tests to Validate

```bash
# Core filetests
cargo test -p lps-filetests rv32lp

# Specific tests
cargo test -p lps-filetests op_add
cargo test -p lps-filetests vec4  # if exists
cargo test -p lps-filetests mat4  # if exists
```

## Integration Tests

Add to `emit.rs` tests:

```rust
#[test]
fn full_pipeline_vec4_return() {
    let func = IrFunction {
        name: "vec4_ret".into(),
        return_types: vec![IrType::Vec4],
        // ... setup ...
    };
    
    let emitted = emit_function_bytes(&func, FloatMode::Q32, false).expect("emit");
    
    // Should compile without error
    assert!(!emitted.code.is_empty());
}

#[test]
fn full_pipeline_with_spills() {
    let func = many_vregs_function(30); // Will spill
    
    let emitted = emit_function_bytes(&func, FloatMode::Q32, false).expect("emit");
    
    // Should handle spills without error
    assert!(!emitted.code.is_empty());
}
```

## Validate

```bash
# All lpvm-native tests
cargo test -p lpvm-native --lib

# All filetests including rv32lp
cargo test -p lps-filetests 2>&1 | head -50

# ESP32 build check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6
```

## Notes

- If existing tests fail, debug before proceeding
- Spill test may not pass yet (needs lowering support for inserting spill insts)
- Goal: infrastructure in place, tests show expected behavior
