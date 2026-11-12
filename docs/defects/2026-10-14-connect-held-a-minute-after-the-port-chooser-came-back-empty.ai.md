---
status: fixed
found: 2026-10-10      # how: the connected feel gate's preview restart (?emu=tab, an offline USB board); finding 7
fixed: this change (core); the polyfill's empty chooser stays a fidelity gap, below
area: lpa-studio-core `connect_device` (the held Connect) × the port chooser's `GrantAnswered` × `public/lpa-link/virtual_serial.js`
class: untested-path
related:
  - lp2025/2026-10-08-2330-connected-in-the-card (the connected feel gate, finding 7)
---
# Connect held a minute after the port chooser came back empty

**Symptom** — Connect on an offline USB board under `?emu=tab` left the card
at "Connecting…" for the full 60 s grace. No chooser was drawn, and the USB
square under Connect a board seemed blocked until the Connect gave up.

**Root cause** — Connect on an offline board on its cable opens the port
chooser (`Reconnect` → `RequestUsbGrant`) and holds the intent until the
board is ready. The hold never read the chooser's answer: a chooser closed
with nothing picked (`GrantAnswered { Dismissed }`), or one that failed,
changed nothing, and the hold ran to its grace. Under `?emu=tab` the only
board was detached, so the polyfill's `requestPort` found no candidate and
answered `NotFoundError` at once without drawing (`virtual_serial.js`,
`pickPort`), which Studio reads as Dismissed. The USB square was never
blocked: it met the same empty list. Real Chrome draws an empty chooser and
waits for Cancel, and a request made outside a click is refused
(`SecurityError` → Failed); either way the hold ignored it.

**Fix** — a Connect that opened the port chooser waits on its answer too
(`ConnectHold::chooser`, `note_connect_chooser`): a board picked keeps the
hold; nothing picked lets it go at once, with no failure (the user said no,
and the card is back at Connect); a chooser that failed lets it go and says
why on the card. A Bluetooth board's chooser keeps the grace: the browser
also watches its granted devices, so the board can come back by itself.

**Not fixed (fidelity)** — the polyfill still refuses at once with no
candidates instead of drawing an empty chooser that waits for Cancel. With
the core fix the card reads the same either way; a walk that wants Chrome's
empty-chooser timing needs the polyfill to draw it.

**Regression coverage** —
`connected_tests::a_chooser_closed_with_nothing_picked_lets_the_connect_go_at_once`.
`a_connect_that_never_lands_gives_up_after_the_grace` now runs on the
Bluetooth bench, where an out-of-range board still waits out its grace.

**Lesson** — a wait that starts by asking the user must end on the user's
answer, not only on its own deadline.
