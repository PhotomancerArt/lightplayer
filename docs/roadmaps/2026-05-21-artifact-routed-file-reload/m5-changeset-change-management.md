# Milestone 5: ChangeSet / Change Management

## Title And Goal

Prove **client-driven change management** in the parallel `lpc-node-registry`
stack: ordered, id'd **ChangeSets** that express authorable edits in memory until
**commit** — alongside fs-driven reload (M4). This is the **architecture gate**
before production engine cutover (M6).

All future client edits should flow through this model (temporary overlay →
commit to disk/registry).

**User stories** (below) drive harness design and acceptance: if we can compose
any example from blank, morph one example into another one ChangeSet at a time,
and cover the core author actions without crashing, the ChangeSet model is
proven for M6.

## Parallel Build

**M5 does not modify `lpc-engine`.** Change management lives in
`lpc-node-registry` + harness tests. Old M4.1 (projection-only) is **folded into
this milestone**.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m5-changeset-change-management/`

## Scope

In scope:

### ChangeSet model

- **ChangeSet** — ordered, id'd collection of changes; in-memory only until
  commit (commit may be harness-simulated: promote to `ArtifactStore` /
  `NodeDefRegistry` base).
- **`NodeDefView` + `AssetView`** — sole read paths: base + active ChangeSet(s)
  projected.

### Change forms (v1)

Single ordered stream; two op families (see Key Decisions):

1. **`NodeChange`** — semantic slot patches (SlotOp-style: set value, map
   insert/remove, add/remove def refs, add/remove inline defs).
2. **`AssetChange`** — non-node project files (GLSL, SVG, …): add, delete,
   **whole-file replace**.

**Asset** = dependency file used by nodes that is **not** a node definition
file. **Artifact** = store identity / freshness entry.

### Global invariants (all user stories)

Every harness scenario must satisfy:

- **No panic / no corrupt base** — applying any single ChangeSet op, or any
  sequence, leaves the harness in a defined state. Base registry and artifacts
  are untouched until **commit**.
- **Intermediate uselessness is OK** — mid-sequence the effective view may have
  parse errors, missing bindings, dangling def refs, or nodes that would enter
  error state at runtime. That is expected during morphs.
- **Commit contract** — commit either promotes a consistent overlay to base
  (emitting `NodeDefUpdates` + artifact bumps) or returns an explicit commit
  error without partial promotion.
- **Discard** — always restores reads to base exactly.

### Interaction with M4 fs-change

Document and test precedence (overlay wins for overlaid paths while uncommitted;
see Key Decisions).

Out of scope:

- Full wire protocol / `lpc-wire` message shapes (follow M6+ or separate plan).
- CRDT merge / concurrent editing (ordered ChangeSet only in v1).
- Byte-range asset patches, binary assets (whole-file text assets only in v1).
- Production `slot_mutation` / engine cutover (**M6**).
- Server fs routing (**M7**).
- Runtime node tree assertions (harness proves **view + updates**; engine
  behavior is M6).

## User Stories

Stories are grouped three ways. Harness tests should map 1:1 to story IDs where
practical. Reference projects live under `examples/` (`basic`, `events`,
`fyeah-sign`, `button-playlist`, `fluid`, …).

### A — Compose from blank

Prove any existing example can be **authored entirely via ChangeSets** starting
from an empty project (empty `project.toml` + empty store).

| ID | Story | Acceptance |
|----|-------|------------|
| A1 | **Blank → `basic`** | Ordered ChangeSets add: `output.toml`, `clock.toml`, `shader.toml` + `shader.glsl`, `fixture.toml` + mapping SVG (or path-only fixture), wire all nodes in `project.toml`. After final commit, effective view matches loaded `examples/basic`. |
| A2 | **Blank → `events`** | Same pattern for dual `ComputeShader` defs + GLSL assets + visual `Shader` + fixture. |
| A3 | **Blank → `button-playlist`** | Includes `Button`, `Playlist` with entry map pointing at child shader defs. |
| A4 | **Blank → `fyeah-sign`** | Full graph (button, radio, playlist, fixture w/ SVG). May split across multiple harness tests or plan sub-phases. |

Each step in the sequence is also a **single-op** story: applying ChangeSet *k*
never crashes; view after step *k* may be incomplete.

### B — Morph between examples

Prove any example can be **mutated into any other** via a sequence of ChangeSets,
**one logical edit at a time**, without ever breaking the harness.

| ID | Story | Acceptance |
|----|-------|------------|
| B1 | **`basic` → `basic2`** | Incremental slot edits (extra nodes, texture, binding tweaks). Each step: apply → read view → optional commit. Final state matches `examples/basic2`. |
| B2 | **`basic` → `button`** | Add `button.toml`, rewire bindings, add button shader asset. Intermediate graphs may lack valid trigger wiring. |
| B3 | **`events` → `basic`** | Remove compute nodes and assets; simplify fixture/shader. Proves delete ops and graph shrink. |
| B4 | **Cross-family morph** | Pick one published transition matrix (e.g. `basic` → `fast` → `rocaille`) as a regression suite; document that full N×N coverage is aspirational, spot-check representative pairs. |

**Morph rule:** between any two consecutive ChangeSets in a morph sequence, the
harness must not panic; `NodeDefView` / `AssetView` remain queryable; commit
errors are surfaced explicitly, not as silent corruption.

### C — User actions (atomic author operations)

These are the **primitives** that compose stories A and B. Each should have at
least one focused harness test.

#### C1 — CRUD node defs and properties

| ID | Story |
|----|-------|
| C1a | **Create** standalone node def (new TOML artifact + parseable content) and **wire** it into `project.toml` `[nodes.*]` or a parent's map slot. |
| C1b | **Read** effective def via `NodeDefView` after overlay (slot values, bindings, consumed/produced shapes). |
| C1c | **Update** scalar / enum slots — e.g. fixture `brightness`, shader `render_order`, playlist `default_fade`, fixture `color_order`. |
| C1d | **Update** map slots — e.g. `[bindings.*]`, playlist `[entries.*]`, `[glsl_opts.*]`. |
| C1e | **Delete** node: remove wiring ref, then remove def (or tombstone + commit). Asset orphans acceptable until cleanup op. |
| C1f | **Update** nested slot paths — e.g. `[entries.2.bindings.trigger]`, `[mapping]` kind switch (may enter error until follow-up ops complete). |

Node kinds to cover across tests (not every test needs all): `Project`, `Shader`,
`ComputeShader`, `Fixture`, `Clock`, `Output`, `Button`, `Radio`, `Playlist`,
`Texture`, `Fluid` (as registry/parser support allows).

#### C2 — Author inline node

| ID | Story |
|----|-------|
| C2a | **Add inline def** under a parent artifact path (e.g. playlist entry `node = { inline … }` or equivalent slot encoding). |
| C2b | **Edit inline def** slots without marking unrelated sibling defs changed (registry `NodeDefUpdates` isolation from M2). |
| C2c | **Remove inline def** and verify parent slot cleared. |

Inline authoring must produce distinct `NodeDefId`s with `{ artifact_id, path_in_artifact }` source paths.

#### C3 — Refactor inline node ↔ standalone node

| ID | Story |
|----|-------|
| C3a | **Extract** inline def → new standalone `.toml` file; parent slot becomes `{ path = "…" }`; inline content removed on commit. |
| C3b | **Inline** standalone def → embed under parent; standalone file deleted (or left orphan + explicit asset delete). |
| C3c | **Round-trip** extract → inline → extract; final committed state identical to start. |

Playlist entry `node` refs are the primary motivating case; inline under
`project.toml` is a secondary case when supported.

#### C4 — Refactor inline source ↔ asset file

| ID | Story |
|----|-------|
| C4a | **Asset → inline** — shader `source`: replace `{ path = "shader.glsl" }` with inline GLSL (`[source]` extension-key form from M3); `AssetChange::Delete` for file optional. |
| C4b | **Inline → asset** — extract GLSL/SVG to new file; slot becomes path ref; `AssetChange::Add`. |
| C4c | **Replace asset only** — `{ path }` unchanged, `AssetChange::Replace` on `shader.glsl`; def slot unchanged → M4-style artifact bump after commit. |
| C4d | **Fixture SVG** — same pattern for `mapping.source` / `SvgPath` (path ↔ inline SVG text). |

Materialization (M3 `SourceFileRef`) reads from **AssetView** so uncommitted
asset replaces are visible before commit.

### D — ChangeSet lifecycle

| ID | Story |
|----|-------|
| D1 | Apply overlay → effective view ≠ base; base unchanged on disk/store. |
| D2 | **Commit** → base updated, overlay cleared, `NodeDefUpdates` + artifact versions match expectation. |
| D3 | **Discard** → base unchanged. |
| D4 | Multiple ordered ChangeSets / op ids stable and replayable. |
| D5 | Active ChangeSet + **fs-change** on same path — precedence per Key Decisions. |

## Longer Term — ChangeSet as stress-test vocabulary

M5 user stories (especially **A** compose and **B** morph) are the seed of a
**project transition test system**. The same high-level assertion — e.g.
`empty → examples/basic` or `examples/basic → examples/fyeah-sign` — can be
written as one integration test while the machinery underneath emits a **long
ordered stream** of `ChangeOp`s (slot patches, asset adds/replaces, wiring
changes).

That decomposition is valuable beyond authoring:

- **Diff two projects → ChangeSet stream** — given base project *A* and target
  *B*, compute a minimal (or canonical) ordered op sequence that morphs *A* into
  *B*. User stories B* are hand-curated instances; automated diff generalizes
  them.
- **Replay at any granularity** — one test, one ChangeSet, or one op per
  message/tick. Exercises incremental registry, view, commit, and (post-M6)
  engine node lifecycle under realistic edit pressure.
- **Cross-target stress** — identical op log on host tests, `fw-emu`, and
  ESP32-C6 firmware. Targets heap peaks, fragmentation, and leak paths that
  `Project::reload()`-style tests hide because they allocate once and drop
  everything.

M5 scope is the **in-memory harness + story IDs**; project diff tooling and
full-engine replay on device are **future** (see `future.md`). M5 must keep op
types serializable and ordered so that log replay is straightforward later.

## Key Decisions

- **Change management is mandatory before engine cutover** — M6 blocked on M5.
- Client edits are **never** direct registry mutation; they go through ChangeSet
  → view → (optional) commit.
- v1 asset edits: **whole-file replacement** only.
- **Single ChangeSet stream** with `ChangeOp::Node(NodeChange)` /
  `ChangeOp::Asset(AssetChange)` variants — ordering across families matters.
- **SlotOp-style** patches for v1 (not CRDT); CRDT deferred to `future.md`.
- **Naming:** **ChangeSet** (not Overlay / Draft) for the authorable unit.
- **Fs vs overlay (v1):** uncommitted ChangeSet wins for reads on overlaid
  paths; fs bump marks artifact stale but does not clobber overlay until
  commit or discard; on commit, client ChangeSet wins over stale fs read.

## Deliverables

- `lpc-node-registry/src/change/` — ChangeSet types, apply, commit, discard.
- `NodeDefView` + `AssetView` integrated with ChangeSet.
- **User-story harness** under `lpc-node-registry/tests/` (or `tests/changeset/`)
  mapping to story IDs A*, B*, C*, D*.
- Design note: precedence rules, commit contract, story → M6 engine expectations.

## Dependencies

- M1, M2, M3, M4 complete.

## Execution Strategy

Full plan. The plan doc should:

1. Turn story IDs into concrete test modules (prioritize C* primitives, then A1,
   B1, then larger A/B).
2. Define minimal blank-project fixture shared by A* stories.
3. Specify how “matches example” is asserted (parsed def equality, slot snapshots,
   asset bytes).

Suggested chat opener:

> M5 plan: ChangeSet types + user-story harness (blank→basic, basic→basic2,
> CRUD / inline / refactor stories). I'll run the plan process then implement.
> Agree?
