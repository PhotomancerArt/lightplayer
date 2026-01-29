# Phase 5: Implement Marker Removal

## Description

Add `remove_expect_fail_marker()` method to `FileUpdate` to remove `[expect-fail]` markers from test files.

## Changes

### `util/file_update.rs`

- Add `remove_expect_fail_marker(&self, line_number: usize) -> Result<()>` method
- Read file, find line with `[expect-fail]` marker
- Strip the marker while preserving formatting and indentation
- Handle marker in various positions:
  - End of line: `// run: test() == 1 [expect-fail]`
  - After tolerance: `// run: test() ~= 1.0 (tolerance: 0.001) [expect-fail]`
  - After comment: `// run: test() == 1 // comment [expect-fail]`
- Update `line_diff` tracking for multiple removals in same file
- Write updated file back

## Success Criteria

- Marker is removed correctly from all positions
- Formatting and indentation are preserved
- Multiple removals in same file work correctly
- Code compiles without errors
- Method can be called multiple times on same file

## Implementation Notes

- Use similar approach to `update_run_expectation()` for consistency
- Strip `[expect-fail]` using string replacement (be careful with whitespace)
- Preserve all other content on the line
- Track line number adjustments for multiple edits
- Test with various marker positions

## Edge Cases

- Marker with extra whitespace: `[expect-fail] ` or ` [expect-fail]`
- Multiple markers on same line (shouldn't happen, but handle gracefully)
- Marker in middle of line (shouldn't happen, but verify)
