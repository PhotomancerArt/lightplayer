---
status: fixed
found: 2026-10-08      # how: report (a code read while planning lp2025/2026-10-08-2330-connected-in-the-card, notes §7)
fixed: this change
area: lpa-studio-web web_app.rs (the view loop's lens sync) × router.rs (`same_session`, `lens_route`)
class: state-conflation
related:
  - docs/adr/2026-09-22-opening-a-board-adopts-its-project.md (`/device/<uid>` is a resolver)
  - lp2025/2026-10-08-2330-connected-in-the-card (P1; ruling Q3)
---
# A board's play address loses play when it heals

**Symptom** — loading `/device/<uid>/play` for a board running a project the
library knows opens the board, then rewrites the address to
`/p/<slug>-prj…?on=mac:…`, without `/play`. The page leaves the play surface
for the editor. Nobody reported it: no walk, test or link in the app ever
loaded a `/device/<uid>/play` address. `walk-ble-emu` and `walk-drop-emu`
reach Play by clicking the Play link on `/p/…`, and nothing in Studio writes a
device address. It was found by reading the lens sync while planning the
connected card, whose "All controls" links to the play page.

**Root cause** — the lens sync's one test, "is the address already this
lens's?", asked two questions with one comparison. `StudioRoute::same_session`
is how the sync asks "is this the same document?". It ignores the view (play is
a zoom on the same session) and the slug, so a lens on `/p/x` never rewrites
`/p/x/play`. Between a `Device` route and a `Project` route it falls back to
`==` and says no. That part is right: `/device/<uid>` is a resolver, not the
project's address, and it should heal. But the heal then wrote the lens's own
route, and `lens_route` always reads `Workspace` (the lens knows nothing of
play). So "this address must heal" also meant "this address's view is thrown
away". The view was a fact about where the user is, and only the device half
of the address was meant to change.

1. At `/device/<uid>/play` the route listener opens the lens.
2. Once the editor shows, `bound = router::lens_route(&next)` is
   `Project { view: Workspace, … }`.
3. `on_shell_route` includes `Device`, and `same_session(Project, Device)` is
   false.
4. The current route is not a `/p/` or example route, so the sync called
   `router::navigate(&target)` with the lens's route as it was: no `/play`.

**Fix** — the inline branch became a pure helper,
`router::lens_sync_target`, beside `lens_route`. When the current route is
`Device { view }`, the healed address keeps that view
(`target.with_view(view)`), so `/device/<uid>/play` lands on
`/p/<slug>-prj…/play?on=mac:…` and every other suffix rides along the same way.
Every other answer is today's, pinned by a test per row. The same change adds
the one narrow case where `/device/<uid>` is written: an unbound lens that has
just taken the editor from a route that is not a lens route (ruling Q3).

**Regression coverage** — `router::tests::a_device_play_address_heals_to_the_projects_play_address`
and `a_device_address_heals_to_the_project_with_its_view` (both fail with the
view arm removed); the other rows: `a_project_address_resolves_in_place`,
`an_unbound_lens_opened_from_a_page_goes_to_its_device_address`,
`an_unbound_lens_on_a_lens_route_stays_put`,
`a_steady_lens_off_the_shell_routes_is_left_alone`. The view loop itself
runs only in the browser and has no host test. No walk loads a
`/device/<uid>/play` address either.

**Lesson** — a heal should change only the part of the address it exists to
fix. The device heal's job is the device half (`/device/<uid>` → the project
plus `?on=`). The view suffix is the user's, the way the slug heal keeps the
hint and the view and the hint heal keeps the view. An address nobody in the
app writes is also one no walk exercises: a resolver address needs its own
test row for every suffix it parses.
