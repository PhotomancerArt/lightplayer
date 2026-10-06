# The Wi‑Fi walk, run on an emulator: Studio over the LAN, two boards on one LAN

**Date** 2026-10-06 · **Plan** `lp2025/2026-10-05-1903-wifi-link-c6` (P13,
PR C) · **Command** `just walk-wifi-emu lan` · **Script**
`scripts/emu/walk-wifi-emu-lan.mjs` · **CI cell** `lp-cli/tests/emu_lan_link.rs`
(and P12's two-board lockstep test in `lp-emu-esp32c6`) · **Silicon twin**
G1, `desk-walk-wifi-c6.md` in the plan directory

> **STATUS: SKELETON.** The walk was written ahead of the hosts it drives
> (P12's `emu serve --lan`, the forward in `/boards`, the LAN probe's door
> endpoint, the `renumber` control verb). Nothing below the line "What each
> step showed" has been run. Every `_(fill)_` is filled from the run
> directory (`target/walk-wifi-emu/lan/`), and every `// ASSUMES:` in the
> script is reconciled with the code before the first run is quoted.

Read **What the emulator does not cover** before quoting anything from the
middle.

---

## 1. The setup

| What | Value |
|---|---|
| Boards | `c6-a`, `c6-b`: the packaged C6 image (`just studio-firmware-package-served`, `fw-esp32c6-merged.bin`), `kind=rom-up`, each its own flash file and MAC |
| The LAN | one virtual LAN, `home`, both boards on it (`lan=home`); the fixture the walk writes to `virtual_lan.toml` in the run directory |
| Access points | `lp-walk-net` (−50 dBm, secured), `lp-walk-guest` (−65 dBm, secured). Made-up names and passwords only |
| Reached by | each board's port forward, `lan:127.0.0.1:<port>`, read from the door's `/boards` |
| Studio | the release bundle, served by the walk on this worktree's stable slot, headless Chrome, `?lan=ws://127.0.0.1:<fwd a>/link,ws://127.0.0.1:<fwd b>/link` (no `?emu=`: the page holds no USB door) |
| The boards' consoles | each board's USB link held for the whole walk by `lp-cli link capture` through a TCP bridge to the door's `/bytes` (the decoded console: log records ride the link) |
| Configuration | _(fill: `/boards` → `configuration`, expected `lp-emu:esp32c6:t1+net=lan`)_ |
| `lp-emu` commit | _(fill: `walk-wifi-emu-lan.json` → `lpEmu`)_ |
| Firmware commit | _(fill: the card's `fw-esp32c6 <sha>`)_ |

## 2. The rule every step follows

Each step waits for **the board's words**: its console (the capture), its
status answers (`lp-cli wifi status` over its USB door before the capture
takes it, over its LAN forward after), or the LAN's own view (`/boards`, the
LAN probe). Studio's words only say when to look. The one exception is W8:
the Radio node's message is a string only the firmware holds
(`fw-esp32-common/src/net/radio_rule.rs`, `RADIO_OFF_FOR_WIFI`), so the page
showing it is the board's answer relayed.

## 3. What each step showed

| ID | Step | Board's words (the gate) | Showed | Shot |
|---|---|---|---|---|
| W1 | Over each board's USB door, add `lp-walk-net` (`lp-cli wifi add serial:ws://…/board/<id>/bytes`, password on stdin) | each status `connected` with an address and its `lp-xxxx.local`; the two differ | _(fill: c6-a ip/host · c6-b ip/host)_ | none (before the page opens) |
| W2 | Studio with `?lan=` both forwards | each console: `[lan] link N … secure session opening`; each card shows its own board's MAC | _(fill)_ | `wifi-lan-2-W2.png` |
| W3 | Push `Peach (1D)` to c6-a over the LAN, open it, turn the first knob | console `Project loaded`; heartbeat `frame_count` advances; the knob's value comes back from the board | _(fill: loaded, frame counts, knob before → after)_ | `wifi-lan-3-W3.png` |
| W4 | c6-b's card and its Wi‑Fi panel while c6-a stays connected | no `[lan] link N: closed` on either console since W2; c6-b answers over its forward at its W1 address | _(fill)_ | `wifi-lan-4-W4.png` |
| W5 | The LAN probe asks for `_lightplayer._tcp` | two instances; each board's `mac=` in its TXT | _(fill: instances, MACs)_ | `wifi-lan-5-W5.png` |
| W6 | c6-b: `lp-walk-guest` with a wrong password, from Studio over the LAN | console `[wifi] trying lp-walk-guest`; status `last: wrongPassword` and back `connected` to `lp-walk-net` | _(fill: station, last, in-row test crossed or interrupted)_ | `wifi-lan-6-W6.png` |
| W7 | c6-b: `lp-walk-nowhere` (no access point has it) | status `last: notFound`, still `connected` to `lp-walk-net` | _(fill)_ | `wifi-lan-7-W7.png` |
| W8 | `lp-cli upload projects/test/button-sign lan:<fwd a>` | console `Project loaded`; frames advance; heartbeat lists `button-sign`; the Radio message on the console or in the editor | _(fill: where the message showed)_ | `wifi-lan-8-W8.png` |
| W9 | c6-a: `renumber` then `reset` on its control channel | console `[wifi] address <new>` ≠ W1's; status over the SAME forward `connected` at the new address; a new `[lan] link … secure session opening` after it | _(fill: before → after)_ | `wifi-lan-9-W9.png` |
| W10 | `lp-cli link rtt lan:<fwd a> --count 40` | `request_rtt_frames` p50 / p90, `link_resets` 0 | _(fill: p50 / p90 frames, both readings below)_ | `wifi-lan-10-W10.png` |

### W10's two frame figures

`lp-cli link rtt` multiplies a **wall-clock** round trip by the board's
**emulated** frame rate (frames over its heartbeats' uptime). On an emulator
that runs slower than silicon those clocks disagree, so the walk also states
the same round trips in frames the board drew **per wall second** (from the
heartbeats' host arrival times). The second is what a person at the page
would count; neither is a silicon number, and neither is a gate.

| Reading | p50 | p90 | Frame rate used |
|---|---|---|---|
| `request_rtt_frames` (board fps) | _(fill)_ | _(fill)_ | _(fill: `idle_fps`)_ |
| frames per wall second | _(fill)_ | _(fill)_ | _(fill: `wallFps`)_ |

Configuration _(fill)_, `lp-emu` _(fill)_.

## 4. Screenshots

In `target/walk-wifi-emu/lan/shots/`, one per step from W2 on, at 2×:
`wifi-lan-2-W2.png` … `wifi-lan-10-W10.png`. W1 runs before Studio opens
(no page yet), so it has none. _(fill: copy the set into the ship
report; list any step whose shot is missing and why.)_

## 5. The run directory

`target/walk-wifi-emu/lan/`:

| File | What |
|---|---|
| `walk-wifi-emu-lan.json` | the summary: per step, ok, note, the device-event records it produced, how many console lines each board wrote |
| `walk.jsonl` | every device-event record Studio streamed (`?record=`) |
| `console/<id>.link.log` | each board's decoded console (`lp-cli link capture`) |
| `console/<id>.console.log`, `<id>.console-untaken.log` | the door's raw USB console files |
| `serve.log` | `lp-cli emu serve`'s own output |
| `virtual_lan.toml` | the fixture |
| `rtt-lan.json`, `rtt-lan.console.log` | W10's full report |
| `shots/` | the screenshots |

## 6. What the emulator does not cover

The network seam answers **above the radio**: frames in and out of an
Ethernet II segment, scans, joins and events. Everything below that line is
absent, and a pass here says nothing about it:

- **The radio.** No air, no channels, no association handshake, no
  WPA2 4-way handshake: a password is compared, not negotiated.
- **Signal.** A signal strength is a configured number in the fixture, not
  a measurement; nothing fades, nothing drops out of range on its own.
- **Airtime and throughput.** No contention, no retransmission, no rate
  adaptation: a frame arrives whole or not at all. W10's figures are a
  host socket and an emulated board, not a radio link.
- **Coexistence.** Wi‑Fi with BLE and ESP-NOW on one radio is not modelled;
  W8 proves the *rule* (the Radio node says why it is off), not the radio
  sharing it guards.
- **The Wi‑Fi driver's heap and timing.** With the seam answering, the
  esp-radio blob's join allocations never happen (plan A7), so `HEAP_RADIO`,
  the driver's stack and its timing are not exercised. G1's N6–N8 are the
  only figures for them.
- **Chrome's Local Network prompt.** The page is served from loopback and
  dials loopback, so Chrome never asks (P07). From
  `https://lightplayer.app` it would (roadmap M8).
- **`.local` in Chrome.** Studio dials `127.0.0.1:<forward>`, never a
  `.local` name; W5 asks the LAN probe, not the host's resolver. G1's N12 is
  the only answer.
- Also not covered: the warm-reset DMA placement (AC6), real DHCP servers'
  quirks, an internet uplink (the gateway has none), IPv6.

## 7. Differences from G1's silicon sheet

**G1 has not run yet.** The sheet (`desk-walk-wifi-c6.md`, N1–N12) is empty;
only its pre-walk checks ran on 2026-10-06 (boot, a 16-network scan, a
made-up network ending `failed notFound`, 10/10 warm resets with the station
searching), and none of those needs the house network. When G1 runs, fill
this table. A difference on something the emulator **models** (the join
policy, the status the board reports, the link, the server, mDNS answers) is
a fidelity defect to file under `docs/defects/`; a difference on something
in section 6 is expected and says so.

| G1 | Emulated (this walk) | Silicon (G1) | Same? | If not |
|---|---|---|---|---|
| Made-up network → `notFound` (pre-check) | W7 _(fill)_ | `failed notFound`, `last: notFound`, within one scan | _(fill)_ | |
| N1 association → address | not comparable: emulated time, no association | _(G1)_ | n/a | |
| N2 request p50 / p90 over Wi‑Fi (frames) | W10 _(fill)_ | _(G1)_ | _(fill)_ | |
| N4 fps joined idle | _(fill: heartbeats in W3)_ | _(G1)_ | _(fill)_ | |
| N12 `.local` in Chrome | not covered (section 6) | _(G1)_ | n/a | |
| Wrong password → `wrongPassword`, back on the good network | W6 _(fill)_ | _(G1 W2)_ | _(fill)_ | |
| `_lightplayer._tcp` lists the board | W5 _(fill)_ | _(G1 W5)_ | _(fill)_ | |
| Radio node message (AC5) | W8 _(fill)_ | _(G1 W6)_ | _(fill)_ | |

## 8. Deviations from P13 as planned

- **The recipe is a lane, not a new recipe.** `just walk-wifi-emu <lane>`
  already existed (Wi‑Fi M5's settings walk, lanes `usb|ble`); this walk is
  `just walk-wifi-emu lan`, in its own script, which the M5 script hands
  `lan` to.
- **It serves the release bundle itself** (as the M5 walk does) instead of
  attaching to a running dev server: no listener is ever adopted, and it
  runs as one foreground command.
- **The boards' consoles come from `lp-cli link capture` over a TCP bridge
  to the door**, held from the end of W1: the door's own console file holds
  raw bytes, and since wire proto 30 a board's log records ride its link and
  leave only once a host has brought it up.
- **W6 and W7 run from Studio over the LAN.** A just-added network is tried
  at once, even while joined (P02), so the board leaves the LAN while it
  tries: Studio's LAN link to it drops and reconnects. The walk gates on the
  board's status after it is back and reports whether the in-row test
  survived the drop (`inRowTest`).
