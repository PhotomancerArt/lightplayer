---
status: fixed
found: 2026-10-03      # report (Yona, the classic DOM-Z-102 on lightplayer.app)
fixed: this change
area: lpa-studio-core studio_controller (attach_lens, the `?on=mac:` open)
class: stale-measurement
related: []           # found in the same sitting as PR #943's Studio feel step
---
# A Studio reload re-pushed the project the board was already running

**Symptom** — Yona refreshed Studio (lightplayer.app) while editing the
classic's `quad-wire-oracle` and the page sent the whole project to the
board again. A reload should reattach to what the board runs, not push. Not firmware: `main`'s Studio does it on
every board, and the classic only made it slow enough to see (the push
is tens of KB over a 921,600-baud UART).

**Root cause** — the address carries the open. While the editor is on a
board, the router writes `?on=mac:<base mac>` into the address so that "a
reload lands back where it was" (`lpa-studio-web` `router.rs`), and a page
loading that address dispatches `HomeOp::OpenPackageOnDevice` with
`over_running_project: false`. That open always pushes once the lens lands
(`attach_lens` → `open_pending_package`, D19: "opening is a push of the
library head") unless the mismatch rule stops it first, and the mismatch
rule (`DeviceByBaseMac::would_push_over`, D50) reads whether the board is
running anything off the roster's heartbeat evidence. A page that has just
loaded has no heartbeat yet, so every board reads "running nothing":

- the same project at the same version was pushed over itself (the
  report);
- a **different** project was pushed over the running one with no
  mismatch page at all (found while fixing; the URL path's own comment
  says "arriving from a URL never authorises a push over a running
  project").

D19 was decided for the sim, which is ephemeral; a board is a place that
keeps what it was given.

**Fix** — `StudioController::open_meets_what_the_board_runs`, asked from
`attach_lens` before the push for a board open the mismatch page has not
answered (`OpenOn::Device { over_running_project: false }` on a non-sim
lens). By then the wire is up, so it asks the board itself: `ListLoaded`
(nothing loaded → push as before) and the package hash. When the hash is
this project's library head, the open connects and binds instead — no
push, one console line saying so. When the hash (or, failing that, the
registry association) names a different library project, the open stops
at the mismatch page, as a warm open would have. Anything else (this
project at another version, a project the library cannot name, a board
that does not answer) opens exactly as it did.

**Regression coverage** — `studio_device_e2e_tests.rs`:
`a_reload_reattaches_to_the_project_the_board_is_running` (the runtime
handle survives the reload, so nothing was loaded; the library head is
unchanged) and
`a_reload_naming_another_project_stops_at_the_page_instead_of_pushing`
(the page names what the board said it runs, and the board still runs it
on the same handle). Both fail without the fix: the first on the handle
(a push loads a new one), the second because no page appears.

**Lesson** — a guard that reads a *live* fact off a cache the page builds
up over time is a different guard on a cold page than on a warm one, and
the cold page is exactly the reload this address exists for. Ask the
source (the board, over the wire you are about to push down) at the
moment of the decision, not the roster's memory of it.
