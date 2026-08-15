---
status: fixed          # registration race in P1; the hold itself in P2
found: 2026-08-14      # how: live-debugging (demo repro, deployed site)
fixed: 2026-08-14      # P2 of the first-click-open-resilience plan
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

**Fix** — in two parts.

P1 closed the registration hole: the uid goes into `syncing` before the
acquire is awaited, and the drop guard un-registers it if the acquire is
refused.

P2 removed the hold itself, per the plan's D1. `mount_for_sync` now
acquires (polling, since the hold it waits out is local work),
mounts both subtrees — memory-primary mounting reads the whole subtree,
so the mount *is* the snapshot — and **releases before returning**. The
trip runs against that snapshot with no lock held; `SyncMount::finish`
banks what it wrote under a second short hold, or hands those writes to
the open's live store when the project was opened in this tab meanwhile
(the first click landing mid-publish — the ordinary case). With no long
holds left, `await_sync_handoff` dropped from 3 s to 500 ms; the open
path's ~500 ms acquire ladder (P1) absorbs what is left. The locking ADR
is amended to say the rule out loud.

**Regression coverage** —
`lpa-fs-opfs/tests/library_locks.rs::a_publishing_sync_trip_does_not_hold_the_project`
plays the trip's new shape and asserts an open-style acquire wins while
the publish is still in flight (and that an *instant* shot does too —
nothing is held). Its companion,
`a_snapshot_banks_only_what_the_trip_wrote`, pins the property that makes
publishing from a copy safe: the write-back lands only the paths the trip
dirtied, so a save that arrived mid-publish survives.
`sync_queue::work_arriving_mid_flight_earns_another_trip` (host) covers
the other half — a project that changed mid-publish earns its own trip.
The registration order is still not directly testable without a browser:
it is an ordering inside one `async fn`, asserted only by the sequence in
`hold_for_sync`.

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
