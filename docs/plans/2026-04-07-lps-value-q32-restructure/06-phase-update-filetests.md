# Phase 6: Update lps-filetests

## Scope

Update `lps-filetests` to use `LpsValueQ32` throughout the Q32 execution path.

## Files to Update

### q32_exec_common.rs

Change imports and trait (as landed: `CallError` / `GlslReturn` from `lpvm`, not a separate `abi` module name):
```rust
use lps_shared::LpsValueQ32;
use lpvm::{CallError, GlslReturn, LpsValueF32};
// (filetests may import CallError/GlslReturn via `lpvm_cranelift` re-exports instead)

pub(crate) trait Q32ShaderExecutable {
    fn call_q32_ret(
        &mut self,
        name: &str,
        args: &[LpsValueF32],
    ) -> Result<GlslReturn<LpsValueQ32>, GlslError>;
}
```

Update helper functions to work with Q32 values.

### lpir_jit_executable.rs and lpir_rv32_executable.rs

Update `call_q32_ret` implementations to use new types.

## Validate

```bash
cargo check -p lps-filetests
```
