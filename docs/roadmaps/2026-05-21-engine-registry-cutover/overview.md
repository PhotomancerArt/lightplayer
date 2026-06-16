# Engine–Registry Cutover

## Motivation

Two parallel stacks exist today:

```text
PRODUCTION                         PROVEN (harness only)
──────────                         ─────────────────────
ProjectLoader → Engine tree        NodeDefRegistry → SlotOverlay → commit
WireSlotMutation → Engine memory   SyncOp → overlay → NodeDefView
Project::reload() on fs change     registry.sync() → SyncResult
```

This roadmap **ends the dual stack** after **M1 hardens the API shape**, then wire,
server, and engine cutover land in order.

Promoted from [artifact-routed M6](../2026-05-21-artifact-routed-file-reload/m6-engine-cutover.md)
(absorbs old M7/M8; M10 provenance stays separate).

## Relationship to other roadmaps

```text
Artifact-routed M1–M4          ChangeSet M1–M10
        │                              │
        └──────────┬───────────────────┘
                   ▼
        M1 API hardening (this roadmap)  ← gate everything else
                   │
                   ├── M2 wire
                   ├── M3 server apply (sequencing TBD in M1)
                   ├── M4–M5 engine cutover + SyncResult policy
                   ├── M6–M7 graph + fs wire-up
                   └── M8 cleanup (incl. legacy mutation deletion)
```

## Architecture (target — shape frozen in M1)

```text
lpc-model::edit          shared serde vocabulary (TBD details in M1)
lpc-wire                 project sync messages (M2)
lpa-server               NodeDefRegistry (M3+)
lpc-node-registry        overlay, commit, NodeDefView
lpc-engine               SyncResult policy (M4+)
```

## Milestones

| # | Milestone | Gate |
|---|-----------|------|
| M0.1 | [**Pre-M1 stabilization**](m0.1-pre-m1-stabilization/00-notes.md) | ArtifactId/store fixes; edit batch design notes |
| M1 | [**API hardening + readiness**](m1-api-hardening.md) | Design signed off; UI parity + mutation inventory |
| M2 | [Wire edit/sync messages](m2-wire-edit-messages.md) | M1 exit criteria |
| M3 | [Server registry + apply](m3-server-registry-apply.md) | M2; sequencing per M1 |
| M4 | [Engine loader cutover](m4-engine-loader-cutover.md) | M1 sequencing decision |
| M5 | [SyncResult engine policy](m5-sync-result-engine-policy.md) | M4 |
| M6 | [Graph reconciliation](m6-graph-reconciliation.md) | M5 |
| M7 | [Server fs wire-up](m7-server-fs-wireup.md) | M5 |
| M8 | [Cleanup + validation](m8-cleanup-validation.md) | Delete legacy mutation stack |

**M3/M4 order is intentionally open** until M1 `m3-m4-sequencing.md` closes.

## Entry criteria

See [notes.md](notes.md).
