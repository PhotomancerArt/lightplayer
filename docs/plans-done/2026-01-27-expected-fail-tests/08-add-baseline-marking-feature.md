# Phase 8: Add Baseline Marking Feature

## Description

Add feature to automatically mark all currently failing tests with `[expect-fail]` markers. This
requires explicit confirmation with a stern warning, as it should only be used when establishing a
baseline.

## Changes

### `lib.rs`

- Check for `LP_MARK_FAILING_TESTS_EXPECTED=1` environment variable
- If set, show stern warning:

  ```
  WARNING: This will mark ALL currently failing tests with [expect-fail] markers.
  This should only be done when establishing a baseline for expected-fail tracking.

  This operation will modify test files. Make sure you have committed your changes.

  Type 'yes' to confirm:
  ```

- Read confirmation from stdin (must be exactly "yes")
- If confirmed, run tests and collect all failing test directives
- For each failing test, call `add_expect_fail_marker()` if not already marked
- Show summary of how many tests were marked
- Exit with appropriate code

### `apps/lp-glsl-filetests-app/src/main.rs`

- Document `LP_MARK_FAILING_TESTS_EXPECTED` in help text (but don't add as flag - keep it env var
  only)

## Success Criteria

- Feature only works with `LP_MARK_FAILING_TESTS_EXPECTED=1` env var
- Stern warning is displayed
- Requires typing "yes" exactly to proceed
- All failing tests are marked with `[expect-fail]`
- Summary shows count of marked tests
- Code compiles without errors

## Implementation Notes

- Use `std::io::stdin().read_line()` for confirmation
- Trim whitespace from input before comparing
- Case-sensitive comparison ("yes" not "Yes" or "YES")
- Only mark tests that are actually failing (not already marked)
- Show progress or summary of marking operation
- Consider showing which files were modified

## Safety Considerations

- Warning should be very clear about what will happen
- Require explicit "yes" confirmation (not just Enter)
- Maybe show count of tests that will be marked before confirmation?
- Consider requiring git clean working directory?

## Edge Cases

- User types something other than "yes" (abort operation)
- User presses Ctrl+C (handle gracefully)
- No failing tests (show message and exit)
- Some tests already marked (skip those)
