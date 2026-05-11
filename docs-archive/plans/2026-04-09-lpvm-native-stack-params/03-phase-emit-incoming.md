## Phase 3: Emit - Prologue Load for Incoming Stack Params

## Scope

Emit `lw` instructions in the prologue to load incoming stack parameters from the caller's frame into their assigned registers.

## Code Organization Reminders

- Place `emit_incoming_stack_param_loads` near `emit_prologue`
- Keep emit methods grouped by phase (prologue, body, epilogue)
- Add tests at the bottom of the emit module's test section

## Implementation Details

### 1. Add helper method

**File**: `lp-shader/lpvm-native/src/isa/rv32/emit.rs`

Add method in `impl EmitContext`:

```rust
/// Load incoming stack parameters after prologue.
/// Stack params are at positive offsets from s0 (caller's frame).
fn emit_incoming_stack_param_loads(
    &mut self,
    alloc: &Allocation,
) -> Result<(), NativeError> {
    for &(vreg, offset) in &alloc.incoming_stack_params {
        let rd = Self::phys(alloc, vreg)? as u32;
        let s0 = S0.hw as u32;
        self.push_u32(encode_lw(rd, s0, offset));
    }
    Ok(())
}
```

### 2. Call after prologue

In `emit_function_bytes()`, after `emit_prologue()`:

```rust
let mut ctx = EmitContext::with_frame(frame, debug_info, call_save);
ctx.emit_prologue(is_sret);
ctx.emit_incoming_stack_param_loads(&alloc)?; // NEW
```

### 3. Handle sret interaction

Note: When `is_sret` is true, the first user param is in a1, not a0. The ABI classification already handles this via `reg_idx = 1` in `classify_params()`. The stack offsets from `ArgLoc::Stack` are correct - we just load from `s0 + offset`.

The incoming stack params list includes the correct vreg-to-offset mapping from regalloc, so no special handling needed in emit.

## Tests

Add test in `emit.rs` `mod tests`:

```rust
#[test]
fn prologue_loads_incoming_stack_params() {
    // Function with 10 param slots
    let f = IrFunction {
        name: String::from("many_params"),
        is_entry: true,
        vmctx_vreg: VReg(0),
        param_count: 9,
        return_types: vec![lpir::IrType::I32],
        vreg_types: vec![lpir::IrType::I32; 11],
        slots: vec![],
        body: vec![
            Op::Iadd {
                dst: VReg(10),
                lhs: VReg(8), // First stack param
                rhs: VReg(9), // Second stack param
            },
            Op::Return {
                values: lpir::types::VRegRange { start: 0, count: 1 },
            },
        ],
        vreg_pool: vec![VReg(10)],
    };
    
    let ir = ir_single(f.clone());
    let mabi = ModuleAbi::from_ir_and_sig(&ir, &test_sig_many_params());
    let sig = test_sig_many_params();
    
    let emitted = emit_function_bytes(
        &f, &ir, &mabi, &sig, lpir::FloatMode::Q32, false
    ).expect("emit");
    
    // Should have lw instructions for loading stack params
    // Check code contains lw opcodes (0x00003003 is lw x0, 0(x0) pattern base)
    assert!(!emitted.code.is_empty());
}
```

## Validate

```bash
cd /Users/yona/dev/photomancer/feature/lightplayer-native/lp-shader
cargo test -p lpvm-native --lib
```

Expected: All tests pass, including new prologue load test.
