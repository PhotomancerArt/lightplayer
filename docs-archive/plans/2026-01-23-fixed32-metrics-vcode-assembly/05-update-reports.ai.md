# Phase 5: Update Reports to Include New Metrics

## Description

Update report generation functions to serialize and include vcode_size and assembly_size fields in TOML reports. No changes needed to report structure types (they use the updated stats types), but verify serialization works correctly.

## Implementation

- Verify `report.rs` types (`TestReport`, `FunctionReport`, etc.) automatically include new fields (they use stats types)
- Test that TOML serialization includes vcode_size and assembly_size fields
- Update any report formatting or display logic if needed
- Verify overall report includes vcode/assembly totals and deltas

## Success Criteria

- Report TOML files include vcode_size and assembly_size fields
- Overall report includes vcode/assembly totals and deltas
- Per-test reports include vcode/assembly metrics
- Per-function reports include vcode/assembly metrics
- Code compiles without errors
- No warnings

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
