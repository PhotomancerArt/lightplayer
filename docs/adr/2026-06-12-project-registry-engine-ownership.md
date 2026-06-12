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
owned directly by `Engine` instead of `ProjectRegistry`, and whether
`ProjectRegistry` should be a field on `Engine` or on the server-side project
container.

## Decision

Keep `lpc-registry` as a separate project-state crate.

`ProjectRegistry` owns:

- the registry `ArtifactStore` for known durable artifact locations and read
  freshness;
- `WithRevision<ProjectOverlay>` for pending edit intent;
- the effective `ProjectInventory`;
- the root `NodeDefLocation`.

`Engine` does not own `ProjectRegistry`. The server-side project container owns
both:

- `ProjectRegistry`, the canonical project state;
- `Engine`, the current runtime projection of that project state.

`Engine` owns runtime projection state:

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

Project loading returns both sides together through a lightweight
`LoadedProjectRuntime` wrapper for direct loader callers and tests. Long-lived
server ownership still lives in the server project container.

The server project container orchestrates registry operations:

```text
load project -> registry load -> runtime projection build
apply overlay mutation -> registry change summary -> rebuild runtime projection
commit overlay -> write artifacts -> rebuild runtime projection
refresh filesystem events -> registry change summary -> rebuild runtime projection
```

M4 deliberately uses a full runtime rebuild after accepted overlay edits or
filesystem refreshes. Incremental runtime updates can later consume registry
change summaries without changing ownership.

This M4 bridge was superseded by
`docs/adr/2026-06-12-incremental-runtime-apply.md`: overlay mutation and
filesystem refresh now apply `ProjectChangeSummary` incrementally, while commit
is persistence-only and full reload is manual/recovery.

Project reads are runtime queries against `Engine` plus `ProjectRegistry`.
Overlay reads, overlay mutations, overlay commits, and inventory reads use a
separate project command API instead of being embedded in project-read
requests.

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

Removing project mutations from project reads gives the UI an explicit
read/write split: `ProjectReadRequest` observes runtime state, while
`WireProjectCommand` carries overlay and inventory operations.
