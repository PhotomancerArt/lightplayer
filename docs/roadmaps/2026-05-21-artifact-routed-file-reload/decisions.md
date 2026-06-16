# Artifact-Routed File Reload — Decisions

#### Parallel build in lpc-node-registry until M6

- **Decision:** M1–**M4** build the new system in `lpc-node-registry` alongside
  the existing `lpc-engine` path. **ChangeSet** is a
  [promoted roadmap](../2026-05-21-changeset-change-management/overview.md).
  **No `lpc-engine` edits until M6.**
- **Why:** Prove fs-change and projection semantics in isolation; keep the app
  working on the old stack during development.
- **Rejected alternatives:** In-place refactor of `lpc-engine` from M1; dual
  models in one loader before harness gate.
- **Revisit when:** M6 cutover (mandatory after M4 + ChangeSet M6 gate).

#### ChangeSet before engine cutover

- **Decision:** **ChangeSet** change management is proven in the promoted roadmap
  [`2026-05-21-changeset-change-management`](../2026-05-21-changeset-change-management/overview.md)
  before **M6** engine cutover here. Client edits are ordered, id'd, in-memory
  until **commit**; express node def slot patches and asset add/replace/delete.
- **Why:** Client-driven edits are as critical as fs-reload; cutover without
  this shape repeats overlay work.
- **Rejected alternatives:** Minimal projection only (old M4.1); defer ChangeSet
  until after M6; keep as nested M5 plan only.
- **Revisit when:** Wire protocol and CRDT merge (ChangeSet roadmap `future.md`).

#### ChangeSet as test and diff vocabulary

- **Decision:** ChangeSet ops are the **canonical edit vocabulary** for client
  UI, future **project diff**, and **incremental stress replay** — not a
  separate ad-hoc mutation path for tests.
- **Why:** One representation supports compose/morph user stories, wire
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
  variants; see [ChangeSet roadmap](../2026-05-21-changeset-change-management/decisions.md).

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

- **Decision:** Registry **`sync`** returns `NodeDefUpdates`
  `{ added, changed, removed }`; driver applies **`apply_fs_changes`** to
  `ArtifactStore` first; engine applies lifecycle from report.
- **Why:** Bounded, testable unit between fs events and node tree mutation.
  Driver owns fs + store; registry owns parse + diff.
- **Rejected alternatives:** Engine re-parses directly; whole `Project::reload()`;
  registry calling `apply_fs_changes` internally.

#### Registry bootstrap via load_root

- **Decision:** **`load_root(absolute_path)`** is the single public bootstrap
  entry. Root may be any node-definition TOML kind; `project.toml` is convention.
  Path-backed child registration is private (walk recursion).
- **Why:** Matches engine/test driver model: init once, then fs loop →
  `NodeDefUpdates`. M5 ChangeSet commit uses same `sync` output shape.
- **Rejected alternatives:** Public `register_file` per artifact; requiring
  `NodeDef::Project` at root.

#### Prove semantics before cutover (M4 + ChangeSet gate)

- **Decision:** No production cutover until **M4** (fs-change) here and
  **ChangeSet roadmap M6** (diff + equivalence gate) pass.
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
  **ChangeSet** projection). Proven in [ChangeSet roadmap](../2026-05-21-changeset-change-management/overview.md) before M6.
- **Why:** All client edits flow through ChangeSet; commit promotes to base.
- **Rejected alternatives:** Direct registry mutation from wire; overlay only in
  UI mockup branch.

#### Inline def change does not imply parent changed

- **Decision:** `NodeDefUpdates` reports def-level deltas; inline child edit
  marks child `changed`, not parent, unless parent payload changed.
- **Why:** Avoid unnecessary parent/node refresh on nested edits.
- **Rejected alternatives:** Mark entire artifact subtree changed on any edit.

#### Kind change requires node delete/recreate

- **Decision:** When a bound def's **`NodeKind`** changes, the engine **deletes
  and recreates** the runtime node — no in-place slot refresh. Registry still
  reports the `NodeDefId` in **`changed`**; shell stubs at invocation sites
  include kind so parent containers also **`changed`** when an inline child's
  kind flips.
- **Why:** Node kind determines runtime type, wiring, and lifecycle; treating
  kind change as a content patch would leave stale node state.
- **Rejected alternatives:** In-place refresh on kind change; separate
  `removed`+`added` ids for kind flips in M2.
- **Revisit when:** Stable `NodeDefId` preservation across kind morph (future).

#### project.toml graph reconciliation deferred (M8)

- **Decision:** Leaf file + source file reload first; topology/wiring changes
  are a separate milestone after server wire-up.
- **Why:** Graph mutation is distinct from def payload updates; large scope.
- **Rejected alternatives:** Full graph diff in first reload slice.

#### Explicit Project::reload retained

- **Decision:** User-initiated full reload keeps drop-and-rebuild; fs watcher
  uses incremental path only.
- **Why:** Escape hatch for unsupported edits and debugging.
- **Revisit when:** Unlikely.

#### Slot provenance via ExplainSlot probe (M10)

- **Decision:** Provenance is **on-demand**, not on tick/registry read paths.
  Client attaches `ExplainSlot` probe to `project_read` (wire types in
  `lpc-wire`; engine stub today) or re-derives locally on host when it holds
  bindings + ChangeSet.
- **Why:** ESP32 memory; values-only hot path; M3.5 resolver trace seeds
  explain output.
- **Rejected alternatives:** Provenance on every `Production`; registry
  `explain_slot()` in v1; dedicated `lpa-server` provenance logic.
- **Revisit when:** Thin remote clients without local edit context.
- **Tracked:** [M10](m10-slot-provenance-client.md).
