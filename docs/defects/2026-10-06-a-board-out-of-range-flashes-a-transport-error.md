---
status: fixed
found: 2026-10-06      # how: hardware-walk (Yona's desk walk of PR #880, Bluetooth on lp-link, a XIAO C6 walked out of range under Play)
fixed: this change
area: lpa-studio-core StudioActor::run_refresh_tick × the lens tap's device inputs × StudioController::record_passive_refresh_failure (plan D13's reconnect rule)
class: stale-measurement
related:
  - docs/defects/2026-10-02-a-dropped-link-sends-the-editor-to-devices.md (the hold, and the withdrawn sync failure this extends)
  - lp2025/2026-09-28-1445-ble-on-lp-link (the desk walk; spikes/ble-lab's runbook step 6, "drop it under Play, three ways")
---
# A Bluetooth board walked out of range flashes a transport error before "Reconnecting…"

**Symptom** — Yona's desk walk of #880 (merged as `647c302ab`), Studio in Play
on a XIAO C6 over Bluetooth, dropped three ways. A power cut and a reboot
raised the "Reconnecting…" curtain directly. Walking the board out of range
first flashed a red box, `Transport error: the device did not respond over
Bluetooth within 5.0s`, and the curtain replaced it a moment later. "That was
kinda ugly."

**Root cause** — the pull's failure was judged before the link's own account
of it was heard. Out of range, the GATT link does not drop at first: it goes
quiet. The page's lp-link notices after its stall time (3.5 s on Bluetooth)
and says `link: stalled — the board has gone quiet; holding the session`, and
the editor's pull, which owns the wire, hears that note on its tap — but the
tap does not fold it. It queues it, behind the batch that is running:

```rust
controller.set_device_input_sink({
    let tx = tx.clone();
    move |input| tx.send(StudioCommand::Device(input))
});
```

So when the pull ran out its 5 s budget, the link health still read healthy.
The failure had already been written on the project as it happened
(`ProjectController::record_sync_failure`: `sync.fail(error.to_string())`),
and the only exception to it was a link that had gone away altogether:

```rust
// A pull that failed because the link went away is not the
// project's failure: the strip says what happened.
if self.lens_hold.is_some() {
    return;
}
```

The batch emitted that view, red. The next batch folded the stall note,
raised the curtain, and dimmed the red box under it. A power cut and a reboot
never showed it because they end in a GATT disconnect, which holds the lens
and withdraws the pull's failure (`hold_device_lens`). Plan D13's reconnect
rule covered only the dead-wire backstop, not the failure the pull marked —
a sibling path the hold's withdrawal never reached.

**Fix** — two halves in `lpa-studio-core`, no web code:

- `StudioActor::run_refresh_tick` folds the device inputs queued during a pull
  the board did not answer (a transport failure, a timeout, an error) before
  it judges the pull: `fold_device_inputs_heard_during_the_pull`, over the new
  `CommandReceiver::take_matching`. Device inputs already go ahead of a
  batch's actions in `process_batch`. This gives them the same precedence over
  the pull's verdict.
- `StudioController::record_passive_refresh_failure` withdraws the pull's sync
  failure while the lens's link is reconnecting (a stall or a reset, within
  `LENS_RECONNECT_GRACE`), the way `hold_device_lens` withdraws it for a link
  that is gone. `mark_passive_project_refresh_failed` does not mark one then.
  The curtain is the existing one, with the existing words ("… stopped
  responding. Attempting to reconnect.").

A request that times out on a link still answering underneath has no stall
note, so it is still the project's error, and it still shows.

**Regression coverage** — `studio_device_e2e_tests::ble_drop_tests`, both
through a real `StudioActor` over the Bluetooth bench (the lens's tap feeds
the actor's queue, as in the page):
`a_bluetooth_board_out_of_range_shows_reconnecting_not_a_transport_error`
(a pull in flight, the link stalls, the pull times out; the batch's one view
shows the curtain and no sync issue; red without the fix) and
`a_request_unanswered_on_a_healthy_bluetooth_link_still_shows_its_error`
(the guard: no stall, the error shows, no curtain).
`studio_view_channel::take_matching_takes_in_order_and_leaves_the_rest_in_theirs`
covers the queue half.

**Lesson** — the lens's tap is the pump's stand-in while the editor holds the
wire, but it is a deferred one: what it hears lands a batch later. Any verdict
the actor draws from a conversation's outcome (failed, timed out, dead wire)
should be drawn after folding what the link said during that conversation.
Otherwise the verdict is made on link health measured before the conversation
began.
