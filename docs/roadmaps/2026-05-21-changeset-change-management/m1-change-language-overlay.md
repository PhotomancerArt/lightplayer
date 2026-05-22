# Milestone 1: Change Language + Overlay Lifecycle

## Title And Goal

Introduce the v1 **change language** ([`change-language.md`](change-language.md)),
**`ChangeOverlay`** inside **`NodeDefRegistry`**, and **apply / discard**
lifecycle. Bootstrap with a **single `ArtifactChange`** (one op is enough to
start). Prove **D1** and **D3**.

## Parallel Build

This milestone touches **`lpc-node-registry` only**. **`lpc-engine` unchanged**
until parent artifact-routed **M6**.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-changeset-change-management/m1-change-language-overlay/`

## Scope

In scope:

- `change/` module — serde types:
  - `ChangeSet`, `ChangeSetId`
  - `ArtifactChange { target, ops }`
  - `ArtifactTarget` (`Id` | `Path`)
  - `ArtifactOp` enum shell (variants filled in M3/M4)
- `overlay/` — path-keyed pending state on `NodeDefRegistry`
- `apply(ArtifactChange)` — implicit create on `Path`; bootstrap op e.g.
  `SetBytes` shell that stores bytes in overlay
- `apply(ChangeSet)` — `for change in changes { apply(change) }` when batching
  needed
- `discard()` — clear overlay; committed `entries` and `ArtifactStore` unchanged
- Harness: overlay populated after apply; base unchanged; discard clears overlay

Out of scope:

- Effective projection / `NodeDefView` (**M2**)
- Full `SetBytes` / `Delete` semantics (**M3**)
- Slot op apply (**M4**)
- Commit (**M5**)
- `RegistryChange::ChangeSet` sync variant (stub OK; wired at M5)

## Key Decisions

- **Overlay inside registry** — between `ArtifactStore` and `entries`; not a
  separate ChangeRegistry.
- **Grouped by artifact** — `ChangeSet` is `Vec<ArtifactChange>`.
- **Implicit create** — `ArtifactTarget::Path(p)` get-or-creates overlay entry.
- **Single-change bootstrap** — full envelope optional until UI/diff need it.

## User Stories / Gate

| ID | Story | Covered |
|----|-------|---------|
| D1 | Apply → pending state visible in overlay | **Yes** |
| D3 | Discard → base unchanged | **Yes** |

## Deliverables

- `lpc-node-registry/src/change/`
- `lpc-node-registry/src/overlay/` (or `registry/overlay.rs`)
- `NodeDefRegistry::apply_change`, `discard_overlay` (names TBD in plan)
- Unit tests for D1, D3

## Dependencies

- Parent artifact-routed M1–M4 complete ([`dependencies.md`](dependencies.md))

## Execution Strategy

Full plan. Overlay ownership and apply pipeline before projection and op semantics.

Suggested chat opener:

> M1 plan: change types, ChangeOverlay in registry, apply/discard. Full plan then implement. Agree?
