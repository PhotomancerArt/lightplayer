# Phase 1: Move FloatMode to lpir crate

## Scope

Add `FloatMode` enum to the `lpir` crate's `types.rs`. Remove the definition
from `lps-naga`. Update all consumers (`lps-naga`, `lps-wasm`)
to import from `lpir`. Rename `Float` variant to `F32`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Add to `lpir/src/types.rs`

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatMode {
    Q32,
    F32,
}
```

### 2. Re-export from `lpir/src/lib.rs`

```rust
pub use types::{CalleeRef, FloatMode, IrType, SlotId, VReg, VRegRange};
```

### 3. Remove from `lps-naga/src/lib.rs`

Delete the `FloatMode` enum definition. Add a re-export for backward
compatibility:

```rust
pub use lpir::FloatMode;
```

This keeps `lps_naga::FloatMode` working for existing consumers without
code changes (the WASM emitter imports it as `lps_naga::FloatMode`).

### 4. Rename `Float` → `F32`

The old enum had `FloatMode::Float`. Rename to `FloatMode::F32` for
consistency with the rest of the codebase. Update all match arms in:

- `lps-naga/src/` (wherever `FloatMode::Float` appears)
- `lps-wasm/src/emit/imports.rs`
- `lps-wasm/src/emit/mod.rs`
- `lps-wasm/src/options.rs`

Search for `FloatMode::Float` across the workspace to find all occurrences.

### 5. Verify

No behavioral changes — this is a pure type migration.

## Validate

```
cargo check -p lpir
cargo check -p lps-naga
cargo check -p lps-wasm
cargo test -p lpir
cargo test -p lps-naga
cargo test -p lps-wasm
```
