# Phase 3: Update ensure_all_nodes_initialized to not fail on Error status

## Description

Modify `ProjectRuntime::ensure_all_nodes_initialized()` to not fail when nodes have `Error` status. Only fail on `InitError` or `Created` status. Nodes with `Error` status are considered successfully initialized - they just have a runtime error (e.g., GLSL compilation failure).

## Changes

### File: `lp-engine/src/project/runtime.rs`

1. **Modify `ensure_all_nodes_initialized()` method**:
   - Update the status check to only fail on `InitError` or `Created`
   - Don't fail on `Error` status - these nodes are initialized, just in an error state
   - Update error message to clarify that `Error` status is acceptable

### File: `lp-server/src/project_manager.rs`

1. **Review project loading**:
   - The `ensure_all_nodes_initialized()` check may need to be updated or removed
   - Projects should start even if some nodes have `Error` status
   - Consider if we still want to call this check, or if we should remove it entirely

## Success Criteria

- `ensure_all_nodes_initialized()` doesn't fail on `Error` status
- Projects can start even if shader nodes have compilation errors
- Code compiles without errors
- Tests pass

## Notes

- This allows projects to start with nodes in error state
- The error state will be visible in the UI and API
- Users can fix the errors and the status will update automatically
