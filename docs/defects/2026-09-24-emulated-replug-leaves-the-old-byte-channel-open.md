---
status: fixed
found: 2026-09-24      # how: walking PR #818 on the emulated C6; also reproduced on main a14fa77f0
fixed: this change
area: lpa-studio-web public/lpa-link/virtual_serial.js (`VirtualSerialPort.unplug` / `markDead`) × emulator_port.js (one `EmulatorPort` shared by every port generation)
class: fidelity
related:
  - docs/adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md
  - docs/reports/2026-09-10-studio-walk-with-no-board.md
  - docs/defects/2026-09-24-a-departed-page-kept-the-doors-boards.md
---
# An emulated replug left the dead port's byte channel open, so the card sat at "Attached — not listening" forever

**Symptom** — open a project on c6-a
(`/p/<slug>-<uid>?on=mac:a0:f2:62:87:b4:8c`), then run
`window.__lpEmuSerial.bus.detach("c6-a")` about 4 s later. Studio correctly
drops to `/devices`. Then run `bus.attach("c6-a")`. The card reads
"Attached — not listening" and "No response — try flashing firmware", and
offers Flash firmware. Meanwhile its terminal shows `wireheartbeat · studio`
and `[perf] frame=… fps=110`: the board is up and talking. The card never
returns to Ready.

`just walk-no-board` has reported the same thing since M3
("replug → Attached, not listening"). It was filed as a product question
about Studio not reopening a port it had adopted. It was not a product
question.

**Root cause** — the device journal names it. After the replug, identify's
open fails:

    Input(Event(Link { link: LinkId(10), event: Error("open failed: NetworkError:
      Failed to open serial port: emulated board c6-a is already open in this page") }))
    Note(ActivityEnded { kind: Identify, outcome: Failed { message: "no response from the device" } })

A replug mints a new `SerialPort` generation over the **same**
`EmulatorPort`: one board, one pair of sockets. `bus.detach()` sends the
emulator's `detach` verb and calls `port.unplug()`. `unplug()` errored the
open port's stream, the way Chrome does, but never closed the byte channel
underneath it. The emulator's `detach` does not drop the door's byte client
either: a cable and a socket are separate facts there (`lp-emu/esp/README.md`).
Studio could not close it: a dead port that has lost its stream answers
`close()` with "already closed". So `EmulatorPort._bytes` stayed set. The
new generation's `open()` refused, the identify ladder read the refusal as
silence, and the card asked for firmware. The terminal was still showing the
old socket's traffic.

On silicon an unplug takes the port away with it. The shim did not, so this
is a fidelity bug in the shim, not in Studio.

**Fix** — `VirtualSerialPort.unplug()` and `markDead()` release the byte
channel (`EmulatorPort.close()`) when the dead generation had it open. That
is the open the unplug took away. `close()` clears `_bytes` synchronously,
so the next generation's `open()` meets a free port.

**Regression tests** —

- `lpa-link` conformance suite:
  `a_port_open_across_a_replug_reopens_on_the_new_generation` (scripted
  door, run in CI). Open, replug, then `openPort` on the adopted session
  must succeed and read.
- `lpa-studio-core` e2e bench:
  `a_replug_under_the_lens_comes_back_ready_and_opens_again`. It pins
  Studio's half of the same walk: lens open, departure on the hotplug edge,
  replug on the same endpoint → Ready, and the project opens again.

**Evidence** (emulated, `lp-emu:esp32c6` via `lp-cli emu serve` at this
change's commit, headless Chrome, `studio-driver.mjs`, board c6-a running
PLAYFUL Choker): before, 15 s after `attach` the card reads "not listening",
with the journal above. After, the replugged link opens, identify succeeds
(`seeed/xiao-esp32-c6 · fw-esp32c6 …`), and the card is Ready 15 s later.
