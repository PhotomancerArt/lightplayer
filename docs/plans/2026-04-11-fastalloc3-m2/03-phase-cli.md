# Phase 3: Update CLI pipeline

## Goal

Update `shader_rv32fa/pipeline.rs` to use `fa_alloc` instead of `rv32::alloc`.

## Changes

### `lp-cli/src/commands/shader_rv32fa/pipeline.rs`

Before:
```rust
use lpvm_native_fa::rv32::alloc;
// ...
let phys = alloc::allocate(&lowered.vinsts, &func_abi, func, &lowered.vreg_pool)
    .map_err(|e| anyhow::anyhow!("fastalloc: {e}"))?;
```

After:
```rust
use lpvm_native_fa::fa_alloc;
// ...
let alloc_result = fa_alloc::allocate(&lowered, &func_abi)
    .map_err(|e| anyhow::anyhow!("fastalloc: {e}"))?;
let phys = alloc_result.pinsts;
```

Also wire up the alloc trace output if verbosity flags request it:
```rust
if alloc_result.trace is non-empty && alloc_trace flag {
    writeln!(debug, "{}", alloc_result.trace.format())?;
}
```

## Status: [ ]
