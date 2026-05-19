# Phase 2: Mutation Queue And Project Poll Wiring

## Scope Of Phase

Turn UI edit intents into real project-read mutations using the existing client
mirror validation path.

In scope:

- add outgoing mutation state to `DebugUiState`;
- prepare mutations through `ProjectView.slots.prepare_set_value(...)`;
- coalesce unsent mutations by `(root, path)`;
- drain queued mutations into `ProjectReadRequest`;
- apply mutation responses through the existing project-read response path.

Out of scope:

- new engine mutation capabilities;
- subscriptions or push messages;
- local optimistic value mutation.

## Code Organization Reminders

- Keep queue plumbing in `ui.rs` unless it grows into a clear separate concept.
- Keep `slot_edit.rs` free of server/client transport details.
- Prefer small helper methods on `DebugUiState` for queue operations.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-cli/src/debug_ui/ui.rs`
- `lp-cli/src/debug_ui/node_cards.rs`
- `lp-cli/src/debug_ui/slot_edit.rs`
- `lp-core/lpc-view/src/slot/mirror.rs`
- `lp-core/lpc-wire/src/messages/project_read/project_read_request.rs`

Expected changes:

- Add to `DebugUiState`:
  - next mutation id counter;
  - outgoing coalescing queue keyed by `(String, SlotPath)`;
  - last mutation id by `(String, SlotPath)` for row status.
- During the egui update:
  - collect edit intents while rendering node cards;
  - after rendering, lock `ProjectView` mutably;
  - call `view.slots.prepare_set_value(...)` for each intent;
  - record path-to-id status;
  - replace any older unsent queued mutation for the same root/path.
- Update `poll_project_if_due` and `debug_ui_project_read(...)` so queued
  mutations are drained into the spawned request.
- Ensure failed local prepare errors surface in `last_error` instead of
  panicking.

Edge cases:

- If a poll is already in flight, keep queued mutations for the next poll.
- If multiple slider changes happen before the next poll, only the latest
  unsent value for that slot should be sent.
- Do not clear mirror pending state locally; response application already does
  that.

## Validate

```bash
cargo check -p lp-cli
cargo test -p lpc-view
```
