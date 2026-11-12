# Phase 4: Cleanup And Validation

## Scope of phase

In scope:

- Remove temporary debugging artifacts and stale clock-specific mutation code paths.
- Run the plan’s final validation commands.
- Fix formatting issues, warnings, and remaining failures directly related to this plan.

Out of scope:

- New features beyond the approved general authored-def mutation scope.
- Follow-on work for runtime-state mutation or container mutation.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files and symbols:

- All files touched by phases 1-3.
- `lp-core/lpc-engine/src/engine/slot_mutation.rs` should no longer read as a clock-specific implementation.

Expected changes:

- Delete dead helpers left over from the clock-only path.
- Ensure naming and comments describe the generic authored-def mutation model accurately.
- Re-run and stabilize validation.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model slot_record_derive
cargo test -p lpc-engine mutation
cargo test -p lpc-view mutation
cargo test -p lp-cli
cargo check -p lpa-server
```
