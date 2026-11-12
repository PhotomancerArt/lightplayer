# Milestone 2: Effective Projection

## Title And Goal

Wire **effective reads** through overlay ∪ base. All public registry/view reads
return **effective** state — never a committed-only shortcut. Prove the read
contract parent **M6** engine cutover will rely on.

## Parallel Build

**`lpc-node-registry` only.** **`lpc-engine` unchanged.**

## Suggested Plan Location

`docs/roadmaps/2026-05-21-changeset-change-management/m2-effective-projection/`

## Scope

In scope:

- `read_effective_bytes(path)` — overlay before `ArtifactStore` / fs
- `NodeDefView::get` — effective `NodeDefState` (committed entry ∪ overlay draft)
- Internal artifact queries on registry route through overlay
- Eager re-parse of touched `.toml` def artifacts (same shape as parent M4
  `sync`, touched paths only)
- Harness:
  - apply overlay change → view ≠ committed `entries`
  - discard → view == committed `entries` again

Out of scope:

- Full file/slot op apply (**M3**, **M4**)
- Commit (**M5**)
- Typed `*DefView` against effective `SlotShapeLookup` — optional; materialized
  effective `NodeDef` OK for M2
- Provenance / mutated badges — client or parent **M10** probes

## Key Decisions

- **`entries` = committed cache** — updated only on commit / `sync_fs`.
- **Public reads = effective only** — callers need not know pending vs committed.
- **No provenance on read path** — values only; memory constraint on ESP32.

## User Stories / Gate

| ID | Story | Covered |
|----|-------|---------|
| D1 | Apply → effective view ≠ committed base | **Extends M1** |

## Deliverables

- Effective read path on `NodeDefRegistry`
- `NodeDefView` updated from passthrough stub
- Projection harness tests

## Dependencies

- M1 change language + overlay lifecycle

## Execution Strategy

Full plan. Projection is the engine-integration contract.

Suggested chat opener:

> M2 plan: effective NodeDefView + artifact reads through overlay. Full plan then implement. Agree?
