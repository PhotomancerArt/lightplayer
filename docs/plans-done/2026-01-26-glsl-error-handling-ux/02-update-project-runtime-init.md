# Phase 2: Update ProjectRuntime to handle compilation errors gracefully

## Description

Modify `ProjectRuntime::init_nodes()` to check for compilation errors after initializing shader nodes and update their status accordingly. Shader nodes with compilation errors should have status `Error`, not `InitError`.

## Changes

### File: `lp-engine/src/project/runtime.rs`

1. **Modify `init_nodes()` method**:
   - After calling `runtime.init()`, check if it's a shader runtime
   - If it's a shader runtime, check if it has a compilation error using the helper method
   - If compilation error exists, set status to `NodeStatus::Error(error_message)`
   - If no compilation error, set status to `NodeStatus::Ok` (or `InitError` if init actually failed)
   - Generate `StatusChanged` change if status is `Error`

2. **Update status handling logic**:
   - Distinguish between initialization failures (`InitError`) and runtime errors (`Error`)
   - GLSL compilation failures are runtime errors, not initialization failures

## Success Criteria

- Shader nodes with compilation errors have status `Error`, not `InitError`
- Status is correctly set based on compilation error state
- Code compiles without errors
- Tests pass (may need to update tests that expect `InitError` for compilation errors)

## Notes

- This phase depends on Phase 1 (helper methods in ShaderRuntime)
- Status changes will be picked up by the sync API in later phases
