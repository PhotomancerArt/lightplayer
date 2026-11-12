# Project history becomes a DAG: clobber joins replace "no merge, ever"

- Status: accepted
- Date: 2026-08-05
- Plan: lp2025/2026-08-05-1642-cloud-folders-sync (P01)

## Context

`lpc-history` was built with a deliberately linear model: one line of
versions per project, no DAG, no in-project branching, and — quoting the
crate's own former invariant — "no merge, by design, ever." Divergence
had exactly two resolutions: adopt the observed copy or fork it into a
new project uid.

That model was chosen when the only multi-copy story was **on-device
copies**: the user story was someone else pushing to your device, or an
old version of yourself. The merge edge case was not big enough to
warrant doing versioning "right," and the linear invariant kept every
consumer simple (`SyncRelation::{AtHead, Behind, Diverged}` against a
single line).

The cloud-folders/share-by-URL vision (planning dir
`2026-08-05-1642-cloud-folders-sync`, D6/D7/D8) changes the class of
problem: multi-user project management online, where two people
holding the same project uid genuinely both advance it. Making one of
them throw away their work — the only move the linear model offers —
is a non-starter for exactly the "someone changed a brightness"
scenario collaboration produces daily. This is also a practical
blocker for onboarding: remote setup and product testing require
sharing one project across accounts.

## Decision

A project's history is now an **append-only ordered event log whose
ancestry forms a DAG**. The only non-linear node is the **clobber
join** (`EventKind::Joined { kept, set_aside }`):

- Two parents; the resulting content is `kept` — always exactly one
  parent's content, never computed. (git's `merge -s ours/theirs`,
  precisely.)
- Precondition: exactly one side of the join is the current head — a
  join resolves a divergence between the local head and one foreign
  version. Replay enforces this, so a persisted log cannot smuggle in
  an unanchored join.
- The losing side is **set aside, never destroyed**: content is
  content-addressed and banked before the join is recorded, the hash
  stays reachable from the event log, and `classify()` reports it
  `Behind` — so a device or peer still carrying the losing version
  fast-forwards instead of re-diverging.
- Joins are head-advancing events like saves: they get version
  numbers and `saved_at` timestamps.

What did *not* change: forks still mint a new project uid
(`ForkedFrom` origin); the head rule (edit at head advances, edit
elsewhere forks) is untouched; the event log remains torn-tail-
tolerant JSONL; there is still no computed content merge.

## Consequences

- **The complexity cost was weighed and accepted.** We have matured
  enough to need a DAG. The benefit — collaborative project
  development where conflict resolution is safe, recorded, and
  non-destructive — is worth it. (User ruling, 2026-08-05, recorded
  in the planning notes.)
- **Real merge has a prepared home.** Per-file and per-field merge
  (planned post-slice) are joins whose content is *derived* rather
  than picked; the event shape and reachability semantics do not
  change again. Live CRDT-style collaboration remains a separate
  ephemeral layer that commits snapshots into this DAG.
- `SyncRelation` survives unchanged in vocabulary; `Behind` now also
  covers superseded (set-aside) versions. Consumers that only
  construct or wildcard-match `EventKind` are unaffected; the new
  variant is additive to the persisted format (old logs replay
  unchanged; logs containing joins are not readable by pre-join
  builds, which is acceptable under the studio-owns-history model —
  firmware never reads history logs).
- The former invariant text in `lpc-history/src/lib.rs` is replaced;
  documents quoting "no merge, ever" (e.g. the optimistic-concurrency
  planning notes) are superseded by this ADR on that point.

## Alternatives considered

- **Stay linear, bank the loser as a side snapshot** (the shape the
  parked optimistic-concurrency plan chose for tab conflicts): avoids
  the DAG today, but re-breaks the invariant the day real merge
  arrives, and leaves the resolution *choice* unrepresented in
  history — the cloud vision explicitly requires "the choice goes in
  the history."
- **Full git-style multi-head branching in the client:** far more
  than the product needs; the head-frontier lives server-side (a
  project's refs), while each local history stays an ordered log with
  joins.
