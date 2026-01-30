# Phase 4: Update server.rs to use strip_prefix()

## Description

Replace manual path prefix stripping logic in `server.rs` with `LpPath::strip_prefix()`.

## Implementation

### 1. Locate manual path manipulation

Find the manual path manipulation logic in `server.rs` (around lines 157-177 in `handle_fs_request()` method) that strips project prefixes and normalizes paths.

### 2. Replace with strip_prefix()

Replace the manual string manipulation (`starts_with`, string slicing, `format!`) with:
- Convert paths to `LpPath` instances
- Use `strip_prefix()` to remove project prefix
- Handle `None` case appropriately (path doesn't start with prefix)

### 3. Update path handling

Ensure path normalization and formatting is handled correctly by `LpPath`.

## Success Criteria

- [ ] Manual path prefix stripping logic is removed
- [ ] `strip_prefix()` is used for prefix removal
- [ ] Paths are normalized correctly
- [ ] Paths that don't match prefix are filtered out correctly
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
