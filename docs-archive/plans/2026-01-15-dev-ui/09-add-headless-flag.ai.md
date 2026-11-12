# Phase 9: Add --headless flag

## Goal

Add `--headless` flag to dev command to disable UI when needed.

## Tasks

1. Update `lp-app/apps/lp-cli/src/commands/dev/args.rs`:
   - Add `headless: bool` field to `DevArgs` struct
   - Add `--headless` flag parsing in `from_args()` or similar
   - Set default to `false` (show UI by default)

2. Update `lp-app/apps/lp-cli/src/commands/dev/handler.rs`:
   - Check `args.headless` flag
   - Only spawn UI if `!args.headless`
   - When headless, run normal client loop (existing behavior)

3. Update help text/documentation:
   - Document `--headless` flag
   - Explain that UI is shown by default

## Success Criteria

- `--headless` flag exists and works
- UI is shown by default
- UI is disabled when `--headless` is set
- Code compiles without warnings
- Help text is updated

## Implementation Notes

- Default behavior: show UI (headless = false)
- Flag should be a simple boolean
- When headless, behavior should match current dev command behavior
