---
status: fixed
found: 2026-08-28 # how: report (signed-in account, projects never readable at /p/ links)
fixed: this change (visibility + the 5xx drop; the legacy-library gap is recorded, not closed); round 2 2026-09-06 (the coarse tick re-derives from the library)
area: lpa-studio-web cloud sync (sync_engine / sync_trip / fetch_cloud_port)
class: silent-drop
related:
  - docs/defects/2026-08-24-request-idle-budget-blind-to-dropped-responses.md
---

# Auto-publish concludes without traffic and without trace, so "never published" has no first question

**Symptom** — Yona, signed in on lightplayer.app, reports projects that
never became readable at their `/p/<slug>-<uid>` links. Nothing in the UI
says anything (by design), and the console warns are the only witness
nobody reads.

**Prod evidence (2026-08-28)** — The deployed service (sha `9773cf656`)
is healthy, and its store held exactly **one** published project — a
fresh, default-named "Project" (two commits, `access: view`, anonymously
readable). So the publish→push→read pipeline works end to end. The fly
request log for a signed-in page load shows the 3 session POSTs and then
**zero** sync traffic: no `getProject`, no `publishProject`, no blob or
tree PUTs. The sign-in sweep — the only path that offers *pre-existing*
projects — concluded something about every project in the library
**without making a single network request**, and none of those
conclusions is visible anywhere. A fresh worktree repro against `just
cloud-serve` (same build) publishes signed-out-created projects on the
sign-in sweep correctly, so the mechanism is specific to what the real
library's projects make the trip conclude locally.

**Root cause (the class, not one bug)** — Every local conclusion in the
trip is indistinguishable from working:

- `run_trip` returns `TripReport::Nothing` when `project.head()` is
  `None` (a package with an origin but no saved version — which is what
  a legacy, pre-history library entry looks like until it is opened and
  saved). The driver logs it at *debug* and the queue treats it as
  `Settled`: dropped, forever, silently.
- A local-state error (`NoLocalHistory`, unreadable manifest, uid that
  will not parse, a mount that fails) classifies as `Refused`: one
  `log::warn`, then the queue forgets the project until the next save or
  sign-in — which for an untouched legacy project is never.
- `classify` sent **every** `TransportError::Protocol` to `Refused`,
  and `fetch_cloud_port` filed HTTP `500` under `Protocol` (only
  502/503/504 were "offline"). A dev proxy answering 500 for a dead
  upstream, or a service mid-crash, therefore *dropped* the publish
  instead of retrying it. One transient 500 = "publish never happens
  until the next save".

Compounding condition (dev only): `Dioxus.toml` pins its `/api` proxy to
`localhost:2812` while `just cloud-serve` hashes a per-worktree port, so
under `dx serve` the cloud is a guaranteed 500 unless the port is pinned
(`LP_CLOUD_PORT=2812 just cloud-serve`) — the long-observed "pre-existing
dev-proxy POST /api 500 noise", which the old classification turned into
silent drops rather than retries.

**Fix** —

- `fetch_cloud_port::status_error`: the whole 5xx family is now
  `TransportError::Offline` (retryable); 4xx stays `Protocol` (the
  version-mismatch family a reload owns).
- `sync_status` (new): the driver records every conclusion — including
  the zero-traffic ones — in a per-tab ledger: engine facts (signed-in,
  sweep time/size, library-host-missing) plus the newest outcome, detail
  sentence, and timestamp per project.
- `/account` grew a diagnostic **Cloud sync** group that renders the
  ledger (badged rows; failures in error/warning tone). No controls — the
  product-level share surface is the separate 2026-08-28 examples & URL
  vision.

**What stays open** — Whether a legacy project with content but no saved
head should be *healed* at sweep time (record a save so it publishes)
is a product decision for that vision, not a contained fix; today the
ledger at least names it ("no saved version yet — nothing to publish").
With the ledger deployed, Yona's `/account` page will state each real
project's actual conclusion, closing the diagnosis this entry opens.

**Regression coverage** — `fetch_cloud_port::statuses_sort_into_the_right_family`
now pins `500 → Offline`; `sync_status` unit tests pin the ledger's
newest-wins and silent-branch-naming behavior.

**The lesson** — "Silence is the design" is only tenable when every
silent branch is *observable somewhere*. An engine allowed to conclude
"nothing to do" with no trace conflates six different conclusions —
working, empty, skipped, refused, denied, and broken — into one
indistinguishable quiet.

## Round 2 (2026-09-06) — the tick never re-derived, so one dropped trip was forever

**Symptom** — Yona flashed a board, generated a project for it (device
card → project picker → "New for this board"), and the relationship
panel kept saying *"Not shared — it lives in this browser's library.
Access controls appear once it reaches the cloud."* Signed in the whole
time. That sentence is the `MineLocal` arm, which `derive_relationship`
returns for unpublished, restricted **and** service-silent alike.

**Not the cause** — the no-saved-head gate above. `GenerateForBoard` goes
through `LibraryStore::install_package`, which ends with
`handle.record_save(now)`: the head exists.

**Root cause** — the install-time `sync_engine::note(uid, Installed)` was
the project's **only** trip, and the driver had no second chance for it:

- `SyncQueue::finish` *removed* the entry on `Refused` (which `classify`
  hands out for everything it does not name, plus the local "no library
  host in this tab" and unreadable-snapshot cases), on `Denied`, and on
  `Settled` — including the zero-traffic `Nothing`.
- `sweep_forever` only `arm_all`-ed entries the queue still tracked. The
  full-library `sweep()` ran on the false→true sign-in edge and nowhere
  else. A project whose one trip was dropped therefore never came back
  until a save happened to land in it.
- `note()` is a no-op while `signed_in` is false — which includes the
  window while `whoami` is still pending after a page load. An install
  landing there queued nothing; the sign-in sweep would catch it, unless
  that sweep gave up waiting for the library host (`host_missing`), in
  which case nothing was offered until the next page load.

**What the dev server showed (2026-09-06, signed in against a local
`cloud-serve` on the pinned port)** — a project from the New menu
publishes on its install-time trip: the bar reads "Shared" at once and
the ledger row is `published` / `pushed`. So the drop is not the create
path itself. With `/api` made unreachable from the page for one create,
the ledger row read `retrying · transport: offline` and the bar
"Private" — Yona's symptom — and the next tick (one minute later, the
cloud log's next request burst) published it, with no traffic for the
project that had already settled. The board-generated flow
(`GenerateForBoard` → the same `catalog` transaction → the same `note`)
could not be exercised without hardware; Yona's own `/account` row is
still the missing datum. The candidates that fit a signed-in tab with a
saved head are the ones above: a refused first trip (a tab left open
across a redeploy answers `VersionMismatch` → `Refused` → dropped), an
install that landed before `whoami` answered, or a sign-in sweep that
gave up on the library host. All three are now re-derived by the tick.

**Fix** —

- `SyncQueue::sweep(library, now)` — the coarse tick is now a sweep over
  the **library roster**, not over the queue: every uid the queue has no
  current verdict on is offered, tracked retries are re-armed, and the
  ones that settled, that a denial latched, or that are waiting out a
  refusal's backoff are left alone. Settled verdicts are remembered
  per tab (and forgotten when the uid leaves the library), which is what
  keeps a tick over a quiet library free of traffic — an up-to-date push
  still costs two round trips and an OPFS snapshot.
- `TripResult::Refused` backs off (`REFUSED_BACKOFF_MS`, five minutes)
  instead of being forgotten. A save or rename pre-empts the backoff; the
  tick does not shorten it.
- `SyncEngine::sweep_forever` → `tick()`: read the roster, `queue.sweep`,
  pump. A tick that offers projects after a `host_missing` sign-in sweep
  supersedes that ledger record, so `/account` stops telling the user to
  reload.

**Regression coverage** — `sync_queue`:
`a_refused_project_is_offered_again_after_the_backoff`,
`a_save_pre_empts_a_refusals_backoff`,
`the_sweep_offers_a_project_the_queue_has_never_seen`,
`the_sweep_leaves_settled_projects_alone`,
`the_sweep_forgets_verdicts_for_projects_the_library_lost`.

**Still open** — the no-saved-head case (`TripReport::Nothing`) is now a
settled verdict the tick respects, so it is still not healed at sweep
time; that remains the product decision recorded above. The
relationship panel's fold of "service-silent" into `MineLocal` closed the
next day (below).

## The panel reads the ledger (2026-09-07)

The relationship panel's `MineLocal` Access sentence now
reads this ledger for its own project: a failure or unfinished conclusion
("Publishing is retrying — the service was unreachable", "Publishing was
refused: …") replaces the generic "Not shared — it lives in this browser's
library" line that used to cover unpublished, restricted, and
service-silent alike. The driver also records the actionable failures in
a person's words (`sync_trip::describe_error`: "the service was
unreachable" rather than `transport: offline`), so `/account` and the
panel read the same sentence. `/account` is unchanged in role — still the
only surface that lists every project.

## The face follows the driver (2026-09-07)

Reading the ledger fixed the panel's sentence and left the bar's face
stale: a project this tab published after its one `GetProject` kept the
"Private" face until a reload
(`docs/debt/relationship-face-stale-after-publish.md`, now retired). The
ledger was the wrong thing to subscribe to — it is a notebook, and giving
it a nervous system would have made every reader a poller — so the driver
grew the one push it was missing: `cloud::sync::publish_notice`, a per-tab
board of published/pushed uids behind a generation counter. The roster
hook parks on it and re-asks only when a notice names the project it is
watching. The ledger's role is unchanged: still diagnostic, still
poll-copied once a second by `/account`, still nothing's dependency.
