# Phase 4: View Apply Path

## Scope Of Phase

Teach `lpc-view` and `lpa-client` to use the new read response shape.

In scope:

- Add `apply_project_read_response`.
- Make `ProjectView` node-centric around revision, node tree, slot mirror, and
  resource cache.
- Remove watch-shaped fields/methods when replaced.
- Add client helper/options for `ProjectReadRequest`.
- Add tests applying a full read response.

Out of scope:

- Generic UI rendering.
- Mutation UI.
- Persistent subscriptions.

## Code Organization Reminders

- Put project apply logic in `lp-core/lpc-view/src/project/apply_project_read.rs`.
- Keep existing `NodeTreeView`, `SlotMirrorView`, and `ClientResourceCache`
  instead of inventing parallel mirrors.
- Keep tests at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-view/src/project/project_view.rs`
- `lp-core/lpc-view/src/project/resource_cache.rs`
- `lp-core/lpc-view/src/tree/apply.rs`
- `lp-core/lpc-view/src/slot/apply.rs`
- `lp-app/lpa-client/src/client.rs`

The client helper should replace get-changes-ish naming with read naming.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-view
cargo check -p lpa-client
```
