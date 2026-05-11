# Phase 4: Cleanup And Validation

## Scope Of Phase

Clean up the milestone and run focused validation.

In scope:

- rustdocs for static/dynamic slot data, version boundaries, and shape ownership,
- removal of obsolete mockup modules,
- warning and formatting cleanup,
- milestone summary.

Out of scope:

- broad workspace validation,
- committing unrelated dirty files.

## Code Organization Reminders

- No commented-out experiments.
- No stray debug prints.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

- Update `future.md` if the implementation clarifies future work.
- Write `summary.md` for the milestone.
- Stage and commit only files related to this milestone; leave unrelated IDE changes alone.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --features schema-gen
```
