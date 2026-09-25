# ADR: A Parent Node Owns Its Children's Residency; the Engine Applies It Before the Tick

- **Status:** Proposed (draft started in multi-pattern plan P3; P9 finalises it)
- **Date:** 2026-09-25
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

A piece should hold about 25 patterns under one playlist on an ESP32-C6.
Loading every entry costs about 17.6 KB of heap each, which the C6 cannot
spare. So only the playing entry can be loaded. Something has to decide
which entry is loaded, and something has to load and unload it safely on
abort-tier firmware, where an allocation that fails is fatal.

The planning discovery (`lp2025/2026-09-24-2351-multi-pattern-projects`,
`notes.md`) found four constraints:

- The registry re-derives the whole inventory on every edit. Dormancy has
  to be known to derivation, or the next edit re-adds every entry.
- `Engine::tick` has no filesystem and only a `&ProjectRegistry`. It cannot
  load anything.
- The top of a tick is already a safe point: no render borrow is live and no
  node is `Executing`. The compile window opens there
  (`2026-08-03-memory-pressure-at-compile-safe-points.md`).
- An archived design had the resolver wake `Pending` nodes when a binding
  first demanded them. It puts the decision in the dataflow graph, which
  only knows about loaded nodes.

## Decision

1. **Loading children is the parent's job** (vision D2). A node asks for a
   load or unload of its own entry-keyed children through a polled
   `NodeRuntime::residency_request() -> Option<ResidencyRequest>`, with at
   most one load and one unload. Only the playlist implements it (D3).
   Taking a request clears it.
2. **The registry owns the residency set** (plan PD1). Derivation stops at
   a dormant entry's `ref`. A dormant entry is absent everywhere: no tree
   node, no inventory rows, no bindings, no artifact locations. Only the
   playlist's own def remembers it. The default at load is the idle entry.
3. **The tick owner applies requests before the tick** (plan PD2).
   `Engine::apply_residency(fs, &mut registry)` runs in `lpa-server`
   `Project::tick`, the one path every edge ticks through (fw-esp32c6,
   fw-emu, fw-browser, the host server). Per request:
   - **Unload first**, so two entries are never held at once: registry
     re-derive, then remove the runtime subtree.
   - **Then load**: registry re-derive, project the spine, attach the
     subtree, then re-wire the whole projection (asset consumers, every
     binding, the resolver). The re-wire is the same code an edit uses.
   - **Tell the owner** through `entry_unloaded`, `entry_loaded(entry,
     child, output_slot)`, `entry_load_failed(entry, reason)` or
     `residency_refused(request, reason)`.
4. **A load failure never fails the tick.** A failure anywhere is rolled
   back: the registry, the attach, a node that attaches `Failed`, or the
   binding re-wire. Nothing is left half-attached. The owner is told, and
   decides what to play (P4: mark the entry failed and move on, never
   black).
5. **Pending edits never wedge a switch.** Transient edits are Debug-role
   overrides and produced paths. A commit never writes them and a reboot
   never keeps them, so the unload drops them. Edits a commit would write
   refuse the whole request, and the owner keeps playing what it has.
6. **Knob memory survives dormancy** (D12). Writers in an entry's sink
   scope are keyed by the playlist's id, which is stable. Writers in scopes
   owned by nodes inside the entry are keyed by ids that a reload replaces.
   A pattern module's own knobs are one example. These are parked by
   persist path on unload and re-engaged on load. Panel restore accepts
   every authored entry's scope. `/.lp/panel.json` is unchanged: it already
   keyed by persist path.
7. **The resolver never wakes nodes** (D6). The archived design is rejected.

## Consequences

- The switch is a structural edit, one entry at a time. Its cost is one
  registry re-derive per half, plus a whole-project binding rebuild. That
  is cheap at a switch and allocation-free when no node asks.
- A newly loaded child is alive and bound before the tick that follows. The
  owner can demand it in that tick. But a shader's first render only asks
  for a compile window and produces nothing, so a shader entry's first
  visual output arrives one tick later, when that window opens
  (`shader_node.rs`, the compile-window deferral). This is why the owner
  holds a frame across a switch (P4 measures the latency, A6).
- A parent whose def names children the tree does not hold is now normal.
  Anything that asks "does the project have X" from the live tree alone is
  suspect. The panel-restore defect
  (`docs/defects/2026-09-25-panel-restore-drops-dormant-entry-knobs.md`)
  was the first case found.
- Keep-last-good is reconciled with dormancy: a failed load leaves the
  owner's previous frame to the owner (P4 holds it). The engine keeps no
  half-loaded entry around as "last good".
- `LoadedProjectRuntime::tick` has no filesystem, so it does not apply
  residency. Direct embedders call `apply_residency` or
  `tick_with_residency`. The product's edges never use it.

## Alternatives Considered

- **Resolver-driven wake-up** (the archived design): rejected (D6). It hides
  loading inside the dataflow graph.
- **`make_only_resident` for a switch:** one registry re-derive instead of
  two. Rejected for now. The engine must remove before it attaches, and two
  calls make that order explicit.
- **Engine-side dormancy only**, where the registry derives every entry:
  rejected. Every edit would re-add them (planning discovery).
- **Keep writers keyed by `NodeId` and re-key persistence:** the file
  already keys by persist path. Only the in-memory store needed a stable
  key for the dormant window, which parking gives it.

## Follow-ups

- P4: the playlist produces requests, holds the lamp-sized frame, handles
  failure.
- P5: touring and next/prev.
- P6: the 25-entry proof. `entry-unload` / `entry-load` perf markers window
  each switch.
- P9: finalise this ADR with the measured numbers.
