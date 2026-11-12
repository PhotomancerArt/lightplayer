# Phase 6: Refactor lp-core-cli to Use lp-server

## Goal

Refactor `lp-core-cli` to use `lp-server` library internally while maintaining all current functionality.

## Tasks

1. **Update lp-core-cli dependencies**:
   - Add `lp-server` dependency to `lp-core-cli/Cargo.toml`
   - Keep existing dependencies

2. **Refactor main.rs**:
   - Instead of creating `LpApp` directly, use `ProjectManager` from `lp-server`
   - Use `ProjectManager::create_project()` when `--create` flag is used
   - Use `ProjectManager::load_project()` to load existing project
   - Access `LpApp` through `Project` wrapper

3. **Maintain current functionality**:
   - Keep GUI (egui) working
   - Keep file watching working
   - Keep all existing features
   - External behavior should be unchanged

4. **Test thoroughly**:
   - Test `--create` flag
   - Test loading existing project
   - Test file watching
   - Test GUI functionality
   - Verify everything works as before

## Success Criteria

- `lp-core-cli` uses `lp-server` library
- All current functionality preserved
- GUI works
- File watching works
- `--create` flag works
- Loading projects works
- No regressions in behavior
- Code compiles without warnings
