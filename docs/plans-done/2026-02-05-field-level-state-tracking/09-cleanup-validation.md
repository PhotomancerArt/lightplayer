# Phase 9: Cleanup, Review, and Validation

## Scope of Phase

Final cleanup phase: remove temporary code, fix warnings, ensure everything compiles and tests pass, and validate the implementation works correctly.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Remove temporary code

Search for TODOs and temporary code:

```bash
grep -r "TODO" lp-core/lp-model/src/state/
grep -r "TODO" lp-core/lp-engine/src/nodes/
grep -r "FIXME" lp-core/
grep -r "XXX" lp-core/
```

Remove or address all temporary code.

### 2. Remove test state struct

If `TestState` from Phase 3 was kept for reference, remove it or move it to a test-only module:

```bash
# Remove or move lp-model/src/state/test_state.rs
```

### 3. Fix all warnings

Run cargo check and fix all warnings:

```bash
cd lp-core
cargo check 2>&1 | grep warning
```

Fix each warning:
- Unused imports
- Unused variables
- Dead code
- etc.

### 4. Run all tests

Ensure all tests pass:

```bash
cd lp-core
cargo test --all-features
```

Fix any failing tests.

### 5. Validate serialization behavior

Create integration tests to validate:
- Initial sync sends all fields
- Incremental sync only sends changed fields
- Partial deserialization merges correctly
- Frame tracking works correctly

### 6. Check for consistency

Ensure all state structs follow the same pattern:
- All fields use `StateField<T>`
- All have `new(frame_id)` constructor
- All implement custom `Serialize` and `Deserialize`
- All runtimes store state directly
- All runtimes update state via `StateField::set()` or `mark_updated()`

### 7. Update documentation

Add/update documentation:
- `StateField<T>` API documentation
- State struct documentation
- Runtime state management documentation

### 8. Format code

Run formatter:

```bash
cd lp-core
cargo +nightly fmt
```

## Validate

Run the following commands to validate this phase:

```bash
cd lp-core
cargo check
cargo test --all-features
cargo +nightly fmt -- --check
```

All code should compile without warnings, all tests should pass, and code should be properly formatted.

## Plan Cleanup

Once validation passes:

1. Add a summary of completed work to `summary.md`
2. Move plan files to `docs/plans-done/` directory

## Commit

Once everything is complete and validated, commit with:

```
feat(state): implement field-level state tracking

- Add StateField<T> wrapper type for tracking field changes
- Refactor all node state structs to use StateField<T>
- Refactor runtimes to store state directly
- Implement custom serialization for partial state updates
- Add frame_id to RenderContext for state updates
- Update state extraction to use field-level tracking

This reduces bandwidth usage by only sending changed fields in state
updates, rather than sending entire state structs when any field changes.
```
