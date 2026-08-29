---
status: fixed
found: 2026-08-28 # how: report (signed-in account, projects never readable at /p/ links)
fixed: this change (visibility + the 5xx drop; the legacy-library gap is recorded, not closed)
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
