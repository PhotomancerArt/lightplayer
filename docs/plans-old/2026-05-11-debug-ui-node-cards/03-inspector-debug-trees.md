# Phase 3: Inspector Debug Trees

## Scope Of Phase

Fill out the right-side inspector with nodes, resources, and shapes.

In scope:
- Node tree list with current selection.
- Resource summary list.
- Shape id list.
- Detail box below the lists.
- Node details use the existing recursive slot debug view.
- Resource details show summary metadata and payload availability.
- Shape details show a compact shape tree/debug description.

Out of scope:
- Resource payload requests.
- Product probes.
- Binding debug panel unless data is already trivially available.

## Code Organization Reminders

- If splitting files, put this in `inspector.rs`.
- Keep selected-item details below tree/list controls.
- Avoid making `mod.rs` a code-hiding file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:
- `lp-cli/src/debug_ui/ui.rs`
- Optional: `lp-cli/src/debug_ui/inspector.rs`
- `lp-core/lpc-view/src/project/resource_cache.rs` if read-only iterator helpers are needed.

Expected changes:
- Add read-only iterator helpers to `ClientResourceCache` only if the UI cannot list summaries cleanly with existing APIs.
- Inspector sections:
  - `Nodes`
  - `Resources`
  - `Shapes`
- On click, update `InspectorSelection`.
- Detail renderer:
  - `Node`: path/status/tree metadata plus recursive slot detail.
  - `Resource`: domain/id/revision/metadata summary.
  - `Shape`: shape id/revision and debug tree.

## Validate

```bash
cargo fmt --check
cargo test -p lp-cli --no-run
cargo test -p lpc-view
```

