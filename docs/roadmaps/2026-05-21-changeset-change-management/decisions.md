# ChangeSet Change Management — Decisions

#### Promoted standalone roadmap

- **Decision:** ChangeSet work lives in
  `docs/roadmaps/2026-05-21-changeset-change-management/`.
- **Why:** Scope warrants full roadmap process; gates parent M6.
- **Revisit when:** Unlikely.

#### Parent M6 gate unchanged

- **Decision:** Parent **M6** starts when this roadmap **M6 (diff + equivalence
  gate)** + parent **M4** are green.
- **Why:** Both fs-reload and client-edit paths must be proven before cutover.

#### Change language: grouped by artifact

- **Decision:** `ChangeSet` is `Vec<ArtifactChange>`. Each block has
  `target + ops` for one file.
- **Why:** Natural authoring unit; avoids repeating artifact ref on every op.
- **Rejected alternatives:** Flat `ChangeOp` stream with per-op target.

#### Artifact target: Id or Path; implicit create

- **Decision:** `ArtifactTarget::Path(p)` get-or-creates overlay entry if absent.
  `ArtifactTarget::Id` for committed artifacts only.
- **Why:** Compose-from-blank never needs ids; no explicit Create op.
- **Rejected alternatives:** Explicit `New { path }`; require reference before create.

#### Node edits are slot ops only

- **Decision:** Node defs are slots. No `CreateDef`. Author via slot ops at
  `SlotPath` within the target artifact (`root()` or inline paths like
  `entries[2].node`). Wiring = slot ops on invocation fields.
- **Why:** Matches `lpc-model` and registry `DefSource { artifact, path }`.
- **Rejected alternatives:** Separate invocation ops; pre-populated def on create.

#### Change ops are not slot-system types

- **Decision:** `ArtifactOp` / `ChangeSet` types live in `lpc-node-registry/change/`,
  serde-serialized. Not part of `SlotData` or slot codec.
- **Why:** Edit vocabulary ≠ payload model; wire/diff/replay friendly.
- **Rejected alternatives:** Embedding ops in slot shapes.

#### File ops vs slot ops

- **Decision:** Per artifact: `Delete`, `SetBytes` (whole-file — assets + TOML
  import escape hatch); slot ops for normal `.toml` authoring.
- **Why:** TOML node files serialize from slot tree on commit.

#### Defaults + follow-up slot ops

- **Decision:** New def at a locus = set kind (slot op) → `KindDef::default()`,
  then patch slots. No bundled initial TOML as primary path.
- **Why:** Smaller op set; matches compose/morph stories.

#### Creatability requirement

- **Decision:** Op vocabulary must reach any `examples/*` from blank via finite
  `ChangeSet` (implicit create + slot ops + `SetBytes`).
- **Why:** Universal edit vocabulary for UI, diff, replay.

#### Overlay vs base refcount

- **Decision:** Overlay is path-keyed scratch space; no base-store refcount.
  Commit writes overlay paths; registry registers defs reachable from root.
- **Why:** Orphans on disk OK; dangling refs mid-sequence OK.

#### No lpc-engine edits until parent M6

- **Decision:** M1–M6 here touch `lpc-node-registry` + tests only.

#### Client edits never mutate base directly

- **Decision:** ChangeSet → view → (optional) commit → `registry.sync`.

#### v1 whole-file asset edits only

- **Decision:** `SetBytes` / `Delete` for text assets; no byte-range patches.

#### Fs vs overlay precedence (v1)

- **Decision:** Uncommitted overlay wins on overlaid paths until commit/discard.
- **Revisit when:** Remote fs sync.

#### M6 diff gate scope

- **Decision:** Gate parent M6 on **A1 + B1 + D1–D3 + D5**, core slot + file ops —
  via **diff + equivalence** (`diff(∅, basic)`, `diff(basic, basic2)`), not
  hand-curated op lists. Full A2–A4 / B2–B4 / C3 matrix deferred.

#### DefView is sole read path

- **Decision:** Effective reads go through view/overlay resolution before M6
  engine cutover. Public reads are **effective only**; `entries` is committed
  cache updated on commit/sync.

#### No provenance on registry read path

- **Decision:** Registry and engine hot reads return **values only** — no
  per-field provenance, no `Production` attribution on tick path.
- **Why:** ESP32 memory; client holds ChangeSet and bindings for UI badges.
- **Provenance later:** Parent roadmap **M10** — `ExplainSlot` probe on
  `project_read` (wire exists) or host client local re-derive. See
  [`m10-slot-provenance-client.md`](../2026-05-21-artifact-routed-file-reload/m10-slot-provenance-client.md).

#### Overlay inside NodeDefRegistry

- **Decision:** `ChangeOverlay` lives inside `NodeDefRegistry`, between
  `ArtifactStore` and parsed `entries`. Internal artifact reads go through
  overlay; commit promotes to store and re-derives entries → `SyncResult`.
- **Why:** Mutations are artifact-level; registry entries are derived state.
- **Rejected alternatives:** Separate ChangeRegistry; overlay above committed
  entries only.

#### Engine cutover: minimal def-read swap

- **Decision:** Parent M6 changes engine consumed-slot def fallback to read
  effective registry defs. Binding cascade unchanged.
- **Why:** Same resolution shape; overlay invisible to resolver except via
  effective def reads.
