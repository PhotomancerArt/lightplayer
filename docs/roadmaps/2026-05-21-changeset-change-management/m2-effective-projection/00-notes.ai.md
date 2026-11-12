# M2 Notes — Effective Projection

## Scope

Wire **effective reads** (overlay ∪ committed base) through `NodeDefRegistry` and
`NodeDefView`. Prove D1 extension: apply changes what callers **see**, not what
`entries` stores.

**Out of scope:** slot-draft overlay (M4), commit (M5), materialize overlay
(M3), `*DefView` typed accessors, provenance.

## Current codebase (post-M1)

```
lp-core/lpc-node-registry/src/
├── change/overlay.rs           # OverlayEntry::{Deleted, Bytes}
├── registry/node_def_registry.rs
│   ├── store.read_bytes      # committed artifact bytes only
│   ├── read_artifact_state   # parse via store (no overlay)
│   └── apply_change / discard_overlay
└── view/node_def_view.rs     # passthrough registry.get (committed)
```

M1 tests prove overlay + committed `entries` diverge on apply, but **no read path
uses overlay yet**.

## Resolved design (2026-05-21)

- **`entries` / `registry.get`** — committed cache only (unchanged on apply).
- **`NodeDefView`** — **effective only**; requires `fs` + `ParseCtx` to parse
  overlay bytes when present.
- **M2 overlay semantics** — whole-file `SetBytes` / `Delete` only (same as M1
  apply). Slot-draft merge deferred to M4.
- **Parse strategy** — on-read: if overlay touches artifact path, parse overlay
  bytes → `NodeDefState`; else clone committed entry state. No persistent
  effective cache in M2 (discard/apply is cheap enough for harness).
- **`read_effective_bytes(path)`** — single choke point: overlay → store/fs.

## Open questions

### Q1: NodeDefView::get signature

- **Context:** Effective parse needs `fs` + `ParseCtx`. Current `get(&NodeDefId)
  -> Option<&NodeDefEntry>` borrows committed entries.
- **Suggested answer:** Change to owned effective entry:
  `get(id, fs, ctx) -> Option<NodeDefEntry>`. Crate has no external callers yet;
  engine cutover is M6.

### Q2: Deleted overlay path

- **Context:** `OverlayEntry::Deleted` on a loaded artifact.
- **Suggested answer:** Effective parse returns `NodeDefState::ParseError` with
  read failure shape (same as missing file from store). Harness asserts view
  shows error/parse failure vs committed loaded state.

### Q3: Path-only overlay (implicit create)

- **Context:** M1 allows overlay on paths not in `artifact_path_to_id`.
- **Suggested answer:** `NodeDefView` only resolves existing `NodeDefId`s.
  Implicit-create paths are visible via `overlay_bytes` until commit registers
  them. No phantom defs in view until M5 commit + register.

### Q4: materialize_source in M2?

- **Context:** Asset reads should eventually use overlay (M3).
- **Suggested answer:** **Defer to M3.** M2 only routes def parse +
  `read_effective_bytes`; materialize keeps store path until M3.

## User stories

| ID | Story | How |
|----|-------|-----|
| D1 ext | Apply → **view** ≠ committed `entries` | SetBytes on `/clock.toml`; view new rate, `registry.get` old |

## Validation baseline

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test overlay_lifecycle
cargo test -p lpc-node-registry --test fs_change_semantics
```
