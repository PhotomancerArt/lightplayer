# Phase 3: Update runtime.rs to use join_relative()

## Description

Replace manual relative path resolution logic in `runtime.rs` with `LpPath::join_relative()`.

## Implementation

### 1. Locate manual path resolution

Find the manual relative path resolution logic in `runtime.rs` (around lines 1145-1194 in `resolve_node()` method).

### 2. Replace with join_relative()

Replace the manual string splitting, component handling (`.`, `..`), and path reconstruction with:
- Use `self.node_path.parent()` to get parent directory
- Use `parent.join_relative(spec_path)` to resolve relative path
- Handle `None` case appropriately (invalid relative path)

### 3. Update error handling

Ensure error handling is appropriate for the new approach.

## Success Criteria

- [ ] Manual path resolution logic is removed
- [ ] `join_relative()` is used for relative path resolution
- [ ] Absolute paths still work correctly
- [ ] Relative paths with `.` and `..` resolve correctly
- [ ] Invalid relative paths (going above root) are handled appropriately
- [ ] All existing tests pass
- [ ] Code compiles without errors
- [ ] Code formatted with `cargo +nightly fmt`

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language
- Avoid emoticons
- Use measured, factual descriptions
