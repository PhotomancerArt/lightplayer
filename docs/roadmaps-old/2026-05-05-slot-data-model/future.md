## Slot Derive Macro

- **Idea:** Add an `lpc-model-derive` crate so Rust-authored config/state structs
  can derive slot shape, slot data traversal, and value codec implementations.
- **Why not now:** The core model vocabulary and one manual runtime/source slice
  should stabilize before committing to macro APIs. A focused derive milestone is
  now part of the roadmap; this future item is for richer macro polish beyond
  that first milestone.
- **Useful context:** Prior art:
  `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data-derive`.

## Generic Wire Slot Sync

- **Idea:** Replace product-specific resource sync with generic slot-data sync
  plus resource payload requests by `ResourceRef`.
- **Why not now:** This depends on first-class `SlotData`, `SlotShape`, and
  `ModelValue::Resource`.
- **Useful context:** Current scaffolding is in
  `lp-core/lpc-wire/src/project/resource_sync.rs`.

## ModelValue Rename

- **Idea:** Rename `ModelValue` / `ModelType` to shorter shared-domain names
  such as `Value` / `ValueShape` or `LpValue` / `LpType`.
- **Why not now:** A rename would churn many files while the slot data concepts
  are still being shaped.
- **Useful context:** The user dislikes `ModelValue`, but agrees the rename can
  wait.

## Source Config Separation

- **Idea:** Refactor node defs so graph wiring stays on `*Def` while authoring
  fields move into Rust-authored `*Config` slot records.
- **Why not now:** First establish the shared slot data model and then migrate
  one node as a vertical slice.
- **Useful context:** `FixtureDef` currently mixes `RelativeNodeRef` wiring with
  config fields in `lp-core/lpc-source/src/node/fixture/fixture_def.rs`.

## Dynamic Shader Param Shapes

- **Idea:** Generate/register `SlotShape` records for shader params defined by
  shader artifacts.
- **Why not now:** Dynamic shape lifecycle depends on the registry design.
- **Useful context:** Shader params are a key reason static Rust-authored shapes
  are not enough.

## Server-Side Project Mutation

- **Idea:** Add server APIs that mutate loaded project slot data directly,
  instead of requiring file edits plus artifact reload for every small change.
- **Why not now:** This depends on stable slot paths, slot versions, and a clear
  source/runtime mutation model.
- **Useful context:** Moving one fixture shape by 10px should not require
  rewriting a file and reloading an artifact.

## Artifact Mutation Through Message API

- **Idea:** Enable artifact mutation through the server/message API so UI edits
  can update authored project data without replacing whole files or forcing
  heavy reload cycles.
- **Why not now:** This is likely its own roadmap-level effort after the slot
  data model can represent authored data and deltas.
- **Useful context:** This may be the next major capability after the slot model;
  it is what makes realistic UI editing possible over a low-bandwidth
  connection.

## Whole NodeDef Slot Modeling

- **Idea:** Treat most or all of a node definition as slot-shaped authoring data,
  including graph references such as output/texture refs.
- **Why not now:** Earlier discussion separated graph wiring from config, but
  server-side mutation may make that distinction too rigid.
- **Useful context:** If node refs are edited through the same mutation/UI model,
  `ModelValue` may need node-reference variants beyond scalar shader-ish values.

## Node References In ModelValue

- **Idea:** Add portable node-reference values to `ModelValue` so slot data can
  represent editable graph links.
- **Why not now:** Needs a settled reference vocabulary and mutation semantics.
- **Useful context:** `ResourceRef` is expected to move into `ModelValue`; node
  refs may follow for editable `NodeDef` fields.
