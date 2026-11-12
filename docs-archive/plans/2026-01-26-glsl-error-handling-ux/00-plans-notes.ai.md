# Plans Notes: GLSL Error Handling UX Improvements

## Context

Currently, when running `lp-cli dev test-project`:
- GLSL compile errors on init cause the project to fail to start
- If main.glsl produces an error, all subsequent changes are ignored
- GLSL errors are stored in `ShaderState.error` but not prominently displayed in the UI
- Node status exists but isn't visually indicated in the UI
- No console logging when node status changes

## Goals

1. Keep track of GLSL compile errors in the shader runtime state (already done)
2. Don't fail project startup on GLSL errors - just record the error in state
3. Don't ignore subsequent changes when there's a compilation error
4. Bring GLSL errors back into the debug client UI so we can easily see them while editing
5. Report node status across the API and show an indicator (red/green circle) in the UI
6. When node state changes from ok -> error or back, print something on the console

## Questions

### Q1: Should we remove `ensure_all_nodes_initialized()` check from project loading?

**Context**: Currently, `ProjectManager::load_project()` calls `ensure_all_nodes_initialized()` which fails if any nodes have `InitError` status. This causes the project to fail to start if there are GLSL errors.

**DECIDED**: GLSL parse/compilation errors are runtime state errors, not initialization errors. Being in an error state is not the same as failing to init. The node initializes successfully but enters an error state.

**Answer**: 
- `ensure_all_nodes_initialized()` should only fail on `InitError` or `Created` status - not on `Error` status
- GLSL compilation failures should set status to `Error`, not `InitError`
- The node is considered initialized even if it has a compilation error - it just can't render
- Projects should start even if some nodes have `Error` status (they're initialized, just in error state)

### Q2: How should we update node status when GLSL compilation fails?

**Context**: Currently, `ShaderRuntime::init()` returns an error on compilation failure, which sets status to `InitError`. For file changes, `handle_fs_change()` also returns an error.

**DECIDED**: GLSL compilation errors are runtime state errors, not initialization failures.

**Answer**: 
- In `init()`: If compilation fails, set status to `NodeStatus::Error(error_message)` and still return `Ok()` - the node initialized successfully, it just has a compilation error. Store the runtime (with no executable).
- In `handle_fs_change()`: If compilation fails, update status to `NodeStatus::Error(error_message)` but don't return an error - just record it and return `Ok()`
- When compilation succeeds (after an error), update status to `NodeStatus::Ok`
- `InitError` should only be used for actual initialization failures (can't create runtime, can't resolve dependencies, etc.)

### Q3: Should we track status changes to detect transitions?

**Context**: To log console messages when status changes from ok -> error or error -> ok, we need to detect these transitions.

**DECIDED**: Status change logging is client-side only (for the CLI tool).

**Answer**: Track status changes in the client (`lp-cli` debug handler or UI). When syncing and applying changes, compare previous status with new status. When we detect a transition from `Ok` to `Error` or `Error` to `Ok`, log to console with the error message.

### Q4: Where should console logging happen?

**Context**: The user wants console output when node status changes. This could be in:
- Server-side (in `ProjectRuntime` or `ProjectManager`)
- Client-side (in `lp-cli` debug handler or UI)
- Both

**DECIDED**: Client-side only.

**Answer**: Client-side logging in `lp-cli` when syncing and detecting status changes. Track previous status in the client view and compare when applying changes. Format: `[shader-1] Status changed: Ok -> Error("GLSL compilation failed: ...")` or `[shader-1] Status changed: Error(...) -> Ok`

### Q5: What visual indicators should we use in the UI?

**Context**: Need to show node status visually (red/green circle or similar).

**DECIDED**: Green/red/yellow circle like a status indicator light.

**Answer**: 
- Add a colored circle indicator next to each node name/header:
  - Green circle for `Ok`
  - Red circle for `Error` or `InitError`
  - Yellow circle for `Warn`
  - Gray circle for `Created`
- Show the status text and error message in the node panel (already done for shader nodes)

### Q6: Should we update status when shader state changes (not just on init/fs_change)?

**Context**: Currently status is only updated during `init()` and `handle_fs_change()`. But we also need to update it when compilation succeeds after an error.

**DECIDED**: Update status in the callers (`init()`, `handle_fs_change()`) after calling `compile_shader()`. The runtime doesn't have direct access to the node entry's status field.

**Answer**: 
- In `init()`: After calling `compile_shader()`, check if compilation succeeded or failed, and update node status accordingly
- In `handle_fs_change()`: After calling `compile_shader()`, check if compilation succeeded or failed, and update node status accordingly
- Also ensure status is current when extracting state for the API (check `compilation_error` field in runtime)
