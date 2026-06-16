# Versioned Source Artifacts Design

## Scope Of Work

Build a unified source/artifact model for authored node definitions and shader
source so simple projects can be written as one TOML file, while split-file and
future library-backed projects remain natural.

The plan introduces:

- `def`-scoped authored references for child node definitions.
- `path`-based authored references for external node defs and shader sources.
- Inline child node definitions in project TOML.
- A first-class `ShaderSource` model with `path`, `glsl`, and future language
  room such as `wgsl`.
- Runtime source identities and opaque versions.
- Lazy source materialization through engine contexts.
- Filesystem-change handling that bumps artifact/source revisions instead of
  calling into individual nodes.
- Node definition reload for both split-file node defs and inline project node
  defs.

## File Structure

```text
lp-core/lpc-model/src/
  artifact/
    artifact_specifier.rs
    artifact_read_root.rs
  node/
    node_invocation.rs
  nodes/
    project/project_def.rs
    shader/
      shader_source.rs
      shader_def.rs
      compute_shader_def.rs
  slots/
    source_path.rs

lp-core/lpc-engine/src/
  artifact/
    artifact_id.rs
    artifact_location.rs
    artifact_store.rs
    artifact_source.rs
    source_resolver.rs
  engine/
    engine.rs
    project_loader.rs
  node/
    contexts.rs
  nodes/
    shader/
      shader_node.rs
      compute_shader_node.rs

docs/design/
  source-artifacts.md
```

New files may be adjusted during implementation if existing module boundaries
suggest better names, but keep one primary concept per file.

## Authored Shapes

### Node Invocation

Canonical split-file child node:

```toml
[nodes.shader]
def = { path = "./shader.toml" }
```

Inline child node:

```toml
[nodes.shader.def]
kind = "Shader"
render_order = 0

[nodes.shader.def.source]
glsl = """
vec4 render(vec2 pos) {
    return vec4(pos, 0.0, 1.0);
}
"""
```

Small inline child nodes may use inline-table syntax:

```toml
[nodes.clock]
def = { kind = "Clock" }
```

Model direction:

```rust
pub struct NodeInvocation {
    pub def: NodeDefRef,
    // future invocation-owned fields:
    // pub bindings: ...
    // pub overrides: ...
    // pub enabled: ...
}

pub enum NodeDefRef {
    Path { path: ArtifactSpecifier },
    Inline(Box<NodeDef>),
}
```

`nodes.<name>` is the invocation namespace. `nodes.<name>.def` is the node
definition namespace. Keep those boundaries strict so future invocation-local
bindings, overrides, labels, or enable flags do not collide with node definition
fields.

The exact Rust shape may use helper structs or custom slot read/write code if
that produces cleaner slot integration. The authored API is the contract.

### Shader Source

Referenced shader source:

```toml
[source]
path = "./visual.glsl"
```

Inline GLSL:

```toml
[source]
glsl = """
vec4 render(vec2 pos) {
    return vec4(pos, 0.0, 1.0);
}
"""
```

Future inline language:

```toml
[source]
wgsl = """
...
"""
```

Model direction:

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Slotted)]
#[slot(enum_encoding = "external", rename_all = "snake_case")]
pub enum ShaderSource {
    Path(ArtifactSpecifier),
    Glsl(String),
}
```

If `ArtifactSpecifier` is awkward as a direct slot value, use a new
`SourcePath`/`ShaderSourcePath` wrapper that parses into `ArtifactSpecifier`.
Do not expose `artifact` as the canonical authored field.

Breaking migration:

- Do not keep `glsl_path` read support.
- Move shader and compute shader TOML to the `source` shape.
- Prefer writing `source` in generated TOML.

## Runtime Architecture

### Artifact Identity

An artifact/source has:

- `ArtifactId`
- `ArtifactLocation`
- content revision
- optional materialized representations

`ArtifactLocation` should grow beyond the current `File` and `InlineNode`
variants. Expected additions:

```rust
InlineShaderSource {
    owner: ArtifactId or LpPathBuf,
    node: NodeId or path/name,
    field: &'static str,
}
```

Use stable owner/name fields rather than process-local `NodeId` if the location
needs to survive reloads or be usable before the runtime node exists.

### Source Version

Nodes compare opaque source versions:

```rust
pub struct SourceVersion {
    artifact: ArtifactId,
    content_revision: Revision,
    abi_revision: Revision,
}
```

The public node-facing contract should treat this as an opaque equality token.
It may start simpler, but leave room for:

- source content revision,
- shader language,
- generated compute header revision,
- compiler ABI/config revision,
- dependency/import revisions.

### Source Resolver

The source resolver owns path resolution, revision checks, and lazy reads.

Conceptual API:

```rust
pub enum SourceUpdate<T> {
    Unchanged(SourceVersion),
    Changed(VersionedSource<T>),
}

pub struct VersionedSource<T> {
    pub version: SourceVersion,
    pub value: T,
}

pub struct ShaderSourceText {
    pub language: ShaderLanguage,
    pub text: String,
}
```

Convenience methods should be available from both `TickContext` and
`RenderContext`:

```rust
ctx.resolve_shader_source(&source_spec, self.last_source_version)
```

The unchanged path must avoid reading referenced file bytes.

### Node Runtime Boundary

Shader nodes store:

```rust
last_source_version: Option<SourceVersion>,
shader: Option<Box<dyn LpShader>>,
compilation_error: Option<String>,
```

Compute shader nodes store the same kind of version token for their source.
They should not store file paths or assume where source came from.

When source changes:

- clear compiled shader,
- clear stale compilation error,
- compile from newly materialized source on demand,
- update `last_source_version` only after successfully observing the new source
  materialization.

### Filesystem Change Flow

1. Server collects `FsChange` values and calls `Engine::handle_fs_changes`.
2. Engine maps changed project-relative paths to node-definition and shader
   source artifact locations.
3. Shader source revisions are bumped and materialized source caches are
   invalidated.
4. Changed node-definition artifacts are re-read and parsed into fresh
   `NodeDef` payloads.
5. Affected runtime nodes are reconciled or recreated from the fresh `NodeDef`
   payloads.
6. Inline child node definitions and inline shader sources derived from a
   changed owning TOML artifact receive new effective revisions.
7. On next tick/render, shader/compute nodes ask for shader source with their
   last seen version.
8. Resolver returns unchanged or changed source.

`handle_fs_changes` should not call shader-specific reload hooks. It may,
however, perform artifact-level reload/reconcile work for node definitions and
mark dependent runtime nodes dirty/recreated through normal engine ownership.

## Main Components And Interactions

### `NodeInvocation`

Responsible for authored project child node specs:

- a `def` field containing path-based child node references,
- a `def` field containing inline child node definitions,
- a stable namespace for future invocation-owned bindings and overrides.

It should not read files.

### `ShaderSource`

Responsible for authored shader source specs:

- path references,
- inline GLSL text,
- future inline source languages.

It should not read files or compile shaders.

### Artifact/Source Catalog

Responsible for:

- resolving authored locators relative to an owner artifact,
- assigning stable `ArtifactId` values,
- tracking revisions,
- invalidating materialized representations,
- mapping filesystem changes to affected artifacts.

This should evolve the current `ArtifactStore` beyond a NodeDef-only payload
cache. It may be implemented as new sibling types first to avoid destabilizing
existing NodeDef loading too much at once.

### Source Resolver

Responsible for:

- comparing last-seen versions,
- reading text only when changed or uncached,
- returning versioned shader source text,
- hiding file/lib/inline provenance from nodes.

### Project Loader

Responsible for:

- reading root and child node defs,
- registering artifact/source identities,
- attaching runtime nodes with specs/handles rather than raw GLSL strings,
- supporting inline child node definitions.

### Runtime Contexts

Responsible for giving nodes ergonomic access:

- `TickContext` for compute shader compilation.
- `RenderContext` for visual shader compilation.

The implementation can share an internal `SourceServices` trait so both
contexts expose the same source resolution semantics.

## Breaking Migration Strategy

- Do not keep legacy `[nodes.x] artifact = "./node.toml"` readable.
- Do not keep legacy `glsl_path = "shader.glsl"` readable.
- Prefer `[nodes.x] def = { path = "./node.toml" }` for new node references.
- Prefer `[source] path = ...` and `[source] glsl = ...` for shader sources.
- Update all examples and tests to the new authored surface as part of this work.
- Preserve split-file behavior through the new `def.path` and `source.path`
  shapes, not through old field aliases.

## Validation Strategy

Validate in layers:

- model parse/write tests for new authored shapes,
- project loader tests for one-file projects,
- engine tests for source version unchanged/changed behavior,
- shader node tests for recompilation on source change,
- targeted host checks for model and engine crates,
- firmware target check for the shader pipeline.
