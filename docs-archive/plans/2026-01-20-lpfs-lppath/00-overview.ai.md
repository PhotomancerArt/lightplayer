# Plan: Update LpFs to use LpPath/LpPathBuf split

## Overview

Create `LpPath` slice type and update `LpFs` trait and all implementations to use `P: AsRef<LpPath>` for path parameters, matching Rust's `Path`/`PathBuf` pattern. This eliminates duplicate normalization logic and ensures all paths are normalized consistently.

## Phases

1. Create LpPath slice type with AsRef implementations
2. Move read-only methods from LpPathBuf to LpPath
3. Implement Deref for LpPathBuf
4. Update LpFs trait signatures
5. Remove normalize_path() functions (causes compilation errors)
6. Update LpFs implementations
7. Update call sites in lp-shared
8. Update call sites in lp-server
9. Update call sites in lp-engine
10. Update call sites in lp-client and lp-cli
11. Cleanup and finalization

## Success Criteria

- `LpPath` slice type exists and matches Rust's `Path` pattern
- `LpPathBuf` implements `Deref<Target = LpPath>`
- `AsRef<LpPath>` implemented for `&str`, `String`, `&LpPath`, and `LpPathBuf`
- `LpFs` trait uses `P: AsRef<LpPath>` for all path parameters
- `list_dir()` returns `Vec<LpPathBuf>`
- All `normalize_path()` functions removed
- All call sites updated to use `&LpPath`/`P: AsRef<LpPath>` in parameters and `LpPathBuf` for storage/returns
- Code compiles without errors
- All tests pass
- No warnings
