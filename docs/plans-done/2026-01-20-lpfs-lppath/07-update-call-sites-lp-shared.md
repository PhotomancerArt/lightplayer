# Phase 7: Update call sites in lp-shared

## Description

Update all call sites in `lp-shared` crate to use `&LpPath`/`P: AsRef<LpPath>` in function parameters and `LpPathBuf` for storage/returns.

## Implementation

1. Search for all `LpFs` method calls in `lp-shared`:
   - Update calls to pass `&LpPath` or `LpPathBuf` (via `AsRef`)
   - Update code that uses `list_dir()` return values to use `LpPathBuf`
   - Update function parameters to use `&LpPath` or `P: AsRef<LpPath>`
   - Update struct fields to use `LpPathBuf` for storage

2. Files to update:
   - `lp-shared/src/project/builder.rs` (if it uses `LpFs`)
   - Any other files in `lp-shared` that call `LpFs` methods

## Success Criteria

- All `lp-shared` call sites updated
- Function parameters use `&LpPath` or `P: AsRef<LpPath>`
- Storage uses `LpPathBuf`
- Code compiles
- Tests pass
