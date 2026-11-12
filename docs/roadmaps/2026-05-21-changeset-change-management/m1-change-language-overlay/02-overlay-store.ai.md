# Phase 02 — ChangeOverlay Store

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Implement path-keyed **`ChangeOverlay`** and attach to **`NodeDefRegistry`**.

**In scope:**

- `change/overlay.rs` — `ChangeOverlay`, `OverlayEntry::{Deleted, Bytes}`
- `BTreeMap<String, OverlayEntry>` keyed by absolute path string (match
  `artifact_path_to_id` convention)
- `ChangeOverlay::clear`, `is_empty`, `contains`, `get`
- Add `overlay: ChangeOverlay` field to `NodeDefRegistry::new()`
- Introspection methods on registry (see design): `overlay_active`,
  `overlay_contains_path`
- Unit tests: empty overlay; manual insert/get in overlay tests only

**Out of scope:** apply pipeline, target resolution, registry `apply_change`.

## Sub-agent reminders

- Do not commit.
- Do not mutate `store` or `entries` from overlay module.

## Implementation details

**Path key:** use `LpPathBuf::as_str()` or existing registry helper for stable
keys. Document that keys must be absolute.

**Registry field:** private `overlay` with public read-only introspection;
mutation only via apply/discard in phase 03.

**Default:** `ChangeOverlay::default()` empty map.

## Validate

```bash
cargo test -p lpc-node-registry overlay
cargo check -p lpc-node-registry
```
