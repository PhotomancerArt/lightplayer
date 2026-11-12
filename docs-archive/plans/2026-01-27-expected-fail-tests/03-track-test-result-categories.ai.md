# Phase 3: Track Test Result Categories

## Description

Update test execution logic in `run_summary.rs` to categorize test results into four categories: passed, expected-fail, unexpected-fail, and unexpected-pass.

## Changes

### `test_run/run_summary.rs`

- In `run()`, for each `RunDirective`:
  - Check `directive.expect_fail` to determine if test is marked
  - Categorize results:
    - **Passed**: Test passes AND not marked `[expect-fail]` → increment `stats.passed`
    - **Expected Fail**: Test fails AND marked `[expect-fail]` → increment `stats.expect_fail`
    - **Unexpected Fail**: Test fails AND not marked `[expect-fail]` → increment `stats.failed`
    - **Unexpected Pass**: Test passes AND marked `[expect-fail]` → increment `stats.unexpected_pass`
  - Don't count expected failures as "failed" in the final determination
  - Denominator for pass rate excludes expected-fail tests (count only non-marked tests)

## Success Criteria

- Four result categories are tracked correctly
- Expected failures don't count as regressions
- Statistics are accurate for all categories
- Code compiles without errors
- Existing tests continue to work

## Implementation Notes

- Update the result matching logic in `run()` function
- Track unexpected passes separately (will be used for marker removal)
- Ensure `total` count includes all tests (for display purposes)
- Pass/fail determination excludes expected-fail tests from denominator

## Edge Cases

- Compilation failures: Should these be categorized based on `expect_fail` marker?
- Trap expectations: How do these interact with `[expect-fail]`?
- Parse errors: Should these respect `expect_fail` marker?
