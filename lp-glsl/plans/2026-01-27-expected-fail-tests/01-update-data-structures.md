# Phase 1: Update Data Structures

## Description

Add the `expect_fail` field to `RunDirective` and extend `TestCaseStats` with fields to track expected failures and unexpected passes.

## Changes

### `parse/test_type.rs`

- Add `expect_fail: bool` field to `RunDirective` struct
- Default to `false` for backward compatibility

### `test_run/mod.rs`

- Add `expect_fail: usize` field to `TestCaseStats` (tests marked `[expect-fail]` that failed as expected)
- Add `unexpected_pass: usize` field to `TestCaseStats` (tests marked `[expect-fail]` that passed)
- Update `Default` implementation to initialize new fields to 0

## Success Criteria

- `RunDirective` has `expect_fail: bool` field
- `TestCaseStats` has `expect_fail: usize` and `unexpected_pass: usize` fields
- Code compiles without errors
- All existing tests still pass

## Implementation Notes

- Place new fields at the end of structs to minimize impact
- Use `Default::default()` for new fields in existing code paths
- No behavior changes yet - just data structure updates
