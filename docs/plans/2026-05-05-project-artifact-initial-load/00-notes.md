# Project Artifact Initial Load Notes

## Scope of work

Replace the current legacy project loading model, where the runtime reads
`/project.json` and discovers `/src/*.kind/node.toml` directories, with an
artifact-rooted initial load model:

- Projects are artifacts.
- Runtime initialization starts from a project artifact spec, initially
  `/project.toml`.
- The project artifact explicitly declares the child nodes to instantiate.
- The loader no longer discovers node directories or derives node type/path from
  directory suffixes.
- The first migration target is `examples/basic`; broad example and integration
  test migration happens near the end of the plan.
- Source reload is out of scope for this plan. The old M4.2 notes remain useful
  context, but this plan is about doing initial load correctly.

User-supplied constraints:

- There is no meaningful external usage yet; do not preserve old project layout
  compatibility just for compatibility's sake.
- Some early breakage in examples/tests is acceptable while the model changes.
- Keep `TextureNode` for this plan. Removing it belongs in a future plan.

## Current state of the codebase

`CoreProjectLoader::load_from_root` currently reads `/project.json`, parses it as
`ProjectConfig`, discovers legacy node directories under `/src`, maps directory
names like `/src/rainbow.shader` into tree paths, loads each `node.toml`, then
attaches runtime nodes in dependency order.

The current basic example uses:

```text
examples/basic/
├── project.json
└── src/
    ├── fixture.fixture/node.toml
    ├── main.texture/node.toml
    ├── rainbow.shader/node.toml
    ├── rainbow.shader/main.glsl
    └── strip.output/node.toml
```

The intended canonical shape is closer to:

```text
examples/basic/
├── project.toml
├── output.toml
├── fixture.toml
├── shader.toml
├── texture.toml
└── shader.glsl
```

`lpc-engine` already has a generic `ArtifactManager<A>`, engine-side
`ArtifactLocation`, and `load_source_artifact` helper, but the active core
project loader is still mostly legacy-layout-specific. It does not currently
load the project root through `ArtifactManager`.

RustRover-assisted renames/moves have already started:

- `SrcArtifactSpec` has moved/renamed to `ArtifactLocator`.
- `SrcNodeConfig` has moved/renamed to `NodeInvocation`.
- `TextureConfig` / `ShaderConfig` / `OutputConfig` / `FixtureConfig` have moved
  to `TextureDef` / `ShaderDef` / `OutputDef` / `FixtureDef` under
  `lpc-source/src/node/`.
- `lpc_model::NodeSpec` has moved/renamed to `NodeLoc`, but it is still the old
  string wrapper shape.
- `lpc-source/src/node/project/` exists but `ProjectDef` is not implemented yet.

The current code still needs stabilization: docs/comments still mention old
names, `NodeInvocation` is still a struct with `artifact + overrides` rather
than the desired enum, `NodeLoc` still needs to become `NodeRef` with absolute
and relative forms, and the runtime loader still reads `/project.json` and
discovers legacy directories.

The emerging source model is stronger than "artifact creates a node":
an artifact is an identified, loadable node spec. The same node spec can be
written inline under `project.toml` or saved as its own `*.toml` file and
referenced from the project. Directory structure becomes optional but
encouraged.

Terminology direction:

- `ArtifactLocator`: authored outside-world locator for a loadable artifact
  (`Path`, future builtin/library variants). This is the source-side form of
  "where to load a node definition from".
- Engine-side `ArtifactLocation`: resolved runtime/cache key used by
  `ArtifactManager`. This can stay separate from source-side
  `ArtifactLocator`.
- `NodeInvocation`: parent-owned instruction to instantiate a node at a `nodes`
  table key. Desired shape:

```rust
pub enum NodeInvocation {
    Artifact(ArtifactLocator),
    Inline(NodeDef),
}
```

- `NodeDef`: authored node body/definition (`kind = "project"`, `kind =
  "shader"`, etc.).
- `ProjectDef`, `ShaderDef`, `TextureDef`, `OutputDef`, `FixtureDef`:
  kind-specific bodies under `NodeDef`.
- `NodeLoc`: source string wrapper for a relative runtime node-tree locator, not
  a filesystem artifact path. For this plan it remains a string wrapper, but it
  must parse and enforce relative node-location semantics.

Node locations intentionally do **not** use slash syntax. Slashes are for
filesystem/artifact paths such as `./shader.toml` and `./shader.glsl`. Node
locations use a relative dot syntax:

```text
.                  current node
.child             child of current node
.child.grandchild  descendant of current node
..                 parent
..sibling          sibling through parent
..sibling.child    sibling's child
```

Node locations are relative-only for this plan. Absolute node-tree paths are not
supported yet; reusable artifacts should not hard-code global project tree
addresses. Future property references can append `#...`, e.g.
`..shader#state.output`, but this plan only needs the node-location part before
`#`.

The node source types have already moved out of the legacy namespace. The
current concrete authored node definition types live under `lpc-source/src/node/`:

- `TextureDef`
- `ShaderDef`
- `OutputDef`
- `FixtureDef`

For this plan, these should be treated as current core node definitions, not as
legacy node configs. The chosen suffix is `Def`, not `Spec` or `Config`.

`CoreProjectRuntime` still carries `legacy_src_dirs: HashMap<String, NodeId>`
and exposes `legacy_src_node_id`. That index exists to support the old
directory-discovery path and should disappear or be replaced by explicit
project-authored child identity once tests have moved.

`CompatibilityProjection` stores legacy authoring snapshots so the current wire
detail path can still construct legacy `NodeDetail` responses. The initial-load
change can keep this projection temporarily, but it should be populated from the
new project artifact declarations rather than from discovered directories.

`lpfs` docs/tests and multiple CLI/server paths still assume a project directory
contains `project.json`. Because this plan intentionally allows breakage early,
those callers can be migrated late, after `examples/basic` and core runtime
loading prove the new shape.

## Questions that need to be answered

### Q1: What is the exact source model for non-artifact builtin nodes?

Context: The desired example layout includes `output.toml`, `fixture.toml`, and
`texture.toml`, but the architecture direction says not everything needs an
artifact. Output in particular is likely just a bare runtime node with config
and bindings.

Answer: for this plan, every kept authored child file is still an artifact.
`shader.toml`, `output.toml`, `fixture.toml`, and `texture.toml` are all
artifacts. A node is only non-artifact-backed if it is declared directly inline
in `project.toml`; that may happen later, but not yet.

Status: resolved.

### Q2: Should shader be the only new artifact kind in this plan?

Context: The user wants projects to be artifacts and artifacts to remain the
way to define nodes. The current visual model has `Pattern`, `Effect`, etc.,
but the immediate legacy port needs shader initial load.

Answer: no. Add/shape project, shader, output, fixture, and texture artifacts
for this plan because each remains an authored file. The artifact kind may map
directly to an existing runtime node and config payload, but the authored file
is still loaded as an artifact.

Status: resolved.

### Q3: What should `project.toml` child declarations look like?

Context: The project artifact must specify child nodes directly and should be
readable enough to replace discovery-by-directory-name. It also needs stable
names for tree paths and references between configs.

Answer: use a named `nodes` table where each key is the project-local node name
and the value is a source node spec. Prefer the table form for readability:

```toml
schema_version = 1
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

The equivalent inline form is valid TOML if it uses TOML assignment syntax:

```toml
nodes.texture = { artifact = "./texture.toml" }
```

A bare string form would also be nice if it remains unambiguous:

```toml
nodes.texture = "./texture.toml"
```

This can deserialize as shorthand for `{ artifact = "./texture.toml" }`, but it
is lower priority than the explicit table/object form.

All artifact paths in this plan should be relative paths, and relative paths
resolve relative to the file that contains the reference. The explicit
`artifact = "./..."` field makes clear that this is a path-style artifact spec
and leaves room for other spec kinds later.

Future inline builtin node specs may look like:

```toml
[nodes.output]
kind = "output"
pin = 18
interpolate = true
dither = false
```

The same spec written as a reusable artifact:

```toml
# output.toml
kind = "output"
pin = 18
interpolate = true
dither = false
```

And referenced from `project.toml`:

```toml
[nodes.output]
artifact = "./output.toml"
```

For this plan, `kind = "output"` is enough. Later, more specific kinds such as
`kind = "output/gpio"` can be introduced when multiple output families exist.
Here `kind` chooses the node/spec kind, while `artifact` imports a node spec from
another address. A referenced artifact and the local project node table are the
same conceptual shape: source node specs, with references used for reuse and
sharing.

Status: resolved.

### Q4: How should references between child nodes be expressed during this
plan?

Context: Existing legacy configs refer to paths like `/src/main.texture` and
`/src/strip.output`. The new project model should not depend on directory
layout or suffix naming. Full binding semantics can evolve later, but initial
load must wire shader→texture and fixture→texture/output/shader.

Answer: use relative dot-syntax node locations, not slash paths and not
filesystem-looking references. `NodeLoc` remains a string wrapper for now, but
early phases should add parsing and resolution semantics for this convention.
For `examples/basic`, child artifacts are siblings under the project root, so
shader and fixture refs should use sibling locations:

```toml
# shader.toml
texture = "..texture"

# fixture.toml
texture = "..texture"
output = "..output"
```

Status: resolved.

### Q4a: What should happen to the existing `lpc_model::NodeSpec` string
wrapper?

Context: `lpc_model::NodeSpec` has already been mechanically renamed to
`NodeLoc`, but it is still a raw string wrapper. The plan needs to preserve the
simple source shape while giving it real relative node-location semantics.

Answer: keep `NodeLoc` as a source string wrapper for now, but add parsing,
validation, docs, and resolver support for the relative dot syntax. Do not add
absolute node-tree paths in this plan.

Status: resolved.

### Q5: Does `ProjectArtifact` instantiate as a real root node?

Context: The user described asking the `NodeTree` to load the project artifact
as the root node. Today `Engine::new(root_path)` creates a root entry, and
runtime skips root in legacy wire projection.

Answer: yes. `project.toml` is a node spec with `kind = "project"` and a
`nodes` table. Loading the project artifact instantiates the root `ProjectNode`.
The root is therefore not special authored data outside the model; it is the
first loaded node spec.

Status: resolved.

### Q6: Should `CoreProjectLoader::load_from_root` be replaced or kept as a
compatibility wrapper?

Context: Many tests and server/CLI paths call `load_from_root`. The new desired
runtime API is "provide the project artifact spec."

Answer: demolish the old loading path as part of this plan. The core runtime
starts at `/project.toml` (or an explicitly supplied `ArtifactLocator`) and
loads from the project artifact. Remove directory discovery and `/project.json`
from the core initial-load path rather than preserving a compatibility wrapper.
Flatten `examples/basic` early, validate the idea there, then migrate remaining
examples/tests near the end.

Status: resolved.

### Q7: Where should `ProjectDef` and the other node definition types live?

Context: `lpv-model` owns visual artifacts, but this work is core runtime
loading and current core node porting. The shader artifact here is not yet the
full future visual `Pattern`; it is the current shader authored definition with
params/config.

Answer: put source-side node definition types in `lpc-source/src/node/`:
`ProjectDef`, `ShaderDef`, `TextureDef`, `OutputDef`, and `FixtureDef`. Avoid
the `Artifact`, `Spec`, and `Config` suffixes for the loaded payload type; an
artifact is a loadable/identified def, while the Rust type is the def itself.

Status: resolved.

### Q8: How aggressive should test migration be in early phases?

Context: The user explicitly said migrating all integration tests and examples
should happen near the end, and some early breakage is acceptable.

Answer: early phases should validate with source-model unit tests and
`examples/basic`-specific loader/render smoke tests. Flatten `examples/basic`
early. Full integration/server/CLI/example migration belongs near the end once
the new idea is validated.

Status: resolved.

### Q9: Should the plan rename `SrcArtifactSpec` now?

Context: The conversation clarified that an artifact spec is the path/reference,
not the authored node spec itself. `SrcArtifactSpec` is already widespread and
works, but "spec" now risks meaning both reference and definition.

Answer: already started. `SrcArtifactSpec` has moved/renamed to
`ArtifactLocator`. The plan should stabilize comments, tests, and relative path
semantics around that name. Keep engine-side `ArtifactLocation` as the resolved
artifact-manager key.

Status: resolved.

### Q10: Should artifact-plus-local-fields merging be implemented in this plan?

Context: The long-term model likely allows:

```toml
[nodes.output]
artifact = "./outputs/gpio.toml"
pin = 18
```

where local fields override/complete the referenced artifact spec.

Answer: no. For this plan, support either an artifact-only use-site
spec or a fully inline spec only where needed for tests. The canonical
`examples/basic` migration should use one file per node artifact and
`project.toml` should only reference those artifacts.

Status: resolved.

### Q11: Should project `nodes` table author order drive instantiation?

Context: TOML author order is useful for humans, but runtime dependencies matter
more. Shader/fixture attachment currently depends on texture/output/shader
availability.

Answer: do not depend on author order. Load all child invocations into a
project-local name index, then instantiate/attach in dependency-safe order for
the current core nodes. Preserve readable source order where cheap, but do not
make it semantically load-bearing.

Status: resolved.

### Q12: Where does the runtime root tree path come from?

Context: `project.toml` has `kind = "project"` and may have authored metadata,
while `RuntimeServices` currently carries a `project_root: TreePath`.

Answer: keep `RuntimeServices::project_root` as the runtime root path for this
plan. `ProjectDef` metadata such as `name` is authored/editor metadata, not the
source of the runtime `TreePath` yet.

Status: resolved.

### Q13: What existing type names need an early stabilization audit?

Context: RustRover-assisted renames/moves already handled much of the old name
cleanup, but the tree is in an intermediate state. The plan should make that
explicit and stabilize the new names before changing the loader.

Answer: make the first implementation phase a source/runtime type
stabilization audit that reviews the current rename state, fixes stale comments
and obvious broken imports, and applies only the foundational shape changes
needed by later phases (`ProjectDef`, `NodeInvocation` shape, `NodeRef` shape).

Initial audit set:

| Existing type | Current role | Likely action |
| --- | --- | --- |
| `lpc_source::ArtifactLocator` | authored artifact path/lib locator | keep and stabilize source-side relative path semantics |
| `lpc_engine::ArtifactLocation` | resolved artifact-manager key | keep, maybe document as engine-side resolved locator |
| `lpc_source::NodeInvocation` | currently struct `{ artifact, overrides }` | stabilize as the artifact-only invocation shape for this plan; inline variant can wait |
| `lpc_model::NodeLoc` | string wrapper for another node | keep wrapper; add relative dot-syntax parser/resolution semantics and rustdocs |
| `lpc_source::node::NodeDef` | trait over concrete node definitions | keep for now; audit no_std/import visibility |
| `TextureDef`/`ShaderDef`/`OutputDef`/`FixtureDef` | current authored node definitions | keep; update stale comments/errors saying Config |
| `ProjectDef` | missing root node definition | add |
| `ProjectConfig` | old `/project.json` metadata | remove from runtime loading path; later migrate CLI/server tests |
| `RuntimePropAccess`/`RuntimeOutputAccess` | temporary produced data access split | keep for this plan; future data model work owns unification |
| legacy `NodeState` wire structs | compatibility detail projection | keep temporarily; not the durable node data model |

All relevant rustdocs for these types should capture the semantic meanings from
this discussion.

Status: resolved.

## Notes

- The old M4.2 source-reload-lifecycle-parity plan is background only. This
  standalone plan should not implement reload, deletion, or fs-change
  lifecycle parity.
- Removing `TextureNode` is explicitly deferred.
- Directory suffixes like `.shader`, `.texture`, `.fixture`, `.output` should no
  longer be semantic. File references and project child declarations own the
  model.
- For this plan, the child TOML files remain artifacts. Direct inline node
  declaration in `project.toml` is a possible future direction, not the current
  one.
