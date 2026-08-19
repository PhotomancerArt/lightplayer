---
status: fixed          # mid-open view emit in open_on_simulator
found: 2026-08-18      # how: throttled-rig repro (release bundle behind an engine-throttling proxy), after the live G1 Q1 miss on 2026-08-17
fixed: 2026-08-18      # regression: an_open_narrates_on_the_card_while_it_runs
area: lpa-studio-core studio_controller (open flow) + lpa-studio-web gallery cards
class: mid-action-state-never-published
related:
  - 2026-07-28-flash-progress-never-reached-the-ui.md
  - 2026-08-14-worker-boot-timeout-races-the-wasm-fetch.md
  - ../adr/2026-08-14-browser-worker-boot-protocol-v2.md
---
# The opening state never escapes the actor parked inside the open

**Symptom** — On a throttled cold load, clicking an example card showed
NOTHING: no dim, no busy cursor, no "Downloading the engine… N%"
pipeline line — the gallery sat inert for the whole download and then
the project simply appeared. The honest-states work (#427 P6/D4) looked
unshipped in exactly the situation it was built for, even though every
state renders correctly in posed stories and passes its unit tests
(the G1 Q1 residual).

**Root cause** — The card's entire opening treatment is gated on
`home.opening`, which reaches the DOM only inside a published view
snapshot. `dispatch_with_updates` publishes exactly two snapshots — one
before the action (when `pending_open` is not set yet) and one after it
(when the open is already over and `pending_open` is cleared). The
studio actor is parked inside the open action for its whole duration,
so no snapshot carrying `opening: Some(card)` ever escaped. The
`OpeningProgressLine` that narrates the engine download polls
page-thread signals precisely so the parked actor cannot starve it —
but its MOUNT was gated on the view state that only the parked actor
could deliver. The two bracketing snapshots make every fast open look
correct; only a slow open exposes the gap, which is why stories and
unit tests (posed state in, rendered state out) never caught it.

Same mechanism class as the flash defect
(2026-07-28-flash-progress-never-reached-the-ui): an op that holds
`&mut controller` for its whole life must emit its own mid-op view or
the user watches nothing happen. The flash fix added exactly such an
emit for card ops; opens needed the same and did not get it.

**Fix** — `open_on_simulator` emits `UxUpdate::View(self.view())`
immediately after setting `pending_open`, through the same
`UxUpdateSink` the flash path uses. The emit runs synchronously on the
dispatch stack before the first await, so the browser paints the
opening card while the actor parks. Regression:
`an_open_narrates_on_the_card_while_it_runs` (studio_edit_e2e_tests),
the open twin of `a_flash_narrates_its_progress_while_it_runs` —
asserts a view carrying `home.opening` escapes during the dispatch and
that the final view clears it.

**Lesson** — A serial actor's "publish views around each action"
wrapper is a lie for any action long enough to watch. Every long-running
action that changes what the user should be looking at must publish the
change itself, at the moment it happens; bracketing snapshots only
cover actions too fast to matter. When a UI state is verified by posed
stories plus pure state-machine tests, the remaining untested seam is
exactly the LIVE feed — reproduce slow paths against a real throttled
rig before declaring an honest-states surface done.
