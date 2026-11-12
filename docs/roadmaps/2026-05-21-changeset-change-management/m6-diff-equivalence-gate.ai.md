# Milestone 6: Diff + Equivalence Gate

## Title And Goal

Add **`diff(base, target) → ChangeSet`** and prove **apply + commit ≡ target**.
This milestone **gates parent artifact-routed M6** (engine cutover).

Primary regression harness — replaces hand-curated op lists for compose/morph.

## Parallel Build

**`lpc-node-registry` only.** **`lpc-engine` unchanged** until parent **M6**
starts after this gate is green.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-changeset-change-management/m6-diff-equivalence-gate/`

## Scope

In scope:

- `ProjectSnapshot` — fs walk or in-memory `BTreeMap<LpPath, bytes>`
- `diff(base, target) -> ChangeSet`:
  - new/removed paths → `ArtifactChange` with file ops
  - assets → `SetBytes` / `Delete`
  - `.toml` → slot-tree diff → minimal slot ops (not `SetBytes` for normal path)
- `assert_equivalent(reg, snapshot)` — path set + asset bytes + parsed def/slot
  equality (TOML need not be byte-identical)
- **Gate tests:**
  - `diff(∅, examples/basic)` → apply on empty registry → commit → ≡ basic (**A1**)
  - `diff(basic, basic2)` → apply → commit → ≡ basic2 (**B1**)
- Empty base snapshot = **truly no files** (creatability)

Out of scope:

- Full A2–A4 / B4 N×N matrix ([`notes.ai.md`](notes.ai.md))
- **C3** inline ↔ standalone ([`future.ai.md`](future.ai.md))
- Engine runtime assertions (parent **M6**)
- Post-M6 replay stress harness (parent [`future.ai.md`](../2026-05-21-artifact-routed-file-reload/future.ai.md))

## Key Decisions

- **Diff output = change language** — same `ArtifactChange` vocabulary as UI edits.
- **Equivalence ≠ byte-identical TOML** — parsed def + slot snapshot equality.
- **Hand-written A1/B1 op lists** — optional once diff gate is green.

## User Stories / Gate

| ID | Story | Covered |
|----|-------|---------|
| A1 | Blank → `basic` | **Via diff(∅, basic)** |
| B1 | `basic` → `basic2` | **Via diff(basic, basic2)** |
| D1–D3, D5 | Lifecycle | **M1, M5** (prerequisite) |
| C1/C4 core | Slot + file ops | **M3–M4** (prerequisite) |

**Sign-off:** this milestone green → unblocks [parent M6](../2026-05-21-artifact-routed-file-reload/m6-engine-cutover.ai.md).

## Deliverables

- `lpc-node-registry/src/diff/`
- `tests/changeset/project_diff.rs` (or equivalent)
- Gate sign-off note in plan `summary.ai.md`

## Dependencies

- M1–M5 complete

## Execution Strategy

Full plan. Diff depends on full apply + commit path.

Suggested chat opener:

> M6 plan: ProjectSnapshot + diff + empty→basic equivalence gate. Full plan then implement. Agree?
