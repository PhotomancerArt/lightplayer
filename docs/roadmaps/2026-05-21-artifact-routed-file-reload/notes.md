# Artifact-Routed File Reload — Roadmap Notes

## Scope

Deliver incremental file reload for a running LightPlayer project: changed files update only the artifacts and nodes they affect, without `Project::reload()` or reconstructing the whole `Engine`.

**Immediate goal:** single-file reload routed through the artifact layer.

**Design constraint:** structure the artifact / definition / runtime stack so
**ChangeSets** (M5) sit between registry base and effective reads — proven in
harness before engine cutover (M6).

Target stack (bottom → top):

```
Filesystem / library sources
        ↓
ArtifactStore            — source identity + freshness (path, version); no long-lived file bytes
        ↓
NodeDefRegistry            — parsed NodeDef storage (file-backed + inline), keyed by NodeDefId
        ↓
ChangeSet layer (M5)       — ordered id'd client edits; in-memory until commit
        ↓
NodeDefView + AssetView    — effective reads (base + active ChangeSets)
        ↓
Engine node tree           — live NodeRuntime instances + dependency index
```

In scope for this roadmap:

- Replace server file-change path that calls `Project::reload()`.
- Split **ArtifactStore** (sources) from **NodeDefRegistry** (parsed definitions).
- Register node-to-artifact dependencies during project load.
- Track GLSL, SVG, and node TOML as file-backed artifacts (metadata only for source files).
- Let shader and fixture nodes react to changed dependent artifacts.
- Handle node TOML changes by reloading/repreparing affected nodes in place.
- Error propagation on reload/parse failure (no last-good retention in v1).
- ESP32 heap discipline: no cached source text, no duplicate engines, staged compile/prepare.

Out of scope (this roadmap):

- Full **project diff → ChangeSet** automation (see `future.md`; M5 stories are manual seed).
- Full optimal graph diff for arbitrary `project.toml` edits in the first slice.
- Library artifact locators.
- Host precompilation or any weakening of on-device GLSL JIT.
- Byte-level artifact diffing / digest in the first pass.

## Current State

### Server reload path (wrong boundary)

- `lp-app/lpa-server/src/server.rs` — `LpServer::tick` collects project-relative `FsChange`s and calls `project.reload()` for any batch.
- `lp-app/lpa-server/src/project.rs` — `Project::reload()` drops `Engine` and runs `ProjectLoader::load_from_root` again.
- Useful for explicit full reload; must not run on filesystem watcher events.

### Engine stub

- `lp-core/lpc-engine/src/engine/engine.rs` — `Engine::handle_fs_changes` is a no-op.
- Engine owns `ArtifactStore`, `artifact_nodes` (`HashMap<String, NodeId>`), `demand_roots`, node tree.
- `TickContext` exposes only the **owning node-definition artifact** (`artifact_ref`, `artifact_content_frame`, `artifact_changed_since`) — not dependent source artifacts (GLSL, SVG).

### ArtifactStore today = de facto NodeDef registry

There is no separate `NodeRegistry` / `NodeDefRegistry` type. Parsed definitions live inside `ArtifactStore`:

| Piece | Location | Role today |
|-------|----------|------------|
| `ArtifactStore` | `artifact/artifact_store.rs` | Maps `ArtifactLocation` → refcounted entry; `load_with` loads **`NodeDef` only** |
| `ArtifactState` | `artifact/artifact_state.rs` | Payload is always `NodeDef` when loaded |
| `ArtifactLocation` | `artifact/artifact_location.rs` | `File(path)` or `InlineNode { owner, name }` |
| `NodeDefHandle` | `node/node_def_handle.rs` | `(ArtifactId, SlotPath)` — points into artifact store, not a registry index |
| `NodeEntry` | `node/node_entry.rs` | `def_handle` + runtime; `artifact()` → owning artifact id |

Historical context: milestone `m2.7-node-def-handandles-and-slot-views/02-concrete-artifact-store` intentionally made `ArtifactStore` own `NodeDef` payloads directly. That was correct for the slot-domain cutover; it is now the wrong boundary for file reload + overlays.

Smells:

- `ArtifactLocation::InlineNode` — inline defs are derived from an owning artifact, not separately loadable sources.
- `ArtifactStore::load_with` — closure returns `NodeDef`; no generic source-file artifact type.
- Load failure **replaces** payload with error state (no last-good + error split).
- No content version / freshness API beyond `content_frame` bump on successful load.

### ProjectLoader bypasses artifacts for dependent sources

`lp-core/lpc-engine/src/engine/project_loader.rs`:

- Node TOML → `acquire_location(File)` → `load_with` → `NodeDef` in artifact store ✓
- `ShaderSource::Path` → `read_shader_source` → `String` passed to `ShaderNode::new` ✗ (not tracked)
- `MappingConfig::SvgPath` → `resolve_fixture_mapping` at attach ✗ (not tracked)

After load, GLSL/SVG paths are invisible to artifact invalidation.

### Nodes hold inlined source / resolved mapping

- `shader_node.rs` — long-lived `glsl_source: String`; compile failure drops `self.shader` (no last-good compile).
- `fixture_node.rs` — `SvgPath` resolved at load; runtime sync ignores path changes.

### Closest existing “incremental update” pattern

- `slot_mutation.rs` — mutates `NodeDef` in place inside `ArtifactStore`, bumps `content_frame`; nodes observe via `TickContext::artifact_changed_since`. Wire-driven, not filesystem-driven.

### Overlay prototype (other branch, not ported)

`lightplayer-app-ui/lp-core/lpc-slot-mockup` explored:

- `ArtifactStoreModel` — base authored slot roots keyed by path
- `OverlayStore` — UI change sets (`SlotOp`, create/delete root)
- `OverlayProjector` — merges enabled overlays → `ProjectedSlotRoots`
- `commit_overlay` — promote overlay into base store

That mockup predates current engine structure. Concept (project base + overlay → effective defs) is still the target for UI edits, but implementation will likely be rewritten to fit `NodeDefRegistry` + engine revision model.

## User Notes

- File reloads should only reload what they need; routed through artifact manager.
- Server/file-watcher must not special-case GLSL or SVG — they are file artifacts.
- Artifacts are sources; `NodeDef` is derived, not the artifact itself.
- `InlineNode` artifact locations are probably wrong.
- Acceptable initially: reload child nodes when a node file changes; avoid whole-project reload.
- No transactional whole-engine reload (two engines in memory) on ESP32 (~300 KB heap).
- Source-file artifacts: metadata only (path + version); lazy read during prepare/compile.
- Preserve on-device GLSL JIT.

## Resolved Decisions (design discussion)

### NodeDefRegistry — core reload orchestrator

The registry (user: "NodeRegistry"; holds **defs**, not live runtimes) is the bounded, testable center of reload logic.

**Entry shape** — similar to current artifact entry:

- Every `NodeDefId` maps to an entry with **source** `{ artifact_id, path }` where `path` is location within the artifact (`SlotPath::root()` / empty path = artifact root).
- **Identity separate from content:** if we know a def should exist, create the entry even when parse/validation fails (`ParseError`, `ValidationError`, etc.).
- Parsed payload lives in entry state when healthy; error state when not.

**Update protocol:**

```
Engine --fs changes to store--> store.apply_fs_changes
Engine --sync--> NodeDefRegistry::sync(&ArtifactStore, …)
NodeDefRegistry --report--> NodeDefUpdates { added, changed, removed, ... }
Engine --applies report--> attach/detach/destroy nodes, propagate parent errors
```

- Registry knows which defs came from which artifacts; on artifact change it re-derives affected defs.
- Report is **def-level**: `added` / `changed` / `removed` (name: **`NodeDefUpdates`** or similar).
- **Def content change ≠ child inventory change:** editing an inline child def does not necessarily mark the parent def `changed` — only the child def entry.
- Distinguish (conceptually, may surface in report or engine follow-up):
  - **Def payload changed** — same identity, new parsed content or error state.
  - **Child defs added/removed** — artifact now derives a different set of inline `NodeDefId`s (e.g. playlist entries).
  - **Graph wiring changed** — parent's invocation map / child references changed (engine tree mutation; own milestone).
- **v1 simplification:** when a node binds multiple defs, may refresh all referenced defs — acceptable short-term, not ideal long-term.
- **Later:** stable `NodeDefId` preservation when edits are unambiguous.

Registry is unit-testable in isolation: feed artifact change → assert `NodeDefUpdates`. Engine response tested separately.

**Build order:** stand up new `ArtifactStore` + `NodeDefRegistry` and nail `NodeDefUpdates` semantics **before** cutting over loader/engine and fs reload. Graph/`project.toml` reconciliation is big enough for **its own milestone** after foundation.

**Def access:** expose defs only through a **view** (projection/effective def), not raw registry entries — overlay/change layer may live inside or adjacent to registry; update report may eventually reflect projected defs. Exact change-layer integration TBD.

### Error semantics (no last-good in v1)

Propagate errors; do **not** retain last-good parsed defs or last-good compiled runtimes on failure.

- Parse/validation error on a def → entry enters error state → **nodes bound to that def are destroyed**.
- Parent nodes referencing the def → reference fails → parent enters error state (cascade).
- Same identity/content split for artifacts: artifact entry exists with error state when load/parse of source fails.

Last-good retention is explicit future work if needed.

### Node-facing source model

**Authored:** `SourceFileSlot` on defs (see TOML encoding below).

**Resolved:** slot values carry a **`SourceFileRef`** handle (inline / file artifact / future URL), **not** file contents — same principle as resources/products (no big data in slot values).

- Node/fixture runtime holds parsed **products** (compiled shader, mapping points), not source text.
- To compile/prepare: ask context to materialize from `SourceFileRef` → `{ version, text, diagnostic_name }`.
- Fixture: state holds resolved `PathPoints`; SVG text materialized only during mapping prepare.

Node tracks `last_seen_version` per source slot; recompile/reprepare when resolved version bumps.

Deprecate / replace: `ShaderSource`, standalone `SourcePathSlot` for this use case.

**Slot family (naming agreed):**

- **`SourceFileSlot`** — authored UTF-8 file-or-inline.
- **`BinaryFileSlot`** — future sibling; inline base64.
- **`SourceFileRef`** — resolved handle in slot data; materialize via context.

### Change layer placement (refined)

Between registry base defs and engine node tree. May be **inside** `NodeDefRegistry` with the update report / def view reflecting projection — exact wiring TBD. Registry def **view** is the hook either way.

### Hard cut, not incremental facade

App is in active dev — do the full ArtifactStore / NodeDefRegistry split now. No temporary facade over the old model. `NodeDefHandle` → **`NodeDefId`** (opaque id, same pattern as `ArtifactId`).

### SourceFileSlot / BinaryFileSlot TOML encoding (agreed direction)

File-or-inline slots use a **custom slot codec** (existing `SlotDataAccess::Custom` path in `slot_codec/`). Acceptable because file sources are fundamental.

**File reference** — `$path` leaves `path` free in the namespace for a future `.path` artifact type:

```toml
source = { $path = "./shader.glsl" }
# shorthand (also valid):
source = "./shader.glsl"
```

**Inline text** — extension key names the inline format (GLSL vs WGSL vs SVG, etc.):

```toml
[source]
glsl = """
vec4 render(vec2 pos) { ... }
"""
```

Exactly one of `$path` or an extension key must be present. Inline table form and inline-table `$path` form are both valid for the same slot.

**`BinaryFileSlot`** (future) — same `$path` for file; inline uses extension key + **base64** payload (`png = "..."`, `jpeg = "..."`). Useful for small embedded assets.

**Authored model (conceptual):**

```rust
enum SourceFileBacking {
    Path(SourcePath),           // from $path
    Inline { ext: String, text: String },  // ext from table key: glsl, svg, wgsl, ...
}
```

Engine resolves to UTF-8 text + effective version regardless of backing. Extension may inform compile/prepare path (GLSL vs WGSL) without nodes caring about file vs inline.

**Resolved source includes a diagnostic label** for compile/load errors and logging — exact format flexible, e.g.:

- File-backed: project-relative path as authored (`mapping1.svg`, `./shader.glsl`)
- Inline: synthetic label anchored to def location (`[shader.toml:56].glsl`, `[shader.toml:source].svg`)

Engine resolves `SourceFileRef` → UTF-8 text + effective version on materialize (not stored in slot value). Extension may inform compile/prepare path.

**Resolved materialization includes a diagnostic label** for compile/load errors and logging — exact format flexible, e.g.:

- File-backed: project-relative path as authored (`mapping1.svg`, `./shader.glsl`)
- Inline: synthetic label anchored to def location (`[shader.toml:56].glsl`, `[shader.toml:source].svg`)

**Migration:** Hard cut — replace `ShaderSource` `{ path = ... }` / `[source] glsl = ...` with `$path` / extension-key forms.

Registry/engine responsibilities (not node runtime):
- Register file artifacts for file-mode slots at load.
- Resolved slot values carry `SourceFileRef`; combine slot revision + file artifact version → effective version on materialize.

### SourceRef sketch (superseded)

Runtime `SourceRef` on nodes — superseded by **`SourceFileRef`** in resolved slot data + context materialization.

### Naming

| Today | Target |
|-------|--------|
| `NodeDefHandle` `(ArtifactId, SlotPath)` | `NodeDefId` opaque registry index |
| `ArtifactStore` holds `NodeDef` | `ArtifactStore` holds source freshness only |
| `NodeEntry.def_handle` | `NodeEntry.def_id: NodeDefId` |
| De facto registry in `ArtifactStore` | `NodeDefRegistry` + `NodeDefUpdates` report |

### Execution order (agreed direction)

**Prove semantics with tests before cutover.** The new stores exist to support fs-change (and later projections); switching loader/engine over before that shape is validated has little value.

Phases:

1. **Parallel build (M1–M5, no `lpc-engine` changes)** — registry + fs harness (M4)
   + **ChangeSet** (M5).
2. **Cutover (M6)** — delete old path; engine → `lpc-node-registry`.
3. **Wire-up (M7)** — server fs-change.
4. **Graph reconciliation (M8)**.
5. **Cleanup (M9).**

## Change management (M5)

See `m5-changeset-change-management.md`. **ChangeSet**: ordered, id'd, in-memory
until commit. User stories drive harness acceptance:

- **Compose** — blank project → any `examples/*` project via ChangeSets.
- **Morph** — any example → any other, one edit at a time, never crashing.
- **Actions** — CRUD defs/slots, inline node authoring, inline↔standalone def,
  inline↔asset source refactor.

**Asset** = non-node file; **artifact** = store identity.

Longer term: **project diff → ChangeSet stream** and **replay stress harness**
(host / emu / device) — see `future.md`. M5 story IDs are the manual seed;
automated diff and full-engine replay follow M6.

## Open Questions

- **Q1:** `project.toml` graph reconciliation details — M8.

(Q8 NodeChange vs AssetChange layering — **resolved**: single ChangeSet stream,
`NodeChange` / `AssetChange` variants; see `m5-changeset-change-management.md`.)

## Roadmap Artifacts

`overview.md`, `m1`–`m9`, `decisions.md`, `future.md`.

## Build Location

- **`lpc-node-registry`** — **M1** crate bootstrap + `ArtifactStore`; **M2–M5**
  fill registry, source, change, view. No `lpc-engine` edits until M6.
- **Delete `lpc-slot-mockup`** at M1 start.
- **`lpc-model`** — `SourceFileSlot` additive in M3; production `ShaderDef` at **M6**.
- **`lpc-engine`** — **M6** cutover only.
