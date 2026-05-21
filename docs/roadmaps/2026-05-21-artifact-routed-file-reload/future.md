# Future Work — Artifact-Routed Reload

## Project diff → ChangeSet stream

- **Idea:** Given two project snapshots (directories or in-memory stores),
  compute an ordered `ChangeOp` sequence that transforms base → target. Same
  vocabulary as client edits and M5 user stories (A compose, B morph).
- **Why not now:** M5 proves manual / story-driven ChangeSets and view/commit
  semantics first. Diff needs stable slot paths, asset identity, and inline-def
  path rules from M2–M5.
- **Useful context:** Hand-written morph stories (`B1` `basic → basic2`) are
  regression fixtures; diff generalizes to arbitrary `examples/*` pairs. Output
  should be replayable one op at a time for stress testing.

## ChangeSet replay stress harness (host / emu / device)

- **Idea:** Record or generate a ChangeSet log; replay through full engine
  (post-M6) with configurable granularity (batch commit vs per-op apply). One
  high-level test name (`empty → fyeah-sign`) drives hundreds/thousands of
  incremental mutations.
- **Why not now:** Requires M6 engine on ChangeSet path and stable wire or
  in-process apply API; M5 only proves registry harness.
- **What it catches:** Panics on partial graph states, OOM spikes on ESP32,
  allocator fragmentation from repeated compile/prepare cycles, refcount leaks
  on artifact bump — failures whole-reload tests rarely trigger.
- **Useful context:** Same log runs on `cargo test`, `fw-emu`, and on-device CI
  when available; compare heap high-water marks across targets.

## Binary file sources (`BinaryFileSlot`)

- **Idea:** Sibling to `SourceFileSlot` for byte payloads (textures, binary blobs) with the same file-or-inline authored shape and engine-side artifact registration + resolution.
- **Why not now:** This roadmap covers text sources (GLSL, SVG) and node TOML reload; no current node def field needs binary file-or-inline yet.
- **Useful context:** Same TOML encoding as `SourceFileSlot` (`$path` or extension key); inline values are base64. See roadmap `notes.md` § SourceFileSlot TOML encoding.

## Last-good state on reload failure

- **Idea:** Retain last-good parsed defs / compiled shaders when a hot reload fails (e.g. bad GLSL edit).
- **Why not now:** v1 propagates errors — def error state destroys dependent nodes; parents cascade to error. Simpler semantics, easier to test.
- **Useful context:** Roadmap `notes.md` § Error semantics.

## `project.toml` / graph reconciliation

- **Idea:** Engine applies tree add/remove/repoint when parent invocation maps
  change; registry reports def-level `NodeDefUpdates`. Incremental add/remove
  of top-level nodes when root project artifact changes.
- **Why not now:** Leaf node TOML + source file reload covers most edit loops;
  requires def-vs-child-def vs wiring distinction solid first (M8).
- **Useful context:** `artifact_nodes` inverse index; `Engine` tree mutation
  APIs; roadmap `notes.md` Q1.

## ChangeSet wire protocol + CRDT merge

- **Idea:** Full `lpc-wire` ChangeSet messages; concurrent edit merge.
- **Why not now:** M5 proves in-memory ordered ChangeSet + commit/discard in harness.
- **Useful context:** M5 milestone; `lightplayer-app-ui` overlay mockup for SlotOp reference.

## Artifact digest / unchanged-write filtering

- **Idea:** Cheap stat/digest to avoid bumping `content_frame` on no-op writes.
- **Why not now:** Filesystem change events as version bumps are sufficient for first pass; ESP32 cost of retaining or hashing full sources is undesirable until proven necessary.
- **Useful context:** Plan notes in original `docs/plans/2026-05-21-artifact-routed-file-reload/00-notes.md`.

## Library artifact locators

- **Idea:** `ArtifactLocator::Lib(...)` resolved through a library namespace.
- **Why not now:** File-backed reload first; `ArtifactLocation::try_from_src_spec` already rejects lib locators.
