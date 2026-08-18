# Per-project library locking: two lock kinds, one ordering rule

- Status: accepted, amended 2026-08-14 (locks guard local OPFS
  consistency only, never network IO — see the Amendment section)
- Date: 2026-07-10 (planned 2026-07-08)

## Context

The browser's local project library is a memory-primary store over OPFS:
sync `LpFs` reads/writes hit memory, and a write-behind flusher drains
the change log as whole-file atomic writes (`createWritable`). That
design *requires* mutual exclusion between writers — two tabs mounting
the same tree write behind each other's backs and the last flush wins.
M2 met that with a page-lifetime, whole-library Web Lock: the second tab
got a banner ("LightPlayer is open in another tab") and no persistence
at all. That was always a placeholder — roadmap D19 promised per-project
locks — and it failed real use: working on project X in one tab must not
block project Y in another.

## Decision

The single-writer guarantee moves to **per-project scope**, with a
short-lived catalog lock for the structure both tabs share. The model is
typed and lives in one module per layer: lock kinds and guards in
`lpa-fs-opfs::library_locks`, the transaction vocabulary and host seam
in `lpa-studio-core::app::library::library_host`, the OPFS
implementation in `lpa-studio-web::library_host_opfs`.

- **`lp-project:<uid>`** — exclusive Web Lock, acquired when a project
  opens, held while it stays open. It guards that project's
  `/packages/<slug>/**` and `/history/<uid>/**` subtrees, which are
  mounted as their own memory-primary stores with their own flushers:
  the held lock is exactly what makes their write-behind correct.
  Web Locks (not lock files) because the browser auto-releases on tab
  death — a crashed tab never strands its projects.
- **`lp-catalog`** — short-lived exclusive Web Lock guarding catalog
  *structure*: package directory create/remove/move (rename moves the
  directory since the slug work), `/registry.json`, and seed-once
  example install (find-by-provenance + install is atomic under it, so
  two fresh tabs racing the same example produce one package). Catalog
  transactions mount fresh, mutate synchronously through the same
  `LibraryStore` CRUD the rest of the app uses, and **flush fully
  before releasing**.
- **Ordering rule: Project before Catalog, never the reverse.** A
  structural op targeting a project (rename/duplicate/delete)
  try-acquires that project's lock first — the refusal doubles as the
  friendly "open in another tab" answer — then the catalog lock. Ops
  targeting the project open in *this* tab are host-rejected
  (`OpenInThisTab`); the gallery can't reach that state, but the host
  does not rely on the UI.
- **Opens re-verify under the lock.** Resolving a slug-or-uid key is a
  lock-free catalog read, so a rename in another tab can race it:
  resolve → acquire the project lock → re-verify the key still maps to
  the same uid → retry once on mismatch.
- **Reads take no locks.** Gallery data is a fresh read-only snapshot
  mount (skipping `/history/*/blobs` and `/history/*/trees` payloads).
  Whole-file-atomic OPFS writes make torn *files* impossible; a torn
  *set* is merely stale, and staleness is repaired by invalidation.
- **Cross-tab invalidation:** BroadcastChannel `lp-library`, coarse
  `"changed"` pings after catalog transactions, project closes, and
  saves; receiving tabs (and tabs becoming visible) re-hydrate their
  cached gallery inputs. `navigator.locks.query()` powers the
  "Open in another tab" card badges — presence display only; the lock,
  not the badge, is the truth.
- **The core stays sans-IO.** The controllers reach all of this through
  an injected async `LibraryHost` seam (runtime-neutral futures, the
  `ClientIo` precedent); tests inject a memory-backed fake with ready
  futures. Sync paths that orphan a held lock (state resets) queue the
  uid; the controller's settle points drive the async close.

The page-level lock, its banner state, and the whole-store global mount
are gone. `pagehide` best-effort-flushes open project stores; the
write-behind loss window (≤ ~100 ms + write time) is unchanged in kind,
now per-project in blast radius.

## Alternatives considered

- **SharedWorker / elected-leader store owner** (one context owns all
  IO, tabs RPC to it): rejected — SharedWorker is missing on Chrome for
  Android, an async RPC seam fights the sync sans-IO core the studio is
  built around, and leader re-election on tab death is distributed
  systems complexity the two-lock model simply doesn't have.
- **Per-file OPFS locking:** no such primitive exists; `createWritable`
  is atomic per file but coordinates nothing across files.
- **Keep the whole-library lock, improve the banner UX:** fails the
  requirement — two tabs on two projects is the normal way to work.

## Consequences

- Two tabs, two projects: both fully functional — edit, save, history.
  The same project in a second tab refuses kindly, by name.
- The gallery is eventually consistent (ping + visibility refresh); a
  stale badge can't cause harm because every mutating path re-checks
  the real lock.
- Read-only viewing of a project open elsewhere (`mode: "shared"`
  readers) and a lock-takeover affordance (`steal`) are clean
  extensions of the same model, recorded as future work.
- The `LibraryHost` seam and the transaction vocabulary port unchanged
  onto a daemon-owned store if a desktop/cloud place ever owns the
  library.
- Empty directory husks (the flusher removes files, never directories)
  are swept at the end of catalog transactions, while the catalog lock
  is still held.

## Amendment (2026-08-14): locks never span network IO; sync works from a snapshot

The decision above says what each lock *guards* and left what a holder
may *do* implicit. Cloud sync then read the guarantee it needed — single
writer over `/history/<uid>/**`, which it writes `/cloud-binding.json`
into — and took `lp-project:<uid>` for the whole trip, network included.
Example seeding publishes at 0 ms, so the first click on a fresh example
raced its own publish and was told `This project is open in another tab`
with one tab open
([defect](../defects/2026-08-14-sync-holds-the-project-lock-across-the-network.md)).

That is worse than slow. This model deliberately makes a project lock's
*refusal* a user-facing claim — the "open in another tab" answer is the
refusal, by design — so a hold whose length is set by somebody else's
latency does not merely delay the UI, it makes the product state
something false.

**A project lock guards local OPFS consistency and nothing else, and is
therefore never held across network IO.** A caller whose work is mostly
elsewhere takes the snapshot under the lock and does the elsewhere-work
outside it. Concretely, a cloud sync trip is three phases:

1. **Snapshot** — acquire (polling: the hold being waited out is local
   work now, so waiting is cheap and refusing is wrong), mount the
   project's package and history subtrees, release. Memory-primary
   mounting already reads the whole subtree, so the mount *is* the
   snapshot at no extra cost. It must be taken inside the one hold: a set
   read across two holds could pair a pre-save package with a post-save
   history and publish a version that never existed.
2. **Publish** — the round trip, from the snapshot, with no lock held.
3. **Bank** — reacquire briefly and flush what the trip wrote, or, when
   the project was opened in this tab meanwhile (a first click landing
   mid-publish — the ordinary case), replay those writes into the open's
   live store, which owns those files now.

Consequences of the fold:

- A project that changed while its publish was in flight is not a
  conflict and not an error: the trip that is running stands, and the
  change earns another trip. `SyncQueue` already worked this way ("one
  attempt against a consistent snapshot"); the snapshot is now literal.
- The write-back lands only the paths the trip dirtied — whole-file
  atomic writes, the same property that makes lock-free reads safe — so a
  save that arrived mid-publish is not clobbered by the stale copy.
- Deleting or renaming a project no longer waits out a publish.
- The open path's compensation for this tab's own trip
  (`await_sync_handoff`) drops from a round-trip-shaped 3 s to a
  local-work-shaped 500 ms, with the acquire ladder behind it.

The rule generalizes past sync: **take the snapshot under the lock, do
the foreign-latency work outside it.** Anything that wants a project lock
across an await it does not control is asking the wrong question.
