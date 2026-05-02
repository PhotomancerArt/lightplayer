# Phase 6: Client resource cache

## Scope of phase

Teach `lpc-view` to apply resource summaries/payloads and resolve semantic
resource refs for the temporary dev UI/helpers.

In scope:

- Add a resource cache to `ProjectView` or a new `resource_view` module.
- Apply resource summaries and payloads from `GetChanges`.
- Update helper methods such as `get_texture_data` and `get_output_data` to use
  cache-backed resource refs where appropriate.
- Keep current dev UI behavior plain but functional.

Out of scope:

- Fancy UI panes.
- Persistent server subscriptions.
- Source reload/deletion handling.

## Code organization reminders

- Keep cache types in their own module if they grow.
- Public cache API first, helper functions at the bottom.
- Keep compatibility bridge code clearly named.
- Avoid duplicating payload bytes unnecessarily beyond the client cache.

## Sub-agent reminders

- Do not commit.
- Do not weaken view tests.
- Do not add hidden server state.
- If wire fields are insufficient for view behavior, stop and report.

## Implementation details

Read:

- `lp-core/lpc-view/src/project/project_view.rs`
- `lp-core/lpc-view/src/lib.rs`
- `lp-core/lpc-wire/src/legacy/project/api.rs`
- `00-design.md`

Add a client-side cache that can store:

- resource summaries by `ResourceRef`;
- runtime-buffer payload bytes by `ResourceRef`;
- render-product materialized texture payload bytes by `ResourceRef`.

Update `ProjectView::apply_changes` to apply resource sections before or after
node details in a deterministic order.

Update existing helper methods:

- `get_output_data` should follow `OutputState.channel_data` resource refs into
  the cache.
- `get_texture_data` should follow texture/render-product refs into the cache
  where applicable.
- Existing inline compatibility snapshots should still work if present.

Add tests for:

- applying summaries without payloads;
- applying payloads and resolving helpers;
- pruning stale cached summaries when domain summary id set changes;
- node details no longer leave watched entries with `state: None`.

## Validate

Run:

```bash
cargo test -p lpc-view project
cargo test -p lpc-view client
cargo test -p lpc-engine --test partial_state_updates
```
