# Source Artifacts

LightPlayer project files now separate a node invocation from the node
definition it uses. The child node namespace is reserved for invocation-level
properties, and the node definition lives under `def`.

Split-file node definitions use a path:

```toml
[nodes.shader]
def = { path = "./shader.toml" }
```

Inline node definitions put the authored node TOML under the same `def`
namespace:

```toml
[nodes.clock.def]
kind = "Clock"
```

The old `[nodes.x] artifact = "..."` shape is intentionally not accepted.

Shader node definitions use a first-class shader source spec. GLSL file sources
use `source.path`, resolved relative to the containing node definition:

```toml
kind = "Shader"
source = { path = "shader.glsl" }
```

Inline GLSL uses `source.glsl`:

```toml
kind = "Shader"

[source]
glsl = """
vec4 render(vec2 pos) {
    return vec4(pos, 0.0, 1.0);
}
"""
```

The old `glsl_path = "..."` field is intentionally not accepted.

`source` is GLSL-specific by design. That leaves room for future sibling source
forms such as `wgsl` without overloading an anonymous `inline` value.

## One-File Projects

A simple project can now put node definitions and GLSL directly in
`project.toml`:

```toml
kind = "Project"
name = "one-file"

[nodes.clock.def]
kind = "Clock"

[nodes.shader.def]
kind = "Shader"
source = { glsl = "vec4 render(vec2 pos) { return vec4(1.0, 0.0, 0.0, 1.0); }" }
```

Use split files when the source is large or shared. Use inline definitions when
the example or project is clearer as one artifact.

## Loading And Reloading

The project loader resolves both forms transparently:

- `def.path` loads a child node definition artifact from the filesystem.
- inline `def` is parsed as a node definition owned by the project TOML.
- `source.path` reads UTF-8 GLSL relative to the owning node definition.
- `source.glsl` materializes the inline string directly.

The server project wrapper stores the project filesystem and service handles.
When the filesystem reports a change inside a loaded project, the wrapper
rebuilds the engine through `ProjectLoader::load_from_root`. That restores live
updates for split-file node definitions, inline node definitions, split-file
GLSL, and inline GLSL because all four cases re-enter the canonical loader.

The next finer-grained step is a versioned source resolver owned by the engine:
shader nodes would hold a source identity, ask the context for
`resolve_shader_source(source, last_seen_version)`, and only materialize bytes
when the source version changes. That keeps file knowledge out of nodes while
avoiding whole-project reloads for small source edits.
