---
status: open           # registration race fixed in P1; the hold itself is P2
found: 2026-08-14      # how: live-debugging (demo repro, deployed site)
fixed:                 # P2 of the first-click-open-resilience plan
area: lpa-studio-web cloud/sync/sync_engine + library_host_opfs
class: lock-held-across-foreign-latency
related:
  - ../adr/2026-07-08-per-project-library-locking.md
  - 2026-08-14-post-acquire-open-failure-leaks-the-project-lock.md
---
# Cloud sync holds a project's lock across a network round trip

**Symptom** — First click on a freshly seeded example, signed in, on a
slow connection: `This project is open in another tab — close it first`,
with exactly one tab open. Clicking again a few seconds later worked.

**Root cause** — `lp-project:<uid>` is a *local* single-writer guard: it
is what makes memory-primary write-behind correct for one project's
`/packages` and `/history` subtrees. The cloud sync driver needs that
guarantee too (it writes `/cloud-binding.json`), so `mount_for_sync`
takes the same lock — and then holds it for the whole trip, network
included: `run_one` mounts, awaits `run_mounted` (fetch, compare, upload,
record), and only then calls `SyncMount::release`
(`lpa-studio-web/src/cloud/sync/sync_engine.rs`). The lock's hold time is
therefore set by the round trip, not by the local writes it guards, and
on a slow link it is seconds.

Seeding an example publishes immediately — `SyncTrigger::Installed` has a
`delay_ms` of `0.0` — so the very first click on an example races its own
publish. The open's compensation, `await_sync_handoff`, waits up to 3 s
(30 × 100 ms) for this tab's own trip to end, and past that bound the
ordinary refusal takes over: a slow trip becomes "open in another tab"
about a project only this tab has ever touched.

The handoff wait had a hole of its own: `mount_for_sync` registered the
uid in the `syncing` set *after* awaiting the acquire, so an open polling
that set in between saw no trip in flight, skipped the wait entirely, and
was refused instantly by the lock the driver was about to take.

**Fix** — P1 (this change) closes the registration hole only: the uid
goes into `syncing` before the acquire is awaited, and the drop guard
un-registers it if the acquire is refused. The hold itself is P2, per the
plan's D1: snapshot the project's files *under* the lock, release, then
publish from the snapshot — project locks never span the network. Once
that lands, the 3 s handoff wait covers a local snapshot rather than a
round trip, and the open path's ~500 ms acquire ladder (P1) absorbs what
is left.

**Regression coverage** — none yet for the hold (P2 owns it, with a
snapshot-outside-the-lock test in `lpa-fs-opfs/tests/library_locks.rs`).
The registration order is not directly testable without a browser: it is
an ordering inside one `async fn`, asserted only by the sequence in
`mount_for_sync`.

**Lesson** — A lock's *name* says what it protects; only its hold says
what it costs. This one is documented as guarding local OPFS writes, and
every reasoning about it (including the "refusal doubles as the open-in-
another-tab answer" rule) assumed hold times in the tens of milliseconds
— then one caller held it across a network operation the guarded
invariant has nothing to do with, and the refusal became a lie the UI
repeated verbatim. When a lock's refusal is also a *user-facing claim*,
holding it across foreign latency does not just slow things down, it
makes the product state something false. Watch for it wherever a
guard-for-local-consistency is taken by a caller whose work is mostly
elsewhere: take the snapshot under the lock, do the elsewhere-work
outside it.
