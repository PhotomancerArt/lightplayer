# Phase 3: Project Loader One-File Projects

- **parallel:** -
- **sub-agent:** main

## Scope Of Phase

Wire authored path/inline node specs and shader source specs through project
loading so simple projects can be represented as one TOML file.

In scope:

- Load child nodes from `def.path`.
- Load inline child node definitions from `def`, including
  `[nodes.x.def] kind = ...`.
- Register shader source specs with the source resolver.
- Attach runtime nodes with source specs/handles instead of eager GLSL strings
  where possible.
- Add project loader tests for one-file shader and compute projects.

Out of scope:

- Shader node recompilation behavior if Phase 4 has not yet happened.
- Filesystem-change handling.
- Converting examples; Phase 5 updates every example after the new authored
  model and runtime behavior are in place.

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

### Update project child loading

File:

- `lp-core/lpc-engine/src/engine/project_loader.rs`

Current logic:

- Iterates `project_def.nodes`.
- Extracts `invocation.artifact_specifier()`.
- Resolves a child artifact path.
- Reads a child TOML file.
- Loads `NodeDef` payload into `ArtifactStore`.

New logic:

- Match `NodeInvocation { def: NodeDefRef::Path { .. }, .. }`.
  - Resolve relative to the containing project artifact.
  - Load child TOML as today.
  - Register artifact identity and loaded `NodeDef`.
- Match `NodeInvocation { def: NodeDefRef::Inline(..), .. }`.
  - Use the inline `NodeDef` directly.
  - Register an inline node artifact identity using stable owner/name.
  - Ensure `artifact_content_frame` tracks the owning project artifact revision.

Keep `nodes.<name>` as the invocation namespace. Do not treat direct
`[nodes.x] kind = ...` as the canonical new inline shape; inline node fields
belong under `nodes.<name>.def`.

Keep `runtime.insert_artifact_node(...)` semantics where possible. If the map is
path-string-only, extend it or add a parallel lookup for inline nodes rather
than forcing fake filesystem paths deep into the model.

### Update shader runtime attachment

Current logic reads shader files immediately:

```rust
let shader_path =
    resolve_path_relative_to_file(&node.artifact_path, &config.glsl_path_buf())?;
let glsl_source = read_utf8_file(root, shader_path.as_path())?;
ShaderNode::new(node.id, config.clone(), glsl_source)
```

New direction:

- Resolve/register `config.source` through the source resolver.
- Pass the shader source spec/handle to `ShaderNode`.
- Avoid reading source text during load unless a temporary implementation bridge
  is needed before Phase 4 lands. Any such bridge must use the new `source`
  model, not old `glsl_path`.

Compute shader header generation:

- Move header generation out of project load if practical.
- Compute shader source version should include header/shape revision later.
- If this phase keeps header generation at load temporarily, mark it clearly and
  remove in Phase 4.

### One-file project examples for tests

Add project loader tests using `LpFsMemory` for:

Visual shader:

```toml
kind = "Project"

[nodes.shader.def]
kind = "Shader"
render_order = 0

[nodes.shader.def.source]
glsl = "vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }"
```

Compute shader:

```toml
kind = "Project"

[nodes.emitters.def]
kind = "ComputeShader"

[nodes.emitters.def.source]
glsl = "void tick() { }"
```

Keep existing split-file project tests green.

### Error reporting

Project load errors should identify:

- owning node artifact or inline node name,
- source path or inline source field,
- parse/materialization failure.

Avoid generic "invalid source path" messages when the problem is an inline
source parse or an ambiguous authored spec.

## Validate

Run:

```bash
cargo test -p lpc-engine project_loader --lib
cargo test -p lpc-model
cargo check -p lpc-engine
```

If shader source attachment touches shader runtime constructors, also run:

```bash
cargo test -p lpc-engine shader_node --lib
cargo test -p lpc-engine compute_shader_node --lib
```
