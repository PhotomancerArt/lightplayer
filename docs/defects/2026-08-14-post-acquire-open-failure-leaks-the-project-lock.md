---
status: fixed
found: 2026-08-14      # how: live-debugging (demo repro, deployed site)
fixed: this change     # P1 of the first-click-open-resilience plan
area: lpa-studio-web library_host_opfs + lpa-studio-core project_controller
class: lifecycle-ownership
related:
  - ../adr/2026-07-08-per-project-library-locking.md
  - 2026-08-14-sync-holds-the-project-lock-across-the-network.md
  - 2026-08-14-worker-boot-timeout-races-the-wasm-fetch.md
---
# An open that fails after acquiring keeps the project lock forever

**Symptom** — Clicking a gallery card failed, and every later click on
that same project then failed differently: `This project is open in this
tab — close it before changing it`, with nothing open. No amount of
retrying helped; only a page reload cleared it. In the live demo the
first failure was a worker boot timeout (`timed out waiting for browser
worker boot`), so the two defects presented as one unrecoverable dead
end.

**Root cause** — Opening a project is two halves owned by two layers.
`OpfsLibraryHost::open_project` (`lpa-studio-web/src/library_host_opfs.rs`)
does the first half: acquire `lp-project:<uid>`, mount the package and
history subtrees, spawn their write-behind flushers, and register the uid
in the host's open map. The second half is the caller's — migrate the
package if needed, read every file, push it to the runtime
(`ProjectController::open_opened_package`) — and it can fail at each
step.

The lock's release path ran only through `close_project`, and the only
thing that ever queued a close was `context.active`: the controller
pushes the *previously* active project onto `pending_close` when a new
one becomes active. A project whose open failed never became active, so
no close was ever queued for it, so `close_project` never ran. The lock
(plus the mount and two flush loops) then lived as long as the page.
Worse, the leaked registration is what the *next* attempt trips over:
`open_project` refuses a uid already in its own open map with
`OpenInThisTab`, which is why the symptom mutates from "boot failed" to
"open in this tab" and never recovers.

A second instance of the same shape sat inside the host itself: the uid
was parsed into a `PrefixedUid` *after* the registration, so a malformed
uid leaked in exactly the same way.

**Fix** — `OpenedProject` now carries an `OpenReceipt`
(`lpa-studio-core/src/app/library/library_host.rs`): an RAII drop guard
holding the host's teardown. Committed once the project reaches
`context.active`; dropped uncommitted — which every `?` between the two
halves does — it runs `OpenRegistry::release_open`, the one named
teardown that `close_project` also awaits (unregister, stop flushers,
flush, release the lock, ping other tabs). The uid parse moved ahead of
the acquire, so that failure now refuses before any lock exists. Hosts
that hold nothing hand back `OpenReceipt::nothing_to_undo`.

**Regression coverage** —
`a_failed_open_gives_the_project_back_to_the_library`
(`lpa-studio-core/src/app/studio/studio_edit_e2e_tests.rs`): a below-floor
package refused by the open pre-flight must leave
`MemoryLibraryHost::abandoned_projects` naming it, with no close ever
queued; the happy-path open test now also asserts the receipt was
committed rather than dropped. Lock-level: `a_failed_open_leaves_the_
project_reopenable` in `lpa-fs-opfs/tests/library_locks.rs` (browser gate
only — `just check test` compiles no wasm32).

**Lesson** — When a resource is acquired in one layer and the operation
it guards completes in another, the release cannot hang off the
*success* state. `pending_close` is derived from "what is active", which
is a fact that only exists once everything worked; every failure path
therefore falls outside it by construction, and no amount of care at the
individual call sites fixes that (there were three, and P6's supersede
would have added a fourth). The shape that does fix it is a receipt the
second layer must either commit or give back, so that `?` is already
correct — and one named teardown both endings call, so a superseded open
later cannot invent its own.
