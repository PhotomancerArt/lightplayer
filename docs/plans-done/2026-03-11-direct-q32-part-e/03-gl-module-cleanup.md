# Phase 3: Remove Transform Methods from gl_module.rs

## Goal

Strip `gl_module.rs` of all transform-related code. After this, `GlModule`
has no knowledge of the transform system.

## Code to remove

### Imports (line 5)
```rust
// REMOVE:
use crate::backend::transform::pipeline::{Transform, TransformContext};
```

### Free function (lines 20–75)
Remove `transform_single_function` entirely. It was used by the old
streaming path; the new streaming path doesn't call it.

### `impl<M: Module> GlModule<M>` methods (lines 393–502)
Remove `apply_transform_impl`. This is the shared helper that both
JIT and Object `apply_transform` called.

### `impl GlModule<JITModule>` method (lines 525–563)
Remove `apply_transform` for JITModule.

### `impl GlModule<ObjectModule>` method (lines 595–633)
Remove `apply_transform` for ObjectModule (cfg(feature = "emulator")).

## Resulting state

`gl_module.rs` retains:
- `GlModule<M>` struct and core methods (new, declare, define, build, etc.)
- `build_executable` for JIT and Object
- `into_module`, `module_internal`, `module_mut_internal`
- `compile_function_and_extract_codegen` (emulator)
- Tests for GlModule (non-transform tests)

## Tests in gl_module.rs

The file contains tests that use `Q32Transform` and `apply_transform`.
These tests should be removed as part of the transform deletion.
Specifically look for `test_transform_single_function` and any test
that creates a `Q32Transform`.
