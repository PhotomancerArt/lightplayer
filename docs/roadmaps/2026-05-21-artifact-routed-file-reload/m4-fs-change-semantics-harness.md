# Milestone 4: Fs-Change Semantics Harness

## Title And Goal

Prove end-to-end **reload semantics** in **`lpc-node-registry` test harness**
only — simulated `FsChange` → artifact bumps → `NodeDefUpdates` → expected
node lifecycle actions. **Production `lpc-engine` unchanged.**

## Parallel Build

The harness proves **`NodeDefRegistry::sync(changes) -> SyncResult`** in tests
only. Production `lpc-engine` unchanged until M6.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m4-fs-change-semantics-harness/`

## Scope

In scope:

- **API refactor:** registry owns state; `sync` takes `RegistryChange` batch
  (fs in M4), applies, returns **`SyncResult`** (factual diff).
- Harness fixtures + scenario tests S1–S6.
- **`engine-policy-v1.md`** — how M6 engine would react (not registry output).

Out of scope:

- Production engine cutover (**M6**).
- `RegistryChange::ChangeSet` variants — [ChangeSet M5](../2026-05-21-changeset-change-management/m5-commit-promotion.md); enum stub OK.
- ChangeSet / client change management ([promoted roadmap](../2026-05-21-changeset-change-management/overview.md)).
- Server `LpServer` fs routing (**M7**).
- `project.toml` topology changes (**M8**).
- Any edits to `lpc-engine` or `lpa-server`.

## Key Decisions

- Harness is prerequisite for [ChangeSet roadmap](../2026-05-21-changeset-change-management/overview.md); **M4 + ChangeSet M6** gate **M6**.
- v1 node refresh rule may be coarse (recreate all nodes bound to changed defs).
- Error propagation: no last-good; def error → node destroy → parent error.

## Deliverables

- **`sync(changes) -> SyncResult`** API on `NodeDefRegistry`
- Scenario tests S1–S6
- `engine-policy-v1.md` for M6

## Dependencies

- M1, M2, M3 complete.

## Execution Strategy

Full plan. Multiple scenarios, harness API, and action expectations need a
design pass before tests are written.

Suggested chat opener:

> This milestone needs a full plan — fs-change harness, scenario matrix, and
> expected engine action assertions. I'll run the plan process then implement.
> Agree?
