# Plan: Expected Fail Tests

## Overview

Implement support for marking tests as expected to fail using `[expected-fail]` syntax. This will help track known failures separately from new regressions, making it easier to see when tests start passing or when new failures are introduced.

## Questions

### Q1: Syntax and Parsing

**Question:** What syntax should we use to mark tests as expected to fail?

**Context:** We need a syntax that:

- Is clear and readable in GLSL comment format
- Doesn't conflict with existing syntax (tolerance, comments)
- Is easy to parse and remove automatically

**Suggested Answer:** Use `[expect-fail]` marker at the end of `// run:` directives:

```glsl
// run: test() == 1 [expect-fail]
// run: test() ~= 1.0 (tolerance: 0.001) [expect-fail]
```

This follows common patterns (like Rust's `#[should_panic]`) and is easy to detect/remove.

**Alternative:** Could use `# expected-fail` or `// expected-fail` but brackets are clearer and less likely to conflict.

**Decision:** Approved - using `[expect-fail]` syntax (shorter form preferred).

---

### Q2: Test Result Categories

**Question:** How should we categorize and report test results?

**Context:** We need to distinguish between:

- Tests that pass (normal success)
- Tests that fail but are marked `[expected-fail]` (known failures, not regressions)
- Tests that fail without `[expected-fail]` (new failures/regressions)
- Tests marked `[expected-fail]` that actually pass (should trigger auto-removal)

**Suggested Answer:** Track four categories:

1. **Passed**: Normal passing tests
2. **Expected Fail**: Tests marked `[expected-fail]` that failed (as expected)
3. **Unexpected Pass**: Tests marked `[expected-fail]` that passed (should remove marker)
4. **Unexpected Fail**: Tests not marked `[expected-fail]` that failed (regressions)

---

### Q3: Reporting Format

**Question:** How should we display these different result categories in test output?

**Context:** Current output shows:

- `✓` for passing files with counts like `7/7`
- `✗` for failing files with counts like `0/10`
- Summary: `150/328 tests passed, 14/40 files passed`

**Decision:**

- **Per-file format:** `✓  2/ 2 function/recursive-static-error.glsl (8 expect-fail)`
  - Green checkmark if all non-expected-fail tests pass
  - Grey count in parentheses for expected fails
- **Summary format:** `150/150 tests passed, 162 expect-fail, 14/40 files passed in 650ms`
  - Expected fails are filtered out from the passed count
  - Expected fails shown separately
- **Unexpected passes:** `155/150 tests passed, 162 expect-fail, 14/40 files passed in 650ms`
  - Shows over 100% when tests marked `[expect-fail]` pass
  - Message: `5 tests newly pass. [expect-fail] removed.` (or `not removed` if flag disabled)

---

### Q4: Auto-Removal Behavior

**Question:** When should `[expected-fail]` markers be automatically removed?

**Context:** User wants automatic removal when tests pass, but with a flag to disable for CI.

**Decision:**

- By default: Do NOT auto-remove (tests should not modify files by default)
- Environment variable `LP_FIX_XFAIL=1` or `--fix` flag enables auto-removal
- When auto-removal is disabled, unexpected passes cause test failure with error message
- When auto-removal happens, show message in summary: `5 tests newly pass. [expect-fail] removed.`
- When auto-removal disabled: Error message: `To fix: rerun tests with LP_FIX_XFAIL=1 or --fix flag to automatically remove markers.`

---

### Q5: File Update Strategy

**Question:** How should we handle file updates for removing `[expected-fail]` markers?

**Context:** We already have `FileUpdate` infrastructure for bless mode. We need to:

- Remove `[expected-fail]` from lines when tests pass
- Handle multiple updates per file
- Preserve formatting and indentation

**Suggested Answer:**

- Extend `FileUpdate` with `remove_expect_fail_marker(line_number)` method
- Process all unexpected passes at the end of test run
- Update files in-place similar to bless mode
- Only update if auto-removal is enabled (not in CI mode)

---

### Q6: Statistics Tracking

**Question:** How should we track and aggregate statistics across multiple test files?

**Context:** `TestCaseStats` currently tracks `passed`, `failed`, `total`. We need to add:

- Expected failures
- Unexpected passes

**Suggested Answer:**

- Add `expect_fail: usize` and `unexpected_pass: usize` to `TestCaseStats`
- Aggregate these across all test files
- Include in summary output
- Don't count expected failures as "failed" in the final pass/fail determination
- Passed count excludes expected failures (only counts non-marked tests that pass)

---

### Q7: Exit Code Behavior

**Question:** What should the exit code be when there are expected failures but no regressions?

**Context:** Currently, any failure causes exit code 1. With expected failures, we might want:

- Exit 0 if only expected failures (no regressions)
- Exit 1 if any unexpected failures or unexpected passes

**Decision:**

- Exit 0: All non-expected-fail tests pass AND no unexpected passes (only expected failures)
- Exit 1: Any unexpected failures (regressions) OR unexpected passes
- **Unexpected passes (default, no fix flag):** Exit 1 with clear error message:
  ```
  Error: 5 tests marked [expect-fail] are now passing.
  To fix: rerun tests with LP_FIX_XFAIL=1 or --fix flag to automatically remove markers.
  ```
- **Unexpected passes with LP_FIX_XFAIL=1 or --fix:** Exit 1 (still error to draw attention), but markers are removed
- This allows CI to fail when tests marked as expected-fail start passing (prevents merging incorrectly ignored tests)
- Tests do not modify files by default (safer behavior)

---

### Q8: Integration with Existing Infrastructure

**Question:** How should this integrate with existing test infrastructure?

**Context:** We have:

- `parse_run.rs` for parsing directives
- `run_summary.rs` for executing tests
- `FileUpdate` for file modifications
- `TestCaseStats` for statistics

**Suggested Answer:**

- Parse `[expect-fail]` in `parse_run.rs` → add `expect_fail: bool` to `RunDirective`
- Track results in `run_summary.rs` → update `TestCaseStats` with new categories
- Add removal logic to `FileUpdate` → new method for removing markers
- Update reporting in `lib.rs` → show new categories in output with specified format

---

### Q9: File-Level Mixed Results Display

**Question:** How should we display files with mixed results (expected-fail, unexpected-fail, unexpected-pass)?

**Decision:**

- **Denominator:** Count only non-expected-fail tests (e.g., 5/7 not 5/10)
- **Show expected-fail count:** Only if there are no unexpected failures (to keep compact)
- **Symbol/Color:**
  - `✗` red if any unexpected failures
  - `✓` green/yellow if only expected failures or unexpected passes
- **Format examples:**
  - `✗  5/7 function/test.glsl (2 unexpected)` - when there are unexpected failures
  - `✓  7/7 function/test.glsl (3 expect-fail)` - when all non-expected-fail pass
  - `✓  7/6 function/test.glsl (3 expect-fail, 1 unexpected-pass)` - over 100% indicates unexpected passes
  - `✗  6/5 function/test.glsl (2 expect-fail, 2 unexpected, 1 unexpected-pass)` - mixed scenario

---

### Q10: Baseline Marking Feature

**Question:** How should we handle marking all currently failing tests as expected failures when introducing this feature?

**Context:** When introducing expected-fail tracking to an existing codebase, we need a way to establish a baseline by marking all currently failing tests.

**Decision:**

- Add `LP_MARK_FAILING_TESTS_EXPECTED=1` environment variable (no CLI flag - keep it hard to use)
- Show stern warning with explicit confirmation required:

  ```
  WARNING: This will mark ALL currently failing tests with [expect-fail] markers.
  This should only be done when establishing a baseline for expected-fail tracking.

  This operation will modify test files. Make sure you have committed your changes.

  Type 'yes' to confirm:
  ```

- Require typing "yes" exactly (case-sensitive) to proceed
- Run tests, collect all failing test directives, add `[expect-fail]` marker to each
- Show summary of how many tests were marked
- This is a one-time setup feature, not for regular use

---

## Notes

- User mentioned reviewing filetests for this feature - we should ensure good test coverage
- The feature should be backward compatible - existing tests without `[expect-fail]` work as before
- Consider adding a command-line flag or env var to list all expected failures (for documentation)
- Verbose output: In single-test mode, show which specific tests had markers removed (for debugging)
- Baseline marking feature should be hard to use and require explicit confirmation (safety measure)
