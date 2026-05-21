# Milestone 4: Fs-Change Semantics Harness

## Title And Goal

Prove end-to-end **reload semantics** in **`lpc-node-registry` test harness**
only — simulated `FsChange` → artifact bumps → `NodeDefUpdates` → expected
node lifecycle actions. **Production `lpc-engine` unchanged.**

## Parallel Build

The harness exercises **only** the new crate (M1–M3). It may duplicate small
fixture projects and loader-like parse glue **inside `lpc-node-registry` tests**
rather than calling production `ProjectLoader`. Expected engine actions are
asserted as a **spec log** for M5 to implement against.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m4-fs-change-semantics-harness/`

## Scope

In scope:

- Harness in **`lpc-node-registry`** tests (memory fs via `lpfs`) loading fixture
  projects into M1/M2/M3 stores.
- Apply `FsChange` batches; bump artifacts; call registry update; assert
  `NodeDefUpdates`.
- Scenarios:
  - Leaf node TOML edit → one def `changed`.
  - GLSL file edit → file artifact bumped; defs referencing `SourceFileRef`
    see materialize version change (no def change if TOML unchanged).
  - SVG file edit → same for fixture mapping source.
  - Inline child def edit → child `changed`, parent not `changed`.
  - Parse error → def error state; expected destroy/cascade markers in harness
    action log.
- Document expected **engine actions** per update (refresh node, destroy node,
  cascade parent error) as harness assertions — not yet wired to real `Engine`.

Out of scope:

- Production engine cutover (**M6**).
- ChangeSet / client change management (**M5**).
- Server `LpServer` fs routing (**M7**).
- `project.toml` topology changes (**M8**).
- Any edits to `lpc-engine` or `lpa-server`.

## Key Decisions

- Harness is prerequisite for **M5**; **M4 + M5** gate **M6**.
- v1 node refresh rule may be coarse (recreate all nodes bound to changed defs).
- Error propagation: no last-good; def error → node destroy → parent error.

## Deliverables

- Reload harness module + fixture projects under **`lpc-node-registry` tests**.
- Scenario table documented in milestone summary (serves as M5 contract).
- CI-running tests for all scenarios above.

## Dependencies

- M1, M2, M3 complete.

## Execution Strategy

Full plan. Multiple scenarios, harness API, and action expectations need a
design pass before tests are written.

Suggested chat opener:

> This milestone needs a full plan — fs-change harness, scenario matrix, and
> expected engine action assertions. I'll run the plan process then implement.
> Agree?
