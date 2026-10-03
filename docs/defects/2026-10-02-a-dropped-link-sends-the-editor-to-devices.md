---
status: fixed
found: 2026-10-02      # how: report (Yona, iPhone + Bluefy at lightplayer.app, a XIAO C6 over Bluetooth)
fixed: this change
area: lpa-studio-core StudioController (drop_device_lens_if_wireless → lens_hold); lpa-studio-web web_app.rs open-ended route
class: state-conflation
related:
  - docs/adr/2026-09-24-ble-transport.md (S3: reconnect needs no gesture)
  - docs/defects/2026-09-23-bluefy-hidden-page-does-not-see-ble-drops.md
  - lp-app/lpa-studio-core/src/app/studio/lens_reconnect.rs (plan D13: stall/reset, the neighbouring case)
---
# A link that drops and comes straight back sends the editor to Devices

**Symptom** — Yona was in Play on an iPhone (Bluefy, lightplayer.app), on a
XIAO C6 over Bluetooth. The link dropped, as Bluetooth links on iOS often do:
Bluefy's phantom drop, a hidden page, the radio. Studio's Web Bluetooth
provider reconnected on its own within about a second, with no gesture
(`browser_ble.js`: `handleDrop` → `startReconnect`, first retry at 250 ms).
But the page had already left Play. It was on `/devices`, and he had to find
the board and open it again. His words: "it's unfortunate that the
connection dying sends us back to the device screen... seems we should be
able to reconnect in studio", and then "kicking the user back to the devices
tab on disconnect is jarring". A USB cable that is pulled out and pushed back
in did the same thing.

**Root cause** — one state stood for two facts. "This link is gone" and "this
editing session with this board is over" were the same event.

1. The provider reports a drop as a departure (`bluetooth link lost: …`,
   which `port_is_gone` reads), or the hotplug sweep stops routing the link.
2. `StudioController::drop_device_lens_if_wireless` closed the lens whenever
   its link was no longer routable or no longer open.
3. Closing the lens reset the project mirror, so the home view showed.
4. `web_app.rs`'s open-ended check read "home shown, no open in flight" as
   "the open ended" and replaced the route with `/devices`.

A reconnect always comes back on a **new** link (the old one was detached),
so nothing could have rejoined the session even if it had stayed. The
neighbouring case (a link that stalls or resets but is still there, plan
D13) had already been taught to ride it out with a "Reconnecting…" strip.
The case one step further out (the link actually goes away) had not.

**Fix** — a wire link (USB serial or Bluetooth; network links join them when
they land) that goes away under the editor now **holds** the session instead
of closing it (`lp-app/lpa-studio-core/src/app/studio/lens_hold.rs`). While
held:

- the project mirror stays;
- the dead wire client is dropped and the borrow given back;
- the page shows the same calm strip ("Reconnecting to *board*… The
  connection dropped. You stay right here…") over Play or the editor;
- passive pulls stop, and the actor looks for the board every 250 ms.

When the same board is Ready again (by uid, on whatever new link it came back
on; over Bluetooth that includes its unlock), `try_resume_held_lens` borrows
the new link and installs a fresh client into the **same** `RuntimeSession`
(`RuntimeSession::rebind_device`). The next tick pulls at once. Only a board
that stays away for 45 s of **awake** time ends the open, the old way (log
line, editor closed, route to `/devices`). The grace counts awake time
because iOS suspends a hidden page, and Studio can't reconnect while it's
suspended either. A gap longer than 5 s between two looks is added to the
start of the hold instead of being spent, so returning to Bluefy after a
minute in another app doesn't close the editor in the same instant the
reconnect lands.

Unchanged:

- Sims and in-tab emulated boards still close. Their link only goes away when
  they are powered off, and that is deliberate.
- A card verb that needs the wire still closes the editor first.
- An open still in flight is not held; its own failure page says what
  happened.

**Regression coverage** — `studio_device_e2e_tests.rs`:

- `unplugging_mid_lens_holds_the_editor_until_the_grace_runs_out`: hold,
  strip, no route to Devices, same session, nothing pulled, then the grace
  closes it and the card offers a way back.
- `a_replug_under_the_lens_resumes_the_same_editor_session`: same session id,
  a different link, the strip gone, and a pull that syncs.
- `a_port_that_dies_under_the_lens_holds_the_editor_through_the_tap`: the
  departure arrives through the lens io's tap only, with no hotplug edge.

The same tap test also asserts the held project shows no sync issue (the
pull that met the dead wire is withdrawn as the link's failure, not the
project's). `lens_hold.rs` unit tests cover the awake-time grace.

`just walk-drop-emu` (new) proves it in real Studio over the emulated USB
cable: connect, push, open, cable out and back in under the editor, Play,
cable out and back in under Play, then a knob turn on the resumed session.
Each pull must keep the route, show the strip and clear it.

`just walk-ble-emu` gained the Bluetooth twin under Play: `drop` (the radio
drops) and `phantom` (Bluefy's phantom drop, found when the page is shown
again). Those steps cannot run yet, because `?ble=emu` stopped identifying
boards at the lp-link USB cut-over
(`2026-10-02-the-ble-emu-polyfill-relays-lp-link-bytes-as-m-lines.md`).

**Lesson** — on a wireless or hot-pluggable transport, "the link went away"
is routine. It is not the end of what the user was doing. Keep the session
above the link and let it rebind, and end it only by a gesture or a bounded
wait. Measure that wait in time the page could actually act in.
