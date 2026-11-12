# Phases: Separate Crates Refactor

## Phase 1: Fix Immediate Issues
- Fix typo in `lp-core-cli/src/main.rs` (extra colons in import)
- Move `log/` module from `lp-core` to `lp-core-util`
- Update `lp-core` to use `lp-core-util` for logging
- Ensure everything still compiles

## Phase 2: Clean Up lp-core Cross-Project Concerns
- Remove `create_default_project()` from `LpApp`
- Update `LpApp::load_project()` to fail if project doesn't exist (no auto-creation)
- Clean up any remaining cross-project logic

## Phase 3: Create lp-api Protocol Messages
- Define `ClientMsg` enum (filesystem ops, sync, debug queries)
- Define `ServerMsg` enum (responses, file sync, debug data, logs)
- Add basic serialization helpers
- Add `std` feature support

## Phase 4: Update Tests to Use File-Based Projects
- Fix tests in `nodes/output/runtime.rs` - remove `nodes` field from `ProjectConfig`
- Fix tests in `nodes/texture/runtime.rs` - remove `nodes` field from `ProjectConfig`
- Fix tests in `runtime/contexts.rs` - update `InitContext::new()` calls (check signature)
- Ensure all tests pass

## Phase 5: Create lp-server Library
- Implement `Project` wrapper (wraps `LpApp` with metadata)
- Implement `ProjectManager` (manages multiple projects)
- Implement project creation logic (`create_project()`)
- Add `std` feature support
- Basic project loading/unloading

## Phase 6: Refactor lp-core-cli to Use lp-server
- Update `lp-core-cli` to use `lp-server` library internally
- Keep all current functionality (GUI, file watching, etc.)
- Ensure external behavior unchanged
- Test that everything still works

## Phase 7: Cleanup and Final Testing
- Remove any temporary code or TODOs
- Fix all warnings
- Ensure all tests pass
- Verify `lp-core-cli` works exactly as before
- Update documentation if needed
