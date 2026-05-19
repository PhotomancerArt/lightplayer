# Versioned Source Artifacts Notes

## Scope Of Work

This plan covers the model and engine changes needed to make authored source
loading uniform, versioned, and lazy enough for live updates. The immediate
user-facing goal is simple LightPlayer projects that can live in one TOML file,
while still supporting split-file projects and future library-backed sources.

In scope:

- First-class authored shader source specs with `path`, `glsl`, and room for
  future `wgsl` or other languages.
- Inline child node definitions in project TOML so small examples can be
  single-file projects.
- Runtime artifact/source identity, revisioning, and lazy source materialization.
- Node-facing APIs that expose "changed or unchanged" versioned source without
  leaking filesystem or file-watch concepts into nodes.
- Restoring live shader source updates through artifact revision invalidation.
- Rust and design docs for the authored shapes and runtime boundary.

Out of scope for the first implementation:

- Full package/library source loading beyond preserving the existing
  `lib:`-friendly vocabulary.
- WGSL compilation or non-GLSL backends.
- Streaming binary slices or large-file partial reads.
- General inline support for every possible artifact kind.

## Current State

### Authored Node References

- `lp-core/lpc-model/src/node/node_invocation.rs` defines `NodeInvocation` as a
  struct with one field:
  - `artifact: ArtifactPathSlot`
- The file comment already says inline node definitions and artifact-plus-local
  field merges are reserved for richer invocation forms.
- The revised design should make that future namespace explicit by adding a
  `def` field. The invocation table owns invocation-local fields; `def` owns
  the referenced or inline `NodeDef`.
- Project TOML currently uses:

```toml
[nodes.shader]
artifact = "./shader.toml"
```

- `lp-core/lpc-model/src/nodes/project/project_def.rs` stores child nodes in:
  - `MapSlot<String, NodeInvocation>`

### Authored Shader Source

- `lp-core/lpc-model/src/nodes/shader/shader_def.rs` has:
  - `glsl_path: SourcePathSlot`
- `lp-core/lpc-model/src/nodes/shader/compute_shader_def.rs` also has:
  - `glsl_path: SourcePathSlot`
- `SourcePath` is a simple path-string slot in
  `lp-core/lpc-model/src/slots/source_path.rs`.
- Current authored shader nodes use:

```toml
kind = "Shader"
glsl_path = "shader.glsl"
```

or:

```toml
kind = "ComputeShader"
glsl_path = "emitters.glsl"
```

### Project Loading

- `lp-core/lpc-engine/src/engine/project_loader.rs` eagerly loads node TOML
  artifacts into `NodeDef` values.
- Shader and compute shader loading currently resolves `glsl_path` relative to
  the node artifact path, reads UTF-8 source immediately, and passes a `String`
  into the runtime node constructors.
- Compute shaders also generate a header during project load and concatenate it
  with the authored source.
- This makes source reload awkward because shader runtime nodes own an already
  materialized string instead of a source identity and version.

### Runtime Nodes

- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs` stores:
  - `glsl_source: String`
  - `shader: Option<Box<dyn LpShader>>`
- Visual shader compilation happens lazily during render via `RenderContext`.
- `lp-core/lpc-engine/src/nodes/shader/compute_shader_node.rs` stores:
  - `glsl_source: String`
  - `shader: Option<Box<dyn LpComputeShader>>`
- Compute shader compilation happens during tick via `TickContext`.
- Any source resolution API must therefore be available from both tick and
  render/materialization paths, or from a shared service reachable by both.

### Artifact System

- `lp-core/lpc-engine/src/artifact/artifact_location.rs` has:
  - `ArtifactLocation::File(LpPathBuf)`
  - `ArtifactLocation::InlineNode { owner, name }`
- `ArtifactLocation::InlineNode` is present, but inline node loading is not
  currently wired through project loading.
- `ArtifactStore` in `lp-core/lpc-engine/src/artifact/artifact_store.rs` maps
  `ArtifactLocation` to `ArtifactId`, but its loaded payload state is currently
  `NodeDef`-specific.
- `ArtifactStore::content_frame` already provides a revision-like timestamp for
  loaded artifact payloads.
- The current store is closer to a NodeDef cache than a general artifact/source
  catalog.

### Filesystem Changes

- `lp-base/lpfs/src/fs_event.rs` defines `FsChange` as a path plus create,
  modify, or delete change type.
- `lp-app/lpa-server/src/server.rs` filters base filesystem changes per loaded
  project and calls `project.engine_mut().handle_fs_changes(&project_changes)`.
- `Engine::handle_fs_changes` currently does nothing.
- A live reload implementation should map changed paths to artifact/source
  identities, bump revisions, and let nodes observe changes when they next ask
  for source.

### Slot System

- Externally tagged enum support was recently added to the slot system and is
  documented in `docs/design/slots/enum-encoding.md`.
- External enums are useful for shapes like:

```toml
[source]
path = "./visual.glsl"
```

or:

```toml
[source]
glsl = """
...
"""
```

- The currently implemented external enum encoding expects exactly one variant
  property. That works well for the first shader source shape.
- Inline node definitions likely need a hand-shaped or carefully modeled form
  because `kind = "Shader"` is itself the discriminator for `NodeDef`.

## User Notes

- The authored field name should be human-readable. `path` is preferred over
  `artifact` because most references are relative, even when the containing
  artifact eventually comes from a library namespace.
- `inline` is accurate but not descriptive enough for shader source. Prefer
  language-specific inline fields such as `glsl = """..."""`; this leaves room
  for `wgsl = """..."""` later.
- Shader source can be a first-class artifact/source type. It is central enough
  to the product to deserve domain-specific treatment.
- Node defs and inline shader source go together because the real product goal
  is one-file projects for simple examples.
- Node invocation and node definition should have separate namespaces. Prefer
  `[nodes.x] def = { path = "./x.toml" }` or `[nodes.x.def] kind = "Shader"`
  so future invocation-level bindings, overrides, labels, or enable flags can
  live beside `def` without colliding with `NodeDef` fields.
- Do not keep backwards-compatible support for old authored fields. Move the
  config surface to the new model and update all examples.
- Nodes should not know about files, where source came from, or
  `handle_fs_changes`.
- The desired node-facing model is:
  - resolve a slotted source spec,
  - pass an optional last-seen version,
  - receive unchanged or a new version plus materialized source.

## Open Questions

### Should `artifact = ...` remain accepted for node invocations?

- **Context:** Existing examples and tests use `[nodes.x] artifact = "./x.toml"`.
  The preferred authored spelling going forward is `def = { path = "./x.toml" }`.
- **Answer:** No. Remove the authored `artifact` field from `NodeInvocation`.
  Update examples/tests to use `def.path`, and add rejection coverage so the
  old field is not silently accepted.

### Should `glsl_path` remain accepted?

- **Context:** `ShaderDef` and `ComputeShaderDef` currently use `glsl_path`.
  The new authored shape should be `[source] path = "./x.glsl"` or
  `[source] glsl = """..."""`.
- **Answer:** No. Remove `glsl_path` from the authored model. Update
  examples/tests to use `source`, and add rejection coverage so old shader TOML
  is not silently accepted.

### Should inline node defs use `[nodes.x] kind = "Shader"` directly?

- **Context:** Direct `[nodes.x] kind = "Shader"` is terse, but it puts
  `NodeDef` fields in the same table as future invocation-owned fields.
- **Suggested answer:** Do not use direct `[nodes.x] kind = ...` as the new
  canonical shape. Use a `def` namespace instead:
  - `def = { path = "./shader.toml" }` means referenced node def.
  - `def = { kind = "Clock" }` or `[nodes.x.def] kind = "Shader"` means inline
    `NodeDef`.
  This preserves namespace room for future bindings, overrides, labels, and
  enable flags on `NodeInvocation`.

### Should shader source versions be revisions or hashes?

- **Context:** Nodes need cheap equality checks and should not care about the
  source backend. File changes arrive as event paths, but inline changes come
  from node artifact reloads.
- **Suggested answer:** Introduce an opaque `SourceVersion`/`ArtifactVersion`
  token composed from resolved source identity and content revision. It can
  start as `ArtifactId + Revision` and evolve later to include content hashes or
  compile ABI revisions without changing node APIs.

### Where should source resolution live?

- **Context:** Visual shader compilation uses `RenderContext`; compute shader
  compilation uses `TickContext`.
- **Suggested answer:** Put the core resolver behind a shared engine service,
  then expose small convenience methods from both contexts. Avoid duplicating
  source logic in shader nodes.

## Constraints

- Keep the on-device GLSL JIT compiler intact. Do not gate, stub, or move the
  compiler out of the embedded runtime path.
- Preserve `no_std + alloc` compatibility for `lpc-model`, `lpc-engine`, and
  shader pipeline code.
- Do not eagerly keep referenced file bytes or source strings in memory after
  compilation unless a specific cache requires it.
- Prefer structured slot/model parsing over ad hoc TOML string manipulation.
- Keep authored config stable and readable; this feature is partly about making
  examples and simple projects less fussy.
