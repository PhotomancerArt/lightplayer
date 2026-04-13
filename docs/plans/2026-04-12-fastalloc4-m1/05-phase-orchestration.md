# Phase 5: Update Orchestration

## Scope

Update `emit.rs` (the orchestration layer) and `fa_alloc/mod.rs` to wire
the new types together. Fix compilation errors from the deletions.

## Files to Update

1. `fa_alloc/mod.rs` — clean up imports, wire types
2. `emit.rs` — call new allocator (stub) and new emitter
3. `rv32/mod.rs` — remove PInst re-exports, add emit module
4. `lib.rs` — update public exports if needed

## Code Organization

Keep changes minimal — just enough to get `cargo check` passing.

## Implementation

### 1. Update fa_alloc/mod.rs

Clean up imports at the top:

```rust
// Remove any imports of walk.rs items that no longer exist
// Keep:
pub mod liveness;
pub mod spill;
pub mod trace;
pub mod pool;

// Add:
pub use pool::RegPool;

// Remove any re-exports from deleted walk.rs
```

Update the `AllocResult` and `AllocError` exports:

```rust
// Keep existing AllocResult if it's used by emit.rs
// Or update it to use AllocOutput

/// Result of register allocation.
pub struct AllocResult {
    pub pinsts: Vec<PInst>,  // REMOVE: no longer using PInst
    pub trace: AllocTrace,
    pub spill_slots: u32,
}

// Replace with:
/// Result of register allocation.
pub struct AllocResult {
    pub code: EmittedCode,      // NEW: direct bytes
    pub trace: AllocTrace,
    pub spill_slots: u32,
}
```

Wait — actually for M1, the allocator returns an error, so we don't need to
change `AllocResult` yet. Just make sure the types compile.

### 2. Update emit.rs (orchestration)

Update to call the new allocator (which returns `Err`) and the new emitter:

```rust
use crate::fa_alloc::{allocate, AllocError, AllocOutput};
use crate::rv32::emit::{emit_function, EmittedCode};

pub fn emit_lowered(
    lowered: &LoweredFunction,
    func_abi: &FuncAbi,
) -> Result<EmittedCode, NativeError> {
    // 1. Allocate (currently stubbed, returns Err)
    let alloc_result = allocate(lowered, func_abi)
        .map_err(|e| NativeError::FastAlloc(e))?;

    // 2. Emit — currently unreachable because allocate() returns Err
    // When M2 implements allocate(), this will be reachable
    let frame = FrameLayout::compute(...);  // existing logic

    emit_function(
        &lowered.vinsts,
        &lowered.vreg_pool,
        &alloc_result,
        frame,
        &lowered.symbols,
        func_abi.uses_sret(),
    )
    .map_err(NativeError::FastAlloc)
}
```

Actually, for M1, since `allocate()` always returns `Err`, the emit code
below it is dead. That's fine — it validates the types compile.

### 3. Update rv32/mod.rs

Update module declarations:

```rust
// Remove:
// pub mod inst;
// pub mod rv32_emit;

// Add:
pub mod emit;

// Re-export from emit:
pub use emit::{EmittedCode, NativeReloc, emit_function};
```

### 4. Update lib.rs

Check if any public exports need updating:

```rust
// Check if these need to change:
pub use rv32::inst::PInst;  // REMOVE

// Keep:
pub use rv32::encode::*;
pub use rv32::gpr::*;
```

## Fix Compilation Errors

After the deletions and additions, fix any remaining compilation errors:

1. Remove references to `PInst` in error types if any
2. Update any code that used `rv32::inst::*`
3. Update any code that used `rv32::rv32_emit::*`

## Code Organization Reminders

- Keep imports grouped: std, external crates, crate-local
- Remove unused imports
- Keep the orchestration thin — it just calls allocator then emitter

## Implementation

1. Fix imports in `fa_alloc/mod.rs`
2. Update `emit.rs` to call new allocator and emitter
3. Update `rv32/mod.rs` module declarations
4. Fix any remaining compilation errors
5. Run `cargo check` and fix warnings

## Validation

```bash
cargo check -p lpvm-native-fa 2>&1
```

Expected: Clean check, no errors. Warnings about dead code (the stub
allocator) are acceptable.

```bash
cargo check -p lpvm-native-fa --lib 2>&1 | grep -E "(error|warning)" | head -20
```

Should show no errors. Any warnings should be reviewed.

## Temporary Code

- `allocate()` returning `Err(NotImplemented)` — will be implemented in M2
- Dead code in `emit_function` that's unreachable due to allocator error —
  will become reachable in M2

These are expected and documented.
