# Milestone 2: NodeDefRegistry + NodeDefUpdates

## Title And Goal

Introduce **`NodeDefRegistry`** in the existing **`lpc-node-registry`** crate
(M1 bootstrap) as the owner of parsed node definitions, with **`NodeDefUpdates`**
reporting when artifacts change.

**Prerequisite:** M1 crate + `ArtifactStore` complete and tested in isolation.

## Parallel Build

**M2 does not modify `lpc-engine`.** Registry and update logic are new code in
`lpc-node-registry`, tested against the M1 artifact store and real `NodeDef`
parse paths — not wired into `ProjectLoader`.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m2-node-def-registry/`

## Scope

In scope:

- `NodeDefId` opaque handle (same pattern as `ArtifactId`).
- Bootstrap: **`load_root(root_path)`** — any node-definition TOML (project is
  convention); registry walks and registers all defs.
- Steady state: driver **`apply_fs_changes`** on store, then **`sync`** →
  `NodeDefUpdates`.
- Registry entry source `{ artifact_id, path_in_artifact: SlotPath }` (root =
  `SlotPath::root()`).
- Identity separate from content: `Loaded` / `ParseError` / `ValidationError`.
- **`sync`** → `NodeDefUpdates` (driver applies fs to store first).
- Inline defs derived from parent artifact at non-root paths.
- Def content change does not mark parent def changed when only a child inline
  def changed.
- Stub **`NodeDefView`** read path (base registry only; **ChangeSet overlay in M5**).
- Unit tests: artifact re-parse scenarios, inline child isolation, add/remove.

Out of scope:

- Production `Engine` / `ProjectLoader` cutover (**M6**).
- `SourceFileSlot` (M3).
- Engine tree mutation from updates (M4 harness).

## Key Decisions

- **Parallel crate only** — no `lpc-engine` / `ProjectLoader` edits.
- Registry is testable in isolation: artifact change in, `NodeDefUpdates` out.
- v1 may recreate def entries wholesale (no stable id preservation yet).
- Graph wiring changes surface as parent def `changed` but tree mutation is
  engine responsibility (**M8**).

## Deliverables

- `lp-core/lpc-node-registry/src/registry/` (registry, updates, view stub).
- Parser integration hook (read artifact bytes → `NodeDef` or error).
- Tests covering leaf TOML change, inline child edit, child add/remove.

## Dependencies

- M1 ArtifactStore freshness model.

## Execution Strategy

Full plan. Registry entry lifecycle, inline path scheme, and update diff logic
need design doc before implementation.

Suggested chat opener:

> This milestone needs a full plan — NodeDefRegistry, NodeDefUpdates diff rules,
> and inline def paths. I'll run the plan process then implement. Agree?
