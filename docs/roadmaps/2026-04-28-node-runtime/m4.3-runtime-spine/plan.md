# M4.3 — Runtime spine

Planning stub for phased work once M4.3 is expanded.

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

**M4.3 detailed phases:** not expanded yet — replace this stub via
`/plan-small` (or `/plan`) when execution starts.

**M4.3a (crate split + `ModelValue`):** tracked in
[`../m4.3a-crate-split-wire-value/`](../m4.3a-crate-split-wire-value/); not
future work relative to this milestone. The runtime spine should assume the
five-crate roles below from the start.

## Tentative scope (subject to plan iteration)

- `lpc-engine::Node` trait — minimal shape from
  [`design/02-node.md`](../design/02-node.md):
  `tick(&mut self, &mut TickContext)`, `destroy`,
  `handle_memory_pressure`, `props() -> &dyn RuntimePropAccess` (or
  temporary `PropAccess` alias during transition; see M4.3a phase 5).
- `lpc-engine::TickContext` — bus access, resolver-cache access,
  tree access (read-only), frame counters.
- `lpc-engine::ArtifactManager` — load / cache / refcount / shed
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
  lives in a new `lpc-derive` proc-macro crate (or temporarily in
  `lpc-engine` until split).

## Out of scope here

- `ProjectDomain` trait + `ProjectRuntime<D>` cutover (M4.4).
- Per-prop sync deltas + extended wire (M4.4).
- Legacy node port (M5).
- Visual subsystem changes (next roadmap).
- Crate split / portable model values (`ModelValue`, …) — **M4.3a owns this** (see sibling folder).
  M4.3 assumes the crate split is done or underway; no "split after M4.3".
