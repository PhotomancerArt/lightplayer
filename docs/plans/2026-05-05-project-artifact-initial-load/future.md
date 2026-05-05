# Future Work

## General Node Source Specs

- **Idea:** Treat every node kind as having a source spec type in `lpc-source`:
  `ProjectSpec`, `OutputSpec`, `FixtureSpec`, `ShaderSpec`, and `TextureSpec`.
  These are no longer "legacy node configs"; they are the authored source shape
  for the current core node set and should evolve toward the final node model.
- **Why not now:** This plan should focus on project-root artifact loading and
  replacing directory discovery. The spec types can begin here, but broad
  semantic cleanup should continue incrementally after initial load works.
- **Useful context:** The old "legacy" label should apply to obsolete loading
  infrastructure and compatibility projection, not to these node types
  themselves.

## Artifact As Identified Node Spec

- **Idea:** Model an artifact as an identified/loadable `NodeSpec`. A node spec
  can be written inline in `project.toml` or saved in its own TOML file and
  referenced with `artifact = "./node.toml"`.
- **Why not now:** The immediate plan only needs file-backed artifact references
  for `examples/basic`; inline specs and artifact-plus-local-override merge
  rules can wait until the base loader is stable.
- **Useful context:** This keeps directory layout optional but encouraged, and
  makes "extract to artifact" / "inline artifact" natural editor operations
  later.

## General Node Data Namespaces

- **Idea:** Move away from hard-coded `params` / `inputs` / indexed `outputs` /
  `state` assumptions and toward a general per-node namespace model. Nodes
  should be able to expose whatever namespaces and paths fit their contract.
- **Why not now:** The current engine and wire sync still have compatibility
  assumptions around legacy node state, `RuntimePropAccess`, and
  `RuntimeOutputAccess`. Project artifact loading can be built before solving
  the full data model.
- **Useful context:** A likely convention:

```text
<node>#config.xyz    authored source/spec data, usually edited by artifact mutation
<node>#param.xyz     dynamic inputs by convention, often bindable
<node>#state.xyz     produced/introspectable runtime data
<node>#state.output  conventional primary output when a node has one
```

The convention should not force every node to expose indexed outputs. A visual
node can expose `state.output`; a node with several products can expose several
named paths; a fixture may expose no output at all.

## Generic Wire Data

- **Idea:** Replace node-specific wire state shapes with a fully general dynamic
  data model. Node-specific client helpers can provide ergonomic typed access on
  top of generic data, but the wire should not need bespoke state structs for
  each node kind.
- **Why not now:** The M4 compatibility wire still needs to serve the existing
  client/demo shape while the runtime loader changes. General wire data is a
  larger sync/view migration.
- **Useful context:** The client can still have `ShaderView`, `OutputView`, etc.
  helpers, but those helpers should read from generic node data rather than
  requiring node-specific protocol payloads.

## Produced And Consumed Slot Access

- **Idea:** Unify `RuntimePropAccess` and `RuntimeOutputAccess` into a clearer
  access model, likely around the distinction between consumed values and
  produced values rather than scalar props versus runtime products.
- **Why not now:** Existing shader/output/resource projection code depends on
  the temporary split. Initial project-artifact loading should not also rewrite
  runtime data access.
- **Useful context:** Possible names include `ProducedSlotAccess` and
  `ConsumedSlotAccess`. The important distinction is dataflow direction:
  consumed values are resolved from source/config/bindings; produced values are
  written by runtime nodes and observed by other nodes/client sync.

## Inline Project Specs

- **Idea:** Eventually allow a whole project to be authored in a single
  `project.toml` by writing node specs directly under `[nodes.<name>]`, e.g.
  an output node with `kind = "output"`, `pin = 18`, and rendering options
  inline.
- **Why not now:** It may be powerful but can become unwieldy in TOML. The
  initial plan should support file-backed artifacts first and keep the
  representation manageable.
- **Useful context:** The intended examples can still encourage one file per
  node artifact:

```text
basic/
├── project.toml
├── output.toml
├── fixture.toml
├── shader.toml
├── texture.toml
└── shader.glsl
```
