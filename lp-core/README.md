# lp-core

Core LightPlayer engine crates.

`lp-core` is about the internals of one LightPlayer engine: the authored
source model it loads, the runtime that ticks it, the wire/view shapes used to
observe it, and small shared utilities used by those pieces.

Application-level orchestration lives outside this directory. Code that starts
servers, opens transports, talks to firmware, or coordinates one or more
engines belongs in `lp-app`, `lp-fw`, or another app-facing layer.

## Crates

- `lpc-model` — shared core vocabulary: ids, paths, frame ids, kinds,
  `ModelValue`, and `ModelType`.
- `lpc-source` — authored/on-disk source model: artifacts, slots, source
  bindings, value specs, TOML loading, and schema migration.
- `lpc-wire` — engine/client wire contract: messages, tree deltas, project
  requests, transport errors, JSON helpers, and partial state serialization.
- `lpc-engine` — runtime for one loaded engine/project, including node trees,
  resolver caches, buses, shader/runtime value conversion, and execution.
- `lpc-view` — client-side view/cache for one engine, built from `lpc-wire`
  updates.
- `lpc-shared` — small shared support utilities used by core/app crates.

## Naming boundaries (M4.3b)

Each `lpc-*` crate uses a consistent prefix policy so parallel concepts in
model, source, wire, view, and engine do not collide:

- **`lpc-model`** — foundational shared nouns stay unprefixed (`NodeId`,
  `TreePath`, …). Portable value/type shapes use `Model*` (`ModelValue`,
  `ModelType`, `ModelStructMember`). No `Wire*` types here.
- **`lpc-source`** — exported authored schema types use `Src*` (`SrcArtifact`,
  `SrcBinding`, …). No short root aliases; call sites use the `Src*` names.
- **`lpc-wire`** — `Message`, `ClientRequest`, and similar already imply the
  wire; use `Wire*` only to disambiguate payload nouns (`WireTreeDelta`,
  `WireNodeSpecifier`, `WireSlotIndex`, …).
- **`lpc-view`** — local cache/view structs use natural `*View` suffixes
  (`ProjectView`, `PropAccessView`, …). Reserve `Client*` for real
  client-facing API surfaces (e.g. `ClientApi`), not mirror data.
- **`lpc-engine`** — runtime nouns stay natural when ownership is obvious
  (`ProjectRuntime`, `NodeTree`, …). Conversion helpers name both sides of the
  boundary (e.g. model vs `LpsValueF32`).

## Dependency Shape

`lpc-model`, `lpc-source`, `lpc-wire`, and `lpc-view` should stay free of
shader runtime dependencies. `lpc-engine` owns the boundary to `lps-*` crates
because compiling and executing GLSL is engine behavior.

Most crates here are `no_std`-compatible so the same engine core can run on
host tools and embedded firmware.
