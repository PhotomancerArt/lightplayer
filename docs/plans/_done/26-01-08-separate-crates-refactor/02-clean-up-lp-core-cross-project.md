# Phase 2: Clean Up lp-core Cross-Project Concerns

## Goal

Remove cross-project logic from `lp-core`, making it purely single-project focused.

## Tasks

1. **Remove create_default_project() from LpApp**:
   - Remove `create_default_project()` method from `LpApp`
   - This will be moved to `lp-server` in a later phase

2. **Update LpApp::load_project()**:
   - Remove auto-creation logic (the `else` branch that creates default project)
   - `load_project()` should return an error if `/project.json` doesn't exist
   - Update error message to be clear that project must exist

3. **Update lp-core-cli**:
   - Ensure `lp-core-cli` handles the case where project doesn't exist
   - The `--create` flag logic should work with the new behavior
   - May need to create project before calling `load_project()`

4. **Verify behavior**:
   - Test that `load_project()` fails appropriately when project doesn't exist
   - Test that `lp-core-cli --create` still works
   - Ensure existing projects still load correctly

## Success Criteria

- `create_default_project()` removed from `LpApp`
- `load_project()` fails if project doesn't exist (no auto-creation)
- `lp-core-cli --create` still works
- All code compiles without warnings
- Tests pass
