# ChangeSet Change Management — Notes

## Scope

Prove client-driven edits in `lpc-node-registry` harness: ChangeSet apply →
effective view → commit/discard → `SyncResult`. No `lpc-engine` changes until
parent artifact-routed reload **M6**.

## Prerequisites (parent roadmap)

Complete before starting M1 here:

| Parent | Deliverable |
|--------|-------------|
| M1 | `ArtifactStore` — freshness-only |
| M2 | `NodeDefRegistry`, `NodeDefId`, `DefSource` |
| M3 | `SourceFileSlot`, `SourceFileRef`, materialize |
| M4 | `registry.sync` → `SyncResult`, fs-change scenarios S1–S6 |

See [`dependencies.md`](dependencies.md).

## Current Codebase State

```
lp-core/lpc-node-registry/src/
├── artifact/          # M1 — done
├── registry/          # M2 + M4 — done; RegistryChange::Fs only
├── source/            # M3 — done
├── change/mod.rs      # stub — implement in ChangeSet M1
└── view/node_def_view.rs  # base passthrough; effective overlay in M2
```

M4 left **`RegistryChange::Fs`** only; commit from this roadmap extends the
registry sync path. See [`change-language.md`](change-language.md).

## Change language (summary)

See [`change-language.md`](change-language.md) for the full spec.

- `ChangeSet` → `Vec<ArtifactChange>` (grouped by artifact)
- `ArtifactTarget`: `Id` | `Path` — **path implies implicit create**
- `ArtifactOp`: file (`Delete`, `SetBytes`) + slot (`SetSlot`, map ops, …)
- Node defs authored only via slot ops; no `CreateDef`

## Global Invariants (all stories)

- **No panic / no corrupt base** until commit.
- **Intermediate uselessness OK** — mid-morph views may have parse errors,
  dangling refs, missing bindings.
- **Commit contract** — all-or-nothing promotion or explicit error.
- **Discard** — reads restore to base exactly.

## User Story Matrix

Reference projects: `examples/basic`, `basic2`, `events`, `button-playlist`,
`fyeah-sign`, `fluid`, …

### A — Compose from blank

| ID | Story | M6 gate? |
|----|-------|----------|
| A1 | Blank → `basic` | **Yes** (via diff) |
| A2 | Blank → `events` | Later |
| A3 | Blank → `button-playlist` | Later |
| A4 | Blank → `fyeah-sign` | Later |

### B — Morph between examples

| ID | Story | M6 gate? |
|----|-------|----------|
| B1 | `basic` → `basic2` | **Yes** (via diff) |
| B2 | `basic` → `button` | Spot-check |
| B3 | `events` → `basic` | Spot-check |
| B4 | Cross-family morph | Aspirational |

### C — Atomic author operations

| ID | Area | M6 gate? |
|----|------|----------|
| C1a–f | Slot + file ops on artifacts | c/d minimum (via diff) |
| C2a–c | Inline via slot ops at nested paths | b/c in M4 |
| C3a–c | Inline ↔ standalone refactor | **Defer** ([`future.md`](future.md)) |
| C4a–d | Source ↔ asset file | c minimum (`SetBytes`); a/b/d in M3 |

### D — Lifecycle

| ID | Story | Milestone |
|----|-------|-----------|
| D1 | Apply → effective view ≠ committed base | M1–M2 |
| D2 | Commit → base updated | M5 |
| D3 | Discard → base unchanged | M1 |
| D4 | Multi-ChangeSet replay | [`future.md`](future.md) |
| D5 | ChangeSet + fs-change precedence | M5 |

## Resolved design (2026-05-21)

Change language locked — see [`change-language.md`](change-language.md) and
[`decisions.md`](decisions.md).

## Open Questions

### C3 inline ↔ standalone timing

- **Context:** Extract/inline playlist entries is complex and not required for
  A1/B1.
- **Suggested answer:** Defer C3 to post diff-gate or parent M8; document in
  `future.md`.

### Fs vs overlay precedence (D5)

- **Context:** Parent M4 `sync` applies fs changes to internal store.
- **Suggested answer (v1):** Uncommitted ChangeSet wins for reads on overlaid
  paths; fs bump marks stale but does not clobber overlay until commit/discard;
  on commit, client ChangeSet wins over stale fs read.

### Asserting "matches example"

- **Suggested answer:** Parsed def equality + slot snapshots + asset bytes after
  final commit; shared blank-project fixture for A* stories.

## Process Notes

- Promoted from parent M5 (2026-05-21).
- Change language v1 locked — [`change-language.md`](change-language.md).
- Parent M6 gated on M6 diff + equivalence gate here.
