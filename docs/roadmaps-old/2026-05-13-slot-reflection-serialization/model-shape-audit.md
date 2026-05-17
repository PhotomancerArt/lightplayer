# Model Shape Audit For Mockup Refresh

## Purpose

The mockup should resemble the current production domain closely enough to test
native serialization decisions before production code adopts them.

This audit captures the current shape differences found on 2026-05-13.

## Persistence Pattern

For the current core node artifact model, the persisted top-level TOML bodies
are slot roots:

- `project.toml` loads as `NodeDef::Project(ProjectDef)`, and `ProjectDef` is
  `#[slot(root)]`.
- `shader.toml` loads as `NodeDef::Shader(ShaderDef)`, and `ShaderDef` is
  `#[slot(root, view)]`.
- `texture.toml` loads as `NodeDef::Texture(TextureDef)`, and `TextureDef` is
  `#[slot(root, view)]`.
- `output.toml` loads as `NodeDef::Output(OutputDef)`, and `OutputDef` is
  `#[slot(root, view)]`.
- `fixture.toml` loads as `NodeDef::Fixture(FixtureDef)`, and `FixtureDef` is
  `#[slot(root, view)]`.

`NodeDef` is the loader/discriminator envelope and delegates `SlotAccess` to
the contained concrete root. It is not itself a separate root shape.

Nested persisted structures are not roots by themselves:

- `NodeInvocation` is persisted under `ProjectDef.nodes`.
- `GlslOpts`, `ShaderParamDef`, `OutputDriverOptionsConfig`, fixture mapping
  variants, and path specs are persisted inside their owning node root.

This pattern is not universal across every persisted type in the repository.
The older/future `lpc-source::SrcArtifact` loader accepts arbitrary typed
`SrcArtifact + DeserializeOwned` values with `schema_version`, and those are
not currently required by the trait to be slot roots. For this roadmap, the
mockup should follow the core node artifact pattern first.

## Production Shape

### Project

`lpc-model/src/nodes/project/project_def.rs`

- `kind: String`
  - skipped from slots
  - loader/discriminator data
- `name: OptionSlot<ValueSlot<String>>`
- `nodes: MapSlot<String, NodeInvocation>`

### Node Invocation

`lpc-model/src/node/node_invocation.rs`

- `artifact: ArtifactPathSlot`
- no overrides yet

### Shader

`lpc-model/src/nodes/shader/shader_def.rs`

- `glsl_path: SourcePathSlot`
- `render_order: RenderOrderSlot`
- `bindings: BindingDefs`
- `glsl_opts: GlslOpts`
- `param_defs: MapSlot<String, ShaderParamDef>`

Important drift from mockup:

- production no longer has `texture_loc` on `ShaderDef`
- binding defs are first-class authored data
- GLSL options use semantic enum leaves, not raw strings

### Texture

`lpc-model/src/nodes/texture/texture_def.rs`

- `size: Dim2uSlot`
- `bindings: BindingDefs`

Important drift from older mockup:

- width/height are no longer separate fields

### Output

`lpc-model/src/nodes/output/output_def.rs`

- `pin: ValueSlot<u32>`
- `bindings: BindingDefs`
- `options: OptionSlot<OutputDriverOptionsConfig>`

`OutputDriverOptionsConfig` includes:

- `lum_power`
- `white_point`
- `brightness`
- `interpolation_enabled`
- `dithering_enabled`
- `lut_enabled`

### Fixture

`lpc-model/src/nodes/fixture/fixture_def.rs`

- `render_size: Dim2uSlot`
- `bindings: BindingDefs`
- `sampling: FixtureSamplingConfig`
  - skipped from slots
- `mapping: MappingConfig`
- `color_order: ValueSlot<ColorOrder>`
- `transform: Affine2dSlot`
- `brightness: OptionSlot<ValueSlot<u32>>`
- `gamma_correction: OptionSlot<ValueSlot<bool>>`

### Fixture Mapping

`lpc-model/src/nodes/fixture/mapping.rs`

- `MappingConfig::PathPoints`
  - `paths: MapSlot<u32, PathSpec>`
  - `sample_diameter: PositiveF32Slot`
- `PathSpec::RingArray`
  - `center: XySlot`
  - `diameter: PositiveF32Slot`
  - `start_ring_inclusive: ValueSlot<u32>`
  - `end_ring_exclusive: ValueSlot<u32>`
  - `ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>`
  - `offset_angle: ValueSlot<f32>`
  - `order: ValueSlot<RingOrder>`

Important drift from mockup:

- production ring lamp counts are a keyed map, not a vector value
- production fixture mapping currently has one mapping variant and one path
  variant, but still uses enum discriminators
- `RingOrder` is a semantic string/dropdown leaf

## Mockup Update Notes

The mockup does not need exact production type reuse, especially for large
supporting types such as `BindingDefs`. It should have matching structural
pressure:

- skipped loader fields
- optional fields with defaults
- nested records
- stable keyed maps
- semantic enum leaves
- enum discriminators
- direct borrowed slot access
- owned `SlotData` snapshots
- JSON sync/wire storage
- TOML authored storage

Any mock simplification should be named in test output or docs so it does not
silently become a false proof.
