# The Wi‑Fi walk, run on an emulator: Studio over the LAN, two boards on one LAN

**Date** 2026-10-06 · **Plan** `lp2025/2026-10-05-1903-wifi-link-c6` (P13,
PR C) · **Command** `just walk-wifi-emu lan` · **Script**
`scripts/emu/walk-wifi-emu-lan.mjs` · **CI cell** `lp-cli/tests/emu_lan_link.rs`
(and P12's two-board lockstep test in `lp-emu-esp32c6`) · **Silicon twin**
G1, `desk-walk-wifi-c6.md` in the plan directory

> **STATUS: RUN, NOT PASSED.** Six of ten steps pass on the board's own
> words (W1, W2, W4, W5, W6, W7); W3 and W8 are **blocked by a known
> PR B finding** (the board's read/load gate refuses while a LAN link is
> open); W9 fails on a product symptom not yet explained, in both runs at
> the current tree (after the reset onto a new lease, nothing reaches the
> board through its forward); W10 passes when it runs before W9 (run 5:
> p50 2.365 / p90 3.682 frames). The walk is to be run again, whole, once
> PR B's heap fix is merged in (section 9).

Read **What the emulator does not cover** (section 6) before quoting
anything from the middle.

---

## 1. The setup

| What | Value |
|---|---|
| Boards | `c6-a`, `c6-b`: the packaged C6 image (`just studio-firmware-package-served`, `fw-esp32c6-merged.bin`), `kind=rom-up`, each its own flash file; MACs `02:4c:50:00:00:00` and `…:01` |
| The LAN | one virtual LAN, `home`, both boards on it (`lan=home`); the fixture the walk writes to `virtual_lan.toml` in the run directory |
| Access points | `lp-walk-net` (−50 dBm, secured), `lp-walk-guest` (−65 dBm, secured). Made-up names and passwords only |
| Reached by | each board's port forward, `lan:127.0.0.1:<port>`, read from the door's `/boards` |
| Studio | the release bundle, served by the walk on this worktree's stable slot, headless Chrome, `?lan=ws://127.0.0.1:<fwd a>/link,ws://127.0.0.1:<fwd b>/link` (no `?emu=`). W6/W7 use a **second** headless Chrome on `?emu=ws://<door>` (section 8) |
| The boards' consoles | each board's USB link held by `lp-cli link capture` through a TCP bridge to the door's `/bytes` (the decoded console: log records ride the link); c6-b's is handed to the W6/W7 page and ends there |
| Configuration | `lp-emu:esp32c6:t1+net=lan` (the door's `/boards` once the seam engaged; its first answer, before boot, says the bare `lp-emu:esp32c6:t1`) |
| `lp-emu` commit | run 4: `ae167ccf0`; run 5: `a84308c4b` (figures only since `ae167ccf0`); run 2: `fbad240d0` |
| Firmware commit | run 4/5: `fw-esp32c6 ae167ccf0-dirty` (wire proto 39, PR B's main merge with #986's OTA); run 2: `d65641584-dirty` (proto 38). "dirty" is other agents' work in flight in the tree, none of it under `lp-fw/` |

Runs, all in `target/walk-wifi-emu/` (not committed):

| Run | Tree | Directory | What it was for |
|---|---|---|---|
| 1 | `d65641584` | (overwritten) | first contact: the `[lan] link <id>` regex was wrong (`link1`, not a number); W2 stopped |
| 2 | `d65641584` | `lan-run2/` | W1–W10 end to end; W6/W7 on a USB card in the LAN page; W10's figure |
| 3 | `ae167ccf0` | (overwritten) | W1 stopped: a board booting ROM-up had not said hello yet (section 8) |
| 4 | `ae167ccf0` | `lan-run4/` | the run this report quotes for W1–W9 |
| 5 | `a84308c4b` | `lan-run5/` (and `lan/`) | W10 moved ahead of W9 (section 8): W1–W8 as run 4, W10 **pass**, W9 the same failure |

## 2. The rule every step follows

Each step waits for **the board's words**: its console (the capture), its
status answers (`lp-cli wifi status` over its USB door before the capture
takes it, over its LAN forward after), or the LAN's own view (`/boards`, the
LAN probe). Studio's words only say when to look. The one exception is W8:
the Radio node's message is a string only the firmware holds
(`fw-esp32-common/src/net/radio_rule.rs`, `RADIO_OFF_FOR_WIFI`), so the page
showing it is the board's answer relayed.

## 3. What each step showed

Run 4 (`ae167ccf0`) unless a row says otherwise; run 5 (`a84308c4b`,
figures-only on top) showed the same on W1–W9 (W3/W8's refusals: largest
block 13,4xx B / 62,031 B; W9: `.100` → `.103`, then silence). Shots are in
`target/walk-wifi-emu/lan-run4/shots/` and `…/lan-run5/shots/`.

| ID | Step | Board's words (the gate) | Showed | Result | Shot |
|---|---|---|---|---|---|
| W1 | Over each board's USB door, add `lp-walk-net` (`lp-cli wifi add serial:ws://…/board/<id>/bytes`, password on stdin) | each status `connected` with an address and its `lp-xxxx.local`; the two differ | c6-a `192.168.4.100` (`lp-0000.local`), c6-b `192.168.4.101` (`lp-0001.local`), both −50 dBm. c6-a needed one more ask: it had not said hello yet (section 8) | **pass** | none (before the page opens) |
| W2 | Studio with `?lan=` both forwards | each console: `[lan] link <id> … secure session opening`; each card shows its own board's MAC | both consoles: `[lan] link link1 from 192.168.4.1:49152: secure session opening (1024 B frames)`; both cards `Ready`, each with its own MAC, "Wi-Fi · 127.0.0.1:<forward>" leading the info line | **pass** | `wifi-lan-2-W2.png` |
| W3 | Push `Peach (1D)` to c6-a over the LAN, open it, turn the first knob | console `Project loaded`; heartbeat `frame_count` advances; the knob's value comes back from the board | console `Project loaded: studio`, frames 11674 → 12393 over one heartbeat (≈144 fps emulated), heartbeat lists `/projects/studio`. The editor never opened: every project read refused, `read refused: board memory busy (free 31456 B, largest block 13438 B; needs 40960 B free and a 16384 B block); retry shortly` (206 such lines on c6-a's console in run 4; 2,344 in run 2, which waited longer). The page says the same in its tree pane | **blocked by known PR B finding: the read/load gate refuses with a LAN link open** | `wifi-lan-3-W3.png` |
| W4 | c6-b's card while c6-a stays connected | no `[lan] link <id>: closed` on either console since W2; both cards `Ready` with their own MACs; c6-b answers over its forward at its W1 address | back on Devices in-app; no LAN link closed on either board; c6-b answered at `192.168.4.101` on its second LAN link slot | **pass** | `wifi-lan-4-W4.png` |
| W5 | The LAN probe asks for `_lightplayer._tcp` | two instances; each board's `mac=` in its TXT | `lp-0000` and `lp-0001`, TXT `mac=024c50000000` / `mac=024c50000001`, `proto=39`, `path=/link`, SRV port 80, A records `.100`/`.101`, answered in 53 ms of the door's wait | **pass** | `wifi-lan-5-W5.png` |
| W6 | c6-b: `lp-walk-guest` with a wrong password, from Studio over **USB** (section 8) | status over c6-b's forward: `last: wrongPassword` and back `connected` to `lp-walk-net` | `last: wrongPassword`, station `connected` `lp-walk-net` `192.168.4.101`; the in-row test: ✓ Looking for lp-walk-guest, ✗ **Checking the password** (crossed, error colour), · Getting an address; "Wrong password · it's saved, but won't connect until the password is changed." | **pass** | `wifi-lan-6-W6.png` |
| W7 | c6-b: `lp-walk-nowhere` (no access point has it), Other network…, over USB | status `last: notFound`, still `connected` to `lp-walk-net` | `last: notFound`, station `connected` `lp-walk-net` `192.168.4.101`; the panel says "Not in range" | **pass** | `wifi-lan-7-W7.png` |
| W8 | `lp-cli upload projects/test/button-sign lan:<fwd a>` | console `Project loaded`; frames advance; heartbeat lists `button-sign`; the Radio message on the console or in the editor | the board stopped c6-a's project (`stop_all_projects before: 6324 B free … after: 124100 B free`) and refused the load: `load_project: load refused: heap headroom too low (largest free block 62091 B < 65536 B); power-cycle the device or load a smaller project`. The Radio rule was never reached | **blocked by known PR B finding: the read/load gate refuses with a LAN link open** | `wifi-lan-8-W8.png` |
| W9 | c6-a: `renumber` then `reset` on its control channel | console `[wifi] address <new>` ≠ W1's; a new `[lan] link … secure session opening` after it; status over the SAME forward `connected` at the new address | `ok renumber lan=home board=c6-a (its next DHCP lease is a different address)`, then the reset (`[RECOVERY] boot: cause=user-reset`); the board rejoined: `[wifi] trying lp-walk-net`, `associated with lp-walk-net in 2 ms`, `[wifi] address 192.168.4.103`, `[mdns] answering for lp-0000.local at 192.168.4.103`, `/boards` `address` `192.168.4.103`. **Then nothing reached it through its forward**: no `[lan]` line of any kind on its console for the rest of the run (≈20 min), Studio's card went "quiet", and W10's `link rtt` over the same forward did not finish in 900 s | **fail (product, open)** — section 9 | `wifi-lan-9-W9.png` |
| W10 | `lp-cli link rtt lan:<fwd a> --count 40` | `request_rtt_frames` p50 / p90, `link_resets` 0 | run 5 (before W9; board idle, nothing loaded): 40/40 requests, p50 **2.365** / p90 **3.682** frames, `link_resets` 0. Run 2 the same within noise (2.376 / 3.575). Run 4, after W9: did not finish in 900 s | **pass** (runs 2, 5); run 4 fail (W9's after-effect) | `lan-run5/shots/wifi-lan-9-W10.png` |

### 3.1 W10's two frame figures

`lp-cli link rtt` multiplies a **wall-clock** round trip by the board's
**emulated** frame rate (frames over its heartbeats' uptime). On an emulator
those clocks disagree, so the walk also states the same round trips in
frames the board drew **per wall second** (from the heartbeats' host arrival
times). Neither is a silicon number, and neither is a gate.

Both runs: the board **idle with no project loaded** (W8's refused load had
stopped W3's), so the frame is the empty render loop's, ≈927 emulated fps.

| Run | Configuration, `lp-emu` | Reading | p50 | p90 | Frame rate used |
|---|---|---|---|---|---|
| 5 | `lp-emu:esp32c6:t1+net=lan`, `a84308c4b`, via `lan:127.0.0.1:49671` | `request_rtt_frames` (board fps) | **2.365** | **3.682** | `idle_fps` 926.7 (emulated) |
| 5 | same | frames per wall second | 6.99 | 10.882 | 2738.9 per wall second |
| 2 | `lp-emu:esp32c6:t1+net=lan`, `fbad240d0`, via `lan:127.0.0.1:61025` | `request_rtt_frames` (board fps) | 2.376 | 3.575 | `idle_fps` 927.7 (emulated) |
| 2 | same | frames per wall second | 6.235 | 9.382 | 2434.5 per wall second |

Run 5 in full (board fps): n 40, min 2.226, p10 2.32, p99 8.733, max 8.733,
mean 2.952 frames; `link_resets` 0. The emulator ran ≈2.6–3.0× faster than
the board's own clock, so a wall second holds more of its frames. These are
frames of a loop that renders nothing; a project-bearing figure
(`Peach (1D)` renders at ≈144–151 emulated fps) needs the upload steps
unblocked first.

## 4. Screenshots

At 2×, in `target/walk-wifi-emu/lan-run4/shots/` (run 4:
`wifi-lan-2-W2.png` … `wifi-lan-10-W10.png`) and
`target/walk-wifi-emu/lan-run5/shots/` (run 5, W10 before W9:
`wifi-lan-9-W10.png`, `wifi-lan-10-W9.png`). W1 runs before Studio opens, so
it has none. W6/W7's shots are of the second (USB) page. Not committed:
copy the set into the ship report from those directories.

## 5. The run directory

`target/walk-wifi-emu/lan/` (each run overwrites it; runs 2 and 4 were
copied to `lan-run2/` and `lan-run4/`):

| File | What |
|---|---|
| `walk-wifi-emu-lan.json` | the summary: per step, ok, note, `seen` (what a step learned before it failed), the device-event records it produced, how many console lines each board wrote |
| `walk.jsonl` | every device-event record both Studio pages streamed (`?record=`) |
| `console/<id>.link.log` | each board's decoded console (`lp-cli link capture`); c6-b's ends at W6 |
| `console/<id>.console.log` | the door's raw USB console files |
| `serve.log` | `lp-cli emu serve`'s own output |
| `virtual_lan.toml` | the fixture |
| `rtt-lan.json`, `rtt-lan.console.log` | W10's full report |
| `shots/` | the screenshots |

## 6. What the emulator does not cover

The network seam answers **above the radio**: frames in and out of an
Ethernet II segment, scans, joins and events. Everything below that line is
absent, and a pass here says nothing about it:

- **The radio.** No air, no channels, no association handshake, no
  WPA2 4-way handshake: a password is compared, not negotiated
  (`associated with lp-walk-net in 2 ms` is the model, not a measurement).
- **Signal.** A signal strength is a configured number in the fixture, not
  a measurement; nothing fades, nothing drops out of range on its own.
- **Airtime and throughput.** No contention, no retransmission, no rate
  adaptation: a frame arrives whole or not at all. W10's figures are a
  host socket and an emulated board, not a radio link.
- **Coexistence.** Wi‑Fi with BLE and ESP-NOW on one radio is not modelled;
  W8 was to prove the *rule* (the Radio node says why it is off), not the
  radio sharing it guards — and was blocked before it got there.
- **The Wi‑Fi driver's heap and timing.** With the seam answering, the
  esp-radio blob's join allocations never happen (plan A7), so `HEAP_RADIO`,
  the driver's stack and its timing are not exercised. G1's N6–N8 are the
  only figures for them. The heap the emulated board *does* run short of in
  W3/W8 is main heap the seam path shares with silicon (the LAN link's own
  cost), which is why that finding transfers.
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
the silicon column. A difference on something the emulator **models** (the
join policy, the status the board reports, the link, the server, mDNS
answers) is a fidelity defect to file under `docs/defects/`; a difference on
something in section 6 is expected and says so.

| G1 | Emulated (this walk) | Silicon (G1) | Same? | If not |
|---|---|---|---|---|
| Made-up network → `notFound` (pre-check) | W7: `last: notFound`, still connected to `lp-walk-net` | `failed notFound`, `last: notFound`, within one scan | same outcome (`notFound`); the emulated board, already joined, stayed on its network, which the pre-check (nothing joined) could not show | |
| N1 association → address | not comparable: emulated time, no association | _(G1)_ | n/a | |
| N2 request p50 / p90 over Wi‑Fi (frames) | W10 run 2: 2.376 / 3.575 frames at board fps, idle board, nothing loaded | _(G1)_ | _(after G1; compare in frames of the same project)_ | |
| N4 fps joined idle | 927.7 emulated fps with nothing loaded; ≈144–151 with `Peach (1D)` (W3's heartbeats) | _(G1)_ | _(G1)_ | |
| N12 `.local` in Chrome | not covered (section 6) | _(G1)_ | n/a | |
| Wrong password → `wrongPassword`, back on the good network | W6: `last: wrongPassword`, back `connected`; in-row test crossed at "Checking the password" | _(G1 W2)_ | _(G1)_ | |
| `_lightplayer._tcp` lists the board | W5: both boards, each `mac=` and `proto=39` | _(G1 W5)_ | _(G1)_ | |
| Radio node message (AC5) | W8 blocked (PR B finding) | _(G1 W6)_ | _(G1)_ | |

## 8. Deviations from P13 as planned, and changes to the walk

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
- **W6 and W7 run from Studio over USB, in a second headless Chrome.** As
  built (P07), a board reached on the LAN wears no Connections group, so its
  card has no Wi‑Fi row (`device_roster_card.rs`: "the network card is
  M8's"); the plan allows "Studio, over USB or LAN". The walk hands c6-b's
  USB door from its console capture to a second page on `?emu=<door>`, picks
  c6-b in the shim's chooser, and closes that page after W7. A second
  browser rather than `?emu=` on the LAN page because the shim holds every
  board's control channel while its page is open (run 2: W9's `renumber`
  was refused, "c6-a's control channel refused the connection"). c6-b's
  evidence from W6 on is its status over its forward; its console capture
  ends there.
- **W4 no longer opens a Wi‑Fi panel** (there is none on a LAN card): it
  checks both cards `Ready` with their own MACs, no LAN link closed, and
  c6-b's answer over its forward.
- **W10 runs before W9** (run 5 on): W9's reset left c6-a unreachable
  through its forward (section 9), and W10 should measure a link, not that.
- **W1 asks again when a board has not said hello yet.** Since #986's OTA
  merge a C6 checks its 1.8 MB engine's digest before it answers
  (`[OTA] engine guard: … matches its digest (1111 ms)`), and lp-cli's
  readiness gave up on a ROM-up board still booting ("device did not become
  ready: timed out waiting for the device hello"). Only that refusal is
  retried, up to four times.
- **A failed step no longer stops the walk** (W1 and W2 still do: nothing
  after them can run without the boards joined and the page holding both
  links). The summary keeps each step's `seen`, so a failed W3 still records
  the board's `Project loaded` and frame counts.
- Reconciled `// ASSUMES:` lines, now "As built:": the firmware's log lines
  (the LAN link id is `link1`, a `LinkId`'s Display, not a number — the
  first run's regex missed it), `serial:ws` opens without a reset (no
  control lines on the bytes socket), the LAN card's `Ready` + MAC + "Wi-Fi ·
  <forward>" line, and the configuration label is read after W1 (the door's
  first answer predates the seam engaging). The one `ASSUMES` left is W8's
  (where the editor draws the Radio node's reason), which no run has reached.

## 9. Product findings, with the board's evidence

1. **The read/load gate refuses while a LAN link is open (known, PR B's).**
   W3: `read refused: board memory busy (free 31456 B, largest block 13438 B;
   needs 40960 B free and a 16384 B block); retry shortly`, every project
   read for the rest of the run; Studio's editor sits at "Syncing project…".
   W8: `load refused: heap headroom too low (largest free block 62091 B <
   65536 B)`, right after `stop_all_projects before: 6324 B free … after:
   124100 B free`.
   Heartbeats: 158,948 B free / 79,667 B largest at 5 s, 31,996 / 13,373 at
   25 s with `Peach (1D)` loaded and Studio's LAN link open. Diagnosed by
   the director as PR B's: one open secure LAN link costs ≈25 KB of main
   heap and each first link strands ≈1 KB mid-heap. Not patched here.
2. **After `renumber` + `reset`, nothing reaches c6-a through its forward
   (open).** The board rejoined at a new address (`.100` → `.103`, the door's
   `/boards` agrees) and served its first frame, but logged no `[lan]` line
   again: Studio never relinked, and `lp-cli link rtt` over the same forward
   did not finish in 900 s. Run 5 the same (`.100` → `.103`, then no
   `[lan]` line; heartbeats after it 61,092 B free / 38,196 B largest). Not
   reproduced in isolation: one board on the
   same fixture (scratch `repro-renumber.mjs`), `renumber` + `reset`, then
   `wifi status` over the same forward answered at the new address — with
   nothing else on the forward, with a WebSocket held on it across the
   reset, and with `peach-1d` loaded over USB first. What the walk had that
   the repro did not: a second board on the LAN, Studio's established secure
   session (not a bare socket) at the reset, the console capture holding the
   USB link through the reset, and a board coming back with a startup
   project auto-loaded (61,092 B free / 38,196 B largest in its heartbeats
   after the reset). The heap state makes finding 1 a suspect, unproven.
3. **Studio's Wi‑Fi panel can open with an empty Nearby list for good
   (Studio, intermittent).** Run 2: "Connect to a network" showed only
   "Other network…"; the records show two `Network/Scan` commands and **no
   `wifi.scan` request sent**, both while the panel's own `wifi.status` read
   was in flight (`network_controller.rs`: a scan asked for while `reading`
   returns without asking again), while `lp-cli wifi scan` over c6-b's
   forward heard both networks at the same moment. Run 4 did not hit it. The
   walk now presses the panel's own "refresh" once if the list stays empty
   for 30 s, and records `scanNeededRefresh`.
4. **A LAN card's preview says "No live picture over Bluetooth"** (Studio,
   copy): the W3/W9 shots, a board reached over Wi‑Fi. The transport chip in
   the editor's header shows the USB glyph for it too.

Re-run when PR B's heap fix is in: the whole walk (`just walk-wifi-emu lan`),
which re-runs W3 and W8, gives W10 a project-bearing figure, and shows
whether finding 2 goes with finding 1.

## 10. `just walk-no-board`, unchanged by the LAN

Run with `--serve-release` (the release bundle and packaged firmware above,
no dev server started), at `ae167ccf0`: **6/6 pass** — flash → connect →
identify → upload (`Project loaded`; "project sent to studio — the board is
running studio") → detach → reattach (`Ready`). Directory
`target/walk-no-board/`.
