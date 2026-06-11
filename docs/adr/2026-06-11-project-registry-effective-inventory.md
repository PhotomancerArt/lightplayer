# ADR 2026-06-11: Project Registry Effective Inventory

## Status

Accepted

## Context

The incremental artifact reload branch started with a node-definition registry
that mixed artifact tracking, overlay application, effective reads, sync
vocabulary, commit promotion, and node-specific inventory logic. That shape was
hard to explain and was too node-only for the UI and engine cutover we need.

The project editor needs three distinct views:

- files: durable artifacts on disk;
- project: referenced node definitions and referenced assets, including error
  entries;
- runtime: instantiated engine nodes and loaded runtime assets.

The registry is responsible for the project view. A project artifact can produce
either node definitions or assets, and both are discovered by walking the loaded
project graph.

## Decision

Replace the old node-only registry concept with `lpc-registry::ProjectRegistry`.
The registry owns:

- an `ArtifactStore` for known durable artifact locations and read freshness;
- `WithRevision<ProjectOverlay>` for pending edit intent;
- one effective `ProjectInventory`;
- the root `NodeDefLocation`.

The effective inventory is the registry truth:

```text
artifacts + overlay -> ProjectInventory { defs, assets }
```

`ProjectInventory` lives in `lpc-model` because clients need to inspect current
project state. It contains:

- `NodeDefEntry` keyed by `NodeDefLocation`;
- `AssetEntry` keyed by `ArtifactLocation`;
- loaded and error states for both definitions and assets.

The registry does not maintain a semantic `base_defs` graph. On load, overlay
apply, filesystem refresh, discard, and commit, the registry recomputes the
effective inventory and compares old inventory to new inventory.

Runtime-facing changes are represented as `ProjectChangeSet`, not as a diff.
`ProjectChangeSet` is identifier-oriented and coarse:

- node defs: added, removed, changed with `Body`, `KindChanged`,
  `EnteredError`, or `LeftError`;
- assets: added, removed, changed with `Body`, `EnteredError`, or `LeftError`.

Callers use the change set to decide what to refresh, then fetch current entries
from the registry. A snapshot-to-overlay helper may still exist for tests and
bootstrap workflows, but it is an operation that derives edit intent between two
file snapshots. It is not the runtime change vocabulary.

`NodeDef::invocation_sites` and `NodeDef::referenced_asset_paths` are the model
APIs for graph walking. The registry does not keep node-kind lists for project
topology. This pass assumes static authored references: no dynamic node-def refs
or dynamic asset refs are discovered at runtime.

Commit persists the already-effective overlay state to durable artifacts and
clears the overlay. A successful commit should normally return no
runtime-facing `ProjectChangeSet`; runtime consumers already reacted when the
overlay became effective.

## Consequences

The registry API is easier to reason about: load artifacts, apply overlay
mutations, refresh durable artifact changes, commit overlay, and observe
project changes.

Assets are first-class project inventory entries instead of being incidental
source reads behind node definitions.

Missing referenced defs/assets remain visible as project inventory errors, which
lets the future UI render a complete project view instead of losing broken
edges.

Full recompute is the first implementation. Incremental dirty tracking can be
added later behind the same API.

The crate is now `lpc-registry`. Historical plans and reviews may still mention
`lpc-node-registry`, `NodeDefRegistry`, `SyncOp`, or `NodeDefView`; live source
should use the new vocabulary.
