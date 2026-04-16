# Phase 4: Update lpvm-cranelift

## Scope

Update `lpvm-cranelift` to use `LpsValueQ32` instead of `LpsValueF64`. Update `JitModule::call()` and `LpvmInstance` implementation.

## Files to Update

### lib.rs

Change exports:
```rust
// Remove
pub use lps_shared::lps_value_f64::{...}

// Add
pub use lps_shared::{LpsValueQ32, lps_value_to_q32, q32_to_lps_value};
pub use lpvm::abi::{CallError, CallResult, GlslReturn, flatten_q32, unflatten_q32};
```

### call.rs

```rust
// Change signature
pub fn call(&self, name: &str, args: &[LpsValueQ32]) 
    -> CallResult<GlslReturn<LpsValueQ32>> 
{
    // Use lpvm::abi::flatten_q32 and unflatten_q32
    let flat_args: Vec<i32> = args.iter()
        .zip(params.iter())
        .map(|(arg, param)| lpvm::abi::flatten_q32(&param.ty, arg))
        .collect::<Result<Vec<_>, _>>()?;
    
    // ... invoke ...
    
    let value = lpvm::abi::unflatten_q32(&gfn.return_type, &words)?;
    Ok(GlslReturn { value: Some(value), outs: vec![] })
}
```

### lpvm_instance.rs

Update `LpvmInstance::call()` to convert F32→Q32 before call and Q32→F32 after:

```rust
fn call(&mut self, name: &str, args: &[LpsValueF32]) 
    -> Result<LpsValueF32, Self::Error> 
{
    // Convert args to Q32
    let gfn = /* get function sig */;
    let q32_args: Vec<LpsValueQ32> = args.iter()
        .zip(gfn.parameters.iter())
        .map(|(arg, param)| lps_value_to_q32(&param.ty, arg)
            .map_err(|e| CallError::TypeMismatch(e)))  // or proper error
        .collect::<Result<Vec<_>, _>>()?;
    
    // Call with Q32
    let result = self.jit_module.call(name, &q32_args)?;
    
    // Convert return to F32
    match result.value {
        Some(q32_val) => q32_to_lps_value(&gfn.return_type, q32_val)
            .map_err(|e| /* convert error */),
        None => Err(...),  // void return
    }
}
```

### Tests

Update tests that used `LpsValueF64::Float(x)` to use `LpsValueQ32::F32(Q32(...))`.

## Validate

```bash
cargo check -p lpvm-cranelift
cargo test -p lpvm-cranelift --lib
```
