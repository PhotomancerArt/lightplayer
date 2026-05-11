# Phase 6: Cleanup, Validation, And Summary

## Scope Of Phase

Remove replaced legacy vocabulary, run final validation, and write the plan
summary.

In scope:

- Delete `WireSlotWatchSpecifier` and old watch-shaped project view code once
  unused.
- Delete `SyncDisabled`.
- Remove or rename get-changes/detail/watch comments that no longer describe
  reality.
- Clean rsdocs and module docs.
- Write `summary.md`.
- Move the completed standalone plan to `docs/plans-old/` after implementation
  is complete.
- Commit the complete implementation.

Out of scope:

- Push/PR/CI watching unless separately requested.
- Unrelated cleanup outside touched sync/client/view files.

## Code Organization Reminders

- Search for stale terms: `SyncDisabled`, `GetChanges`, `watch`, `detail
  toggle`, `slot_watch`, `ResourceSummarySpecifier`, `RuntimeBufferPayloadSpecifier`.
- Keep docs concise and future-facing.

## Sub-Agent Reminders

- Do not commit unless explicitly told to own the final commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire
cargo test -p lpc-view
cargo test -p lpc-engine
cargo check -p lpa-server
cargo check -p lpa-client
git diff --check
```
