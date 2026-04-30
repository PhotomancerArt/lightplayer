# M4.4 — Domain trait + extended sync (placeholder)

Generalise `ProjectRuntime` over a `ProjectDomain` trait, and extend the
wire protocol with per-prop deltas so clients can mirror produced
runtime values, not just structural tree state.

References:

- [`../design/00-overview.md`](../design/00-overview.md)
- [`../design/01-tree.md`](../design/01-tree.md)
- [`../design/05-slots-and-props.md`](../design/05-slots-and-props.md)
- [`../design/07-sync.md`](../design/07-sync.md)

## Status

**Not yet planned.** Waiting on **M4.3a** (crate split + `WireValue`,
[`../m4.3a-crate-split-wire-value/plan.md`](../m4.3a-crate-split-wire-value/plan.md))
which itself waits on M4.3 (runtime spine). The `PropsChanged` delta
introduced here is the first real wire load for produced values;
designing it against `WireValue` (not `LpsValue`) is cleaner than
retrofitting after.

When M4.3 commits, expand this file via `/plan-small` (or `/plan` if
the scope grows) and replace this placeholder with the real plan.

## Tentative scope (subject to plan iteration)

- `lpc-runtime::ProjectDomain` trait — abstracts domain-specific
  artifact instantiation (`fn instantiate(spec, ...) -> Box<dyn
  Node>`), domain-specific status mapping, and any per-domain
  resources `TickContext` exposes.
- `lpc-runtime::ProjectRuntime<D: ProjectDomain>` — the central
  engine runtime, parameterised over the domain. Replaces / wraps
  the legacy `ProjectRuntime` flat-map.
- `lpc-model::TreeDelta::PropsChanged` — wire variant for produced
  values: `{ id: NodeId, changed: Vec<(PropPath, LpsValue, FrameId)> }`.
  Authored against `PropAccess::iter_changed_since`.
- `lpc-model::NodeView.prop_cache` — client-side mirror of produced
  props, keyed by `PropPath`. `prop_cache_ver` frame counter.
- `lp-engine-client::apply_tree_delta` — handle `PropsChanged`,
  bump `prop_cache_ver`.
- `lpc-runtime::tree_deltas_since` — emit `PropsChanged` deltas
  alongside structural deltas, walking each `Alive` entry's
  `PropAccess::iter_changed_since(since)`.
- Plumb `ProjectDomain` through `lp-engine` / `lp-server` so
  existing visual / legacy entry points still work.

## Out of scope here

- `Node` trait + `TickContext` (M4.3).
- Resolver cache implementation (M4.3).
- Legacy node port to the new `Node` trait (M5).
- Cleanup + final validation (M6).
