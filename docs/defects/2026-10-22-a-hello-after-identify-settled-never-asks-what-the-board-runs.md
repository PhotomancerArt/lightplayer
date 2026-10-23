---
status: fixed
found: 2026-10-22      # how: emulator walk (walk-ota-ble-emu --s3, the S3 updates plan's P12a, #1263)
fixed: 2026-10-22      # lpa-devices: a hello after a hello-less verdict re-identifies
area: lp-app/lpa-devices device.rs (identify's verdict, the project ask) × a board that boots slower than 5 s
class: fixed-budget-over-variable-work
related:
  - docs/debt/emulated-s3-runs-forty-times-slower-than-real-time.md
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M7 P12a)
---
# A hello after identify settled never asks what the board runs

**Symptom** — `walk-ota-ble-emu --s3 --steps update`, and its USB twin
`walk-ota-emu --s3 --steps update`, connected Studio to an emulated ESP32-S3
and stalled at "waiting for the board to say what it runs". The card showed
the board's firmware and access and offered Update, but no project, and no
`push` or `edit`. Studio's terminal said `▸ Identifying`, then
`unrecognized firmware`. The board's hello came about 180 s later, after its
boot, its project load and its shader compile. The emulated S3 runs about
40× slower than real time with Bluetooth on and a project rendering.

**Cause** — identify settles after `identify_deadline_ms` (5 s of wall
time). It had heard boot lines and no hello, so it settled on
`Classification::Foreign { label: None }`: "unrecognized firmware". The
device's classification is live, so the hello that came later did make the
board a LightPlayer (its facts, its Update offer). But the one request that
learns what a board runs, `IdentifyActivity::ask_what_it_runs`
(`ListLoadedProjects`), is sent only when an identify **ends**, and none was
running. The only other source is the board's own heartbeat, every 5 s of
the board's time, which at 40× is minutes away. Until then the card had no
project and offered nothing to do with one. Any board slower than 5 s from
port-open to hello takes this path: a cold board compiling a big shader at
boot, a board behind a slow link, an emulated chip.

**Fix** — `Device::late_hello_reidentifies`. When an identify ends on an
open port without having heard a hello, the device remembers it. The first
hello heard later in the same window, with nothing running, spawns identify
again. That identify settles on the board's answer and asks what it runs.
It fires once per such verdict; any identify that starts clears it, and a
port another tab holds never sets it. The core test is
`a_hello_after_identify_settled_unrecognized_reidentifies_and_asks_what_it_runs`
(`lpa-devices/src/device.rs`): boot lines only, the 5 s verdict
"unrecognized firmware", then a hello 180 s later. The device re-identifies,
asks `ListLoadedProjects` and ends `Ready`, a recognized LightPlayer, with
no gesture. No text a person sees changed.
