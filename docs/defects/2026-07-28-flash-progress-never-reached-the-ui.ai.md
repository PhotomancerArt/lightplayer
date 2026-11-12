---
status: fixed
found: 2026-07-28      # how: prod, user-reported
fixed: this change
area: lpa-studio-core (studio_actor / studio_controller)
class: silent-drop
---
# A firmware update ran for a minute with nothing on screen

**Symptom** — On prod Studio, a device card offered *Update firmware*.
Clicking it produced **no UI change whatsoever** — no progress, no
narration, no console output — for the entire flash. The only evidence
the click had done anything was the browser console:

```
[esp32-flash] Writing at 0x286c68... (87%)
[esp32-flash] Writing at 0x28ce57... (88%)
```

The card's own progress bar and technical-details log region were
already built and already correct (`card_op_overlay` in `device_card.rs`
renders a determinate bar plus a log tail). They had simply never been
reachable. The action's own summary copy promised what the user did not
get: *"the card shows progress and you can walk away."*

**Root cause** — The card-owned op flow's state never reached a card
while the op ran. Three facts compose:

1. The overlay is driven by `card.ui.op`, which **only a full view build
   populates** (`overlay_card_ui` reads the controller's
   `device_card_op` slot).
2. `run_device_management` installs that slot and then holds
   `&mut controller` for the entire operation — so it cannot rebuild a
   view mid-flight. The only snapshot emitted before it starts is
   `dispatch_with_updates`'s seed, built *before* the slot exists, whose
   cards are therefore all bare.
3. Progress ticks emitted `UxUpdate::Activity`, which patches a **pane
   section** — never a card. The op's own progress had no delta that
   could reach the surface the user was looking at.

So the slot was faithfully updated in memory, tick after tick, and
nothing that carried it ever left the controller. A view carrying the op
*did* eventually escape — during the post-flash reattach, when
`attach_runtime` emits views — which is precisely the reported shape:
a minute of nothing, then the finished result.

The card's log region was dark for a related reason: `card.console_tail`
is fed from the session's tail at view-build time, but the management
sink routes its lines to `captured_logs` + `UxUpdate::Log`, so the
overlay's technical-details region read "Waiting for device output…"
throughout.

**Ruled out** — the actor's live-view guard (`run_action` drops
`Activity`/`Log` deltas until a full snapshot seeds `live`). That
ordering contract *is* unenforced and load-bearing, and it looks like
the culprit; it is not, because `dispatch_with_updates` already emits a
seed at the top of every dispatch. Worth knowing it is there — the next
producer that emits deltas outside a dispatch will meet it.

**Fix** — Three parts, all in the delivery path (no new UI):

1. `run_device_management` emits `UxUpdate::View(self.view())` **after**
   installing the op slot, so the overlay mounts when the work starts
   rather than when it ends.
2. New `UxUpdate::CardOp { uid, op }` delta, emitted by
   `management_event_sink` on every progress tick and on the
   expected-disconnect transition, applied by the actor via
   `UiStudioView::apply_card_op`. The card-op matching rule now lives in
   one place — `UiDeviceCard::takes_card_op` — called by both the
   controller's view build and the actor's patch, so the mid-flight card
   and the snapshot that replaces it cannot disagree.
3. The actor also routes `UxUpdate::Log` into the mid-op card's
   `console_tail` (`push_card_op_console`, capped at `CONSOLE_TAIL_LEN`),
   so the overlay's technical-details region streams esptool's own
   output.

**Blast radius beyond the flash** — erase and runtime reset share
`run_device_management` and were equally mute.

**Regression coverage** — `a_flash_narrates_its_progress_while_it_runs`
(studio_link_e2e_tests) drives a real scripted flash and pins both
halves: the overlay mounts *before* the first progress tick, and the
scripted 50%/100% both reach the card. Verified to fail without the fix
("progress must reach the card as it ticks, got []"). Unit coverage for
the appliers in ui_studio_view (`a_stamped_op_lands_on_its_own_card_only`,
`an_unstamped_op_rides_the_live_card_never_a_remembered_one`,
`op_console_lines_reach_the_card_mid_op_and_stay_bounded`,
`the_lens_card_tracks_the_op_it_is_the_same_card_grown`).

**Lesson** — State that only a full rebuild can publish is invisible to
any code path that cannot afford a rebuild. The op slot was updated
correctly on every tick; it just had no way out, because the one
mechanism that reads it (`view()`) needs `&self` on a controller the op
holds mutably for its whole duration. When a long-running operation owns
the thing that renders it, "write it down and let the next render pick
it up" silently means *after I finish*.

Worth naming separately: the progress bar and log tail the user asked
for **already existed**, fully built, in `card_op_overlay`. Two sound
pieces — a renderer that draws op state, a controller that maintains it
— with no wire between them under the one condition that mattered. The
absence of any error made every instinct point at the renderer, which
was innocent and already complete.
