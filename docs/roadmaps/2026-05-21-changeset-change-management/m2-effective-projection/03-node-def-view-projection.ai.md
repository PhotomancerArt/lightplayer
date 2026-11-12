# Phase 03 — NodeDefView Projection

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Wire `NodeDefView` to effective parse; add integration tests.

**In scope:**

- `NodeDefRegistry::view(&self) -> NodeDefView`
- Update `NodeDefView::get(id, fs, ctx) -> Option<NodeDefEntry>` — owned effective
- Update `NodeDefView::state(id, fs, ctx) -> Option<NodeDefState>`
- Clone committed entry shell (`id`, `source`, `last_seen_revision`); replace
  `state` with effective parse at entry's artifact root when overlay active on
  that artifact path; else clone committed state
- `tests/effective_projection.rs` — D1 extension tests from design
- Update `view/mod.rs` docs (remove M5 stub comment)

**Out of scope:** materialize (M3), changing `registry.get` semantics.

## Test scenarios

1. **toml setbytes** — apply new clock.toml body; view shows new rate; `registry.get` old
2. **no overlay** — view matches committed
3. **discard** — view matches committed after discard
4. **delete overlay** — view parse error on that def; committed still loaded

## Sub-agent reminders

- Do not commit.
- `registry.get` stays committed-only.

## Validate

```bash
cargo test -p lpc-node-registry --test effective_projection
cargo test -p lpc-node-registry
```
