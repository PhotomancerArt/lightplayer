# Engine–Registry Cutover — Notes

## Scope

Promote the old **artifact-routed M6** (engine switchover) into a standalone
roadmap. **M1 is API hardening** — resolve shape, UI parity, and sequencing before
implementation churn.

Registry **behavior** stays in `lpc-node-registry`. Shared **vocabulary** moves to
**`lpc-model::edit`** (pending M1 sign-off). Engine **policy** stays in
`lpc-engine`.

## Current state

See [overview.md](overview.md). Registry harness green; production stack unchanged.

## User direction (2026-05-21)

- Promote parent M6 into its own roadmap.
- **M1 = cleanup + hardening** — answer open questions; commit to API shape before
  moving types / wire / cutover.
- **M3/M4 split** — not decided; not blocked on fear of cutover.
- **Mutation cleanup** — inventory in M1; delete legacy path after cutover (M8).
- **UI parity** — M1 documents what debug UI needs vs edit language.

## Open questions → M1

All question catalogs live in [m1-api-hardening.md](m1-api-hardening.md). Summary:

| Area | Status |
|------|--------|
| Model vs registry split | M1 |
| SyncOp in model? | M1 (lean yes) |
| Wire envelope | M1 |
| Path vs node-id addressing | M1 |
| M3 vs M4 sequencing | M1 — user TBD |
| UI parity / commit UX | M1 |
| Legacy mutation inventory | M1 doc → M8 delete |

## Dependencies (entry criteria)

- Artifact-routed M4 green
- ChangeSet M6 diff gate green
- ChangeSet M8 sync green
- ChangeSet M10 slot/asset split on branch

## Risks

- Skipping M1 and moving types too early → wire churn
- UI commit model vs instant mutation — product decision in M1
- Graph reconciliation scope (M6)
