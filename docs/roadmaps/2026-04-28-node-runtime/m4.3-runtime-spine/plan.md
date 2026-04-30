# M4.3 — Runtime spine (placeholder)

Land the **runtime contract** for nodes on top of the M4.2 schema layer:
the `Node` trait, `TickContext`, the `ArtifactManager` state machine,
the binding resolver, slot view wrappers, and the generalised TOML
artifact loader.

References:

- [`../design/02-node.md`](../design/02-node.md)
- [`../design/03-artifact.md`](../design/03-artifact.md)
- [`../design/04-config.md`](../design/04-config.md)
- [`../design/06-bindings-and-resolution.md`](../design/06-bindings-and-resolution.md)

## Status

**Not yet planned.** Waiting on M4.2 (schema types) to settle the data
shape that the runtime contract reads / writes.

When M4.2 commits, expand this file via `/plan-small` (or `/plan` if
the scope grows) and replace this placeholder with the real plan.

## Tentative scope (subject to plan iteration)

- `lpc-runtime::Node` trait — minimal shape from
  [`design/02-node.md`](../design/02-node.md):
  `tick(&mut self, &mut TickContext)`, `destroy`,
  `handle_memory_pressure`, `props() -> &dyn PropAccess`.
- `lpc-runtime::TickContext` — bus access, resolver-cache access,
  tree access (read-only), frame counters.
- `lpc-runtime::ArtifactManager` — load / cache / refcount / shed
  with the `Resolved | Loaded | Prepared | Idle | Error` state
  machine ([`design/03-artifact.md`](../design/03-artifact.md)).
- Binding resolver — pull-based three-layer cascade (overrides →
  artifact `bind` → slot default), populating the resolver cache
  from M4.2.
- Slot view wrappers — read API across the four namespaces
  (params / inputs / outputs / state).
- Generalised TOML artifact loader — replaces the `std`-only
  one-shot loader in `lpv-model`. `no_std`-compatible.
- `PropAccess` derive macro (if not already shipped in M4.2) —
  lives in a new `lpc-derive` proc-macro crate.

## Out of scope here

- `ProjectDomain` trait + `ProjectRuntime<D>` cutover (M4.4).
- Per-prop sync deltas + extended wire (M4.4).
- Legacy node port (M5).
- Visual subsystem changes (next roadmap).
- **Crate split / `WireValue` introduction** — see
  [`../m4.3a-crate-split-wire-value/plan.md`](../m4.3a-crate-split-wire-value/plan.md).
  M4.3 ships against the current `lpc-model` shape; the split lands
  as a focused refactor between M4.3 and M4.4.
