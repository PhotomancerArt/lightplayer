# Future Work — ChangeSet

## Project diff → ChangeSet stream

- **Tracked as [M6](m6-diff-equivalence-gate.md)** in this roadmap — primary
  parent M6 gate (`diff(∅, basic)`, `diff(basic, basic2)`).
- Hand-written morph fixtures remain useful as diff regression inputs.

## ChangeSet replay stress harness (host / emu / device)

- **Idea:** Replay ChangeSet log through full engine (post parent M6) at
  configurable granularity.
- **Why not now:** Requires engine on ChangeSet path; this roadmap proves
  registry harness only.
- **Useful context:** Parent `future.md`; catches OOM/fragmentation whole-reload
  tests miss.

## ChangeSet wire protocol + CRDT merge

- **Idea:** Full `lpc-wire` messages; concurrent edit merge.
- **Why not now:** v1 is ordered in-memory ChangeSet + commit/discard.
- **Useful context:** `lightplayer-app-ui` SlotOp mockup.

## C3 inline ↔ standalone refactor

- **Idea:** Extract/inline playlist entry defs; round-trip harness.
- **Why not now:** High complexity; not required for A1/B1 gate.
- **Useful context:** User story IDs C3a–c in `notes.md`.

## Multi-ChangeSet replay (D4)

- **Idea:** Stable op ids; replay ordered stack of ChangeSets.
- **Why not now:** Single active ChangeSet sufficient for v1 gate.
- **Revisit when:** Wire batching or undo stacks.

## Binary file assets

- **Idea:** `BinaryFileSlot` sibling to M3 text sources.
- **Why not now:** v1 whole-file text assets only.
- **Useful context:** Parent roadmap `future.md`.
