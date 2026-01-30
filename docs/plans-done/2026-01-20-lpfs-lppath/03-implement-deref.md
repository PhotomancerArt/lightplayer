# Phase 3: Implement Deref for LpPathBuf

## Description

Implement `Deref<Target = LpPath>` for `LpPathBuf` so it can automatically use all `LpPath` methods. This matches Rust's `PathBuf` → `Path` relationship.

## Implementation

1. Update `lp-model/src/path.rs`:
   - Add `impl Deref for LpPathBuf`:
     ```rust
     impl Deref for LpPathBuf {
         type Target = LpPath;
         fn deref(&self) -> &LpPath {
             LpPath::new(&self.0)
         }
     }
     ```
   - Add `as_path()` method to `LpPathBuf` that returns `&LpPath` (convenience method)

2. Update tests to verify `Deref` works:
   - `LpPathBuf` can call `LpPath` methods directly
   - `&LpPathBuf` can be used where `&LpPath` is expected

## Success Criteria

- `Deref` implementation compiles
- `LpPathBuf` can use `LpPath` methods (e.g., `buf.is_absolute()`)
- `&LpPathBuf` can be coerced to `&LpPath`
- Code compiles without errors
- Tests pass
