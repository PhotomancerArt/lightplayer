# Phase 1: Bypass GlModule::declare_function for Streaming

## Problem

`GlModule::declare_function` creates a placeholder `GlFunc` with `Function::new()`
for every declared function. With two modules (float + Q32) and ~11 functions each,
this adds ~22 KB at peak (21,684 bytes measured) plus two large HashMap rehashes.

The streaming path doesn't need these placeholders — it never looks up functions
from `GlModule.fns`. The float module is only used for `declare_func_in_func`
resolution (which uses the inner `JITModule`'s declarations), and the Q32 module
goes straight from transform to `define_function` on the inner module.

## Fix

In `glsl_jit_streaming`, replace calls to `float_module.declare_function()`
and `q32_module.declare_function()` with direct calls to the inner JITModule:

```rust
// Before:
let float_func_id = float_module
    .declare_function(name, linkage, float_sig.clone())?;

// After:
let float_func_id = float_module
    .module_mut_internal()
    .declare_function(name, linkage, &float_sig)?;
```

Same for the Q32 module:

```rust
let q32_func_id = q32_module
    .module_mut_internal()
    .declare_function(name, linkage, &q32_sig)?;
```

This skips creating `GlFunc` placeholders, avoids `Function::new()` allocations,
and avoids growing the `fns` HashMap.

Note: `module_mut_internal()` is `pub(crate)`, so this works from within the
crate. The cranelift `Module::declare_function` takes `&Signature` (borrowed),
not owned, so no need to clone the signature either.

## Expected savings

~22 KB from eliminating placeholder `GlFunc` entries and HashMap overhead.

## Validate

```bash
cd lp-shader/lp-glsl-compiler && cargo test --features std -- test_streaming
cd lp-shader/lp-glsl-compiler && cargo test --features std
```
