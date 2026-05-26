# Milestone 6: Engine + Node Cutover

**Promoted to standalone roadmap:**

[`docs/roadmaps/2026-05-21-engine-registry-cutover/`](../2026-05-21-engine-registry-cutover/overview.md)

Engine switchover, wire edit messages, `lpc-model` vocabulary, server registry
apply, SyncResult policy, graph reconciliation, and server fs wire-up are tracked
there (M1–M8).

## Gate (unchanged)

- **M4** fs-change harness here — green
- **[ChangeSet M6 diff gate](../2026-05-21-changeset-change-management/m6-diff-equivalence-gate/summary.md)** — green

## Historical note

Original M6 scope: delete old `lpc-engine` artifact path; `ProjectLoader` +
`Engine` → `lpc-node-registry` + ChangeSet. See promoted roadmap for expanded
milestones (model + wire + server prerequisites).
