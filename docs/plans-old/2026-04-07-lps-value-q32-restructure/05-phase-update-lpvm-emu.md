# Phase 5: Update lpvm-emu

## Scope

Update `lpvm-emu` to use `LpsValueQ32` instead of the old f64 types. Similar changes to lpvm-cranelift.

## Files to Update

### emu_run.rs

Change `glsl_q32_call_emulated` signature:
```rust
pub fn glsl_q32_call_emulated(
    load: &ElfLoadInfo,
    ir: &IrModule,
    meta: &LpsModuleSig,
    options: &CompileOptions,
    name: &str,
    args: &[LpsValueQ32],  // Changed from LpsValueF64
) -> Result<GlslReturn<LpsValueQ32>, CallError>  // Changed
{
    // Use lpvm::abi::flatten_q32 and unflatten_q32
}
```

### instance.rs

Update `LpvmInstance::call()` similar to cranelift:
```rust
fn call(&mut self, name: &str, args: &[LpsValueF32]) 
    -> Result<LpsValueF32, Self::Error> 
{
    // Convert F32→Q32
    // Call glsl_q32_call_emulated with Q32
    // Convert Q32→F32
}
```

## Validate

```bash
cargo check -p lpvm-emu
cargo test -p lpvm-emu --lib
```
