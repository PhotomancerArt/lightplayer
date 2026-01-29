# Design: Expected Fail Tests

## Overview

Implement support for marking tests as expected to fail using `[expect-fail]` syntax. This enables tracking known failures separately from new regressions, making it easier to see when tests start passing or when new failures are introduced. Tests marked `[expect-fail]` that pass will automatically have the marker removed (unless `LP_KEEP_XFAIL=1` is set).

## File Structure

```
lp-glsl/crates/lp-glsl-filetests/src/
├── parse/
│   ├── parse_run.rs                    # UPDATE: Parse [expect-fail] marker
│   └── test_type.rs                    # UPDATE: Add expect_fail field to RunDirective
├── test_run/
│   ├── mod.rs                          # UPDATE: Add expect_fail and unexpected_pass to TestCaseStats
│   └── run_summary.rs                  # UPDATE: Track expected failures and unexpected passes
├── util/
│   └── file_update.rs                  # UPDATE: Add remove_expect_fail_marker() method
└── lib.rs                              # UPDATE: Update reporting format, exit code logic, add fix_xfail parameter

lp-glsl/apps/lp-test/src/
└── main.rs                             # UPDATE: Add --fix flag to TestOptions, pass to run()
```

## Types Summary

### RunDirective (`parse/test_type.rs`)

```
RunDirective - # UPDATE: Add expect_fail field
├── expression_str: String              # EXISTING
├── comparison: ComparisonOp            # EXISTING
├── expected_str: String                # EXISTING
├── tolerance: Option<f32>               # EXISTING
├── line_number: usize                   # EXISTING
└── expect_fail: bool                    # NEW: Whether test is marked [expect-fail]
```

### TestCaseStats (`test_run/mod.rs`)

```
TestCaseStats - # UPDATE: Add expect_fail and unexpected_pass fields
├── passed: usize                        # EXISTING: Tests that passed (excluding expect-fail)
├── failed: usize                        # EXISTING: Tests that failed unexpectedly
├── total: usize                         # EXISTING: Total test cases
├── expect_fail: usize                   # NEW: Tests marked [expect-fail] that failed (as expected)
└── unexpected_pass: usize                # NEW: Tests marked [expect-fail] that passed
```

### FileUpdate (`util/file_update.rs`)

```
FileUpdate - # UPDATE: Add marker removal method
├── new(path: &Path) -> FileUpdate       # EXISTING
├── update_run_expectation(...)          # EXISTING
├── update_clif_expectations(...)        # EXISTING
└── remove_expect_fail_marker(line_number: usize) -> Result<()>  # NEW: Remove [expect-fail] from line
```

## Function Changes

### parse_run_directive (`parse/parse_run.rs`)

```
parse_run_directive(line: &str, line_number: usize) -> Result<RunDirective>
  # UPDATE: Detect [expect-fail] marker at end of line
  # Strip [expect-fail] from expected_str before parsing
  # Set RunDirective.expect_fail = true if marker found
```

### run (`test_run/run_summary.rs`)

```
run(test_file: &TestFile, path: &Path, line_filter: Option<usize>) -> Result<(Result<()>, TestCaseStats)>
  # UPDATE: Track four categories:
  #   1. passed: non-expect-fail tests that pass
  #   2. failed: non-expect-fail tests that fail (regressions)
  #   3. expect_fail: expect-fail tests that fail (as expected)
  #   4. unexpected_pass: expect-fail tests that pass (need marker removal)
  # UPDATE: Don't count expect_fail as "failed" in stats
```

### run (`lib.rs`)

```
run(files: &[String]) -> anyhow::Result<()>
  # UPDATE: Add fix_xfail: bool parameter (from --fix flag or LP_FIX_XFAIL env var)
  # UPDATE: Check for LP_MARK_FAILING_TESTS_EXPECTED env var
  # UPDATE: If baseline marking enabled, show warning and require confirmation
  # UPDATE: Pass fix_xfail to test execution and file update logic
  # UPDATE: Collect failing tests and mark them if baseline marking confirmed
```

### format_results_summary (`lib.rs`)

```
format_results_summary(
    passed_test_cases: usize,
    failed_test_cases: usize,
    total_test_cases: usize,
    passed_files: usize,
    failed_files: usize,
    elapsed: Duration,
    expect_fail_count: usize,
    unexpected_pass_count: usize,
    fix_enabled: bool
) -> String
  # UPDATE: Add expect_fail_count, unexpected_pass_count, and fix_enabled parameters
  # UPDATE: Format: "150/150 tests passed, 162 expect-fail, 14/40 files passed in 650ms"
  # UPDATE: Show unexpected passes: "155/150 tests passed, 162 expect-fail, 14/40 files passed"
  # UPDATE: Add message: "5 tests newly pass. [expect-fail] removed." (if fix_enabled) or "not removed" (if not)
```

### remove_expect_fail_marker (`util/file_update.rs`)

```
remove_expect_fail_marker(&self, line_number: usize) -> Result<()>
  # NEW: Remove [expect-fail] marker from specified line
  # Preserve formatting and indentation
  # Handle multiple markers in same file (track line_diff)
```

### add_expect_fail_marker (`util/file_update.rs`)

```
add_expect_fail_marker(&self, line_number: usize) -> Result<()>
  # NEW: Add [expect-fail] marker to specified line
  # Preserve formatting and indentation
  # Skip if marker already exists
  # Handle multiple additions in same file (track line_diff)
```

### TestOptions (`apps/lp-test/src/main.rs`)

```
TestOptions - # UPDATE: Add fix flag
├── files: Vec<String>                  # EXISTING
└── fix: bool                            # NEW: Enable auto-removal of [expect-fail] markers
```

## Reporting Format

### Per-File Display

- **All pass, some expected-fail:** `✓  7/7 function/test.glsl (3 expect-fail)`
- **Some unexpected failures:** `✗  5/7 function/test.glsl (2 unexpected)`
- **Unexpected passes:** `✓  7/6 function/test.glsl (3 expect-fail, 1 unexpected-pass)`
- **Mixed:** `✗  6/5 function/test.glsl (2 expect-fail, 2 unexpected, 1 unexpected-pass)`

### Summary Display

- **Normal:** `150/150 tests passed, 162 expect-fail, 14/40 files passed in 650ms`
- **With unexpected passes:** `155/150 tests passed, 162 expect-fail, 14/40 files passed in 650ms`
- **Removal message:** `5 tests newly pass. [expect-fail] removed.` (or `not removed` if `LP_KEEP_XFAIL=1`)

## Exit Code Logic

- **Exit 0:** All non-expect-fail tests pass AND no unexpected passes
- **Exit 1:** Any unexpected failures (regressions) OR unexpected passes
- **Error message for unexpected passes (default, no fix flag):**
  ```
  Error: 5 tests marked [expect-fail] are now passing.
  To fix: rerun tests with LP_FIX_XFAIL=1 or --fix flag to automatically remove markers.
  ```

## Auto-Removal Behavior

- **Default:** Do NOT auto-remove (tests should not modify files by default)
- **LP_FIX_XFAIL=1 or --fix flag:** Enable auto-removal of `[expect-fail]` markers
- **Process:** Collect all unexpected passes, update files at end of test run (only if flag enabled)
- **Message:** Show count and whether markers were removed or not
- **Error message for unexpected passes without fix flag:**
  ```
  Error: 5 tests marked [expect-fail] are now passing.
  To fix: rerun tests with LP_FIX_XFAIL=1 or --fix flag to automatically remove markers.
  ```

## Baseline Marking Feature

A separate feature to automatically mark all currently failing tests with `[expect-fail]` markers. This is useful for establishing a baseline when introducing this feature to an existing codebase.

- **Environment variable:** `LP_MARK_FAILING_TESTS_EXPECTED=1`
- **Purpose:** Mark all failing tests as expected failures (establish baseline)
- **Safety:** Requires explicit confirmation with stern warning
- **Warning message:**
  ```
  WARNING: This will mark ALL currently failing tests with [expect-fail] markers.
  This should only be done when establishing a baseline for expected-fail tracking.
  Type 'yes' to confirm:
  ```
- **Confirmation:** Must type "yes" exactly to proceed
- **Behavior:** After confirmation, run tests and add `[expect-fail]` to all failing test directives
- **One-time use:** Intended for initial setup only, not regular use

## Implementation Notes

1. **Parsing:** Strip `[expect-fail]` from the end of run directive lines before parsing expected value
2. **Statistics:** Denominator excludes expect-fail tests (count only non-marked tests)
3. **File Updates:** Process all marker removals at end of run, only if `LP_FIX_XFAIL=1` or `--fix` flag is set
4. **Backward Compatibility:** Existing tests without `[expect-fail]` work unchanged
5. **CI Integration:** Tests fail on unexpected passes by default (no file modifications), use `LP_FIX_XFAIL=1` or `--fix` to enable auto-removal
6. **Command-line flag:** Add `--fix` flag to `TestOptions` in `lp-test/src/main.rs`, pass to `run()` function
7. **Flag precedence:** Check both `LP_FIX_XFAIL` env var and `--fix` flag (either enables auto-removal)
8. **Baseline marking:** Separate feature with `LP_MARK_FAILING_TESTS_EXPECTED=1`, requires explicit "yes" confirmation
