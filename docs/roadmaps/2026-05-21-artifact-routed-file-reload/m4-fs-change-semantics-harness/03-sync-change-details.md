# Phase 03 — DefChangeDetail in sync diff

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

`sync` populates **`change_details`** by diffing def state snapshot (inside sync).

**In scope:**

- `DefChangeDetail` enum in `sync_result.rs`
- During sync: snapshot def states before apply; after re-derive, classify each
  `changed` id → `Content`, `KindChanged`, `EnteredError`, `LeftError`
- Unit tests for detail classification

**Out of scope:** Integration file, engine policy.

## Validate

```bash
cargo test -p lpc-node-registry sync
```
