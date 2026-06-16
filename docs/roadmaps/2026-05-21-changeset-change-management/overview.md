# ChangeSet Change Management

## Motivation

Client edits today mutate node defs in place (`slot_mutation`) with no commit
model, no overlay, and no shared vocabulary with filesystem reload. That blocks
incremental hot reload (artifact-routed file reload) from covering the full edit
loop the UI needs.

This roadmap proves **client-driven change management** in `lpc-node-registry`:
ordered, id'd **ChangeSets** that express authorable edits in memory until
**commit**. All future client edits should flow through this model (overlay →
view → optional commit).

ChangeSets also become the **universal edit vocabulary**: the same ordered op
stream powers the UI, **project diff**, and incremental stress replay on
host, RV32 emulator, and device.

## Relationship to Artifact-Routed File Reload

This roadmap was **promoted from M5** of
[`2026-05-21-artifact-routed-file-reload`](../2026-05-21-artifact-routed-file-reload/overview.md)
for process clarity. It is a **prerequisite** for that roadmap's **M6 engine
cutover** — not a fork.

```text
Artifact-routed reload (parent)          ChangeSet (this roadmap)
─────────────────────────────────        ─────────────────────────
M1 ArtifactStore                    ──┐
M2 NodeDefRegistry                  ──┼── prerequisites (complete)
M3 SourceFileSlot                   ──┤
M4 fs-change sync → SyncResult      ──┘
                                      │
M6 engine cutover  ◄── gated on ──────┘  M1–M6 here green
M7–M10 server / graph / probes / cleanup
```

**Parallel build rule unchanged:** this roadmap does **not** modify
`lpc-engine` until parent M6.

## Architecture

```text
NodeDefRegistry (owns committed + pending)
  store: ArtifactStore           — committed bytes + freshness
  overlay: ChangeOverlay         — pending artifact mutations (path-keyed)
  entries, indexes               — committed parse cache; re-derived on commit/sync

  Internal reads: overlay ∪ store → effective artifact bytes / slot trees
  Public reads (NodeDefView):     effective only — always base ∪ overlay
  entries + SyncResult:           committed truth after commit/sync

ChangeSet (wire / UI / diff)
  ChangeSet { id, changes: Vec<ArtifactChange> }
  apply → overlay; discard → clear overlay; commit → flush → SyncResult

Engine (parent M6 — minimal change)
  consumed slot: bindings → effective registry def read → value
  provenance: not on tick path; parent M10 ExplainSlot probe when client asks
```

## Change language

Full spec: [`change-language.md`](change-language.md).

Summary:

```text
ChangeSet → Vec<ArtifactChange>     // grouped by artifact

ArtifactChange {
  target: Id(ArtifactId) | Path(LpPathBuf),   // Path → implicit create if missing
  ops: Vec<ArtifactOp>,
}

ArtifactOp:
  file:  Delete | SetBytes              // assets; TOML import escape hatch only
  slot:  SetSlot | MapInsert | …       // node defs are slots; wiring included
```

**Asset** (user term) = non-node file (GLSL, SVG). **Artifact** = store path
identity (any file, including `.toml`).

## Alternatives Considered

- **Defer ChangeSet until after engine cutover** — rejected.
- **Flat op stream with per-op artifact ref** — rejected; group by artifact.
- **Explicit Create / New artifact target** — rejected; implicit create on `Path`.
- **CreateDef / pre-populated def blobs** — rejected; slot ops + defaults.
- **Ops as slot-system types** — rejected; serde change vocabulary in `change/`.
- **CRDT / concurrent merge in v1** — deferred to `future.md`.

## Risks

- Slot op apply touches `lpc-model` mut paths — phased in M4.
- C3 inline ↔ standalone refactor — defer past diff gate.
- ESP32 heap — overlays must not retain duplicate file bytes long-term.
- Diff engine complexity — M6 gate; hand-written stories not required once diff green.

## Scope Estimate

Seven milestones. Parent **M6** starts only when **M6 (diff + equivalence
gate)** here is green.

## Milestones

| # | Milestone | Gate |
|---|-----------|------|
| M1 | [Change language + overlay](m1-change-language-overlay.md) | Types, `ChangeOverlay` in registry, apply/discard, D1/D3 |
| M2 | [Effective projection](m2-effective-projection.md) | `NodeDefView` effective reads; overlay ∪ base |
| M3 | [File ops + asset reads](m3-asset-overlay.md) | `SetBytes`/`Delete`, materialize from overlay; C4* |
| M4 | [Slot ops + serialize](m4-node-slot-patches.md) | Slot ops, TOML serialize path; C1*, C2* |
| M5 | [Commit promotion](m5-commit-promotion.md) | Commit → base + `SyncResult`; D2, D5 |
| M6 | [Diff + equivalence gate](m6-diff-equivalence-gate.md) | `diff(∅, basic)`, `diff(basic, basic2)`; **parent M6 gate** |
| M7 | [Cleanup + validation](m7-cleanup-validation.md) | CI, docs, parent cross-links |

## User Story Index

Full story matrix: [`notes.md`](notes.md).

**M6 gate (minimum):** D1–D3, D5, C1 slot ops + C4c, **A1** via diff, **B1** via
diff. Hand-written story tests optional once diff covers compose/morph.
