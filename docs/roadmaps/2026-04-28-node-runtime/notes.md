# node-runtime roadmap — notes

Working notes for the `node-runtime` roadmap: refactor `lp-core` in
place to absorb the new domain ideas (node tree, slots, artifact
manager, TOML), split the existing crates along a model/runtime
boundary, and fold `lp-domain`'s foundational types into the new
`lp-core` core. End-state of this roadmap: `lpc-model`, `lpc-runtime`,
`lpl-model`, `lpl-runtime` exist; legacy `Texture`/`Shader`/`Output`/
`Fixture` nodes run on the new spine; ESP32 / emulator / lp-cli all
green; a filetest harness hosts a legacy node end-to-end. lpfx and
lp-vis ship in subsequent roadmaps that consume what we build here.

This file follows the `/roadmap` process. See `decisions.md` (later)
for the distilled key calls; this file is the working log.

Pre-roadmap brainstorming and the user's first-pass answers live in
`docs/notes/2026-04-28-node-system2/00-notes.md` — kept in place as a
historical record.

## Resolved direction (post-pivot)

After question iteration, the strategic shape is:

- **Refactor `lp-core` in place** (Path R). Don't build a parallel
  spine and migrate later; that path under-served the
  client/server architecture, which is the most novel and
  load-bearing part of `lp-core` and not worth re-implementing
  from scratch. lp-engine was already designed to be runnable
  outside lp-server, so the filetest-as-consumer story works on
  Path R too.
- **Per-domain model/runtime split**, two crates per subsystem.
  Build hygiene + dep restriction + per-target feature gating
  justify crate-level boundaries on a codebase that targets
  ESP32 / browser / host / emulator.
- **`lpfx` is the rendering abstraction only.** The visual
  subsystem (Pattern / Effect / Stack / Transition / Live /
  Playlist) is a different concept and lives in a separate crate
  group (`lp-vis`, was tentatively `lp-show`). `lpfx` becomes
  trait + CPU impl + GPU impl (future); `lp-vis` consumes it.
- **`lp-domain` is dismantled in M2.** Foundational stuff (Slot,
  Kind, Constraint, ValueSpec, Binding, Presentation, paths,
  ids, Artifact / Migration traits) moves into `lpc-model` (M2
  C3). The visual-types-only remainder is renamed to
  `lp-vis/lpv-model/` in the same milestone (M2 C4). `lp-domain`
  is gone by the end of *this* roadmap. The next roadmap adds
  `lpv-runtime` alongside; `lpv-model` standalone in the
  meantime is fine, since `lpc-model` and `lpl-model` also exist
  standalone for parts of M2.
- **Crate naming convention:** two-letter subsystem prefix per
  domain (`lpc-` for `lp-core`, `lpl-` for `lp-legacy`, `lpv-`
  for `lp-vis`, `lpfx-` for `lpfx`). Already in use in this repo
  (`lp-shader/lps-*`); just generalising the established
  pattern.
- **`Uid(u32)`** stays as in `lp-domain`. `lp-model::NodeHandle(i32)`
  becomes a re-export / wrapper.
- **Slot is one type, four namespaces** — `params`, `inputs`,
  `outputs`, `state` — confirmed.
- **Lifecycle methods on the trait, status enum on the container**
  — confirmed (matches `lp-core` pattern).
- **Children come from two sources** — structural slots (Stack
  effects etc.) and param-promoted slots (`Kind::Gradient` filled
  with a Pattern artifact). One mechanism, two entry points.

## Scope of the effort

Build the runtime spine inside `lp-core` (refactor-in-place):

1. **Crate restructure.** Split `lp-core/lp-model` and
   `lp-core/lp-engine` into a 4-crate layout:
   - `lp-core/lpc-model` — generic identity / addressing / domain
     vocabulary (Uid, Name, NodePath, PropPath, Slot, Kind,
     Constraint, ValueSpec, Binding, Presentation, Artifact +
     Migration traits). Absorbs `lp-domain`'s foundational types.
   - `lp-core/lpc-runtime` — the spine: Node trait, NodeTree,
     ArtifactManager, lifecycle, status, frame-versioned change
     tracking, fs-watch routing, panic recovery, shed.
   - `lp-legacy/lpl-model` — current `lp-model::nodes/*` configs
     (Texture / Shader / Output / Fixture).
   - `lp-legacy/lpl-runtime` — current `lp-engine::nodes/*`
     impls (TextureRuntime / ShaderRuntime / OutputRuntime /
     FixtureRuntime), each implementing the new
     `lpc-runtime::Node` trait.
2. **New core concepts in `lpc-runtime`:**
   - `Node` trait (tree-aware, lifecycle, slot views, sidecar
     state).
   - `NodeTree` container (Uid → Node, NodePath ↔ Uid index,
     parent / child links, subtree teardown).
   - `ArtifactManager` trait + refcounted in-memory impl.
   - Frame-versioned `NodeStatus`, change events, sync API
     (lifted from `lp-engine::ProjectRuntime`).
   - Lifecycle hooks (init / render / destroy /
     shed_optional_buffers / update_config / handle_fs_change),
     adapted to the new trait signature.
3. **Legacy bridge.** `lpl-runtime` impls satisfy the new `Node`
   trait. The flat-map `ProjectRuntime` cuts over to be
   `NodeTree`-backed. ESP32 / emulator / lp-cli stay green.
4. **Filetest harness.** A small filetest can host one legacy
   node (likely `Shader`), tick it through its lifecycle, and
   feed `lp-perf`. This is the first non-`lp-server` consumer
   and the v0 demonstration that the spine is consumer-agnostic.

Out of scope (deferred to subsequent roadmaps):

- `lp-vis` model/runtime crates (Pattern / Effect / Stack instances).
- `lpfx` rename and split (`lpfx` + `lpfx-cpu` + `lpfx-gpu`).
- The bus / `BindingResolver` impl.
- Editor UI of any kind.
- `lp-rig` extraction (Fixture / Output split out from legacy into
  its own subsystem).

## Current state of the codebase

### What already exists in `lp-domain`

- `Uid(u32)`, `Name`, `NodePath`, `PropPath`, `NodePropSpec`,
  `ArtifactSpec`, `ChannelName` — identity + addressing.
- `Node` trait — minimal property-access only (`uid`, `path`,
  `get_property`, `set_property`); no tree, no lifecycle.
- `Slot { shape, label, description, bind, present }` and the full
  `Shape` / `Kind` / `Constraint` / `ValueSpec` Quantity model.
- Six Visual artifact types: `Pattern`, `Effect`, `Stack`,
  `Transition`, `Live`, `Playlist`, all with serde + `Artifact`.
- `Binding::Bus(ChannelName)` — pure routing.
- `VisualInput::Visual(...) | Bus(...)` — explicit "structural
  composition vs binding" split (see `visual_input.rs` doc).
- `ArtifactManager` does **not** exist; `artifact/load.rs` is a
  std-only one-artifact-at-a-time loader.

In this roadmap (M2), the foundational pieces (everything except
the visual artifact types) move to `lpc-model` (C3), and what
remains of `lp-domain` is renamed to `lp-vis/lpv-model/` (C4).
`lp-domain` no longer exists after M2.

### What already exists in `lp-core`

`lp-core/lp-engine/src/project/runtime.rs::ProjectRuntime` is what
this roadmap evolves from:

- Owns `BTreeMap<NodeHandle, NodeEntry>` — flat node map (no tree).
- `NodeEntry`: `path`, `kind`, `config`, `config_ver`, `status`,
  `status_ver`, `runtime: Option<Box<dyn NodeRuntime>>`, `state_ver`.
- `NodeStatus`: `Created | InitError | Ok | Warn | Error`.
- `NodeRuntime` trait (per-node behavior): `init`, `render`,
  `destroy`, `shed_optional_buffers`, `update_config`,
  `handle_fs_change`, `as_any` / `as_any_mut`.
- `NodeKind` enum: hardcoded `Texture | Shader | Output | Fixture`.
  Nodes live as `<name>.<kind>` directories with a `node.json`.
- Frame-versioned change tracking + `get_changes(since_frame, ...)`
  for client sync.
- Filesystem watcher integration: `handle_fs_changes(&[FsChange])`
  routes creates / modifies / deletes to the right runtime,
  including `node.json` reloads via `update_config` and
  `*.glsl` reloads via `handle_fs_change`.
- Lazy demand-driven render: `ensure_texture_rendered` traverses
  shader → texture targets in `render_order`.
- `shed_optional_buffers` is real and used: shaders shed before
  recompile to maximise free memory on ESP32.
- Memory stats hook (`MemoryStatsFn`) for embedded logging.
- Panic-recovery feature: `catch_node_panic` wraps `render()`.

This works. It runs on ESP32-C6, in the emulator, talks to lp-cli
over the client/server protocol. **What it lacks**:

- A node *tree* (parent / child ownership; subtree teardown).
- Domain-aware kinds (it predates `lp-domain`; only knows
  Texture / Shader / Output / Fixture).
- Slot grammar (configs are flat structs, not Slot trees).
- Visual artifact composition (no Pattern / Effect / Stack model).

### What `lpfx` has today

- `FxModule` / `FxManifest` / `FxInputDef` / `FxValue` parallel
  domain. To be deleted in the lpfx + lp-vis roadmap (next).
- `FxEngine` / `FxInstance` traits — shape is fine; will become
  the basis for the rendering-abstraction trait in the lpfx
  rename.
- `lpfx-cpu` — lpvm impl that survives into the lpfx rename.

### Filetests today

- `lp-shader/lps-filetests/` runs GLSL → LPVM compile + render,
  asserts pixel/snapshot results.
- Performance tools exist (`lp-perf` events, profilers) but cannot
  be exercised from filetests directly — you have to run a full
  project to get representative timing.
- This roadmap fixes that for legacy `Shader` nodes; lp-vis
  filetest support is the next roadmap's deliverable.

### Filesystem and protocol surface

- `lpfs` (`LpFs`) — sync, `no_std`-friendly fs trait. `LpFsStd`
  (host), `LpFsMem` (browser/test). `FsChange` carries
  create / modify / delete events.
- `lp-server` / `lp-cli` — client/server protocol that
  `ProjectRuntime::get_changes` services. The new spine has to
  preserve this contract or the embedded story breaks.

## End-state crate map (post-roadmap)

```
lp-core/                          # foundation; no domain knowledge
  lpc-model/                      # NEW (rename + absorb lp-domain foundation)
                                  # Uid, Name, NodePath, PropPath, NodePropSpec,
                                  # ArtifactSpec, ChannelName, Slot, Shape, Kind,
                                  # Constraint, ValueSpec, Binding, Presentation,
                                  # Artifact + Migration traits.
  lpc-runtime/                    # NEW (carved out of lp-engine)
                                  # Node trait (tree + lifecycle + slot views),
                                  # NodeTree, ArtifactManager,
                                  # NodeStatus + frame versioning,
                                  # change events, fs-watch routing,
                                  # panic recovery, shed,
                                  # client/server protocol surface.

lp-legacy/                        # NEW container (current legacy nodes)
  lpl-model/                      # NEW (= lp-model::nodes/*)
                                  # Texture / Shader / Output / Fixture configs.
  lpl-runtime/                    # NEW (= lp-engine::nodes/*)
                                  # TextureRuntime / ShaderRuntime / OutputRuntime
                                  # / FixtureRuntime, each impl lpc-runtime::Node.

lp-vis/                           # NEW container (visual subsystem)
  lpv-model/                      # RENAMED from lp-domain after foundation moves out.
                                  # Pattern, Effect, Stack, Transition, Live,
                                  # Playlist, VisualInput, EffectRef, ParamsTable.
                                  # Next roadmap adds lpv-runtime here.

# Hosts / clients (mostly just renaming if needed)
lp-server                         # consumes lpc-runtime + lpl-* impls
lp-client / lp-engine-client      # consumes lpc-runtime protocol; generic
lp-cli                            # consumes lp-client; unchanged in shape

lpfx/                             # UNCHANGED in this roadmap
                                  # Will rename to rendering abstraction in next roadmap.
```

Subsequent roadmaps split out `lp-vis/lpv-model + lpv-runtime` for
the visual subsystem (Pattern/Effect/Stack...), rename `lpfx` and
add `lpfx-gpu`, and eventually carve out `lp-rig/lpr-*` from
`lp-legacy`.

## Open questions

These remain after the strategic pivot. The big strategic
questions (Q1–Q3, Q6, Q7 from the previous draft) are resolved.

### Q-A: Bridge legacy nodes onto the new spine, or run them in parallel mode?

Two paths during the transition phase of the refactor:

- **Bridge (preferred).** `lpl-runtime` impls implement
  `lpc-runtime::Node` immediately. The flat-map `ProjectRuntime`
  cuts over to a `NodeTree`-backed engine in one motion. Cost:
  the new trait surface has to absorb everything legacy needs
  before legacy can run on it. Benefit: ESP32 validates the
  spine on real workloads.
- **Parallel-run.** Both runtimes coexist for some milestones;
  legacy stays on the old `ProjectRuntime` while the new spine
  matures. Eventually a switchover commit retires the old
  runtime. Cost: two runtimes for some weeks; risk of drift.
  Benefit: can ship intermediate milestones without spine being
  fully feature-complete.

**Suggested:** **Bridge.** The whole point of refactor-now is
that legacy validates the spine. Running parallel defeats the
argument that motivated Path R.

**Status:** to confirm.

### Q-B: Filetest harness target  *(RESOLVED — defer to next roadmap)*

The whole point of "filetest as a node consumer" is performance
and correctness comparison, **especially between the CPU and GPU
`lpfx` backends.** A filetest hosting a *legacy* `Shader` node
gets us most of the trait-shape signal but doesn't actually
exercise the thing we want to compare (CPU vs GPU). It would
also need rework once `lp-vis` lands.

**Resolved:** **drop filetest from this roadmap entirely.** It
moves to the `lpfx + lp-vis` follow-up roadmap, where
CPU-vs-GPU correctness/perf comparison is the actual point.
This roadmap's v0 acceptance signal is "ESP32 / emulator /
lp-cli still work; legacy nodes run on the new spine; conformance
tests prove no behavioural regression vs the old runtime."

### Q-C: Path segment derivation rules (carried)

How does a child node *derive its segment* from the slot it
fills? Today's grammar (`/<name>.<type>/...`) covers top-level
nodes. Need rules for:

| Slot kind | Proposed segment | Example |
|---|---|---|
| Param-promoted (named) | `<paramname>.<artifactkind>` | `/main.pattern/gradient.pattern` |
| Single structural slot | `<slotname>.<artifactkind>` | `/main.stack/input.pattern` |
| Indexed structural slot | `<slotname>_<i>.<artifactkind>` | `/main.stack/effects_0.effect` |

Sub-questions:

- Brackets (`effects[0]`) vs underscore (`effects_0`) for
  indexed segments. Brackets read better but require relaxing
  `Name`'s grammar.
- Slot index in the parent's vec (stable across reloads,
  unstable across reorders) vs artifact-derived name with
  collision suffix.

**Suggested:** underscore form for v0 (`effects_0.effect`).
Avoids grammar churn. Revisit if real users object.

**Status:** open; not blocking the early milestones.

### Q-D: Prior-art investigation timing  *(RESOLVED)*

**Resolved:** prior art is its own milestone (M1 of this
roadmap), running **in parallel** with the mechanical crate
restructure (M2). Research feeds the post-move design pass (M3);
the mechanical move (M2) doesn't depend on it.

Scope: clone Godot 4 and VCV Rack to `~/dev/reference/` (sibling
to this repo, untracked). Write `prior-art.md` focused on the
specific design surface we're building: lifecycle hooks
(`_ready` / `_process` / `_exit_tree`), node tree teardown
order, NodePath grammar, Resource refcount (Godot's `Ref<T>`),
scene instantiation, change/dirty tracking. **Not** a
comprehensive engine review — focused on the calls we have to
make.

### Q-F: Implementation order — class side first, then instance side  *(RESOLVED)*

When we start implementing the new spine concepts (post-move,
post-design), do `ArtifactManager` + slot views + TOML loader
**first**, then `Node` trait + `NodeTree` + lifecycle?

**Resolved:** yes, **artifacts (class) before nodes (instance)**.
Conceptually they're tightly intertwined, but if we have to
pick an order, building the "class" side first matches how a
node is born: load an artifact, then instantiate it. The two
milestones may bleed into each other (slot views are a shared
abstraction); we accept that and `/plan` each milestone with
the bleed acknowledged.

### Q-G: Post-move design refinement milestone  *(RESOLVED)*

Big refactors don't go exactly to plan. M2's mechanical move
will surface things that change the trait shape (a type
turned out to depend on something we didn't expect; a split
got drawn slightly differently). We need a checkpoint between
"moved everything" and "implementing for real" to reconcile.

**Resolved:** add an M3 "spine design pass" milestone, after the
mechanical move (M2) and prior-art (M1) both land. Output is a
`design.md` (or equivalent) capturing the final trait shape.
Subsequent implementation milestones (artifacts, nodes,
cutover) implement against that doc.

### Q-E: How aggressively does this roadmap touch `lp-domain`?

Two options for handling `lp-domain` in this roadmap:

- **Half-dismantle.** Foundation moves to `lpc-model`; visual
  types stay in `lp-domain` until the next roadmap carves out
  `lpv-model`. `lp-domain` exists in a shrunken form for the
  duration.
- **Full dismantle.** Both the foundation and the visual types
  move now. Visual types go to a freshly-created `lpv-model`
  with no `lpv-runtime` (the runtime crate doesn't exist yet
  because lpfx + lp-vis is the next roadmap's job). `lp-domain`
  dies completely here.

**Resolved (full dismantle).** Once the foundation moves out,
`lp-domain` *is* the visual model — calling it `lp-domain` is a
misleading name, and leaving it as the lone outlier violates the
per-domain `lp{x}-` prefix convention (D-2 / D-5). The "no
`lpv-runtime` companion yet" objection doesn't survive contact
with M2: `lpc-model` and `lpl-model` also exist standalone for
parts of M2, and there's no visual *runtime* code anywhere yet
to be torn between locations. M2 finishes the rename in C4
(after foundation extraction in C3).

## Notes (resolved decisions, captured here as we go)

- **Path R (refactor lp-core in place).** lp-engine is already
  designed to be runnable outside lp-server, so filetest works
  on Path R; the client/server architecture is the most
  load-bearing part of lp-core and not worth re-implementing.
- **Per-domain model/runtime split.** Two crates per subsystem.
- **`lpfx` becomes rendering abstraction only.** Visual
  subsystem moves out (to `lp-vis`, formerly tentatively
  `lp-show`).
- **`lp-domain` is fully dismantled in M2.** Foundation moves
  to `lpc-model` (C3); the visual-types-only remainder is
  renamed to `lp-vis/lpv-model/` (C4). Every workspace crate
  matches the `lp{x}-` prefix convention by end of M2.
- **Crate naming: two-letter subsystem prefix.** `lpc-`, `lpl-`,
  `lpv-`, `lpfx-`. Carbon-copies the established `lps-*` pattern
  under `lp-shader`.
- **`Uid` stays `u32`.** `lp-model::NodeHandle(i32)` becomes a
  re-export / wrapper.
- **Slot is one type, four namespaces:** `params`, `inputs`,
  `outputs`, `state`.
- **Lifecycle methods on the trait, status enum on the
  container.** Matches lp-core's pattern.
- **Children from two sources, one mechanism:** structural slots
  (Stack effects etc.) and param-promoted slots
  (`Kind::Gradient` filled with a Pattern artifact).
- **Filetest harness deferred** to the lpfx + lp-vis roadmap;
  this roadmap's v0 acceptance is "legacy still works, on the
  new spine."
- **Implementation order: artifacts (class) before nodes
  (instance)** within the spine milestones.
- **Post-move design refinement** is its own milestone (M3),
  reconciling the M1 prior art + the as-built M2 restructure.
- **Prior art is its own milestone (M1), parallel with the
  mechanical move (M2).**
- **M2 (crate restructure) is user-driven** in RustRover; agent
  assists with import fixes and Cargo.toml updates after each
  major move.
- **M2 has scoping checkpoints.** Sketched here for reference;
  details when M2 is actually planned:
  1. C1: split `lp-model` into `lpc-model` (generic /
     foundation) + `lpl-model` (legacy node configs).
  2. C2: split `lp-engine` into `lpc-runtime` (spine code:
     `ProjectRuntime`, change events, fs-watch, status,
     versioning) + `lpl-runtime` (legacy node runtimes).
  3. C3: move `lp-domain` foundation into `lpc-model`
     (visual types still live in `lp-domain` between C3
     and C4, importing foundation from `lpc-model`).
  4. C4: rename `lp-domain` → `lp-vis/lpv-model/` (the
     visual-types-only remainder).
  5. C5: workspace polish — naming consistency, Cargo.toml
     features and target gating, ESP32 + emulator + lp-cli
     verification.
  After each checkpoint, the user pings the agent for import
  cleanup + verification before moving to the next.
- **No "bridge" intermediate state.** M5 ports legacy nodes
  into the new `Node` shape *and* cuts `ProjectRuntime` over to
  the new tree-backed engine in one milestone — not a separate
  bridge step. Validating the shape is the *point* of porting
  legacy.
- **M3 is direct-execution, no `/plan`.** It's a design doc
  milestone separated from M4 specifically because `/plan`
  phases are about implementation details, not high-level
  shape decisions. We may run additional design passes later.
- **Roadmap ends with `summary.md`** capturing what shipped and
  pointing at the next roadmap (which will rework
  `docs/roadmaps/2026-04-23-lp-render-mvp/` for lpfx + lp-vis).

## M1 outcomes (prior-art investigation)

Survey covered five references (Godot 4, Bevy, VCV Rack, LX
Studio, Three.js) on 10 design surfaces. Single research pass
produced enough coverage; Pass 2 was not needed.

- Per-reference raw answers:
  `m1-prior-art/pass1/answers-{godot,bevy,vcv,lx,threejs}.md`
- Cross-comparison observations:
  `m1-prior-art/pass1/notes.md`
- Distilled judgement (what to copy / what to avoid, per
  surface), with citations: **`prior-art.md`** at the
  roadmap root.

### Headline findings carried forward to M3

Detail and citations in `prior-art.md`.

- **F-1 — Three Lightplayer features are novel** (no prior
  art across all 5 references): client / server architecture
  + frame-versioned wire sync, per-node panic-recovery
  isolation, unified `NodeStatus` enum on the container.
  M3 designs these from lp-engine's existing implementation,
  not from external prior art.
- **F-2 — Param-promoted-to-child has no prior art *and* is
  under-designed in the strawman.** Closest analog: Godot's
  internal-mode children. M3 must sketch this in `design.md`.
- **F-3 — `Handle<T>` + `Asset<T>` (Bevy) is directly
  portable** for `ArtifactRef<T>` + `Artifact<T>`, with one
  adaptation: drop semantics. Use Godot's `Ref<T>`
  synchronous decrement-and-evict instead of Bevy's
  channel-based drop.
- **F-4 — LX = closest *domain* analog; Godot = closest
  *engine* analog.** Use them in those roles.
- **F-5 — Tree composition + bus modulation is the validated
  model.** Don't introduce graph / DAG.
- **F-6 — Path grammar:** Godot's `NodePath` shape (`/`,
  `..`, `%Name`) plus *strict* sibling-name uniqueness.
- **F-7 — Schema versioning:** LX's `addLegacyParameter`
  pattern adapted to per-type
  `migrate(toml, from_version) -> toml` chained through
  versions.

### Specific design calls now resolved (move to decisions.md
after M3 ratifies)

- `Uid(u32)` stays flat (no generational indexing).
- Sibling-name uniqueness enforced at add-child time.
- Hot reload preserves handles, replaces content.
- LX `Placeholder` pattern for missing artifacts with
  full-JSON round-trip (verified in code).
- Bus-binding cycles: detect-and-error at bind-time.
- Per-frame hook is opt-in (not all-nodes-tick).

### Pass 2 was not needed

Three spot-checks during synthesis confirmed the answer
files: LX `Placeholder` does preserve full JSON; Bevy
`StrongHandle::drop` is channel-based (confirming the F-3
adaptation note); Godot `_propagate_ready` is bottom-up
(children before parent).

## Pre-M2 protocol unbake (completed)

Before the mechanical crate split, line-by-line analysis of
`message.rs` / `server/api.rs` / `project/api.rs` revealed
that the protocol envelope was already mostly generic — the
*only* legacy-aware tie point was
`ServerMsgBody::ProjectRequest::response: SerializableProjectResponse`.
A pre-M2 refactor pass parameterized on that response shape
and now the entire envelope (including `Message`,
`ClientRequest`, `ClientMsgBody`, `ServerMsgBody`,
`ProjectRequest`, `WireNodeSpecifier`, `NodeStatus`, etc.) is
slated for `lpc-model`; `lpl-model` only holds the
legacy-aware payload (`NodeDetail`, `NodeState`,
`SerializableNodeDetail`, `SerializableProjectResponse`,
`ProjectResponse`, `NodeChange`) plus the `LegacyMessage` /
`LegacyServerMessage` / `LegacyServerMsgBody` aliases.

Specific changes that landed:

- `Message<R>`, `ServerMessage<R>`, `ServerMsgBody<R>` are now
  generic. Call-sites use the legacy aliases for type
  positions; constructor and pattern uses go through the bare
  names where inference is simple.
- `pub enum NoDomain {}` (uninhabited) added in `message.rs`,
  re-exported from `lp-model/lib.rs`.
- All consumers (lp-server, lp-client, lp-cli, lp-shared,
  fw-core, fw-emu, fw-esp32, lp-engine-client) updated.
- One incidental fix: `lp-engine/src/project/mod.rs` was
  re-exporting from a deleted `runtime` module on this
  branch; redirected to `project_runtime`. Unrelated to the
  protocol unbake but needed for `cargo check -p lp-engine`.
  Can be split into a separate commit when packaging.
- `m2-crate-restructure/move-map.md` updated to reflect the
  cleaner post-unbake C1 split.

Verification (all green): `cargo check` on `lp-model`,
`lp-engine`, `lp-engine-client`, `lp-client`, `lp-server`,
`lp-cli`, `fw-emu` (RV32 release-emu), `fw-esp32` (RV32
release-esp32 with `esp32c6,server`). `cargo test -p lp-model`
passes (round-trip serialization).

## M2 outcomes (complete)

All five checkpoints landed. End-state crate map matches the post-roadmap
target in this file's earlier section:

- `lp-core/lpc-model/`, `lp-core/lpc-runtime/`,
- `lp-legacy/lpl-model/`, `lp-legacy/lpl-runtime/`,
- `lp-vis/lpv-model/`.

`lp-domain/`, `lp-core/lp-model/`, `lp-core/lp-engine/` no longer exist.
No transitional shells.

Verification (all green):

- `just check` (fmt + clippy host + clippy RV32 release-esp32 + clippy
  release-emu).
- `just test` (`cargo test` host + filetests; 15410/15410 filetest
  pass, all rust tests pass; ~2m25s).
- `cargo check -p fw-emu --target riscv32imac-... --profile release-emu`.
- `cargo check -p fw-esp32 --target riscv32imac-... --profile
  release-esp32 --features esp32c6,server`.

Commits (in order):

```
f9a49014 refactor(lp-vis): rename lp-domain to lpv-model and move to lp-vis/lpv-model
116f7f04 refactor(lpc-model/lpl-model): split lp-model into foundation + legacy crates
cf442ab0 refactor(lpc-model/lpv-model): move foundation types from lpv-model to lpc-model
da2f0a51 refactor(lpc-runtime/lpl-runtime): split lp-engine into spine + legacy runtimes
21cdc288 fix(style): address clippy lints and formatting from M2 C1-C4
0214948a fix(test): correct imports in lp-engine-client tests after C1 split
f6b73e29 docs(roadmap): update M2 progress in move-map and notes
```

### Flags carried into M3 (deviations from the move-map design intent)

The M2 split is mechanical and the crates compile, but three layering
invariants the move-map promised are not actually delivered. These are
**not bugs to fix before M3** — they're exactly the kind of thing M3's
"reconcile design with M2's reality" deliverable is for. Flagging them
here so M3 picks them up explicitly:

- **F-M2-1: `lpc-runtime` depends on `lpl-model`.** Per `cargo tree`,
  the dep is a regular runtime dep (not dev-only). The leaks:
  - `lpc-runtime/src/project/loader.rs` imports `lpl_model::{NodeConfig,
    NodeKind}` and hardcodes the four legacy suffixes (`texture`,
    `shader`, `output`, `fixture`) in `node_kind_from_path` /
    `is_node_directory`.
  - `lpc-runtime/src/project/hooks.rs::ProjectHooks::get_changes`
    returns `lpl_model::ProjectResponse`.
  Any future `lpc-runtime` consumer (e.g. an `lpv-runtime`) transitively
  inherits `lpl-model`. The "lpc is domain-agnostic spine" property is
  not actually delivered. M3 must decide between (a) accepting this and
  documenting it, or (b) abstracting it (probably a generic
  `ProjectRuntime<H>` + associated `Response` type, or trait-objected
  response).
- **F-M2-2: `ProjectHooks` is process-wide global state.** To break
  the otherwise-cyclic `lpc-runtime ↔ lpl-runtime` dep,
  `lpc-runtime/src/project/hooks.rs` introduces a singleton
  `static HOOKS: Mutex<Option<Arc<dyn ProjectHooks>>>`. Consumers must
  call `lpl_runtime::install()` before using `ProjectRuntime` or get a
  runtime error. This is the M5 cutover idea ("ProjectRuntime is
  generic; legacy nodes plug in") landing in M2 as a stopgap. It works
  (lp-server + lpc-runtime tests use it correctly) but it's a new
  piece of design surface that wasn't in the M2 plan. M3 decides
  whether this is the long-term shape or a transitional bridge.
- **F-M2-3: Hardcoded legacy kind list in `loader.rs`.** Even after M3
  decides on the artifact spine (M4), `node_kind_from_path` matches
  literal suffix strings. The artifact-spec story should subsume this:
  the class side answers "is this a node?" — the loader shouldn't
  enumerate four hardcoded suffixes.

### Note on the "108GB lp-cli" observation during M2 verification

During a backgrounded `just test` run mid-C5, Activity Monitor showed
an `lp-cli` process at 108GB resident memory. After fixing two
lingering import bugs from the C1 split (`ProjectResponse` and
`NodeState` paths in `lp-engine-client/tests/client_view.rs` and
`lp-fw/fw-tests/src/lib.rs`), `just test` is reproducibly green and
the spike does not recur. Most likely explanation is parallel-build
pressure (cargo + filetests racing on disk + multiple linker
instances) hitting macOS-reported memory accounting weirdly on a test
binary linked against `lp-cli`. **No reproducer** as of the M2 close;
flagged here to keep an eye on during M3 verification.

## M3 design discussion — Slot / Prop / Value / Binding

This is a working-log of the conversation iteration. The decisions
land in `design.md` once we're done converging; this section is the
log. All references to the design doc below point at sections still
to be rewritten — `design.md` §4 / §5 / §6 will be redone in a
follow-up commit once this lands.

### Where we landed (decisions, pending design.md write-up)

**Schema vs runtime values are distinct, but the schema layer
already exists.**

- `lpc_model::Slot` is the schema (recursive `Shape` ∈
  {`Scalar`, `Array`, `Struct`} + `Kind` + `Constraint` +
  `default: ValueSpec` + optional `bind` and `present` metadata).
  Lives on artifacts. Already TOML-mature; we keep it as-is.
- `StateField<T>` (current) is the runtime change-tracking
  primitive. Rename → **`Prop<T>`**. Same shape: `value: T,
  changed: FrameId`. Composite values continue to live inside
  `LpsValue` (no parallel value tree).
- Today's `*State` structs (`TextureState`, etc.) get renamed to
  `*Props` and stay as the **server-typed** per-node-type
  representation. They're a struct of `Prop<T>` per top-level
  field. A derived `PropAccess` impl yields the generic view.

**Granularity is asymmetric across server / wire / client:**

| Layer       | Representation                                          |
|-------------|---------------------------------------------------------|
| Server (typed) | `*Props` struct, `Prop<T>` per top-level field. FrameId tracking is **for delta computation only**. |
| Wire        | Per-node delta: `{ node_id, changed: [(PropPath, LpsValue)], last_frame }`. Only fields whose `Prop::changed >= since_frame` go on the wire. |
| Client mirror | `NodeView { values: HashMap<PropPath, LpsValue>, last_seen: FrameId }`. **No** per-field FrameIds — just latest value, one timestamp per node. |

The client doesn't need the rename; `Prop<T>` is internal to the
server. Generic-of-a-whole-node on the client is `NodeView`.

**Three "layers" was misleading.** It's really one slot with a
short *stack of override sources*:

```
artifact slot default                        ← always there
   ↓ overridden by
parent's per-instance override (in TOML)     ← optional
   ↓ overridden by
slot.bind (if active and the channel/source produces a value)
   ↓
resolved value @ this frame
```

**Resolution is pull-based, top-down search**, fall through to
`slot.default`:

```rust
fn resolve(path, frame) -> &LpsValue {
    cached if cache.changed >= frame, else:
    walk binding stack high-to-low; first to produce a value wins;
    if none, return slot.default.
}
```

A `Bus` binding to an empty channel returns `None` and we fall
through. Resolution result cached on `NodeEntry`, keyed by frame.
Cache invalidates on `config_ver` bump.

**Bindings:**

- For M5 scope: **two binding sources** —
  1. Artifact's `Slot.bind` (default for the slot kind), and
  2. Per-instance override authored in the parent's TOML at the
     use site.

  Modulator chains, grandparent inheritance, cross-tree
  patching: deferred to lp-vis or later.
- **There is no separate "value override" concept.** A `params.x =
  0.7` in the parent's TOML compiles to `Binding::Literal(0.7)`.
  One overrides map per node, keyed by `PropPath`.
- Authored truth lives on the **parent**'s `NodeEntry`
  (`child_overrides: HashMap<NodeId, HashMap<PropPath, Binding>>`).
  The child caches its effective bindings for fast resolution.
  The cache is regenerated on `config_ver` change.
- **Binding TOML uses `source`, not `target`.** Example:
  ```toml
  [params.time.bind]
  source = "/bus/time/0"
  ```
- Internal Rust enum (parsed once at config-load):
  ```rust
  pub enum Binding {
      Literal(LpsValue),
      Bus      { bus: BusSpec, channel: ChannelName },
      NodeProp { node: NodeSpec, prop: PropPath },
  }
  ```

**Bus naming and multi-bus.**

- Each bus gets its **own root in the path**:
  `/bus/<channel>` for local, `/group/<channel>` for the
  inter-Lightplayer sync bus, `/sync/<channel>` or
  `/flock/<channel>` if we end up wanting a 3rd (GlowFlock-style
  collaboration). No bus indices, no wildcards.
- Rust representation is a small **compile-time enum**:
  ```rust
  pub enum BusSpec { Local, Group /* , Sync, Flock, ... */ }
  ```
- For M5 we only implement `Local`. The rest exist in the type
  to keep the wire format and binding parser stable as more
  busses come on line.

**Namespace semantics sharpened:**

- `params` and `inputs` are **slots / bindable** — values come
  from outside the node (literal authored, bus, child output,
  another node's output).
- `outputs` and `state` are **produced by the node** — the node
  writes them; they're not bindable. Both are introspectable for
  the editor / sync layer; outputs feed downstream consumers,
  state is read-only debug.
- `params` vs `state` is sharpened to **authored vs computed**.
  (Today's `width / height / format` on Texture are conceptually
  `params`, not `state`. Whether to physically move them is a
  separate question.) Naming-wise: `params` are values the user
  *sets*; `state` are values the node *records*.
- `params` and `inputs` differ only in named-vs-indexed storage
  and default presentation; runtime treatment is symmetric.
  Keep them as separate namespaces for ergonomics, treat them
  symmetrically in resolution code.

### Open questions / flagged

**~~O-1 — Path grammar~~ — RESOLVED, retired.** A small spike
through what's already in `lp-core/lpc-model/src/binding.rs`
and `lp-vis/lpv-model/src/visual/visual_input.rs` showed the
decision had effectively already been made:

- `Binding` is an enum that serialises as a flat-key-mutex
  inline TOML table (`bind = { bus = "time" }`). Its rsdoc
  explicitly states future variants land additively as sibling
  keys (`bind = { constant = ... }`, etc.).
- `VisualInput` follows the same pattern (`#[serde(untagged)]`
  + per-variant `deny_unknown_fields`): `[input] visual = "..."`
  vs. `[input] bus = "..."`.
- `Kind::default_bind` already exists and returns
  `Some(Binding::Bus(...))` for time / texture / audio_level —
  so eliding `bind` in artifact TOML already picks up sensible
  defaults.

We **lock in object syntax** for `Binding`. Extension is
additive: add `Binding::Literal(LpsValue)` and
`Binding::NodeProp(NodePropRef)` as sibling variants. No
string-path "bus:time/0" / "node:../lfo:output" sigil
grammar — it bought less than it cost (literal-vs-path
ambiguity, weaker editor introspection, custom parser, fragile
multi-bus extension).

The only string-form path that survives is the `[bindings]`
cascade-table key (`"candidates/0#emitter_x"` in
`main.live.toml`), where TOML keys must be strings. That's a
combined `NodePath#PropPath` reference — and we lock in **`#`
as the `NodePropSpec` separator**, which the existing artifact
already uses.

What remains open out of "path grammar" is the `NodePath`
grammar itself (segment separator, indices, `..` / `.`,
`%name`), which is the long-running Q-C in this file and is
already partially designed in `design.md` §6. That work
survives unchanged.

**~~O-2 — `NodeProp` binding scope~~ — RESOLVED.**
**Outputs-only**, for now. Syntactically a future authoring
form could let an author target `state` / `params` / `inputs`,
but resolution rejects it. `state` is internal-debug, made
introspectable via `NodeProperties::get_property` but not
bindable: don't turn implementation detail into API. `params`
and `inputs` are sink-side on the target — if you want to
share a value, the producer mirrors it to an output (forcing
function for explicit contracts). May relax later if a real
use case arrives.

**~~O-3 — Inline child binding inside params~~ — RESOLVED.**
Three child kinds, three lifetime policies, one runtime
mechanism. See the dedicated subsection
"Child kinds and lazy lifecycle" below.

**O-4 — Whether to physically move existing `width / height /
format` from `*State` to `*Params`.** Sharpening the semantics
doesn't require a code move now — the legacy ports in M5 can
live with the historical placement and we re-classify on the
domain-model rewrite. Flag, defer.

**~~O-5 — Default values vs default bindings~~ — RESOLVED.**
**Both coexist; they have orthogonal roles.**

- `Slot.default` is **mandatory** for every value-bearing
  slot (already required for `Shape::Scalar`; we extend the
  rule to Array/Struct via the existing derived defaults). No
  `Option`-shaped values reach GLSL: a slot always has a
  literal value to fall back to. This is the "value when no
  signal is flowing" position.
- `Slot.bind` is **optional**. It's the artifact author's
  hint: *"the suggested signal source for this slot."*
  `Kind::default_bind` populates it for kinds like `Instant`
  (→ `Bus("time")`) and `AudioLevel`
  (→ `Bus("audio/in/0/level")`). Authors can override or
  clear it.
- **Per-instance binding overrides the artifact's `bind`
  entirely** — replace, not stack. When an author at the use
  site writes "bind to audio level," they mean it; silently
  falling through to the artifact's default `bind` if audio
  drops out would surprise. The literal `default` is the
  universal floor.
- Resolution rule (one sentence):
  > The active binding is whichever is highest priority and
  > produces a value (per-instance > artifact `bind`); if no
  > binding produces, return `Slot.default`.

**O-6 — `Binding` enum extension shape.** Concrete proposal,
pending design.md write-up:

```rust
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Binding {
    Bus(ChannelName),                 // bind = { bus = "time" }
    Literal(LpsValue),                // bind = { literal = 0.7 }
    NodeProp(NodePropRef),            // bind = { node = { node = "../lfo", prop = "output" } }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodePropRef {
    pub node: NodePath,
    pub prop: PropPath,
}
```

Multi-bus deferred (no rabbit hole). When it lands it will
either grow `Bus` to take a struct or add a `BusOn` variant —
either is purely additive and doesn't break artifacts in the
field.

### Child kinds and lazy lifecycle

Three child kinds, three lifetime policies, one runtime
mechanism. Resolves O-3.

| ChildKind | TOML form                                | Lifetime         | Use case |
|-----------|------------------------------------------|------------------|----------|
| `Input`   | `[input] visual = "..."`                 | parent's lifetime | structural composition (Stack effects) |
| `Sidecar` | `[children.<name>]`                      | parent's lifetime | shared LFOs, programmer model — one node, many bindings reference it |
| `Inline`  | `[params.<name>.bind] visual = "..."`    | slot binding's lifetime | author UX — drop a Visual on a slot in one operation |

```rust
pub enum ChildKind {
    Input    { source: WireSlotIndex },     // [input.N]
    Sidecar  { name: Name },          // [children.<name>]
    Inline   { source: PropPath },    // [params.<name>.bind] visual = ...
}
```

All three live as `NodeId`s in the parent's `children: Vec<NodeId>`,
are addressable by `NodePath`, and are bindable from anywhere via
`NodeProp { node, prop }`. The `ChildKind` only governs **when the
child is destroyed** and **what TOML form authored it**.

TOML key for `Sidecar` is `[children.<name>]` even though the
runtime kind is `Sidecar` — small split between authoring
vocabulary (where "children" reads naturally) and code vocabulary
(where "Sidecar" disambiguates from the broader fact that all
three kinds are children).

**Authoring asymmetry on `params.<name>.bind`:** the binding
table accepts a 4th key `visual = "..."` that is **loader-side
sugar only**. The runtime `Binding` enum stays at three variants
(`Bus`, `Literal`, `NodeProp`). On config-load, `bind = { visual =
"...", params = {...} }` desugars to (a) creating an `Inline`
child, (b) storing `Binding::NodeProp { node: <child path>, prop:
outputs[0] }` for that slot.

**Removing the last binding to a `Sidecar` does nothing** to the
sidecar's lifetime — sidecars are parent-owned, not
binding-owned. That matches the "I want a shared LFO" use case.

**Removing or changing an `Inline`'s authoring binding destroys
the child** — its lifetime is the slot binding's lifetime by
definition. If the binding switches from `visual = "..."` to
`literal = ...`, the child is destroyed; if the new binding is
also `visual = "..."` (different artifact), the old child is
destroyed and a new one created.

**Sharing across slots: only `Sidecar`.** `Inline` is 1:1 with
its slot by construction. Trying to bind a different slot to an
`Inline` child would imply two different lifecycle owners — flag
as authoring error.

### Lazy lifecycle (EntryState machine)

Children are **always-lazy** by default. Memory pressure on
ESP32 is the dominating constraint, and "user has 30 sidecars in
their library, uses 4 at a time" is a real authoring shape.
Eager instantiation would be a footgun.

```rust
pub enum EntryState {
    /// Artifact handle resolved + refcounted; node not instantiated.
    Pending,
    /// Node instantiated and ticking.
    Alive(Box<dyn Node>),
    /// Instantiation failed; resolution falls through to slot.default.
    Failed(ErrorReason),
}
```

**Init pass for a parent** does just enough to know the child is
real:

1. Resolve the `ArtifactSpec` via `ArtifactManager` — this
   validates the path, parses the artifact TOML, schema-checks
   it, increments the refcount. Cheap: no `Node::init()`, no
   shader compile.
2. Create a `NodeEntry` with `EntryState::Pending`, register it
   in the tree (it has a `NodeId`, `NodePath`, `parent`,
   `ChildKind`, the resolved artifact handle, and the
   per-instance config).
3. That's it. No `Box<dyn Node>` until the entry is woken.

**Two error tiers:**

- **Parse-time** (artifact-level, surfaces at parent-init).
  Path-not-found, TOML schema error, type error in the
  artifact. The entry never reaches `Pending`; the parent's
  init returns `Err` and the user sees the failure
  immediately.
- **Init-time** (node-level, surfaces lazily on first wake).
  Shader compile failure, OOM during resource allocation, etc.
  Entry transitions `Pending → Failed`. Resolution treats
  `Failed` like an empty channel — falls through to
  `Slot.default`. The `NodeEntry` records the reason; editor
  surfaces it. Optional retry on next `config_ver` bump or
  `MemoryPressure` event.

**Memory pressure interaction.** A natural response to a
`MemoryPressure` event is "evict the most-recently-unused
`Alive` sidecar back to `Pending`". Drop the box, keep the
entry. The next access re-instantiates. This is the killer
feature of always-lazy: it gives us a memory-pressure release
valve essentially for free.

### ArtifactManager state machine

The artifact side of laziness needs its own state model. An
artifact has multiple potential states between "spec referenced"
and "instances running":

```rust
pub enum ArtifactState {
    /// Path validated; refcount > 0; TOML not yet parsed.
    Resolved,
    /// TOML parsed and schema-validated; ready to spawn instances.
    Loaded,
    /// One-time prep done (e.g. shader compiled into a shared
    /// program). For artifacts without expensive prep, identical
    /// to Loaded.
    Prepared,
    /// Refcount = 0 but cached; eligible for eviction under
    /// MemoryPressure. Idle artifacts still answer queries.
    Idle,
    /// Path lookup or filesystem read failed.
    ResolutionError(ErrorReason),
    /// TOML parse / schema validation failed.
    LoadError(ErrorReason),
    /// One-time prep failed (e.g. shader didn't compile).
    PrepareError(ErrorReason),
}
```

State transitions:

- `<unknown>` → `Resolved` on first reference (path probe + ref).
- `Resolved` → `Loaded` lazily on first `Pending → Alive` of any
  instance (or eagerly during parent-init if we choose to
  pre-validate, see below).
- `Loaded` → `Prepared` on first instance wake that requires
  prep (shader compile etc.).
- `*` → `Idle` when refcount drops to 0; entry retained.
- `Idle` → eviction (drop entry entirely) under `MemoryPressure`
  or some LRU policy.
- `Idle` → `Loaded`/`Prepared` again on next reference (no
  re-parse needed unless evicted).
- Any → corresponding `*Error` state on failure; the error
  propagates to whatever called for the transition.

**Open: when does `Resolved → Loaded` happen?** Two options:

- **(a)** Eagerly at parent-init. Catches schema errors at the
  earliest possible moment. Cost: every referenced artifact
  parses, even if its instances never wake. Probably fine — TOML
  parse is cheap, and this is the model that gives us the "two
  error tiers" cleanly (parse errors at init, compile errors on
  wake).
- **(b)** Lazily on first instance wake. Saves the parse for
  artifacts that never get used. Errors only surface when an
  instance actually demands the artifact.

Going with **(a)** for now — simpler error model, parse cost is
real but bounded, and authors get fast feedback on TOML errors.
Revisit if profiling shows TOML parse dominating cold-start.

**Refcounting:** each `EntryState::Pending | Alive | Failed`
holds one ref on its artifact. `Failed` retains the ref because
a `MemoryPressure → retry` sequence may re-attempt instantiation
without re-parsing.

### Open questions / flagged

**O-7 — Wake trigger for `Pending → Alive`.** Three candidates:

- **Pull from binding resolution.** Cleanest semantically: when
  a slot's binding lands on `NodeProp { node: <pending child> }`,
  the resolver wakes it up. But this means instantiation
  happens inside `tick`, which is the hot path and may include
  shader compilation (= JIT, = real time). Risky.
- **Pre-tick warmup pass.** Walk the binding graph at the start
  of a frame; wake any reachable `Pending` nodes before tick
  starts. Keeps `tick` clean, but requires a separate traversal
  each frame (cheap if cached: only changed bindings produce new
  wake-ups).
- **Out-of-band wake.** The editor / sync layer asks for the
  child explicitly (e.g. when the user opens its detail view).
  Useful for sidecars that live in a library but aren't yet
  bound to anything.

Decision: **start with hot-path wake from binding resolution
(option 1)** — simplest, gets us shipping. We'll likely need to
refine to "schedule for waking between frames" or option (b)
warmup pass once we measure JIT cost in the hot path. Option (c)
explicit wake lands when editor flows need it. Flag for
iteration during M5 implementation.

**O-8 — Cycles in binding resolution.** `A` binds to `B.output`,
`B` binds to `A.output`. Forbidden in M5 (compile-time error
during binding resolution build-up). Future direction: **read
last frame's value** for backward edges in a detected cycle.
That handles feedback-style use cases (an input can bind to its
own output for one-frame delay) elegantly, at the cost of an
implicit per-cycle-edge buffer of last-frame values. Defer the
implementation; record the design intent.

### What changes in `design.md` once we lock these

- §4 (Slot views) gets rewritten. Drop the table headers'
  "named-vs-indexed-vs-bus-bindable" framing in favour of the
  sharpened "consumed/produced + authored/computed" framing.
- A new §4.5 or §5.5 on **Resolution** (pull-based, top-down,
  cached on `NodeEntry`, with the binding stack search
  explicitly described).
- §6 (paths) doesn't absorb a binding-path grammar — bindings
  are objects, not strings. §6 is just `NodePath` + `PropPath`
  + the `#` separator for combined `NodePropSpec` strings.
- A new §6.5 on the **`Binding` enum extension** captures the
  Bus / Literal / NodeProp variants (per O-6 above).
- §7 (lifecycle) gets rewritten to **three child kinds** —
  `Input` / `Sidecar` / `Inline` — with the authoring forms
  and lifetime policies in the table above. The existing
  "Structural / ParamPromoted" framing becomes "Input /
  Inline" with `Sidecar` as the new third kind.
- A new §7.5 on the **`EntryState` machine** (`Pending` /
  `Alive` / `Failed`) and the lazy-instantiation contract.
- §8 (sync layer) extended: client tree mirror reports
  `state: pending | alive | failed` per entry; only `Alive`
  entries push prop values.
- A new §10.5 on the **`ArtifactManager` state machine**
  (`Resolved` / `Loaded` / `Prepared` / `Idle` / errors), the
  refcount discipline, and the parse-eagerly / compile-lazily
  error model.
- §11 unchanged.

## M2 C4 done (out of order, via cargo-rename + agent)

Experiment: validate that an agent using `cargo rename` can
do a real LightPlayer crate-rename safely. Outcome:
**successful**, commit `f9a49014`. C4 executed first (out of
the original C1→C5 order) because it's a pure rename — the
ideal cargo-rename use case — with minimal external
consumers (one test file).

- `lp-domain` is renamed to `lpv-model` and lives at
  `lp-vis/lpv-model/`.
- The `lp-domain/` parent directory is deleted.
- Workspace `Cargo.toml` `members` and `default-members`
  updated automatically.
- Two trivial manual fixes: empty parent dir cleanup, stale
  doc-comment update in test file.
- Verified across host + both RV32 targets.

Implications for remaining checkpoints:

- **C3** wording updated in `m2-crate-restructure/move-map.md`
  to reflect that the source crate is now `lpv-model`, not
  `lp-domain`.
- **C1 / C2** (which involve splits, not just renames) are
  the next test of the workflow. The plan is: agent uses
  `cargo rename` for the rename portion (e.g.,
  `lp-model` → `lpc-model`), then does mechanical file
  moves for the split portion (extracting `lpl-model` from
  `lpc-model`), then sweeps imports.
- The cargo-rename + agent workflow is now the default
  approach for M2; manual RustRover work is no longer
  required.
