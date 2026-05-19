# Phase 1: Authored Model Shapes

- **parallel:** -
- **sub-agent:** supervised

## Scope Of Phase

Add the authored model types for path-or-inline node definitions and first-class
shader source specs.

In scope:

- Add a `ShaderSource` model with `path` and `glsl` variants.
- Update `ShaderDef` and `ComputeShaderDef` to use `source`.
- Update `NodeInvocation` to support canonical `def.path` and inline
  `def`-scoped `NodeDef`.
- Remove the old authored `artifact` and `glsl_path` fields from the model.
- Add model and slot tests for the authored TOML shapes.

Out of scope:

- Runtime source resolution.
- Filesystem change handling.
- Actual shader node recompilation.
- WGSL compilation or a real `wgsl` variant unless it is only a reserved
  design note.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.
- Keep tests at the bottom of Rust files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

### Add `ShaderSource`

Create:

- `lp-core/lpc-model/src/nodes/shader/shader_source.rs`

Expected authored TOML:

```toml
[source]
path = "./visual.glsl"
```

```toml
[source]
glsl = """
vec4 render(vec2 pos) {
    return vec4(pos, 0.0, 1.0);
}
"""
```

Suggested model:

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Slotted)]
#[slot(enum_encoding = "external", rename_all = "snake_case")]
pub enum ShaderSource {
    Path(SourcePath),
    Glsl(String),
}
```

Use `SourcePath` if direct `ArtifactLocator` slot support is not clean. The
engine can parse `SourcePath` into `ArtifactLocator` later.

Update exports in:

- `lp-core/lpc-model/src/nodes/shader/mod.rs`
- `lp-core/lpc-model/src/lib.rs` if needed.

### Update shader defs

Files:

- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`
- `lp-core/lpc-model/src/nodes/shader/compute_shader_def.rs`

Replace primary authored field:

```rust
pub source: ShaderSourceSlot or ShaderSource
```

Keep the default equivalent to `source.path = "main.glsl"` if the existing
default behavior still makes sense.

Breaking migration:

- Existing TOML with `glsl_path = "main.glsl"` should not parse as a valid
  shader or compute shader definition.
- Unknown old fields should not be silently ignored. Add `deny_unknown_fields`
  or equivalent slot/dynamic-reader validation if needed.
- Writers should prefer `[source] path = ...` once dynamic writer support makes
  that practical.

Likely implementation options:

- Use custom serde for `ShaderDef`/`ComputeShaderDef` only if the new `source`
  shape needs it.
- Or use the derived model directly if the slot/dynamic reader handles the new
  shape cleanly.

Update or replace helpers:

- remove `ShaderDef::glsl_path_buf`
- remove `ComputeShaderDef::glsl_path_buf`

with source-oriented helpers, for example:

```rust
pub fn shader_source(&self) -> &ShaderSource
```

Do not keep compatibility helpers for old authored fields.

### Update `NodeInvocation`

File:

- `lp-core/lpc-model/src/node/node_invocation.rs`

Desired authored TOML:

```toml
[nodes.shader]
def = { path = "./shader.toml" }
```

Inline authored node:

```toml
[nodes.clock]
def = { kind = "Clock" }
```

For inline node definitions with nested tables:

```toml
[nodes.shader.def]
kind = "Shader"
render_order = 0

[nodes.shader.def.source]
glsl = "vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }"
```

Suggested public model:

```rust
pub struct NodeInvocation {
    pub def: NodeDefRef,
    // future invocation-owned fields:
    // pub bindings: ...
    // pub overrides: ...
    // pub enabled: ...
}

pub enum NodeDefRef {
    Path { path: ArtifactLocator },
    Inline(Box<NodeDef>),
}
```

This may need a custom serde implementation because inline node definitions use
the same `kind` discriminator as top-level `NodeDef`.

Keep the namespace boundary explicit:

- fields under `nodes.<name>.def` belong to the node definition or reference,
- fields beside `def` belong to the invocation and are reserved for future
  bindings, overrides, labels, enable flags, or similar invocation-local
  behavior.

Slot integration:

- If recursive `NodeDef` inside `NodeInvocation` is awkward for `Slotted`, use
  a narrow custom slot wrapper or postpone dynamic editing support for inline
  node invocations while keeping serde/project loading support.
- Do not break existing `ProjectDef` slot registration.

### Tests

Add or update tests for:

- `ShaderSource` parses path and GLSL forms.
- `ShaderDef` parses `[source] path = ...`.
- `ShaderDef` parses `[source] glsl = ...`.
- `ShaderDef` rejects `glsl_path`.
- `ComputeShaderDef` has equivalent coverage.
- `ProjectDef` parses `[nodes.x] def = { path = ... }`.
- `ProjectDef` rejects `[nodes.x] artifact = ...`.
- `ProjectDef` parses inline `[nodes.x] def = { kind = "Clock" }`.
- `ProjectDef` parses nested inline `[nodes.x.def] kind = "Shader"` with
  `[nodes.x.def.source]`.
- Ambiguous specs produce useful errors:
  - `def.path` plus inline node `kind` inside the same `def`.
  - multiple shader source variants if external enum read does not already
    cover this.

## Validate

Run:

```bash
cargo test -p lpc-model --features derive --test slotted_enum_derive
cargo test -p lpc-model slot_codec --lib
cargo test -p lpc-model
cargo test -p lpc-slot-macros
```

If this phase changes generated slot views or schema behavior, also run:

```bash
cargo check -p lpc-engine
```
