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

**Not yet planned.** M4.3a (**in progress / executed per sibling folder**) already
defines the crate split, `ModelValue` / `ModelType`, `RuntimePropAccess` (`LpsValueF32`) vs
`PropAccessView`, and where tree deltas live. M4.4 builds on that — design
`PropsChanged` against `lpc-wire` payloads (`ModelValue`), not as future
dependency on an unstarted split.

When M4.3 runtime-spine phases land, expand this file via `/plan-small`
(or `/plan`).

## Tentative scope (subject to plan iteration)

- `lpc-engine::ProjectDomain` trait — abstracts domain-specific
  artifact instantiation (`fn instantiate(spec, ...) -> Box<dyn
  Node>`), domain-specific status mapping, and any per-domain
  resources `TickContext` exposes.
- `lpc-engine::ProjectRuntime<D: ProjectDomain>` — the central
  engine runtime, parameterised over the domain. Replaces / wraps
  the legacy `ProjectRuntime` flat-map.
- `lpc-wire::WireTreeDelta::PropsChanged` — wire variant for produced
  values, carrying `ModelValue` entries converted by `lpc-engine`.
  Authored against `RuntimePropAccess::iter_changed_since`.
- `lpc-view` node view prop cache — client-side mirror of produced
  props, keyed by `PropPath`. `prop_cache_ver` frame counter.
- `lpc-view::apply_tree_delta` — handle `PropsChanged`,
  bump `prop_cache_ver`.
- `lpc-engine::tree_deltas_since` — emit `PropsChanged` deltas
  alongside structural deltas, walking each `Alive` entry's
  `RuntimePropAccess::iter_changed_since(since)`.
- Plumb `ProjectDomain` through `lp-engine` / `lp-server` so
  existing visual / legacy entry points still work.

## Out of scope here

- `Node` trait + `TickContext` (M4.3).
- Resolver cache implementation (M4.3).
- Legacy node port to the new `Node` trait (M5).
- Cleanup + final validation (M6).
