# Phase 6: Test and Cleanup

## Description

Run the app, verify all files are generated correctly, check that statistics and reports include new
metrics, fix any issues, and clean up code.

## Implementation

- Run `./scripts/lp-glsl-q32-metrics-app.sh` to generate a report
- Verify vcode files (`.pre.vcode`, `.post.vcode`) are generated for all functions
- Verify assembly files (`.pre.s`, `.post.s`) are generated for all functions
- Check that `stats.toml` includes vcode_size and assembly_size fields
- Check that `report.toml` includes vcode/assembly totals and deltas
- Verify existing functionality still works (CLIF generation, statistics)
- Fix any warnings
- Run `cargo +nightly fmt` on entire workspace
- Remove any temporary code, TODOs, debug prints, etc.
- Ensure all code is clean and readable

## Success Criteria

- App runs successfully and generates all expected files
- VCode and assembly files are present and contain expected content
- Statistics include vcode_size and assembly_size
- Reports include vcode/assembly metrics
- All existing functionality works
- No warnings
- Code is formatted
- No temporary code or debug statements

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
