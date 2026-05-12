# Phase 2: Delete Old JitModule/jit() API

## Scope

Remove the legacy `jit()` function, `JitModule` struct, and all supporting code
from `lpvm-cranelift`. This is only safe after Phase 1 completes - all consumers
must be migrated to `CraneliftEngine`/`CraneliftModule`.

## Files to Delete/Modify

### Delete: `lp-shader/lpvm-cranelift/src/jit_module.rs`

This entire file becomes obsolete:
- `pub struct JitModule` → replaced by `CraneliftModule`
- `unsafe impl Send for JitModule` → `CraneliftModule` already has this
- `unsafe impl Sync for JitModule` → `CraneliftModule` already has this
- `impl JitModule` methods → moved to `CraneliftModule`

### Modify: `lp-shader/lpvm-cranelift/src/compile.rs`

**Delete:**
```rust
// Remove these functions entirely:
pub fn jit(source: &str, options: &CompileOptions) -> Result<JitModule, CompilerError>
pub fn jit_from_ir(ir: &IrModule, options: &CompileOptions) -> Result<JitModule, CompilerError>
pub fn jit_from_ir_owned(...)
```

The `build_jit_module` internal function can become `build_cranelift_module` or
be absorbed into `CraneliftEngine`.

### Modify: `lp-shader/lpvm-cranelift/src/lib.rs`

**Delete from exports:**
```rust
// Remove these lines:
pub use compile::jit;
pub use jit_module::JitModule;
```

**Update documentation** to remove references to `JitModule` and `jit()`.

### Check: `lp-shader/lpvm-cranelift/src/direct_call.rs`

`DirectCall` is still used by `CraneliftModule`. Keep it but check imports:
```rust
// Was:
use crate::jit_module::JitModule;
// Should be:
use crate::lpvm_module::CraneliftModule;
```

Update `impl JitModule` block to `impl CraneliftModule` if needed.

### Check: `lp-shader/lpvm-cranelift/src/call.rs`

Same as `direct_call.rs` - may reference `JitModule`. Update to `CraneliftModule`.

## Code Organization Reminders

- Delete `jit_module.rs` entirely
- Update imports in `direct_call.rs`, `call.rs` to use `CraneliftModule`
- Run `cargo fix` to catch any missed imports

## Validate

```bash
cargo check -p lpvm-cranelift --lib
cargo test -p lpvm-cranelift --lib

# Verify no consumers left:
rg "jit\(|JitModule" lp-shader/ --glob "*.rs"  # should find nothing (or only in tests/examples)

# Full engine tests:
cargo test -p lp-engine --tests
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

## Phase Notes

- This phase should ONLY touch `lpvm-cranelift`
- If any other crate fails to compile, Phase 1 was incomplete
- `CraneliftModule` must already have all the same methods as `JitModule`
