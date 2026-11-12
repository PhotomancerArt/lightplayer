# Phase 10: Cleanup and Finalization

## Description

Final cleanup phase: remove temporary code, fix all warnings, ensure all tests pass, and format code. Move plan directory to `_done` when complete.

## Tasks

1. Remove temporary code:
   - Remove any debug prints or logging statements
   - Remove commented-out code
   - Remove unused imports
   - Remove unused functions or methods

2. Fix all warnings:
   - Run `cargo check` and fix all warnings
   - Address clippy warnings if any
   - Remove `#[allow(dead_code)]` attributes if code is actually used
   - Add `#[allow(dead_code)]` only if code will be used in future phases

3. Ensure all tests pass:
   - Run `cargo test` for all tests
   - Fix any failing tests
   - Ensure integration tests pass
   - Ensure unit tests pass

4. Code formatting:
   - Run `cargo +nightly fmt` on entire workspace
   - Ensure consistent formatting across all files

5. Documentation:
   - Ensure all public functions have doc comments
   - Ensure all public types have doc comments
   - Update module-level documentation if needed

6. Final verification:
   - Verify `lp-cli dev` command works correctly
   - Verify local server mode works
   - Verify websocket mode works (if implemented)
   - Verify file watching works
   - Verify UI works (if not headless)

7. Move plan directory:
   - Move `plans/2026-01-16-dev-architecture-refactor/` to `plans/_done/2026-01-16-dev-architecture-refactor/`

## Success Criteria

- No temporary code or debug prints
- No warnings (except intentional `#[allow(...)]`)
- All tests pass
- Code is formatted with `cargo +nightly fmt`
- All public APIs have documentation
- `lp-cli dev` command works correctly
- Plan directory moved to `_done`

## Implementation Notes

- Use `cargo check --all-targets` to check all targets
- Use `cargo test --all-targets` to test all targets
- Use `cargo clippy` to check for clippy warnings
- Consider running tests in CI if available
- Document any known limitations or future improvements
