# Phase 1: Shape Paging Regression

## Scope Of Phase

Fix shape registry paging so clients using `ShapeReadQuery { after, limit }`
receive every shape exactly once.

In scope:

- define and document the cursor contract for `ShapeReadResult.next`;
- fix `SlotShapeRegistry::snapshot_page`;
- add registry-level and debug-UI-level tests for `limit = 1` paging.

Out of scope:

- changing slot root payload encoding;
- changing resource payload behavior;
- changing the debug UI polling strategy beyond what is needed for paging.

## Code Organization Reminders

- Keep tests at the bottom of each file.
- Keep helper functions below test functions inside test modules.
- Prefer focused helper names such as `collect_paged_shape_ids`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-wire/src/messages/project_read/shape_read.rs`
- `lp-cli/src/debug_ui/ui.rs`

Current bug:

- `snapshot_page(after, limit)` filters `id > after`.
- It currently returns `next` as the first omitted id.
- The debug UI sends `next` back as `after`.
- That skipped id is then filtered out by `id > after`.

Expected behavior:

- `next` is the cursor the client should send back as `after`.
- With an exclusive `after` query, `next` should be the last id included in the
  current page when more entries remain.
- If there are no more entries, `next` is `None` and `complete` is true.

Suggested implementation:

- In `SlotShapeRegistry::snapshot_page`, track the last included id.
- When iteration stops because `limit` was reached and there are more entries,
  return `Some(last_included_id)`.
- Handle `limit == 0` explicitly. Prefer returning an empty page with
  `next = after` only if the caller supplied one and there are still later
  entries; otherwise document and test the chosen behavior. If this gets awkward,
  clamp zero to one at the project-read layer.
- Update rustdocs/comments in `ShapeReadResult` so `next` means "cursor to pass
  as the next request's `after`".

Tests to add:

- Registry test with four known ids and `limit = 1`; repeatedly call
  `snapshot_page(cursor, 1)` until `next == None`, collect ids, and assert all
  four appear in order.
- Debug UI test that applies multiple shape pages through
  `apply_debug_ui_project_read_response` and confirms earlier pages remain.
  Extend the existing test rather than creating a parallel near-duplicate if it
  stays readable.

## Validate

```bash
cargo test -p lpc-model snapshot_page
cargo test -p lp-cli paged_shape_sync_keeps_prior_pages_when_final_page_is_complete -- --nocapture
```
