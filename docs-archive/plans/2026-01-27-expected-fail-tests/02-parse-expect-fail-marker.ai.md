# Phase 2: Parse `[expect-fail]` Marker

## Description

Update parsing logic to detect `[expect-fail]` marker at the end of `// run:` directives and set the `expect_fail` field accordingly.

## Changes

### `parse/parse_run.rs`

- In `parse_run_directive()`, check for `[expect-fail]` marker at the end of the line
- Strip the marker before parsing expected value and tolerance
- Set `RunDirective.expect_fail = true` if marker is found
- Handle marker appearing after tolerance: `test() ~= 1.0 (tolerance: 0.001) [expect-fail]`
- Handle marker appearing after comments: `test() == 1 // comment [expect-fail]`

## Success Criteria

- `[expect-fail]` marker is detected and parsed correctly
- Marker is stripped before parsing expected value
- `expect_fail` field is set correctly in `RunDirective`
- Existing tests without marker continue to work
- Code compiles without errors

## Implementation Notes

- Check for `[expect-fail]` using `strip_suffix()` or similar
- Strip marker before parsing tolerance and expected value
- Marker can appear anywhere after the comparison operator
- Preserve whitespace handling for backward compatibility

## Test Cases

- `// run: test() == 1 [expect-fail]` - marker at end
- `// run: test() ~= 1.0 (tolerance: 0.001) [expect-fail]` - marker after tolerance
- `// run: test() == 1 // comment [expect-fail]` - marker after comment
- `// run: test() == 1` - no marker (should set `expect_fail = false`)
