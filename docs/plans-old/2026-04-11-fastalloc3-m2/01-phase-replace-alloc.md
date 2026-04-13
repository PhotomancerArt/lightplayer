# Phase 1: Replace `rv32::alloc` with `fa_alloc` in compile pipeline

## Goal

Switch `compile_function`, `emit_vinsts`, and `emit_function_fastalloc_bytes`
from the old `rv32::alloc::allocate` to `fa_alloc::allocate`.

## Changes

### `compile.rs`

Before:
```rust
let func_abi = crate::rv32::abi::func_abi_rv32(fn_sig, func.total_param_slots() as usize);
let pinsts = crate::rv32::alloc::allocate(&lowered.vinsts, &func_abi, func, &lowered.vreg_pool)
    .map_err(NativeError::FastAlloc)?;
```

After:
```rust
let func_abi = crate::rv32::abi::func_abi_rv32(fn_sig, func.total_param_slots() as usize);
let alloc_result = crate::fa_alloc::allocate(&lowered, &func_abi)
    .map_err(NativeError::FastAlloc)?;
let pinsts = alloc_result.pinsts;
```

Note: `lowered` is already available (built at step 1). Previously it was
destructured into `lowered.vinsts` and `lowered.vreg_pool`. Now we pass
the whole `LoweredFunction` reference.

### `emit.rs`

`emit_vinsts` currently calls `rv32::alloc::allocate` with individual fields.
Replace with `fa_alloc::allocate`. This requires building a `LoweredFunction`
inside `emit_vinsts` — but actually this function is a convenience wrapper that
is only used from one place. Check if it's still needed; if so, update it.

### `rv32/mod.rs`

`emit_function_fastalloc_bytes` calls `alloc::allocate` on the old path. This
needs the same conversion. It constructs `lowered` from `lower_ops`, so pass
that directly to `fa_alloc::allocate`.

## Status: [ ]
