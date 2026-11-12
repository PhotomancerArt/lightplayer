# Project Artifact Initial Load Design

## Scope of work

This plan replaces the current initial project load path:

```text
/project.json + discovered /src/*.kind/node.toml directories
```

with an artifact-rooted load path:

```text
/project.toml -> ProjectDef -> ProjectNode root -> declared node artifacts
```

The core runtime should start from a project artifact specifier, load that
artifact as the root node definition, then instantiate the root `ProjectNode`
and its declared node artifacts. Directory discovery and special directory
suffix semantics are removed from the core initial-load path.

In scope:

- Stabilize the new source terminology and rustdocs around `ArtifactSpecifier`,
  `NodeInvocation`, `NodeDef`, `NodeLoc`, and concrete `*Def` node bodies.
- Add `ProjectDef` with `kind = "project"` and a named `nodes` table.
- Flatten `examples/basic` early to the new canonical source layout.
- Rework the core project loader to load `/project.toml` and the node artifacts
  it declares.
- Keep `TextureNode` for now. Removing it is a future plan.
- Keep the current compatibility wire/detail projection temporarily, populated
  from artifact-loaded definitions instead of discovered legacy directories.
- Migrate broad integration tests, server/CLI assumptions, and remaining
  examples near the end, after `examples/basic` validates the idea.

Out of scope:

- Source reload, deletion, and lifecycle parity.
- Removing `TextureNode`.
- General wire data model redesign.
- `RuntimePropAccess` / `RuntimeOutputAccess` unification.
- Artifact-plus-local-field merge semantics.
- Absolute node-tree paths.
- Full inline project authoring. This plan may keep the type door open, but
  the canonical path is one artifact file per node.

## File structure

```text
lp-core/
├── lpc-model/src/node/
│   ├── node_loc.rs                 # UPDATE: relative dot-syntax parser/docs for NodeLoc
│   └── mod.rs                      # UPDATE: exports/docs away from old NodeSpec wording
│
├── lpc-source/src/
│   ├── artifact/
│   │   └── artifact_specifier.rs         # UPDATE: ArtifactSpecifier docs and source-relative path semantics
│   └── node/
│       ├── node_def.rs             # UPDATE: NodeDef docs/visibility/no_std cleanup
│       ├── node_invocation.rs      # UPDATE: artifact-only invocation semantics for this plan
│       ├── project/
│       │   └── mod.rs              # NEW/UPDATE: ProjectDef { kind, name?, nodes }
│       ├── shader/shader_def.rs    # UPDATE: flattened TOML names, NodeLoc refs
│       ├── texture/texture_def.rs  # UPDATE: artifact definition docs/TOML shape
│       ├── output/output_def.rs    # UPDATE: artifact definition docs/TOML shape
│       └── fixture/fixture_def.rs  # UPDATE: flattened TOML names, NodeLoc refs
│
├── lpc-engine/src/project_runtime/
│   ├── project_loader.rs           # REWRITE: load /project.toml artifact tree, no discovery
│   ├── core_project_runtime.rs     # UPDATE: remove legacy_src_dirs loading assumptions
│   ├── compatibility_projection.rs # UPDATE: snapshots from artifact-loaded defs
│   ├── detail_projection.rs        # UPDATE: tolerate new paths/defs
│   └── runtime_services.rs         # UPDATE: OutputDef references as needed
│
└── lpc-engine/src/nodes/core/
    ├── placeholder.rs              # UPDATE/ADD: ProjectNode or project placeholder
    ├── shader_node.rs              # UPDATE: refs from NodeLoc-resolved ids
    ├── fixture_node.rs             # UPDATE: refs from NodeLoc-resolved ids
    ├── texture_node.rs             # UPDATE: TextureDef
    └── output_node.rs              # UPDATE: OutputDef

examples/basic/
├── project.toml                    # NEW: kind = "project", [nodes.*] artifact refs
├── texture.toml                    # MOVE/FLATTEN from src/main.texture/node.toml
├── shader.toml                     # MOVE/FLATTEN from src/rainbow.shader/node.toml
├── shader.glsl                     # MOVE from src/rainbow.shader/main.glsl
├── output.toml                     # MOVE/FLATTEN from src/strip.output/node.toml
└── fixture.toml                    # MOVE/FLATTEN from src/fixture.fixture/node.toml
```

## Conceptual architecture

```text
ArtifactSpecifier("/project.toml")
        │
        ▼
load ProjectDef
  kind = "project"
  nodes = { texture, shader, output, fixture }
        │
        ▼
ProjectNode becomes runtime root NodeEntry
        │
        ├─ NodeInvocation::artifact("./texture.toml") ─► TextureDef ─► TextureNode
        ├─ NodeInvocation::artifact("./shader.toml")  ─► ShaderDef  ─► ShaderNode
        ├─ NodeInvocation::artifact("./output.toml")  ─► OutputDef  ─► OutputNode
        └─ NodeInvocation::artifact("./fixture.toml") ─► FixtureDef ─► FixtureNode
```

The source model has four separate address concepts:

```text
ArtifactSpecifier  authored outside-world locator for a loadable artifact
ArtifactLocation engine-side resolved artifact-manager cache key
NodeLoc          source-side relative locator into the runtime node tree
NodeId           resolved runtime handle
```

An artifact is a loadable, identified node definition. A node definition can be
written inline in a parent someday or saved as its own TOML artifact and invoked
with `artifact = "./node.toml"`. This plan uses file-backed node artifacts for
the canonical example.

## Main components

### ArtifactSpecifier

`ArtifactSpecifier` is the source-side authored specifier for loading an artifact.
For this plan the important variant is path-based:

```toml
artifact = "./shader.toml"
glsl = "./shader.glsl"
```

Relative artifact paths resolve relative to the file containing the reference.
Slash syntax belongs to filesystem/artifact paths, not node-tree locations.

Engine-side `ArtifactLocation` remains the resolved `ArtifactManager` key.

### NodeInvocation

`NodeInvocation` is the value in a project node's `nodes` table. For this plan
the canonical shape is artifact-only:

```toml
[nodes.shader]
artifact = "./shader.toml"
```

The long-term conceptual shape may be:

```rust
pub enum NodeInvocation {
    Artifact(ArtifactSpecifier),
    Inline(NodeDef),
}
```

but inline definitions and artifact-plus-local-field merges are not required for
this plan.

### NodeDef and concrete definitions

Concrete node definition types live in `lpc-source/src/node/` and use the `Def`
suffix:

```text
ProjectDef
ShaderDef
TextureDef
OutputDef
FixtureDef
```

The old `Config` suffix should not describe the whole authored source body. A
node definition may eventually contain config-like fields, params, bindings,
state/output declarations, and nested node invocations.

### ProjectDef and ProjectNode

`project.toml` is itself a node artifact:

```toml
kind = "project"
name = "basic"

[nodes.texture]
artifact = "./texture.toml"

[nodes.shader]
artifact = "./shader.toml"

[nodes.output]
artifact = "./output.toml"

[nodes.fixture]
artifact = "./fixture.toml"
```

Loading this artifact instantiates the runtime root `ProjectNode`. The root is
not special authored data outside the node model; it is the first loaded node
definition.

For this plan, `RuntimeServices::project_root` remains the source of the runtime
root `TreePath`. `ProjectDef.name` is metadata.

### NodeLoc

`NodeLoc` remains a source string wrapper for now, but it must parse and enforce
relative node-location semantics. Node locations intentionally do not use slash
syntax.

```text
.                  current node
.child             child of current node
.child.grandchild  descendant of current node
..                 parent
..sibling          sibling through parent
..sibling.child    sibling's child
```

Node locations are relative-only in this plan. Absolute node-tree paths are not
supported yet.

For `examples/basic`, the current core node artifacts are siblings under the
project root:

```toml
# shader.toml
texture = "..texture"

# fixture.toml
texture = "..texture"
output = "..output"
```

Future property references may append `#...`, for example
`..shader#state.output`, but this plan only needs the node-location part before
`#`.

### Loader

The core loader should:

1. Load the project artifact from `/project.toml` or an explicitly supplied
   `ArtifactSpecifier`.
2. Validate that it is `kind = "project"`.
3. Create/attach the root `ProjectNode`.
4. Load all project node invocations into a project-local name index.
5. Resolve `NodeLoc` dependencies against that index and parent context.
6. Instantiate/attach current core nodes in dependency-safe order.
7. Populate compatibility snapshots from loaded definitions.

Author order in the TOML `nodes` table is not load-bearing. The loader should
preserve readable source order where cheap, but dependency correctness wins.

### Compatibility projection

The old node-specific wire/detail shapes remain temporarily. They should be
fed from the new artifact-loaded definitions and runtime nodes rather than from
discovered legacy directories.

This plan should not use compatibility requirements to reintroduce directory
discovery, `/src/*.kind` semantics, or `/project.json` into the core initial
load path.
