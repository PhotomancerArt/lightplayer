# M4.3a — Crate split + `WireValue` (placeholder)

Architectural cleanup that decouples wire / disk / runtime concerns
in `lp-core/`, and replaces the wire-side use of
`lp-shader::LpsValue` with a focused `WireValue` enum.

References:

- This file is the placeholder; the full plan lands when we get
  here. Originating discussion in chat history during M4.2 plan
  iteration (April 2026).

## Status

**Not yet planned.** Sized as a focused refactor — comparable to
M2's `lp-domain → lpv-model` rename, plus the `WireValue` design.
Plan when M4.3 commits and we have concrete evidence about which
types cross which boundaries.

## Origin — why this exists

`lpc-model` is currently doing **four** jobs that should be three or
four crates:

1. **Foundation primitives** (`NodeId`, `NodeName`, `TreePath`,
   `PropPath`, `FrameId`, `Kind`, errors) — used by everyone.
2. **On-disk authored model** (`Slot`, `Shape`, `Artifact`,
   `NodeConfig`, `Binding`, TOML loading) — used by server +
   editor + filetests.
3. **On-wire protocol** (`Message`, `TreeDelta`, `EntryStateView`,
   future `NodeView`) — used by server + client.
4. **GLSL value bridging** — re-exports `LpsValue` from
   `lp-shader`, dragging the GPU/JIT stack transitively into
   anything that imports `lpc-model`.

The smoking gun: **`lp-engine-client` has to know about
`lps-shared::LpsTexture2DValue`** to receive a sync delta. The
client should not need GLSL types in its dependency closure.

A second symptom: `value_spec.rs` ships a private `LpsValueWire`
mirror enum precisely because the wire boundary should _not_ be
`LpsValue`. That's a workaround for missing structure — the
real answer is that `LpsValue` is GLSL-runtime-only, and the wire
needs a different type.

## Scope (sketch — refine when we get here)

### Proposed crate shape

```
┌─────────────────────────────────────────────────────────────┐
│ lpc-model           Foundation primitives.                  │
│   - NodeId, NodeName, TreePath, PropPath, FrameId           │
│   - Kind, Constraint, PropNamespace                         │
│   - WireValue (NEW — replaces LpsValue at the wire boundary)│
│   - errors                                                  │
│   no deps on lp-shader.                                     │
└────┬────────────────────────────────────────────────────────┘
     │
     ├──► lpc-artifact     On-disk authored model.
     │     - Slot, Shape, Binding, ValueSpec, NodeConfig
     │     - TOML loading, schema validation, migrations
     │     no deps on wire types.
     │
     ├──► lpc-protocol     Server↔client wire shapes.
     │     - Message, ClientMessage, ServerMessage
     │     - TreeDelta, EntryStateView, NodeView, ...
     │     depends on lpc-model only (and lpc-artifact for NodeConfig)
     │
     └──► lpc-runtime      Server-side runtime types.
           - NodeTree, NodeEntry, EntryState, ResolverCache, Bus
           - Boundary: LpsValue → WireValue conversion lives HERE
           depends on lpc-model + lpc-artifact + lpc-protocol

         lp-engine-client  Client mirror.
           depends on lpc-model + lpc-protocol
           NOT on lpc-artifact, NOT on lp-shader.
```

### `WireValue` enum (new in `lpc-model`)

A focused enum that excludes runtime-only variants:

```rust
pub enum WireValue {
    I32(i32), U32(u32), F32(f32), Bool(bool),
    Vec2(...), Vec3(...), Vec4(...),
    IVec2(...), IVec3(...), IVec4(...),
    UVec2(...), UVec3(...), UVec4(...),
    BVec2(...), BVec3(...), BVec4(...),
    Mat2x2(...), Mat3x3(...), Mat4x4(...),
    Array(Vec<WireValue>),
    Struct { name: Option<String>, fields: Vec<(String, WireValue)> },

    /// Texture by stable identity, plus optional metadata for the
    /// editor. The client never sees pixel handles; the editor renders
    /// previews from metadata or from a separate thumbnail channel.
    Texture(TextureRef),
}
```

- `lpc-runtime` owns the `From<&LpsValue> for WireValue` boundary
  conversion. Lossy on `Texture2D` (preserves descriptor id; drops
  storage handle).
- Backward conversion is `WireValue → ValueSpec`-recipe-driven, not
  direct (the wire doesn't carry real handles).
- The private `LpsValueWire` mirror in
  `lpc-model/src/value_spec.rs` retires; `ValueSpec::Literal`'s
  payload becomes `WireValue`.

### Concrete moves

- `lpc-model/src/{prop/shape, prop/binding, artifact/, value_spec, ...}` →
  new `lpc-artifact`.
- `lpc-model/src/{message, json, server, transport_error, tree/tree_delta, ...}` →
  new `lpc-protocol`.
- `lpc-model/src/{node, prop/prop_path, prop/prop_namespace, prop/kind,
prop/constraint, ...}` stays in `lpc-model` (foundation).
- `lpc-model::LpsValue` re-export retires; `WireValue` (new in
  `lpc-model`) replaces it everywhere except inside `lpc-runtime`'s
  conversion boundary.
- Update import sites across `lp-engine`, `lp-server`, `lp-client`,
  `lp-engine-client`, `lpfx`, filetest harness, etc.

## Out of scope (do NOT sneak this in earlier)

- M4.2 (schema types) ships `Binding::Literal(ValueSpec)` through
  the existing `lpc-model::LpsValue` re-export. The crate split
  _moves_ `Binding` between crates; it does not change `Binding`'s
  shape. Don't anticipate the move.
- M4.3 (runtime spine) ships `Node` / `TickContext` /
  `ArtifactManager` against the current `lpc-model` shape. Same
  reasoning.
- `WireValue`'s `TextureRef` design (thumbnails? metadata channel?
  stable id only?) lands when M4.4 (`PropsChanged` deltas) makes
  the texture-on-the-wire question concrete.

## When to plan this

After **M4.3 commits**. Reasons:

1. M4.3 finishes the runtime contract (`Node` trait, `TickContext`,
   `ArtifactManager`, resolver). Once that's in, we know exactly
   which types cross which boundaries — the seams to cut along
   become visible.
2. M4.4's `PropsChanged` delta is the first real wire load for
   produced values. Designing `WireValue` alongside the delta is
   cleaner than retrofitting after.
3. Doing the split as a focused milestone (M4.3a, slotting
   between M4.3 and M4.4) keeps each refactor narrow:
   - M4.3a: move files, add `WireValue`, no behaviour change.
   - M4.4: extend sync to ship produced props using the new
     `WireValue`.

If M4.4 lands first against the messy `lpc-model`, the cleanup
becomes more painful (more import sites to update).

## Decisions captured here

- **`WireValue` is the right answer, not adding serde to
  `LpsValue`.** Considered "just teach `lps-shared` serde";
  rejected because it propagates the architectural smell deeper
  (the dependency direction should not have client code transitively
  importing GLSL value types). The texture-handle problem is
  structural, not M2 expedience.
- **Crate split + `WireValue` together, not separately.** The
  split without `WireValue` leaves `lpc-protocol` depending on
  `lp-shader`. The `WireValue` without the split leaves
  `lpc-model` overloaded. They reinforce each other.
- **Timing: after M4.3, before M4.4.** Slotted as M4.3a so the
  number ordering preserves the dependency story.
