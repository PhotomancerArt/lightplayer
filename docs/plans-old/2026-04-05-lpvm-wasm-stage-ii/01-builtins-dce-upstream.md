# Phase 1: Move `ensure_builtins_referenced()` into `lps-builtins`

## Scope

Move the auto-generated DCE prevention from `lps-builtins-wasm` (now in
`legacy/`) into `lps-builtins` itself, so any consumer can call it.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation

### 1. Copy `builtin_refs.rs` into `lps-builtins`

Copy `lps-builtins-wasm/src/builtin_refs.rs` to `lps-builtins/src/builtin_refs.rs`.

The file contains auto-generated `use` statements for every builtin function
and an `ensure_builtins_referenced()` function that creates volatile reads of
each function pointer to prevent dead-code elimination.

Adjust the `use` paths: in `lps-builtins-wasm` they are
`use lps_builtins::builtins::glsl::sin_q32::__lps_sin_q32;` — inside
`lps-builtins` itself these become `use crate::builtins::glsl::sin_q32::__lps_sin_q32;`.

### 2. Expose from `lps-builtins/src/lib.rs`

Add `pub mod builtin_refs;` to `lps-builtins/src/lib.rs` and re-export:

```rust
pub mod builtin_refs;
pub use builtin_refs::ensure_builtins_referenced;
```

### 3. Update `legacy/lps-builtins-wasm`

Replace its `builtin_refs.rs` content to delegate:

```rust
pub fn ensure_builtins_referenced() {
    lps_builtins::ensure_builtins_referenced();
}
```

### 4. Update `lps-builtins-gen-app`

The codegen tool that generates `builtin_refs.rs` should be updated to
target `lps-builtins/src/builtin_refs.rs` as the primary output location,
with `crate::` prefixed paths. This is a stretch goal for this phase — if
the codegen change is complex, a manual copy with a TODO is acceptable.

## Validate

```bash
cargo check -p lps-builtins
cargo check -p lps-builtins-wasm --target wasm32-unknown-unknown
cargo test -p lps-builtins
```
