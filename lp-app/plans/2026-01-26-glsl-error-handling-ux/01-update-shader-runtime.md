# Phase 1: Update ShaderRuntime to not fail on compilation errors

## Description

Modify `ShaderRuntime::init()` and `ShaderRuntime::handle_fs_change()` to not return errors on GLSL compilation failures. Instead, they should store the compilation error in the runtime state and return `Ok()`. The caller will check the compilation error and update the node status accordingly.

## Changes

### File: `lp-engine/src/nodes/shader/runtime.rs`

1. **Modify `init()` method**:
   - After calling `load_and_compile_shader()`, check if compilation failed
   - If compilation failed, don't return the error - just store it in `compilation_error`
   - Return `Ok()` even if compilation failed
   - The node is considered initialized, just in an error state

2. **Modify `handle_fs_change()` method**:
   - After calling `load_and_compile_shader()`, check if compilation failed
   - If compilation failed, don't return the error - just store it in `compilation_error`
   - Return `Ok()` even if compilation failed
   - The file change is processed, just the shader has a compilation error

3. **Add helper methods**:
   - `has_compilation_error(&self) -> bool` - check if shader has compilation error
   - `compilation_error(&self) -> Option<&str>` - get compilation error message

## Success Criteria

- `init()` returns `Ok()` even when GLSL compilation fails
- `handle_fs_change()` returns `Ok()` even when GLSL compilation fails
- Compilation errors are stored in `compilation_error` field
- Helper methods are available to check compilation error state
- Code compiles without errors

## Notes

- GLSL compilation errors are runtime state errors, not initialization errors
- The node initializes successfully but enters an error state
- This allows the project to start and file changes to be processed even with compilation errors
