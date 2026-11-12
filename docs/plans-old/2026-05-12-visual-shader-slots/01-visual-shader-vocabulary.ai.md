# Phase 1: Visual Shader Vocabulary

## Scope Of Phase

Update the model vocabulary for regular visual shaders.

In scope:

- Rename the authored kind from `shader` to `shader/visual`.
- Replace `ShaderDef::param_defs` with `consumed`.
- Keep using `ShaderSlotDef` for consumed slot declarations.
- Update model parsing/tests and obvious source/template call sites.

Out of scope:

- Runtime resolution of consumed shader inputs.
- Loader binding registration.
- Backend uniform changes.
- Texture inputs.

## Code Organization Reminders

- Keep shader model concepts in `lp-core/lpc-model/src/nodes/shader/`.
- Prefer direct field names matching TOML, using serde rename only where it
  improves authoring compatibility.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`
- `lp-core/lpc-model/src/nodes/node_def.rs`
- `lp-core/lpc-model/src/node/kind.rs`
- `lp-cli/src/commands/create/project.rs`
- `lp-app/lpa-server/src/template.rs`
- `lp-core/lpc-shared/src/project/builder.rs`
- `examples/*/shader.toml`

Expected changes:

- Set `ShaderDef::KIND` to `shader/visual`.
- Rename field:

  ```rust
  pub param_defs: MapSlot<String, ShaderSlotDef>
  ```

  to:

  ```rust
  #[serde(default, rename = "consumed", skip_serializing_if = "MapSlot::is_empty")]
  pub consumed_slots: MapSlot<String, ShaderSlotDef>
  ```

- Update `Default`, tests, builders, templates, and examples.
- Decide whether to accept old `kind = "shader"` as a temporary alias in
  `NodeDef::from_toml_str`. If added, do not emit old kind from examples or
  builders.
- Add a parsing test for:

  ```toml
  kind = "shader/visual"
  glsl_path = "shader.glsl"

  [consumed.time]
  kind = "value"
  value = "f32"
  ```

## Validate

```bash
cargo fmt
cargo test -p lpc-model shader_def
cargo check -p lp-cli
```

