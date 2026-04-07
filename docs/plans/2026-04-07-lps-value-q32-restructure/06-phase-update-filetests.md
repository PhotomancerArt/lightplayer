# Phase 6: Update lps-filetests

## Scope

Update `lps-filetests` to use `LpsValueQ32` throughout the Q32 execution path.

## Files to Update

### q32_exec_common.rs

Change imports and trait:
```rust
use lpvm::abi::{CallError, GlslReturn, LpsValueQ32};  // Updated

pub(crate) trait Q32ShaderExecutable {
    fn call_q32_ret(
        &mut self,
        name: &str,
        args: &[LpsValueF32],  // Takes F32, converts internally
    ) -> Result<GlslReturn<LpsValueQ32>, GlslError>;  // Returns Q32
}
```

Update helper functions to work with Q32 values.

### lpir_jit_executable.rs and lpir_rv32_executable.rs

Update `call_q32_ret` implementations to use new types.

## Validate

```bash
cargo check -p lps-filetests
```
