# Phase 1: Create LpPath slice type with AsRef implementations

## Description

Create the `LpPath` slice type (`#[repr(transparent)] pub struct LpPath(str)`) and implement `AsRef<LpPath>` for `&str`, `String`, `&LpPath`, and `LpPathBuf`. This establishes the foundation for the split.

## Implementation

1. Update `lp-model/src/path.rs`:
   - Add `#[repr(transparent)] pub struct LpPath(str);`
   - Implement `LpPath::new(s: &str) -> &LpPath` using unsafe cast (like Rust's `Path::new()`)
   - Implement `AsRef<LpPath>` for `&str` (via `LpPath::new()`)
   - Implement `AsRef<LpPath>` for `String` (via `as_str()` then `LpPath::new()`)
   - Implement `AsRef<LpPath>` for `&LpPath` (trivial - returns `self`)
   - Implement `AsRef<LpPath>` for `LpPathBuf` (will use `Deref` once implemented)
   - Add `as_str()` method to `LpPath` that returns `&str`

2. Export `LpPath` from `lp-model/src/lib.rs`

## Success Criteria

- `LpPath` type exists and compiles
- `LpPath::new()` works correctly
- `AsRef<LpPath>` implementations compile
- Code compiles without errors
- Basic tests pass (if any)
