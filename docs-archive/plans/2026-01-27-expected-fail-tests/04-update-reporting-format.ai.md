# Phase 4: Update Reporting Format

## Description

Modify output formatting to show expected-fail counts and handle unexpected passes in both per-file and summary displays.

## Changes

### `lib.rs`

- Update `format_results_summary()` signature to accept:
  - `expect_fail_count: usize`
  - `unexpected_pass_count: usize`
  - `fix_enabled: bool`
- Format summary line: `150/150 tests passed, 162 expect-fail, 14/40 files passed in 650ms`
- Handle unexpected passes: `155/150 tests passed, 162 expect-fail, 14/40 files passed`
- Add removal message: `5 tests newly pass. [expect-fail] removed.` (or `not removed` if fix disabled)
- Update per-file display logic:
  - `✓  7/7 function/test.glsl (3 expect-fail)` - all pass, some expected-fail
  - `✗  5/7 function/test.glsl (2 unexpected)` - some unexpected failures
  - `✓  7/6 function/test.glsl (3 expect-fail, 1 unexpected-pass)` - unexpected passes
- Aggregate statistics across all test files
- Use appropriate colors (green for pass, red for unexpected failures, yellow for expected failures)

## Success Criteria

- Summary shows expected-fail counts
- Per-file display shows expected-fail counts when appropriate
- Unexpected passes are shown with over 100% pass rate
- Removal message appears when applicable
- Colors are used appropriately
- Code compiles without errors

## Implementation Notes

- Update all call sites of `format_results_summary()` to pass new parameters
- Aggregate `expect_fail` and `unexpected_pass` across all test files
- Determine if fix was enabled from environment variable or flag
- Format numbers consistently (e.g., `162 expect-fail` not `162 expect-fails`)
