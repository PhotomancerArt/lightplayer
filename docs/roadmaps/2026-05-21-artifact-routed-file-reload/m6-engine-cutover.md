# Milestone 6: Engine + Node Cutover

## Title And Goal

**End parallel build:** delete the old `lpc-engine` artifact-as-registry path and
hard-cut **`ProjectLoader`**, **`Engine`**, and **shader/fixture runtimes** to
**`lpc-node-registry`** — only after **M4** here and the
[ChangeSet roadmap](../2026-05-21-changeset-change-management/overview.md)
**M6 diff + equivalence gate** pass.

## Parallel Build → Cutover

This milestone **ends** the dual-stack period:

1. Add `lpc-node-registry` dependency to `lpc-engine`.
2. Switch loader/engine/nodes to new stores, **`NodeDefView`**, and **ChangeSet**
   path (client edits via ChangeSet; wire `slot_mutation` aligns with M5 model).
3. **Delete** old `lpc-engine/src/artifact/` payload model.
4. Migrate production `ShaderDef` / fixture defs to `SourceFileSlot`; remove
   `ShaderSource`.

Until this milestone lands, production continues on the old path.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m6-engine-cutover/`

## Scope

In scope:

- **`lpc-engine` depends on `lpc-node-registry`**; delete old artifact-as-registry.
- `ProjectLoader` + `Engine` use M1–M3 stores; apply `NodeDefUpdates` per M4/M5.
- **`NodeDefView` + ChangeSet** wired for reads and client mutation path.
- **Shader / compute / fixture nodes** → `SourceFileRef` + materialize (M3).
- `NodeEntry.def_id: NodeDefId`; remove `NodeDefHandle`.

Out of scope:

- Server fs-change routing (**M7**).
- `project.toml` graph reconciliation (**M8**).

## Key Decisions

- **Gate:** M4 here + [ChangeSet roadmap M6 diff gate](../2026-05-21-changeset-change-management/m6-diff-equivalence-gate.md) green before starting M6.
- Hard cut; no dual-store in production.

## Deliverables

- Engine cutover + shader/fixture nodes + integration tests.
- Old artifact path removed.

## Dependencies

- M1–M4 here complete and passing.
- [ChangeSet roadmap](../2026-05-21-changeset-change-management/overview.md) M6 diff + equivalence gate green.

## Execution Strategy

Full plan. Cross-cutting cutover; implement against M4/M5 harness contracts.

Suggested chat opener:

> M6 engine cutover needs a full plan — M4 + ChangeSet M6 gate must be green first. Agree?
