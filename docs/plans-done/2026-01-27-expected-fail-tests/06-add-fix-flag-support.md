# Phase 7: Add Fix Flag Support

## Description

Add `--fix` command-line flag to `lp-glsl-filetests-app` and support for `LP_FIX_XFAIL` environment
variable to enable automatic marker removal.

## Changes

### `apps/lp-glsl-filetests-app/src/main.rs`

- Add `fix: bool` field to `TestOptions` struct with `#[arg(long)]` attribute
- Pass `fix` flag value to `lp_glsl_filetests::run()` function
- Check both `--fix` flag and `LP_FIX_XFAIL` environment variable

### `lib.rs`

- Update `run()` function signature to accept `fix_xfail: bool` parameter
- Check `LP_FIX_XFAIL` environment variable if flag not provided
- Pass `fix_xfail` value to test execution and file update logic
- Collect unexpected passes during test run
- At end of run, if `fix_xfail` is true, call `remove_expect_fail_marker()` for each unexpected pass

## Success Criteria

- `--fix` flag is recognized by CLI
- `LP_FIX_XFAIL=1` environment variable works
- Flag is passed through to test execution
- Marker removal happens when flag is enabled
- Marker removal does NOT happen when flag is disabled
- Code compiles without errors

## Implementation Notes

- Use `clap`'s `#[arg(long)]` for the flag
- Check env var using `std::env::var("LP_FIX_XFAIL")`
- Either flag or env var enables removal (OR logic)
- Store unexpected passes with file path and line number for removal
- Process removals at end of run (after all tests complete)

## Edge Cases

- Both flag and env var set (flag takes precedence, or use OR logic?)
- Empty env var value (should be treated as false)
- Case sensitivity of env var name
