---
status: retired
since: 2026-08-30
logged: 2026-09-07
retired: 2026-09-07
area: lpa-studio-web share (relationship control) + cloud sync driver
related:
  - lp-app/lpa-studio-web/src/app/share/project_roster.rs
  - lp-app/lpa-studio-web/src/cloud/sync/publish_notice.rs
  - lp-app/lpa-studio-web/src/cloud/sync/sync_engine.rs
  - docs/adr/2026-08-30-relationship-control-three-segment-bar.md
  - docs/defects/2026-08-28-auto-publish-outcomes-invisible.md
---
# The relationship face stays "Private" after this tab publishes

**Condition.** `derive_relationship` turns a library project into
`MinePublished` only when the `GetProject` roster answer is in
(`roster_answered`). The project roster is fetched when the popover's
inputs are gathered (`web_app.rs`, `use_project_roster`), not when the
auto-publish driver concludes a trip. A project that this tab publishes
after that fetch — the first save of a new project, or a `Retrying` row
that the coarse tick finally lands — keeps the bar's "Private" face and
the popover's `MineLocal` skeleton until something re-fetches the roster
(a reload, a project switch).

Since #576 the popover is honest about it: the `MineLocal` Access
sentence reads the ledger row, so a `Published` / `Pushed` row under a
still-silent roster says "Published from this tab — waiting on the
service for its access and roster." The *sentence* is right; the *face*
is stale.

**Why it matters.** The bar's face is the one-word answer to "is this
shared?", and after a successful first publish it says no. The fix is
small and lives at the seam the ledger already names: re-fetch the roster
when the ledger records a `Published` / `Pushed` conclusion for the open
project (the ledger is a notebook by design — the trigger should be the
driver's conclusion, not a poll of the notebook), or let the roster
fetch retry on its own when it answered before the publish.

**Done looks like.** Create a project signed in, watch the bar flip from
"Private" to "Shared" without a reload; the "mine, published" story is
unchanged; no new ledger controls.

Filed 2026-09-07 from PR #576's ship report.

**Retired 2026-09-07.** The driver files a *publish notice* — a per-tab
board of published/pushed uids behind a generation counter
(`cloud::sync::publish_notice`) — on every trip that put something in the
cloud, and the roster hook parks on it rather than polling the ledger. A
notice naming the project the hook is watching costs one `GetProject`;
every other notice costs nothing. The second ask never blinks through
`Loading`, so a face that is already "Shared" does not flash "Private" on
its way to being re-confirmed, and the ledger stayed the diagnostic
notebook it was built as — no new controls, no subscription on it.
