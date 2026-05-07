# Phase 2: Core Source Defs

## Scope Of Phase

Convert the simpler real source defs to slot-aware domain objects and prove
generated static registration covers them.

In scope:

- Convert:
  - `ProjectDef`
  - `NodeInvocation`
  - `TextureDef`
  - `ShaderDef`
  - `ShaderParamDef`
  - `OutputDef`
  - `OutputDriverOptionsConfig`
  - `GlslOpts` and its option modes as needed
- Add `#[derive(lpc_model::SlotRecord)]` and `#[slot(root)]` where appropriate.
- Keep or update serde so authored TOML remains clean.
- Remove the defunct `uid` from `examples/basic/project.toml`.
- Update `examples/basic/texture.toml` if `TextureDef` moves to semantic
  `size: Dim2uSlot`.
- Simplify `OutputDef` from enum to struct.
- Add source tests that register static shapes and walk/snapshot these roots.

Out of scope:

- Fixture mapping conversion.
- Runtime node roots.
- Client mutation.
- Full downstream engine cleanup beyond compile fallout.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Add `shader_param_def.rs` instead of hiding the new type in `shader_def.rs`.
- Keep helpers lower in the file when that improves readability.
- Keep tests at the bottom of files or in `src/tests`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-source/src/node/project/mod.rs`
- `lp-core/lpc-source/src/node/node_invocation.rs`
- `lp-core/lpc-source/src/node/texture/texture_def.rs`
- `lp-core/lpc-source/src/node/shader/mod.rs`
- `lp-core/lpc-source/src/node/shader/shader_def.rs`
- `lp-core/lpc-source/src/node/shader/shader_param_def.rs`
- `lp-core/lpc-source/src/legacy/glsl_opts.rs`
- `lp-core/lpc-source/src/node/output/output_def.rs`
- `examples/basic/project.toml`
- `examples/basic/texture.toml`

Expected source shape:

- `ProjectDef`:
  - Keep `kind` as loader data if needed for deserialization, but skip it from
    slot exposure.
  - Remove `uid`; do not add it as a field.
  - `name` may be `OptionSlot<ValueSlot<String>>` or another slot-aware
    optional string form if the existing wrappers support it cleanly.
  - `nodes` should be a stable-key map over `NodeInvocation`.
- `NodeInvocation`:
  - `artifact` should expose an artifact path semantic leaf.
  - `overrides` may be skipped from slot exposure if it does not fit M2.
- `TextureDef`:
  - Prefer `size: Dim2uSlot`.
- `ShaderDef`:
  - `glsl_path: SourcePathSlot`
  - `texture_loc: RelativeNodeRefSlot`
  - `render_order: RenderOrderSlot`
  - `glsl_opts: GlslOpts`
  - `param_defs: MapSlot<String, ShaderParamDef>` with default empty serde.
- `ShaderParamDef`:
  - Mirror the mockup’s useful shape: label, description, value type, default,
    and simple scalar hints.
- `OutputDef`:
  - Convert to a struct with `pin` and optional options.
  - Keep current flat `output.toml` shape.

Tests should print and assert generic walks over:

- `project#nodes[shader].artifact`
- `shader#glsl_path`
- `shader#texture_loc`
- `shader#glsl_opts.add_sub`
- `shader#param_defs[...]` in a focused test
- `texture#size`
- `output#pin`
- `output#options.some.brightness`

## Validate

```bash
cargo fmt --package lpc-source
cargo test -p lpc-source --lib --tests
cargo check -p lpc-source --features schema-gen
```
