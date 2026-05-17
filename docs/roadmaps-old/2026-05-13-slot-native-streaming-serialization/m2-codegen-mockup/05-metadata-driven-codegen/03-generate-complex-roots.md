# Phase 3: Generate Complex Roots

## Scope Of Phase

Replace hardcoded generated code for the complex source roots with SlotCodec renderers.

In scope:

- Generate root readers/writers from SlotCodec metadata for:
  - `FixtureDef`
  - `ShaderDef`
- Keep specialized helper functions explicit.
- Preserve current TOML read and JSON round-trip tests.

Out of scope:

- Inferring `MappingConfig`, `PathSpec`, `GlslOpts`, or `ShaderParamDef`
  helpers from slot metadata.
- Production adoption.
- Derive-macro/module-local generation.

## Code Organization Reminders

- Prefer granular files with one main concept per file if needed.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/source/fixture_def.rs`
- `lp-core/lpc-slot-mockup/src/source/shader_def.rs`
- `lp-core/lpc-slot-mockup/src/tests/generated_shape_codec.rs`

Expected changes:

- Use the same root renderer introduced in Phase 2 for:
  - `FixtureDef`
  - `ShaderDef`
- Metadata should express:
  - `FixtureDef` fields:
    - `render_size`
    - `bindings` as read-discard/omit
    - `sampling` as read-discard/omit
    - `mapping`
    - `color_order`
    - `transform`
    - `brightness`
    - `gamma_correction`
  - `ShaderDef` fields:
    - `glsl_path`
    - `render_order`
    - `bindings` as read-discard/omit
    - `glsl_opts`
    - `param_defs`
- Keep explicit helper calls for:
  - `read_mapping_config` / `write_mapping_config`
  - `read_affine2d` / `write_affine2d`
  - `read_scalar_hint` / `write_scalar_hint`
  - `read_glsl_opts` / `write_glsl_opts`
  - `read_shader_param_def` / `write_shader_param_def`

Edge cases:

- `FixtureDef` must preserve default behavior for omitted option fields.
- `ShaderDef.param_defs` uses `string_key_map`.
- `MappingConfig` and `PathSpec` discriminators must remain explicit and
  friendly.

## Validate

```bash
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-mockup
```
