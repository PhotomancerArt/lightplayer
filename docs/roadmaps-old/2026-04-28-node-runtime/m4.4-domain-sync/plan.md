# M4.4 — Sync + view projection (placeholder)

Project the M4.3 engine runtime spine over the wire and into `lpc-view`.
M4.3 owns engine-side runtime contracts and value resolution; M4.4 owns
the client-facing sync/view mirror of that runtime state.

References:

- [`../design/00-overview.md`](../design/00-overview.md)
- [`../design/01-tree.md`](../design/01-tree.md)
- [`../design/05-slots-and-props.md`](../design/05-slots-and-props.md)
- [`../design/07-sync.md`](../design/07-sync.md)

## Status

**M4.3 spine landed** in `lpc-engine` (phases 01–07 under
[`../m4.3-runtime-spine/`](../m4.3-runtime-spine/)): `Node`, `TickContext`,
`ArtifactManager`, resolver cascade, `SrcNodeConfig` on spine `NodeEntry`,
and source artifact load orchestration — still **side-by-side** with legacy
`ProjectRuntime`.

**Planning:** This file remains the M4.4 placeholder until expanded via
`/plan-small` (or `/plan`).

**Boundary:** M4.3a/M4.3b define the crate split and naming boundary:
`ModelValue` / `ModelType`, engine-side `RuntimePropAccess` (`LpsValueF32`),
view-side `PropAccessView` (`ModelValue`), and `lpc-wire` tree deltas. M4.4
does not re-plan those engine contracts; it consumes them for sync/view.

## Tentative scope (subject to plan iteration)

- `lpc-wire` produced-prop delta shape — likely a new
  `PropsChanged`-style delta carrying `ModelValue` entries converted
  by `lpc-engine` from `RuntimePropAccess`.
- `lpc-view` node view prop cache — client-side mirror of produced
  props, keyed by `PropPath`. `prop_cache_ver` frame counter.
- `lpc-view::apply_tree_delta` / related apply path — handle
  produced-prop deltas and bump `prop_cache_ver`.
- `PropAccessView` integration — expose cached produced props to
  editor/UI code using `ModelValue`.
- `lpc-engine` sync adapter only as needed — walk each alive entry's
  `RuntimePropAccess::iter_changed_since(since)`, convert through
  `lps_value_f32_to_model_value`, and emit wire payloads. The
  runtime access itself belongs to M4.3.
- Initial snapshot path for watched node details, including produced
  props as well as structural node data.

## Out of scope here

- `Node` trait + `TickContext` (M4.3).
- Artifact manager / artifact refs (M4.3).
- Resolver cache implementation and binding resolution (M4.3).
- Engine-side `NodeProp` dereference (M4.3).
- Legacy node port to the new `Node` trait (M5).
- Cleanup + final validation (M6).
