# Phase 9: Update Exit Code Logic

## Description

Ensure tests fail on unexpected passes with appropriate error messages, and update exit code determination logic.

## Changes

### `lib.rs`

- Update exit code logic:
  - Exit 0: All non-expect-fail tests pass AND no unexpected passes
  - Exit 1: Any unexpected failures (regressions) OR unexpected passes
- Add error message when unexpected passes occur:
  ```
  Error: 5 tests marked [expect-fail] are now passing.
  To fix: rerun tests with LP_FIX_XFAIL=1 or --fix flag to automatically remove markers.
  ```
- Show error message even if `fix_xfail` is enabled (still exit 1 to draw attention)
- Update error message to show which files/lines had unexpected passes (in single-test mode)

## Success Criteria

- Exit code 0 only when no unexpected failures and no unexpected passes
- Exit code 1 for unexpected failures or unexpected passes
- Clear error message shown for unexpected passes
- Error message mentions both `LP_FIX_XFAIL=1` and `--fix` flag
- Code compiles without errors

## Implementation Notes

- Check `stats.unexpected_pass > 0` to determine if error message needed
- Format error message with count of unexpected passes
- In single-test mode, show specific file/line information
- In multi-test mode, show aggregate count
- Exit 1 even if markers are being removed (to ensure review)
