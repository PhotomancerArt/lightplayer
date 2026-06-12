# ADR 2026-06-12: Project Registry And Engine Ownership

## Status

Accepted

## Context

The incremental artifact reload branch introduced `lpc-registry` as a separate
crate while the registry work was still a spike. The next phase cuts
`lpc-engine` over to that project model.

There are three distinct domains:

- files: durable project artifacts and filesystem freshness;
- project: effective node definitions, assets, overlay state, and project
  change sets;
- runtime: instantiated engine nodes, buffers, bindings, services, and ticking.

Current engine loading still owns a separate artifact cache under
`lpc-engine/src/artifact`. That cache mixes resolved artifact locations,
opaque runtime handles, loaded `NodeDef` payloads, content revision, and
refcount-like lifecycle behavior. The new registry already owns project artifact
freshness and effective node-definition state, so keeping a second engine
artifact truth would make the cutover harder to reason about.

We also considered whether `ProjectOverlay` and `ProjectInventory` should be
owned directly by `Engine` instead of `ProjectRegistry`.

## Decision

Keep `lpc-registry` as a separate project-state crate.

`ProjectRegistry` owns:

- the registry `ArtifactStore` for known durable artifact locations and read
  freshness;
- `WithRevision<ProjectOverlay>` for pending edit intent;
- the effective `ProjectInventory`;
- the root `NodeDefLocation`.

`Engine` owns a `ProjectRegistry`, but does not own the registry's overlay or
inventory directly. The engine also owns runtime projection state:

- `NodeTree<Box<dyn NodeRuntime>>`;
- runtime buffers and resources;
- resolver and dataflow state;
- services, graphics, and hardware-facing runtime dependencies;
- projection indexes from project graph node instances to runtime `NodeId`s.

`NodeTree` remains a runtime tree. It must not become the project discovery
model.

The old engine artifact cache should be removed or fully superseded by registry
and model identities. Runtime tree entries should use project/model identities
such as `NodeDefLocation` and project node instance keys instead of an engine
local `ArtifactId`.

`Engine` may expose convenience methods that orchestrate registry operations:

```text
load project -> registry load -> runtime projection build
apply overlay mutation -> registry change set -> runtime projection update
refresh filesystem events -> registry change set -> runtime projection update
```

Those methods should preserve the registry as the consistency boundary for
overlay, inventory, artifact freshness, and project change calculation.

## Consequences

Server and UI code can inspect and edit project state through `lpc-registry`
without depending on engine runtime internals.

Registry APIs remain responsible for keeping overlay, artifact freshness,
effective inventory, and project change sets consistent.

Engine work becomes a projection problem: turn current registry state and
registry changes into runtime tree, bindings, resources, and node lifecycles.

Removing engine-local `ArtifactId` avoids two parallel identities for the same
project definition. If a temporary compatibility handle is needed during the
cutover, it should be documented as transitional and removed before the cleanup
milestone completes.

The crate boundary adds one dependency from `lpc-engine` to `lpc-registry`, but
keeps project/editor state out of concrete runtime node code.
