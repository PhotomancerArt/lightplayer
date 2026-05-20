# Phase 07: Cleanup And Validation

## Scope Of Phase

Clean up compatibility shims, document the new shape catalog model, and capture
before/after memory data.

## Code Organization Reminders

- Remove temporary debug logging and obsolete TODOs.
- Keep compatibility helpers clearly documented if any remain.
- Tests stay at the bottom.

## Sub-Agent Reminders

- Main-agent phase preferred.
- Do not commit unless explicitly asked.
- Do not suppress warnings or weaken tests.
- Report changed files, validation, and deviations.

## Implementation Details

Tasks:

- Search for static shape registration still happening in engine startup.
- Search for broad `SlotShapeRegistry::snapshot()` use that accidentally
  serializes only dynamic shapes without catalog metadata.
- Add/update docs explaining static catalog export behavior.
- Run memory/profile validation once M1 load-only mode is available.

## Validate

```bash
cargo fmt --check
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo test -p fw-tests --test profile_alloc_emu
```
