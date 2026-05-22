# Milestone 4: Slot Ops + TOML Serialize

## Title And Goal

Implement slot-level **`ArtifactOp`** (`SetSlot`, `MapInsert`, `MapRemove`,
`OptionSet`, …) on overlay TOML artifacts and **serialize overlay slot trees to
TOML bytes** (commit path prep — promotion itself is **M5**). Prove **C1*** and
**C2*** stories.

## Parallel Build

**`lpc-node-registry` only.** **`lpc-engine` unchanged.**

## Suggested Plan Location

`docs/roadmaps/2026-05-21-changeset-change-management/m4-node-slot-patches/`

## Scope

In scope:

- Apply slot ops at `SlotPath` within target artifact ([`change-language.md`](change-language.md))
- Overlay holds slot draft per `.toml` path (lazy fork from committed parse OK)
- **Serialize** effective slot tree → TOML text (slot codec); used by M5 commit
- Harness:
  - **C1a–f** — kind + defaults + slot patches; wiring via slot ops on parent
  - **C2a–c** — inline child edits at nested paths (`entries[n].node`, …)
  - Inline child edit marks child changed in `NodeDefUpdates` shape (post-commit
    expectation documented for M5)

Out of scope:

- **C3** inline ↔ standalone refactor ([`future.md`](future.md))
- Commit flush to store (**M5**)
- `CreateDef` op — kind + defaults via slot ops only

## Key Decisions

- **Node defs are slots** — all TOML authoring via slot ops at `root()` or inline
  paths.
- **Wiring = slot ops** — e.g. `SetSlot` on `nodes[shader].def` path locator.
- **Normal TOML not via `SetBytes`** — serialize from slot tree; `SetBytes` is
  import escape hatch only.

## User Stories / Gate

| ID | Story | Covered |
|----|-------|---------|
| C1a–f | Standalone def slot + file ops | **Yes** (minimum c/d for gate via diff) |
| C2a–c | Inline nested slot ops | **b/c in M4** |

## Deliverables

- Slot op apply on overlay drafts
- TOML serialize helper for overlay → bytes
- `tests/changeset/` slot scenarios (C1/C2)

## Dependencies

- M1–M3

## Execution Strategy

Full plan. Largest milestone — slot mut access on overlay copies.

Suggested chat opener:

> M4 plan: slot ops on overlay + TOML serialize path. Full plan then implement. Agree?
