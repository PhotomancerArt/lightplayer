# Milestone 5: Commit Promotion

## Title And Goal

Implement **commit**: promote overlay → `ArtifactStore` (+ optional fs write) →
re-derive `entries` → **`SyncResult`** (same shape as parent M4 fs sync) → clear
overlay. Prove **D2** and **D5**.

## Parallel Build

**`lpc-node-registry` only.** **`lpc-engine` unchanged.**

## Suggested Plan Location

`docs/roadmaps/2026-05-21-changeset-change-management/m5-commit-promotion/`

## Scope

In scope:

- `NodeDefRegistry::commit` (or equivalent facade)
- Flush overlay paths: asset bytes + serialized TOML from M4 slot trees
- Re-derive affected defs; populate `SyncResult` / `NodeDefUpdates`
- `RegistryChange::ChangeSet` or dedicated commit entry point (plan decides)
- All-or-nothing commit; failure leaves base untouched
- **D5** harness — uncommitted overlay wins on effective read; after commit, fs
  `sync` on same path follows committed rules; overlay cleared

Out of scope:

- Engine cutover (parent **M6**)
- Project diff (**M6**)
- Wire `slot_mutation` alignment (parent **M6**)

## Key Decisions

- **Commit reuses parent M4 re-derive path** where possible.
- **`discard`** = overlay clear only; **`commit`** = only path that mutates
  committed `entries` from client edits.
- **Failed commit** — base unchanged; overlay may retain pending state (plan
  specifies).

## User Stories / Gate

| ID | Story | Milestone |
|----|-------|-----------|
| D2 | Commit → base updated; overlay clear | **M5** |
| D5 | Overlay vs fs-change precedence | **M5** |

## Deliverables

- Commit API on registry
- D2 + D5 harness tests
- `commit-contract.md` design note in plan folder

## Dependencies

- M1–M4 (meaningful commit requires file + slot ops + serialize)

## Execution Strategy

Full plan. Commit touches registry ownership after op semantics land.

Suggested chat opener:

> M5 plan: commit promotion to base + SyncResult + D5 precedence. Full plan then implement. Agree?
