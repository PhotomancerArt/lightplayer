# ADR 2026-06-12: Effective Project Graph

## Status

Accepted

## Context

`ProjectRegistry` currently derives an effective `ProjectInventory` containing
node definitions keyed by `NodeDefLocation` and assets keyed by `AssetLocation`.
That flat inventory is enough to answer "which definitions and assets are
currently referenced?", but it is not enough to build or update the engine
runtime tree.

The engine needs node instances and parent-child edges. A single external node
definition can be referenced more than once:

```toml
[nodes.left]
ref = "./shader.toml"

[nodes.right]
ref = "./shader.toml"
```

That project has one definition identity for `/shader.toml`, but it needs two
runtime node instances.

Inline node definitions add another wrinkle: they are definitions located
inside an owning artifact at a slot path, but they are still instantiated as
project graph nodes.

## Decision

Add an effective project graph/topology model to the registry output.

The graph models project node instances separately from node definition
identity:

- `NodeDefLocation` remains definition identity.
- A new project node instance identity, such as `ProjectNodeKey`, identifies one
  authored node instance in the effective project graph.
- Runtime `NodeId` remains engine-local runtime identity and is not used as
  project graph identity.

`ProjectInventory` may own the graph directly or expose it through an adjacent
registry-owned type, but the registry must provide this information as part of
its effective project state.

The graph needs enough data for engine projection:

- root project node instance;
- all reachable project node instances;
- parent-child relationships;
- child name;
- child invocation slot path;
- authored `NodeInvocation`;
- resolved `NodeDefLocation`;
- role or ownership metadata, such as root, project child, and playlist entry;
- indexes from `NodeDefLocation` to project node instances;
- indexes from `AssetLocation` to project node instances that consume the asset.

`ProjectNodeKey` should be deterministic, serializable, stable across refreshes
when authored topology does not change, distinct from `NodeDefLocation`, and
independent of runtime `NodeId`. An ancestry-based project node path is the
preferred first implementation.

The first implementation may continue using current model discovery APIs:

- `NodeDef::invocation_sites`;
- `NodeDef::referenced_assets`.

The engine must consume the registry graph and must not reimplement discovery by
matching directly on `ProjectDef`, `PlaylistDef`, shader source, or fixture
mapping internals.

## Consequences

The engine can build a runtime `NodeTree` from registry state without keeping
its own project walker.

Duplicate external references produce multiple project graph nodes and later
multiple runtime nodes while sharing one `NodeDefEntry`.

Missing or errored referenced definitions remain visible as graph nodes that
point at error `NodeDefEntry`s. Their children and assets cannot be discovered
until the definition loads successfully.

Future generic node-reference metadata can feed the same graph API. The cutover
does not need to wait for a fully generic node slot model.

Project graph changes can later drive incremental runtime apply. The first
runtime update strategy may still be conservative subtree rebuilds.
