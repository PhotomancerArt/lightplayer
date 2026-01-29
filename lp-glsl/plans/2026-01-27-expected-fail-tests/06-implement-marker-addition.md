# Phase 6: Implement Marker Addition

## Description

Add `add_expect_fail_marker()` method to `FileUpdate` to add `[expect-fail]` markers to test files. This is used for baseline marking when introducing expected-fail tracking to an existing codebase.

## Changes

### `util/file_update.rs`

- Add `add_expect_fail_marker(&self, line_number: usize) -> Result<()>` method
- Read file, find the `// run:` directive at specified line
- Add `[expect-fail]` marker to the end of the line
- Handle various line formats:
  - Simple: `// run: test() == 1` → `// run: test() == 1 [expect-fail]`
  - With tolerance: `// run: test() ~= 1.0 (tolerance: 0.001)` → `// run: test() ~= 1.0 (tolerance: 0.001) [expect-fail]`
  - With comment: `// run: test() == 1 // comment` → `// run: test() == 1 // comment [expect-fail]`
- Preserve formatting and indentation
- Don't add marker if it already exists
- Update `line_diff` tracking for multiple additions in same file

## Success Criteria

- Marker is added correctly to all line formats
- Formatting and indentation are preserved
- Multiple additions in same file work correctly
- Duplicate markers are not added
- Code compiles without errors

## Implementation Notes

- Use similar approach to `update_run_expectation()` for consistency
- Append ` [expect-fail]` to the end of the line (with space)
- Check if marker already exists before adding
- Preserve all other content on the line
- Track line number adjustments for multiple edits

## Edge Cases

- Line already has marker (should skip)
- Line has trailing whitespace (preserve or normalize?)
- Line ends with comment (add marker before or after comment?)
