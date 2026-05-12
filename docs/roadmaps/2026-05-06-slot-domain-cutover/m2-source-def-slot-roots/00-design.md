# M2 Source Def Slot Roots Design

## Scope Of Work

M2 converts real `lpc-source` authored node definitions into production slot
roots. Source defs become slot-aware domain objects that can be registered,
walked, snapshotted, diffed, and eventually edited through the same generic slot
model proven in `lpc-slot-mockup`.

In scope:

- Convert core source defs to slot-aware fields and `#[derive(SlotRecord)]`:
  - `ProjectDef`
  - `NodeInvocation`
  - `TextureDef`
  - `ShaderDef`
  - `ShaderParamDef`
  - `OutputDef`
  - `OutputDriverOptionsConfig`
  - `FixtureDef`
  - fixture mapping structs/enums
- Add generated static slot-shape bootstrap to `lpc-source`.
- Preserve `lpc-source` `no_std + alloc`.
- Remove defunct `uid` from `examples/basic/project.toml`.
- Convert `examples/basic` fixture mapping away from arrays and toward stable
  keyed maps.
- Add source-side tests that register shapes, load TOML, snapshot roots, and
  print/assert generic slot paths.
- Fix downstream compile fallout from source fields becoming slot-aware.

Out of scope:

- Runtime node state/output/param slot roots.
- Replacing project wire sync or legacy node detail projection.
- Client-driven production source/artifact mutation.
- Final generic UI rendering.
- Full resource sync cleanup.
- Multiple real output node subtypes.

## File Structure

```text
lp-core/
  lpc-source/
    build.rs
    Cargo.toml
    src/
      lib.rs
      node/
        node_invocation.rs
        project/mod.rs
        texture/texture_def.rs
        shader/
          mod.rs
          shader_def.rs
          shader_param_def.rs
        output/output_def.rs
        fixture/
          fixture_def.rs
          mapping.rs
      tests/
        mod.rs
        source_slot_roots.rs
        source_slot_fixture.rs

examples/basic/
  project.toml
  texture.toml
  fixture.toml
```

Expected supporting changes may touch:

```text
lp-core/lpc-model/src/slot/slots/
lp-core/lpc-engine/src/project_runtime/
lp-cli/src/commands/create/
lp-app/lpa-client/src/
lp-app/lpa-server/src/
```

Only touch downstream crates where source API changes require it.

## Architecture Summary

`lpc-source` owns static source shapes. Its Rust-authored source defs derive
`SlotRecord`, expose `StaticSlotAccess`, and are registered through a generated
`slot_shapes` module emitted by `lpc-slot-codegen` at build time.

```text
lpc-source source def
  #[derive(SlotRecord)]
  #[slot(root)]
        |
        v
StaticSlotShape + SlotAccess
        |
        v
lpc-source/build.rs -> OUT_DIR/slot_shapes.rs
        |
        v
source tests / later engine register static source roots
        |
        v
lpc-wire snapshot/diff over SlotAccess roots
```

Source defs store versioned slot-aware fields directly. This is intentional:
source data is not plain inert data anymore. It is authored, versioned,
structured domain state. Reads that need the raw domain value should use
`.value()`, and writes should use slot-aware mutation methods when they exist.

Static source shapes are type-owned because their field layout is Rust-authored.
Dynamic runtime roots whose shape varies by artifact or instance remain outside
M2 and must use artifact-/instance-owned `SlotShapeId`s.

## Main Components And Interactions

### Source Shape Bootstrap

`lpc-source` adds a `build.rs` that calls `lpc-slot-codegen` and includes:

```rust
pub mod slot_shapes {
    include!(concat!(env!("OUT_DIR"), "/slot_shapes.rs"));
}
```

Tests and later engine code can call:

```rust
lpc_source::slot_shapes::register_all_static_slot_shapes(&mut registry)?;
```

### Project And Invocation Defs

`ProjectDef` remains the root project artifact, but `uid` is not revived. The
old example field is removed. `kind` remains loader/discriminator data and is
not part of editable/display slot roots.

`ProjectDef.nodes` becomes `MapSlot<String, NodeInvocation>` or an equivalent
stable-key map. `NodeName` may remain the engine/runtime naming type elsewhere,
but authored slot traversal should use map keys.

`NodeInvocation.artifact` becomes an artifact-path-like slot field. `overrides`
remain transitional and may be skipped from slot exposure if they do not fit the
current slot model cleanly.

### Texture Def

`TextureDef` moves from flat `width`/`height` to semantic `size: Dim2uSlot` if
downstream fallout stays manageable. This intentionally changes
`examples/basic/texture.toml` to a `[size]` table and proves generic rendering
of an opaque semantic struct leaf.

### Shader Def

`ShaderDef` uses semantic slots:

- `glsl_path: SourcePathSlot`
- `texture_loc: RelativeNodeRefSlot`
- `render_order: RenderOrderSlot`
- `glsl_opts: GlslOpts`
- `param_defs: MapSlot<String, ShaderParamDef>`

`ShaderParamDef` is a Rust-authored record with label, description, value type,
default, and simple scalar hint fields. Source `param_defs` has a static map
value shape; runtime shader param values remain dynamic and are handled later.

### Output Def

`OutputDef` becomes a slot-derived struct for the current GPIO output shape:

- `pin: ValueSlot<u32>`
- `options: OptionSlot<OutputDriverOptionsConfig>`

Future output forms should become separate output node kinds/artifacts such as
`output/gpio`, `output/e131`, or `output/artnet`; M2 does not model this as a
source enum.

### Fixture Def And Mapping

`FixtureDef` uses semantic slots for refs, color order, transform, brightness,
and gamma correction.

Fixture mapping should move toward the map-first slot vocabulary:

- No `SlotShape::Array`.
- No custom serde that hides arrays behind maps.
- Paths and ring counts should use stable keyed maps.

The preferred M2 outcome is structured mapping through `SlotEnumShape`,
`SlotEnumAccess`, `MapSlot<u32, PathSpec>`, and nested record access. If this
phase becomes too large, stop and report rather than silently collapsing the
shape into an opaque value.

### Source Tests

Add tests that:

- Load `examples/basic/project.toml` and referenced child TOML files.
- Register generated source shapes.
- Build source slot roots.
- Snapshot roots through `lpc-wire`.
- Print server/client-style tree walks for evidence.
- Assert paths such as:
  - `project#nodes[shader].artifact`
  - `shader#glsl_path`
  - `shader#texture_loc`
  - `shader#glsl_opts.add_sub`
  - `shader#param_defs[...]`
  - `texture#size`
  - `output#pin`
  - `output#options.some.brightness`
  - `fixture#mapping`

## Design Decisions

### Source Defs Are Slot-Aware

- **Decision:** Convert real source defs to slot-aware fields rather than
  building adapters around plain structs.
- **Why:** The domain model should be the source of truth for shape, metadata,
  serde, versioning, sync, and future mutation.

### `kind` Is Loader Data

- **Decision:** Do not expose `kind` as a slot field.
- **Why:** The shape id and loaded def type already identify the slot root.

### `uid` Is Removed

- **Decision:** Remove `uid` from `examples/basic/project.toml`.
- **Why:** It is a defunct old concept.

### Output Is A Struct For Now

- **Decision:** Simplify current `OutputDef` to a struct.
- **Why:** There is only one output form today. Future output forms should be
  separate output node kinds/artifacts, not a premature enum in M2.

### Fixture Collections Use Maps

- **Decision:** Apply the slot-domain no-array rule to fixture mapping.
- **Why:** Stable keyed maps are the versioning and UI-friendly structure we
  want for authored fixture data.
