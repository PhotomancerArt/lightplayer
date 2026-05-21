# Artifact-Routed File Reload — Decisions

#### Parallel build in lpc-node-registry until M6

- **Decision:** M1–**M5** build the new system in `lpc-node-registry` alongside
  the existing `lpc-engine` path. **No `lpc-engine` edits until M6.**
- **Why:** Prove fs-change and projection semantics in isolation; keep the app
  working on the old stack during development.
- **Rejected alternatives:** In-place refactor of `lpc-engine` from M1; dual
  models in one loader before harness gate.
- **Revisit when:** M6 cutover (mandatory after M4 + **M5**).

#### ChangeSet before engine cutover (M5)

- **Decision:** **M5** proves **ChangeSet** change management in
  `lpc-node-registry` harness before **M6** engine cutover. Client edits are
  ordered, id'd, in-memory until **commit**; express node def slot patches and
  **asset** add/replace/delete (whole-file v1).
- **Why:** Client-driven edits are as critical as fs-reload; cutover without
  this shape repeats overlay work.
- **Rejected alternatives:** Minimal projection only (old M4.1); defer ChangeSet
  until after M6.
- **Revisit when:** Wire protocol and CRDT merge (future).

#### ChangeSet as test and diff vocabulary

- **Decision:** ChangeSet ops are the **canonical edit vocabulary** for client
  UI, future **project diff**, and **incremental stress replay** — not a
  separate ad-hoc mutation path for tests.
- **Why:** One representation supports compose/morph user stories (M5), wire
  messages, and high-level tests decomposed into long op streams for host/emu/device.
- **Rejected alternatives:** Filesystem-only test setup; whole-`reload()` per
  scenario on embedded targets.
- **Revisit when:** Project diff tooling (`future.md`); post-M6 replay harness.

#### Asset vs artifact naming

- **Decision:** **Asset** = non-node dependency file (GLSL, SVG, …).
  **Artifact** = store identity / freshness entry (any file path). ChangeSet
  carries **asset** ops; commit bumps **artifact** store + registry.
- **Why:** Clear vocabulary for node TOMLs vs dependency files.
- **Resolved:** Single ChangeSet stream with `NodeChange` / `AssetChange`
  variants; SlotOp-style patches — see `m5-changeset-change-management.md`.

#### lpc-node-registry crate; retire lpc-slot-mockup

- **Decision:** New **`lpc-node-registry`** crate (`no_std` + alloc); delete
  **`lpc-slot-mockup`** at M1 start.
- **Why:** Clean namespace; fast isolated tests; avoid lpvm-heavy `lpc-engine`
  churn before semantics are proven.
- **Rejected alternatives:** Build directly in `lpc-engine`; keep mockup.
- **Revisit when:** Unlikely — crate boundary matches domain.

#### ArtifactStore vs NodeDefRegistry split

- **Decision:** `ArtifactStore` tracks source freshness only; `NodeDefRegistry`
  owns parsed `NodeDef` entries keyed by `NodeDefId`.
- **Why:** Artifacts are sources; defs are derived. Conflating them blocked
  file-only reload (GLSL/SVG) and overlay projection.
- **Rejected alternatives:** Generic artifact payload enum; incremental facade
  over current store.
- **Revisit when:** Never for this boundary — extend with `BinaryFileSlot` etc.

#### NodeDefUpdates drives reload

- **Decision:** Registry `update_from_artifacts` returns `NodeDefUpdates`
  `{ added, changed, removed }`; engine applies lifecycle from report.
- **Why:** Bounded, testable unit between fs events and node tree mutation.
- **Rejected alternatives:** Engine re-parses directly; whole `Project::reload()`.

#### Prove semantics before cutover (M4 + M5 gate)

- **Decision:** No production cutover until **M4** (fs-change) and **M5**
  (ChangeSet) harness tests pass.
- **Why:** Both reload and client edit paths must be proven in parallel stack.
- **Rejected alternatives:** Cutover after M4 only.

#### SourceFileSlot + SourceFileRef (not node SourceRef)

- **Decision:** Authored `SourceFileSlot` with `$path` / extension-key inline
  TOML; resolved `SourceFileRef` in slot data; materialize via context.
- **Why:** File vs inline is a slot concern; nodes read resolved values like
  other slots; no big data in slot values.
- **Rejected alternatives:** `ShaderSource` enum; runtime `SourceRef` on nodes.
- **Revisit when:** `BinaryFileSlot` for byte payloads.

#### Error propagation, no last-good (v1)

- **Decision:** Parse/load/validation errors put defs/artifacts in error state;
  destroy dependent nodes; cascade parent errors.
- **Why:** Simpler semantics and tests; avoids stale runtime after bad edits.
- **Rejected alternatives:** Last-good def + last-good compiled shader on failure.
- **Revisit when:** Live editing UX demands retaining last-good visuals.

#### DefView is the sole read path

- **Decision:** Nodes read through **`NodeDefView`** (base registry + active
  **ChangeSet** projection). Proven in **M5** before M6.
- **Why:** All client edits flow through ChangeSet; commit promotes to base.
- **Rejected alternatives:** Direct registry mutation from wire; overlay only in
  UI mockup branch.

#### Inline def change does not imply parent changed

- **Decision:** `NodeDefUpdates` reports def-level deltas; inline child edit
  marks child `changed`, not parent, unless parent payload changed.
- **Why:** Avoid unnecessary parent/node refresh on nested edits.
- **Rejected alternatives:** Mark entire artifact subtree changed on any edit.

#### project.toml graph reconciliation deferred (M8)

- **Decision:** Leaf file + source file reload first; topology/wiring changes
  are a separate milestone after server wire-up.
- **Why:** Graph mutation is distinct from def payload updates; large scope.
- **Rejected alternatives:** Full graph diff in first reload slice.

#### Explicit Project::reload retained

- **Decision:** User-initiated full reload keeps drop-and-rebuild; fs watcher
  uses incremental path only.
- **Why:** Escape hatch for unsupported edits and debugging.
