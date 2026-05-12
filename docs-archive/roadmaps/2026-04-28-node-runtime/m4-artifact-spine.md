# Milestone 4: Artifact spine — manager + slot views + TOML loader

## Goal

Implement the **class** half of the new spine in `lpc-engine`
(plus supporting types in `lpc-model`):

- `ArtifactManager` — load / cache / refcount / shed.
- Slot view wrappers — one `Slot` type expressed across the
  four namespaces (params / inputs / outputs / state).
- TOML artifact loader — generalised across legacy +
  domain-aware artifacts; supersedes the `std`-only one-shot
  loader that lived in `lp-domain` (now in `lpv-model`
  post-M2).

`Node` trait + `NodeTree` + lifecycle + `ProjectRuntime`
cutover land in M5. M4 does **not** touch the running
runtime; the artifact pieces stand on their own and have
unit-level tests.

This is the **class side first** order from M3:
loading / caching / shedding artifacts is well-defined without
a node tree existing. Nodes need an `ArtifactManager` to live
on top of, so this comes first.

## Suggested plan location

`docs/roadmaps/2026-04-28-node-runtime/m4-artifact-spine/`

Full plan: `plan.md` plus per-phase notes / checkpoints as
the plan defines.

## Scope

**In scope:**

- `ArtifactManager` trait + default impl in `lpc-engine`:
  - `load(spec) -> ArtifactRef` with refcount semantics.
  - `release(spec)` with shed-on-zero behaviour.
  - Cache shape (probably `BTreeMap<ArtifactSpec,
    ArtifactEntry>` to mirror the existing `BTreeMap<Uid,
    NodeEntry>` pattern; M3 fixes the exact choice).
  - Error model for missing / malformed / version-mismatched
    artifacts.
  - `no_std + alloc` compatible.
- Generic `Artifact` value (parsed TOML body + metadata).
  May be `Box<dyn Artifact>` or an enum; M3 design pass
  decides.
- TOML artifact loader (generalised):
  - Reads bytes (host fs or embedded fs, abstracted).
  - Parses + validates against the type's schema.
  - Calls `Migration::migrate` if version-skewed.
  - Lives behind a `no_std`-friendly interface; the existing
    `std`-only loader (originally in `lp-domain`, in
    `lpv-model` post-M2) is replaced.
- `SlotsView` wrappers (or however M3 names them) — one
  `Slot` model viewed through four namespaces:
  - `Params(&Slots)`, `Inputs(&Slots)`, `Outputs(&Slots)`,
    `State(&Slots)`.
  - Iteration, indexed lookup, named lookup as appropriate
    per namespace.
  - Read-only at this layer; write-paths land with the
    `Node` trait in M5.
- Tests:
  - Round-trip load → instantiate (placeholder, since
    `NodeTree` arrives in M5; instantiate stub returns an
    `ArtifactRef`).
  - Refcount: load same spec twice → cached; release →
    decrement; release-to-zero → drop.
  - Shed: budget-driven shed of zero-refcount artifacts.
  - Malformed TOML → typed error.
  - Migration: version-skewed TOML triggers migrate path.
  - All four artifact namespaces (params / inputs / outputs /
    state) are addressable through their respective views.
- Existing `lpv-model` callers continue to load visual
  artifacts (`Pattern`, `Effect`, `Stack`, ...) — they now
  go through `ArtifactManager` instead of the one-shot
  loader.

**Out of scope:**

- `Node` trait + `NodeTree` (M5).
- `ProjectRuntime` cutover (M5).
- Legacy node port (M5).
- Sync layer changes (M5).
- Refining the visual model (`lpv-model`) — the M2 rename
  carries types as-is; semantic refinements are next
  roadmap.
- Performance tuning beyond meeting baseline.
- Editor / `lp-cli` UX changes; only existing surfaces stay
  working.

## Key decisions

- **Class before instance.** Artifacts (load / cache /
  refcount) are well-defined without nodes existing. Node
  trait + tree + lifecycle ride on top in M5.
- **No cutover in M4.** `ProjectRuntime` keeps using the old
  shape; `lpl-runtime` keeps building. The new artifact
  spine is reachable via tests + new code, not via the
  running engine.
- **`ArtifactManager` is the public surface.** All artifact
  loads go through it, including the existing `lpv-model`
  visual loaders; the `std`-only one-shot loader retires.
- **TOML loader is `no_std`-compatible from day one.** Per
  the embedded-JIT rules: don't gate the loader behind
  `std`; if a dep doesn't support `no_std`, fix the dep.
- **Error model is typed and explicit.** Loaders return
  structured errors (artifact-spec, file-line, message)
  rather than `anyhow::Error`. The runtime needs to map
  errors to `NodeStatus::Error` deterministically.
- **Slot views are read-only here.** Writing through a slot
  view (binding a param, mutating state) is a `Node` trait
  concern in M5.

## Deliverables

- `lpc-engine::ArtifactManager` + impl.
- `lpc-engine::SlotsView` (and per-namespace wrappers).
- Generalised TOML loader (replaces the `std`-only one-shot
  loader that came from `lp-domain` / now `lpv-model`).
- Test suite covering load / refcount / shed / migrate /
  malformed.
- All existing `lpv-model` visual TOML examples load through
  the new path.
- Workspace gates green: `just check`, `just build-ci`,
  `just test`, ESP32 + emu release builds.

## Dependencies

- M3 (spine design pass) — implements `design.md`'s
  `ArtifactManager` and slot-view shape.
- Blocks: M5 (node spine + cutover) — `Node` trait + tree
  ride on top of `ArtifactManager`; tree instantiation calls
  it.

## Execution strategy

**Option C — full plan (`/plan`).**

Justification: M4 is real implementation with multiple
non-trivial decisions surfaced at impl time (refcount
semantics in detail, TOML loader's `no_std` story, error
model, namespace view ergonomics). The plan-iteration loop
catches them earlier than direct execution would. The plan
also gates how M4 hands off to M5 — `Node` trait expects a
specific `ArtifactManager` shape.

> I suggest we use the `/plan` process for this milestone, after
> which I will automatically implement. Agree?
