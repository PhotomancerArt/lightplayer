---
status: fixed
found: 2026-09-24      # how: walking PR #818 on the emulated C6 (`just studio-dev-emu`, headless Chrome)
fixed: this change
area: lpa-studio-web public/lpa-link (virtual_serial.js `VirtualSerial.dispose`, emulator_port.js `EmulatorPort.dispose`) × index.html's `?emu=` install seam
class: assumed-context
related:
  - docs/adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md
  - docs/defects/2026-09-24-emulated-replug-leaves-the-old-byte-channel-open.md
---
# A page that left `?emu=` kept the door's boards, so the next load fell back to the real `navigator.serial`

**Symptom** — while walking PR #818: a Studio page that had OPENED c6-a's
port was navigated away (to `about:blank`, or reloaded). For the next few
minutes every new `?emu=` load failed to install the shim:

    [emu] the shim over ws://127.0.0.1:26607 did not install: NetworkError:
    emulated board c6-a: control channel refused (a second client is refused
    with 409 — one application per port)

Studio then booted with no emulated boards, and nothing on the page said
why, so it looked like a Studio bug. The door's `serve.log` showed the old
page's connections logging `TcpHost: client closed` only minutes later. A
page that never opened a port reloaded fine.

Reproduced 2026-09-25 with `studio-driver.mjs`: Ready on c6-a, navigate to
`about:blank`, come back → `did not install: NetworkError: emulated board
c6-b: control channel refused`. The door logged the old sockets closing only
when the whole headless browser exited.

**Root cause** — the `pagehide` handler already existed to hand the boards
back. Two `await`s made sure it never finished:

1. `VirtualSerial.dispose()` disposed its boards **one after another**
   (`for … await emulator.dispose()`).
2. `EmulatorPort.dispose()` closed its byte socket and **awaited the close
   handshake** before it closed its control socket.

A page that is leaving (and one Chrome freezes into the back/forward cache)
does not run the continuation after a close handshake. So c6-a's byte socket
was closed, and nothing after it ever ran. c6-a's control socket stayed
open, and so did c6-b's and c6-c's. Those are the door's one-client claims,
and they stay held until Chrome finally tears the frozen page down. With no
port open there was no byte socket to wait on, and each board's control
close went out before the first await, which is why a page that never opened
a port was fine.

The door was not at fault. Its pumps hear a Close as soon as one is sent,
and a unit test that stopped reading and then sent Close passed against the
unchanged pump.

**Fix** —

- `VirtualSerial.dispose()` and `EmulatorPort.dispose()` **start every
  close in the same turn** (`Promise.all`), so nothing depends on a
  continuation running in a page that is going away.
- `VirtualSerial.load()` disposes the boards it already connected before
  rethrowing a refusal. A bus that never installs must not keep control
  channels, or its own next attempt meets a 409.
- `index.html` retries a 409 at install for about 8 s (250 ms, doubling).
  A reload can reach the door a moment before the old page's Close does.
- When the shim still does not install, the page says so:
  `installFailureBanner` (`#lp-emu-banner[data-state=failed]`) names the
  door and the reason. For a 409 it says another tab holds a board; for no
  answer it asks whether `just studio-dev-emu` is still running. The page no
  longer falls silently back to the real `navigator.serial`.

**Evidence** (emulated, `lp-emu:esp32c6` via `lp-cli emu serve` at this
change's commit, headless Chrome, `studio-driver.mjs`):

- before: return after `about:blank` → `failed` (c6-b 409), and a reload →
  `failed`.
- after: return → `installed`, reload → `installed`. A second tab opened
  while the first holds the boards shows the held banner after about 8 s of
  retries, and gets the boards once the first tab leaves.

**Lesson** — cleanup that runs from `pagehide` must do all of its work
before the first `await`. Anything after an `await` there is best effort at
most, and in a page entering the bfcache it does not run at all.
