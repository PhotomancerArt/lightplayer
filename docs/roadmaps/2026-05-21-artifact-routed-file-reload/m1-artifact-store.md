# Milestone 1: `lpc-node-registry` + ArtifactStore

## Title And Goal

Bootstrap **`lpc-node-registry`** and implement a **freshness-only
`ArtifactStore`**: file identity, monotonic version, and load error state — **no
parsed payloads, no cached file bytes**.

This milestone establishes the new crate and the bottom layer of the target
stack. Everything above (parsed defs, ChangeSets, engine) builds on it.

## Parallel Build

**M1 does not modify `lpc-engine`.** Production keeps the old
`lpc-engine/src/artifact/` path (NodeDef payloads, `InlineNode`, etc.) until
**M6**. New types live in `lpc-node-registry` with clear module boundaries.

## Relationship To M2 (split vs combined)

**Recommendation: keep M1 and M2 as separate milestones**, implement back-to-back
(often one plan folder with phase 1 / phase 2).

| | M1 (this milestone) | M2 |
|---|---------------------|-----|
| **Question answered** | "Which files exist, what version are they, did read fail?" | "What `NodeDef`s derive from those files?" |
| **Owns** | Crate bootstrap, `ArtifactStore`, `FsChange` → bump | `NodeDefRegistry`, `NodeDefUpdates`, parse hook |
| **Testable alone?** | Yes — acquire, bump, error state, no parser | Needs M1 store feeding artifact changes |
| **Gate** | `cargo test -p lpc-node-registry` artifact tests green | Registry update tests green |

Combining into one milestone is workable if you prefer a single PR, but the **M1
done gate** should still be: store complete and tested **before** registry work
starts — otherwise artifact semantics get buried in parse/diff noise.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m1-artifact-store/`

## Scope

### Crate bootstrap (first deliverable)

- **Delete `lp-core/lpc-slot-mockup`** — remove crate and all workspace /
  `default-members` / clippy exclusion references (not in CI `justfile`; defunct
  slot-pressure harness).
- **Create `lp-core/lpc-node-registry`**:
  - `#![no_std]` + `alloc`
  - Deps (initial): `lpc-model`, `lpfs`, `lpc-shared` (paths/revisions as needed)
  - Layout: `src/lib.rs`, `src/artifact/` (M1); stub modules or `mod` declarations
    for `registry/`, `source/`, `change/`, `view/` (filled in later milestones)
  - `cargo test -p lpc-node-registry` runs in host CI (`just check` path)

### ArtifactStore (freshness-only)

Track **file artifacts callers acquire** — node TOMLs, GLSL, SVG, etc. — as
**`ArtifactLocation::File(path)`** entries. v1 has **no `InlineNode`**
location type (inline defs are registry paths in M2, not separate artifact
locations).

Per entry:

- **`ArtifactId`** — opaque handle (same pattern as today; may reuse type from
  `lpc-model` or define in this crate).
- **Identity** — path + stable id even when load/read fails.
- **`revision`** — monotonic bump on invalidation (fs change,
  simulated bump in tests). No content hash/digest in v1. (Not `content_frame`.)
- **Requester ownership** — entries exist because a caller **acquired** them;
  **`release`** at refcount zero removes the entry. Fs changes do not register
  artifacts.
- **State** — `ArtifactReadState` (`Unread` / `ReadOk` / `Failed(ArtifactReadFailure)`);
  failures: `Deleted`, `NotFound`, `Io`, `InvalidPath`.

API surface (conceptual):

- `acquire_locator` / `acquire_location` → `ArtifactId` (always entry unless resolution fails)
- `release(id)`
- `revision(id) → Revision`
- `apply_fs_changes(&[FsChange], frame)` — bumps **held** entries only
- `read_error` via `read_state` on entry
- Optional **`read_bytes(id, fs) → Result<..., ArtifactError>`** for **transient**
  reads during tests / later registry parse — bytes not stored on the entry

### Tests (M1 gate)

Prove the store **without** `NodeDefRegistry` or TOML parsing:

- Acquire same path twice → same `ArtifactId`.
- Simulated `FsChange` on path → `revision` bumps.
- Unrelated path change → other entries unchanged.
- Read failure → entry stays registered, error state set, version semantics defined.
- No long-lived payload after read helper returns (if read helper exists in M1).

Out of scope:

- **`NodeDefRegistry`**, **`NodeDefUpdates`**, inline def paths (**M2**).
- TOML / `NodeDef` parse integration (**M2**).
- `SourceFileSlot` / `SourceFileRef` (**M3**).
- Reload harness, expected engine actions (**M4**).
- ChangeSet (**M5**).
- Any `lpc-engine` / `ProjectLoader` edits (**M6**).

## Key Decisions

- **M1 = new home + artifact freshness layer** — not "a refactor of engine
  ArtifactStore in place."
- **Metadata only** — path + version + error; lazy transient read at parse/prepare time.
- **Fs event = version bump** — no byte-level diff in v1.
- **All file types equal** — GLSL and SVG are artifacts from M1, not special cases
  added later.
- **Parallel crate only** — zero `lpc-engine` churn this milestone.

## Deliverables

- `lp-core/lpc-node-registry/` crate in workspace, CI-clean.
- `lpc-slot-mockup` removed.
- `src/artifact/` — store, id, location, state, fs-change bump.
- Unit tests satisfying M1 gate above.

## Dependencies

- None (first milestone).

## Execution Strategy

Full plan written — see `m1-artifact-store/` (`00-design.md`, phases `01`–`05`).

Dispatch: **`composer-2.5-fast`** default; phase 01 uses **`composer-2-fast`**
(bootstrap only). Implement via `/implement`; single commit at end.

Suggested chat opener:

> M1 plan is ready in `m1-artifact-store/`. Run `/implement` to dispatch phases
> 01–05. Agree?
