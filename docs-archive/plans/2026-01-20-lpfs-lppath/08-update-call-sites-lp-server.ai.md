# Phase 8: Update call sites in lp-server

## Description

Update all call sites in `lp-server` crate to use `&LpPath`/`P: AsRef<LpPath>` in function parameters and `LpPathBuf` for storage/returns.

## Implementation

1. Update `lp-server/src/handlers.rs`:
   - Update `handle_fs_request()` to use `LpPath`/`LpPathBuf`
   - Update all `LpFs` method calls

2. Update `lp-server/src/project_manager.rs`:
   - Update function parameters to use `&LpPath` or `P: AsRef<LpPath>`
   - Update struct fields to use `LpPathBuf`
   - Update `list_dir()` usage to handle `Vec<LpPathBuf>`

3. Update `lp-server/src/server.rs`:
   - Update function parameters and `LpFs` calls

4. Update `lp-server/src/project.rs`:
   - Update any path-related code

5. Update `lp-server/tests/`:
   - Update test code to use `LpPath`/`LpPathBuf`

## Success Criteria

- All `lp-server` call sites updated
- Function parameters use `&LpPath` or `P: AsRef<LpPath>`
- Storage uses `LpPathBuf`
- Code compiles
- Tests pass
