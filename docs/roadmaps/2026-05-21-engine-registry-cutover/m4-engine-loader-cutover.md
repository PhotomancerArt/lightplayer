# Milestone 4: Engine Loader Cutover

## Title and goal

**Hard cut:** `ProjectLoader` / `Engine` → **`NodeDefRegistry`**; delete old
artifact payload store.

## Suggested plan location

`docs/roadmaps/2026-05-21-engine-registry-cutover/m4-engine-loader-cutover/`

## Scope

**In:** `lpc-engine` depends on registry; loader via registry + `NodeDefView`;
remove `lpc-engine/src/artifact/`.

**Out:** SyncResult incremental policy (M5) unless merged forward.

## Key decisions (from M1)

May run **before, with, or immediately after M3** per M1 sequencing doc.

## Dependencies

- M1 sequencing choice
- M2 wire types (if client edits during cutover testing)
- Artifact-routed M4 + ChangeSet M6 diff gate

## Execution strategy

**Full plan**
