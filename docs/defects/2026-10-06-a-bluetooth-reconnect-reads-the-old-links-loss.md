---
status: fixed
found: 2026-10-05      # how: hardware-walk (PR #880's silicon desk check, bench XIAO C6 A0:F2:62:87:B4:8C) and e2e (walk-ble-emu `drop-back`)
fixed: this change
area: lpa-link device_link/browser_ble.rs (BrowserBleLink open) × providers/browser_ble/browser_ble.js (the session's error queue) × the effects layer's departure sweep
class: lifecycle-ownership
related:
  - docs/defects/2026-10-02-a-dropped-link-sends-the-editor-to-devices.md (the hold this defeated)
  - lp2025/2026-09-28-1445-ble-on-lp-link (data/desk-check-2026-10-05/README.md, findings 1 and 6)
---
# A Bluetooth reconnect reads the old link's loss and closes itself

**Symptom** — PR #880's desk check, Studio in Play on a locked XIAO C6 over
Bluetooth: after the board restarted (a power cut, and once a `--request
reboot` over USB), the page showed "Reconnecting…", and the radio came back
5.5 s later: the GATT tap shows the board's hello and Studio's packed opt-in
(`setEncoding`) on the new connection. Then nothing. Studio never sent a
`hello`, never logged in, and the board closed the link at its 10 s login
deadline ("no login within 10 s — closing", ten times in a row as Studio's
reconnect loop tried again). After 45 s the editor's hold ran out ("did not
come back within 45 s; the editor is closed") and the page went to
`/devices`. `walk-ble-emu`'s `drop-back` failed the same way against a
trusted emulated board, which ruled out the login: the card read "1
remembered board not connected" while the polyfill's connection was up.

**Root cause** — the drop's message outlived the link it ended. A GATT
disconnect is recorded once, as `bluetooth link lost: …`, in the JS
session's error queue, which the session keeps across connections. The
presence edge that comes with the drop runs the effects layer's departure
sweep, and that detached the model's link before the link's pump had read
the error (260 ms in the recording; the pump only runs when the effects
layer polls). So the loss waited in the session. The reconnect then
announced the board, the sweep attached a NEW `BrowserBleLink` on the same
session, and the new link's first pump drained the queue: it read the old
connection's loss as its own and closed at once (`LinkDetached` 1 ms after
`Opened`, in the recording). With the session present again, no further
edge came, so nothing attached the board again: its connection was up and
saying hello into a session no model link was reading. Studio's
`setEncoding` came from the session's own lp-link end, which starts serving
a connection the moment it exists. That is why the tap showed it and no
login.

Two layers each owned part of a link's life. The effects layer decides when
a link ends (the sweep). The JS session holds the words that say it ended.
Neither handed the other the boundary.

**Fix** — a link hears only its own loss. `BrowserBleLink`'s `Open`
discards a `bluetooth link lost` recorded before it opened (it can only be
an earlier link's) and still passes any other queued error on. The rest of
the resume path was already right: the access controller checks and logs in
on every new connection window, and the held editor resumes once the link
holds a tier.

**Regression coverage** —
`a_link_opened_on_the_reconnect_does_not_read_the_old_links_loss`
(`lpa-link/tests/browser_ble_conformance.rs`, `just lpa-link-browser-test`,
headless Firefox in CI): drop, detach the link undrained, reconnect, open a
new link on the session. It fails without the fix ("the new link read the
old link's loss"). Studio's half, host-only:
`a_bluetooth_board_that_restarts_under_the_editor_resumes_it` and
`a_locked_bluetooth_board_that_restarts_under_the_editor_logs_in_again_and_resumes`
(`lpa-studio-core`, `studio_device_e2e_tests/ble_drop_tests.rs`). The locked
board is the fake device's new untrusted link (`with_untrusted_link`): a
fresh server after the restart answers the editor's pull only after Studio
has logged in on the new link. These two pass with or without the fix,
because the host bench's links are fakes; they pin the resume and the
re-login the fix makes reachable. `walk-ble-emu` now holds the board away
for seconds (`drop`), blips the radio (`blip`) and rides Bluefy's phantom
drop, all on Play.

**Lesson** — a queue shared across a resource's generations must be cut at
the generation boundary by whoever starts the next generation. The session
already did this for reads (`clearFrames` on every connect, and the Rust
port's `generation` check) but not for errors, which were treated as
"whatever the next drainer sees". When an outer layer can end a consumer
without draining it, the next consumer inherits its mail.
