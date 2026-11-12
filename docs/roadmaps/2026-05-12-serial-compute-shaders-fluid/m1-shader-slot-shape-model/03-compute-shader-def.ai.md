# Phase 03: Compute Shader Def

## Scope Of Phase

Add `ComputeShaderDef` as a first-class authored node definition.

In scope:

- Add `compute_shader_def.rs`.
- Add `NodeKind::ComputeShader`.
- Add `NodeDef::ComputeShader`.
- Parse `kind = "shader/compute"` in `NodeDef::from_toml_str`.
- Support authored TOML sections `consumed` and `produced`, mapped to Rust
  fields `consumed_slots` and `produced_slots`.
- Add round-trip/parse tests for a compute shader artifact.

Out of scope:

- Engine runtime support.
- Node tree loading/runtime construction for compute shaders.
- Header generation.

## Code Organization Reminders

- Keep compute shader model beside visual shader model under `nodes/shader/`.
- Keep tests at the bottom of the relevant files.

## Sub-Agent Reminders

- Do not commit.
- Do not add runtime stubs that imply compute shader execution exists.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/shader/compute_shader_def.rs`
- `lp-core/lpc-model/src/nodes/shader/mod.rs`
- `lp-core/lpc-model/src/nodes/node_def.rs`
- `lp-core/lpc-model/src/node/kind.rs`
- `lp-core/lpc-model/src/lib.rs`

Expected TOML shape:

```toml
kind = "shader/compute"
glsl_path = "emitters.glsl"

[consumed.time]
kind = "value"
value = "f32"

[produced.emitters]
kind = "map"
key = "u32"
value = "lp::fluid::Emitter"
mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }
```

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model compute_shader
cargo test -p lpc-model node_def
```

