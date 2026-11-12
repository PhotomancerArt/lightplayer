# Project Load Memory Notes

## Scope

Reduce heap used by project loading, the resident node graph, artifact metadata,
and supporting indexes before shader compilation begins. This roadmap does not
move shader compilation off-device, feature-gate the compiler, or weaken the
runtime GLSL JIT path.

## User Context

- `button-sign` now gets past initial project load on ESP32-C6 but reaches only
  about 81 KB free before later compile work, which is too tight for reliable
  operation.
- The current failure mode is no longer primarily compiler memory. The suspect
  area is project load, node graph residency, artifact storage, and resolver
  structure.
- The graph/artifact representation appears to cost on the order of 150 KB in
  the serial trace. A plausible target for graph-specific resident memory is
  closer to 50 KB, pending measurement.
- `lp-cli profile` can help, but the existing startup profile includes first
  frame/render activity, so it needs a more precise load-only mode or event.

## Observed Shape

- `lp_perf::EVENT_PROJECT_LOAD` exists, but project loading does not appear to
  emit begin/end events for it yet.
- `lp-cli profile --mode startup` stops at first frame, which is useful for
  pressure testing but too broad to isolate load-only memory.
- `Engine::with_services` registers authored slot shapes for each engine, which
  likely puts static schema-like data on the per-project heap.
- `ArtifactStore` keeps `NodeDef` values resident even after runtime nodes have
  been attached.
- Runtime node attachment clones pieces of config out of authored definitions,
  so authored definitions and runtime structures can both be live.
- `NodeTree`, artifact lookup, binding indexes, slot maps, and path lookup use
  general-purpose map/string-heavy structures that are comfortable on host but
  expensive on a microcontroller.
- `NodeArtifact::read_toml` parses into `toml::Value` before converting to typed
  `NodeDef`, which can inflate peak load memory and allocator churn.

## Initial Evidence

Two startup allocation profiles gave a useful directional signal:

- `examples/button-sign`, startup mode: live end about 168 KB.
- `examples/basic`, startup mode: live end about 152 KB.
- The delta is only about 16 KB in that profile, so the base project/runtime
  representation may be the larger issue than `button-sign` alone.

Because startup includes first-frame work, this should be treated as evidence
for prioritization, not as final accounting.

## Working Assumptions

- Static slot shape data should be shared or flash-resident where possible.
- Authored project definitions are cold data after load. Runtime execution wants
  compact handles, typed parameters, bindings, and state.
- Client/debug APIs may still need access to authored definitions, but that can
  come from flash reload, a cold cache, or an opt-in diagnostic path rather than
  always-resident heap.
- General-purpose maps should be justified by mutation/query needs. Many project
  load indexes may be build-once, query-many tables.

## Questions To Settle

- What is the load-only resident budget before compilation starts?
- Which project editing/debug APIs require authored `NodeDef` access on device?
- Which indexes must support live mutation, and which can become compact frozen
  tables after project load?
- How much of the measured memory is resident data versus temporary parse/load
  peak or heap fragmentation?
