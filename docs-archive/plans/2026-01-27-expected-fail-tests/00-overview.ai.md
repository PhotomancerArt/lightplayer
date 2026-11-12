# Plan: Expected Fail Tests

## Overview

Implement support for marking tests as expected to fail using `[expect-fail]` syntax. This enables tracking known failures separately from new regressions, making it easier to see when tests start passing or when new failures are introduced. Tests marked `[expect-fail]` that pass will cause the test run to fail, with an option to automatically remove the marker using `LP_FIX_XFAIL=1` or `--fix` flag.

## Phases

1. **Update data structures** - Add `expect_fail` field to `RunDirective` and extend `TestCaseStats` with new categories
2. **Parse `[expect-fail]` marker** - Update parsing logic to detect and strip the marker from run directives
3. **Track test result categories** - Update test execution to categorize results into passed, expected-fail, unexpected-fail, and unexpected-pass
4. **Update reporting format** - Modify output to show expected-fail counts and handle unexpected passes
5. **Implement marker removal** - Add `remove_expect_fail_marker()` method to `FileUpdate`
6. **Implement marker addition** - Add `add_expect_fail_marker()` method to `FileUpdate` for baseline marking
7. **Add fix flag support** - Add `--fix` flag to CLI and `LP_FIX_XFAIL` env var support
8. **Add baseline marking feature** - Add `LP_MARK_FAILING_TESTS_EXPECTED` with confirmation prompt
9. **Update exit code logic** - Ensure tests fail on unexpected passes with appropriate error messages
10. **Add filetests** - Create filetests to verify the expected-fail functionality
11. **Cleanup and finalization** - Fix warnings, format code, verify all tests pass

## Success Criteria

- Tests can be marked with `[expect-fail]` syntax
- Expected failures are tracked separately and don't count as regressions
- Unexpected passes cause test failure with clear error messages
- `--fix` flag and `LP_FIX_XFAIL=1` enable automatic marker removal
- Reporting shows expected-fail counts in compact format
- All existing tests continue to work unchanged
- New filetests verify expected-fail functionality
