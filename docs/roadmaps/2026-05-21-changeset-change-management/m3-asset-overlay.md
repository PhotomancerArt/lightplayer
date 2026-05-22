# Milestone 3: File Ops + Asset Reads

## Title And Goal

Implement file-level **`ArtifactOp`** (`SetBytes`, `Delete`) on overlay and wire
**effective asset byte reads** (including `materialize_source` from overlay).
Prove **C4*** stories.

## Parallel Build

**`lpc-node-registry` only.** **`lpc-engine` unchanged.**

## Suggested Plan Location

`docs/roadmaps/2026-05-21-changeset-change-management/m3-asset-overlay/`

## Scope

In scope:

- Apply `SetBytes` / `Delete` on path-keyed overlay (implicit create on `Path`)
- `read_effective_bytes` for asset paths (`.glsl`, `.svg`, …)
- `materialize_source` reads overlay when present (before store/fs)
- Harness spot tests:
  - **C4c** — replace `.glsl` via overlay; def slot unchanged; source revision
    bumps after commit
  - **C4a/b/d** — add asset, delete asset, replace without touching def TOML

Out of scope:

- Slot op apply on `.toml` (**M4**)
- Binary assets ([`future.md`](future.md))
- Commit promotion (**M5**) — may test bytes in overlay only until M5

## Key Decisions

- **Assets use file ops** — not slot ops; same `ArtifactChange` grouping as TOML.
- **Whole-file text only** in v1 — no byte-range patches.
- **User term "asset"** = non-node file; store identity remains **artifact**.

## User Stories / Gate

| ID | Story | Covered |
|----|-------|---------|
| C4c | Replace GLSL; def unchanged; source revision after commit | **Yes** |
| C4a/b/d | Add / delete / replace asset files | Spot tests |

## Deliverables

- File op apply on overlay
- Asset effective read + materialize integration
- `tests/changeset/` asset scenarios

## Dependencies

- M1 overlay lifecycle
- M2 effective projection

## Execution Strategy

Full plan.

Suggested chat opener:

> M3 plan: SetBytes/Delete + overlay asset reads for materialize. Full plan then implement. Agree?
