# Phase 10: Add Filetests

## Description

Create filetests to verify the expected-fail functionality works correctly.

## Changes

### `filetests/function/expect-fail-basic.glsl`

- Create test file with various `[expect-fail]` scenarios:
  - Test marked `[expect-fail]` that fails (should be counted as expected)
  - Test marked `[expect-fail]` that passes (should be unexpected pass)
  - Test not marked that fails (should be unexpected failure)
  - Test not marked that passes (should be normal pass)
- Verify reporting format shows expected-fail counts correctly

### `filetests/function/expect-fail-removal.glsl`

- Create test file with `[expect-fail]` markers on passing tests
- Run with `--fix` flag and verify markers are removed
- Verify file is updated correctly

## Success Criteria

- Filetests verify all four result categories
- Filetests verify marker removal works
- Filetests verify reporting format
- Filetests verify exit codes
- All new filetests pass

## Implementation Notes

- Create test files that exercise different scenarios
- Use existing test patterns for consistency
- Test both with and without `--fix` flag
- Verify file updates are correct
- Test edge cases (marker positions, whitespace, etc.)

## Test Scenarios

1. Basic expected-fail: Test marked `[expect-fail]` that fails
2. Unexpected pass: Test marked `[expect-fail]` that passes
3. Normal pass: Test not marked that passes
4. Unexpected fail: Test not marked that fails
5. Marker removal: Verify `--fix` removes markers
6. Reporting: Verify counts are shown correctly
