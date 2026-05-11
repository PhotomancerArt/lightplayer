# Phase 9: Update call sites in lp-engine

## Description

Update all call sites in `lp-engine` crate to use `&LpPath`/`P: AsRef<LpPath>` in function parameters and `LpPathBuf` for storage/returns.

## Implementation

1. Update `lp-engine/src/project/loader.rs`:
   - Update function parameters to use `&LpPath` or `P: AsRef<LpPath>`
   - Update `LpFs` method calls
   - Update `list_dir()` usage to handle `Vec<LpPathBuf>`

2. Update `lp-engine/src/project/runtime.rs`:
   - Update function parameters to use `&LpPath` or `P: AsRef<LpPath>`
   - Update path manipulation code to use `LpPath`/`LpPathBuf` methods
   - Update `list_dir()` usage

3. Update `lp-engine/src/nodes/` (if any files use `LpFs`):
   - Update path-related code

4. Update `lp-engine/tests/`:
   - Update test code to use `LpPath`/`LpPathBuf`

## Success Criteria

- All `lp-engine` call sites updated
- Function parameters use `&LpPath` or `P: AsRef<LpPath>`
- Storage uses `LpPathBuf`
- Code compiles
- Tests pass
