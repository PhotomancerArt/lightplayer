# Phase 4: Update Tests to Use File-Based Projects

## Goal

Update tests that still use the old `ProjectConfig` structure with `nodes` field to use the new file-based project structure.

## Tasks

1. **Fix tests in nodes/output/runtime.rs**:
   - Remove `nodes: Nodes { ... }` field from `ProjectConfig` creation
   - Update to just `ProjectConfig { uid, name }`
   - Ensure `InitContext::new()` calls are correct (takes node maps as separate parameters)

2. **Fix tests in nodes/texture/runtime.rs**:
   - Remove `nodes: Nodes { ... }` field from `ProjectConfig` creation
   - Update to just `ProjectConfig { uid, name }`
   - Ensure `InitContext::new()` calls are correct

3. **Fix tests in runtime/contexts.rs**:
   - Update `InitContext::new()` calls to match current signature
   - Signature: `new(project_config, textures, shaders, outputs, fixtures)`
   - Pass empty `BTreeMap`s for node types if needed

4. **Run all tests**:
   - Ensure all tests in `lp-core` pass
   - Fix any compilation errors
   - Fix any test failures

## Success Criteria

- All tests updated to use new `ProjectConfig` structure (no `nodes` field)
- All `InitContext::new()` calls match current signature
- All tests pass
- No warnings (except unused code)
