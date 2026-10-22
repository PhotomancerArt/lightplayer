# The S3's Bluetooth desk walk (S3 radio plan M3, AC4 and AC6)

2026-10-21. Agent-run, no person at the desk.

- **Board:** FS3 `fixture-s3`, XIAO ESP32-S3 Plus (16 MB), MAC
  `D8:3B:DA:47:29:70`, **antenna attached**, powered from USB.
- **Central:** Mac Chrome 155.0.8059.86 on macOS 26.5.2 (an M2 Max), a scratch
  profile driven over CDP; the Web Bluetooth chooser answered with
  `DeviceAccess.selectPrompt` (`spikes/ble-lab/scripts/cdp-central.mjs` and
  `scripts/emu/studio-driver.mjs`).
- **Distance:** desk distance, the board on the same desk as the Mac; not
  measured. FS3's own Wi‑Fi scan heard 7 access points from −47 to −77 dBm.
- **Studio:** `just studio-dev` from this branch (`127.0.0.1`, this worktree's
  port), nothing shimmed: no `?ble=emu`, no `?emu=`.
- **Firmware:** `fw-esp32s3` split image, `esp32s3,server,float-f32,ble` at
  `c5b8d4c01` plus this PR's two fixes, flashed at `0x10000` over USB. The
  board's console was read over USB throughout (`lp-cli link capture`), so the
  board's own words back every claim below.

This is Mac Chrome on silicon, not a phone. Bluefy and Android are not
measured here; the milestone owes neither.

## Before the walk

| check | result |
|---|---|
| FS3 hears the room | 7 APs, strongest −47 dBm (ch 11), weakest −77 dBm; well above −87 |
| Chrome's unfiltered chooser lists the board | `LP-2970` among the desk's other boards (`LP-b48c`, `LP-8098`, `LP-5490`) |

## AC4: Studio over Bluetooth

| step | result | the board's words |
|---|---|---|
| connect (Connect a board › Bluetooth, the real chooser) | ✓ 1.9 s to pick | `link…: connected`, `notifications on — link open (ATT MTU 247, frames 180 B + 8 …)`, `session 0 up`, `replies are now packed` |
| identify | ✓ the card: XIAO ESP32-S3 Plus, Bluetooth connected, its switch locked on over itself, "Firmware updates need USB" | `hello · proto 44 · seeed/xiao-esp32-s3-plus` |
| push (the empty board's Add a project, Peach (1D)) | ✓ "project sent to studio — the board is running studio" | `Project loaded: studio`; two shaders JIT-compiled on the board in 51 and 46 ms; `startup_project set to studio` |
| card: Connect, turn a control, Done | ✓ the board's panel on the card; brightness turned | |
| the card's picture | ✓ moving, "Shown two or three times a second over Bluetooth" | |
| edit (open in the editor, turn a knob) | ✓ the card's write read held in a new session (the board's word); the editor's knob 1 → 8 | |
| Play, turn a knob | ✓ 1 → 8 | |
| board reset → reconnect (reboot under Play) | ✓ the page stayed on Play behind Reconnecting (strip 5.2 s after the request) and was back 9.0 s after it; the same knob turned | `RECOVERY boot: cause=software-reset level=green`, the project reloaded, `link1: connected` |
| a locked board drops an unauthenticated link | ✓ at 10 s; Studio reconnected | `radio link link2: no login within 10 s — closing`, `disconnected, reason=0x16` |
| unlock a locked board (typed password; a remembered key) | ✓ both, after the fix below | |

The walk's steps are scripted, the phone walk's steps
(`scripts/phone/walk-ble-phone.mjs`) against Mac Chrome; the push step waits
for Studio's "the board is running" (a real Bluetooth link does not stream the
board's console, so the emulated walk's "Project loaded" never appears there).
The reboot was a USB `--request reboot`: `board power-cycle fixture-s3`
switched hub `1-1.3.1.3` port 3 and the board kept running (uptime unbroken), so
the desk's switch for FS3 does not reach its VBUS (a `board` registry matter,
outside this repo).

One page console line during the reboot: `project sync failed: transport
error: … bluetooth link lost: a write failed`. It is the drop itself; no
request's timeout reached the page.

### Defects the walk found, fixed in this PR

1. **The S3 stopped advertising after its first connection**
   (`docs/defects/2026-10-21-the-s3-stops-advertising-after-its-first-connection.md`).
   BTDM counts the advertiser against `max_connections`; it is now the slots
   plus one (3). After the fix the board advertises beside a connection, and
   two links ran side by side (`slot 0`, `slot 1`) in the lock check.
2. **A locked S3 refused every login**
   (`docs/defects/2026-10-21-a-locked-s3-refuses-every-login.md`). The S3's
   server had no entropy source, so a password and a remembered key both
   failed; Studio's sheet sat on "Unlocking…". Now the hardware RNG, as on the
   C6. Studio's spinner on an answered-with-an-error login is filed to the
   auto-queue.

### Also seen

- The first fresh connections after a board restart failed in Chrome (the GATT
  link opened and closed with reason 0x13 before the radio link opened). A
  relaunched Chrome connected at once: the runbook's Mac Chrome wedge, not the
  board.
- A second Chrome profile's chooser listed nothing while the first ran; Studio
  on another origin (`localhost`) in the working profile gave a key-less page
  for the lock check instead.
- The card read "This board's project has an error. No project." on the empty
  board before the walk: the ledger's yellow from M3's own boot loop (fixed in
  #1251). Studio's Clear faults, over Bluetooth, put it back to green.
- A `--request reboot` that lands in the first frames after a flash's own reset
  counts as an interrupted startup load, and the next boot skips the project
  (`Boot: not loading a project: studio was interrupted …`). That is the
  try-and-recover rule as written; a clean reboot loaded it again.
- The Mac central sometimes refused the board's 15 ms interval
  (`Unacceptable Connection Parameters`) and kept 30 ms / 720 ms; the link ran
  either way.

## AC6: what Bluetooth costs on silicon

Project `/projects/studio` (Peach (1D), 2 shaders, one WS281x output on D10).
fps is the median of `[perf]` 5 s windows; frame times are each window's
buckets. The baseline is the same project on the default image of the same
commit **without** `ble` (radio off).

| state | window | fps (median) | vs radio off | p99 (worst window) | max | > 100 ms | > 1 s |
|---|---|---:|---:|---|---:|---:|---:|
| radio off | 125 s | 37 | — | ≤ 33 ms | 36 ms | 0 | 0 |
| advertising, no central (this PR's image) | 125 s | 36 | −2.7 % | ≤ 33 ms | 40 ms | 0 | 0 |
| advertising, no central (before the entropy fix) | 125 s | 35 | −5.4 % | ≤ 33 ms | 41 ms | 0 | 0 |
| connected, editing (39 knob turns over 135 s) | 135 s | 32 | −13.5 % | ≤ 75 ms | 108 ms | 1 | 0 |

The budget (`docs/adr/2026-10-06-radio-frame-rate-budget.md`): ≤ 10 % and p99 ≤
100 ms idle, ≤ 50 % and p99 ≤ 1 s editing. Both inside.

### Memory on silicon, connected

| | high water | of | headroom |
|---|---:|---:|---:|
| main stack (`[stack]`) | 29,312 B | 54,296 B | 24,984 B |
| radio region (`[radio-heap]`) | 26,084 B | 32,768 B | 6,684 B (0 B overflowed to main) |
| radio region, advertising only | 24,490 B | 32,768 B | |

`[ble] up: controller +33400 B, host + GATT +3520 B` (M3 measured +32,568 with
the old two-activity setting). One link's RAM is 7,208 B; the main heap with a
project loaded and two links open had about 200 KB in use and 60 KB free
(`[MEM] free=59272 … largest_free=42488`). The radio-off image's main stack is
51,888 B (high water 29,264 B).

## What the walk does not cover

Phones (Bluefy, Android); range; a power cut (the desk's switch does not reach
FS3, above); BLE beside Wi‑Fi (M4's walk); updates over Bluetooth (not on the
S3 yet).
