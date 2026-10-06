# The Wi‑Fi walk, run on an emulator: Studio over the LAN, two boards on one LAN

**Date** 2026-10-06 · **Plan** `lp2025/2026-10-05-1903-wifi-link-c6` (P13,
PR C) · **Command** `just walk-wifi-emu lan` · **Script**
`scripts/emu/walk-wifi-emu-lan.mjs` · **CI cell** `lp-cli/tests/emu_lan_link.rs`
(and the two-board lockstep cell, `lp-cli/tests/emu_lan_lockstep.rs`) · **Silicon twin**
G1, `desk-walk-wifi-c6.md` in the plan directory

> **STATUS: RUN AT THE SHIPPING TREE, NOT PASSED: 7/10 (run 9) and 6/10
> (run 10).** The tree is `df4ca2831`: PR B's heap fix, one LAN slot,
> lp-emu `201dc56a1`, no host pacing. Every step ran in both runs.
> W1, W2, W5, W6 and W7 pass in both. W3 passes in run 9 and W4 in run 10.
> The steps that fail do so for three reasons, all named in section 9.
> - **Finding 6 (the emulator's open fidelity defect, which also turns
>   into firmware behaviour).** The guest clock outruns a wall-clock host.
>   On a board running a project with Studio's LAN link open, that turns
>   into a 5 s reply deadline that blocks the server tick, and then a
>   watchdog reset loop. This is W4 in run 9; run 6 was the same, through
>   W8.
> - **Finding 1, again (PR B: firmware heap).** With `Peach (1D)` loaded
>   and one LAN link open, the largest block sits about 1 KB under the
>   read floor. This is W3 in run 10. After a stop, the load floor refuses
>   (W8 in runs 6 and 9).
> - **Finding 7 (side not yet known).** A Studio page reloaded right after
>   another LAN client let go of a board opens a link the board sees
>   (`secure session opening`) but never sends a frame on it. This is W9
>   in run 9 and the page half of W8 in run 10.
>
> The walk's own wall-clock deadlines also tripped in run 10 (W9, W10) on a
> box at load average 73.
>
> With the held-back host-pacing patch in the tree (run 8,
> `201dc56a1+dirty`), the same script passed **10/10**: W3, W4 and W8
> pass, W8 with the Radio node's own message, and W10 with a project
> rendering. That patch does not ship (section 9, finding 6).

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
| The boards' consoles | each board's USB link held by `lp-cli link capture` through a TCP bridge to the door's `/bytes` (the decoded console: log records ride the link). It is let go and taken back when the walk asks a board over USB (W4, W9), and the console continues in `console/<id>.link.2.log`. c6-b's is handed to the W6/W7 page and ends there |
| One LAN link per board | since #989 a C6 holds **one** LAN link (`LAN_LINK_SLOTS` = 1). A second dial is closed with WebSocket 1013, logged as `[lan] every LAN link is in use: a new one was told to try again later`. So the walk never dials a board's forward while the Studio page holds that board: status reads go over USB (W4, W9), and the LAN tools (W6/W7's status reads, W8's upload, W10's `link rtt`) run with the LAN page at `about:blank`, each console showing its link closed |
| Configuration | `lp-emu:esp32c6:t1+net=lan` (the door's `/boards` once the seam engaged; its first answer, before boot, says the bare `lp-emu:esp32c6:t1`) |
| `lp-emu` commit | runs 6, 9, 10: `201dc56a1` (W9's forward fix; `lp-emu/` unchanged since). Runs 7, 8: `201dc56a1+dirty`, the host-pacing patch that was held back (section 9, finding 6; diff fingerprints `da41d0f52ced` and `89fbfe2eef72`). Run 4: `ae167ccf0`; run 5: `a84308c4b`; run 2: `fbad240d0` |
| Firmware commit | runs 6–10: `fw-esp32c6 f3feec073-dirty-061842PT` (wire proto 39, #989's heap fix and one LAN slot; "dirty" is this walk's script only). HEAD then moved to `8e188d18f`, a main merge that touches `fw-esp32c6/src/ota/` (#994's hw SHA and engine window); no run here used that image. Runs 4/5: `ae167ccf0-dirty`; run 2: `d65641584-dirty` (proto 38) |
| Studio build | the release bundle built at `f3feec073`, with #989's latest Studio fixes (the scan owed after a status read, the LAN redial read as a departure, "Wi-Fi" in the card and header) |

Runs, all in `target/walk-wifi-emu/` (not committed):

| Run | Tree | Directory | What it was for |
|---|---|---|---|
| 1 | `d65641584` | (overwritten) | first contact: the `[lan] link <id>` regex was wrong (`link1`, not a number); W2 stopped |
| 2 | `d65641584` | `lan-run2/` | W1–W10 end to end; W6/W7 on a USB card in the LAN page; W10's figure |
| 3 | `ae167ccf0` | (overwritten) | W1 stopped: a board booting ROM-up had not said hello yet (section 8) |
| 4 | `ae167ccf0` | `lan-run4/` | the run this report quotes for W1–W9 |
| 5 | `a84308c4b` | `lan-run5/` (and `lan/`) | W10 moved ahead of W9 (section 8): W1–W8 as run 4, W10 **pass**, W9 the same failure |
| W9 re-run | `a84308c4b` + the forward fix | `pr993-w9/` | finding 2's fix: the W9 board gates pass |
| 6 | `f3feec073` (#989 merged), lp-emu `201dc56a1` | `lan-run6/` (and `lan-final/`) | first run with one LAN slot, the walk reworked for it. **7/10**: W3, W4 and W8 fail on the reply-deadline watchdog loop (finding 6); W9 **pass**, card `Ready` (finding 5 resolved) |
| 7 | the same + the held-back pacing patch (`201dc56a1+dirty`) | `lan-run7-wip/` | **8/10**: W3 and W4 pass. W8 is the board's right answer that lp-cli exits 1 on (walk fixed); W10's idle window was too short (walk fixed) |
| 8 | the same + the pacing patch, later revision | `lan-run8-wip/` | **10/10** with the fixed script. Not the shipping emulator |
| 9 | `df4ca2831` (the pacing reverted), lp-emu `201dc56a1` | `lan-run9/` | **7/10**, the run quoted below. W4 (finding 6), W8 (finding 1) and W9 (finding 7) fail |
| 10 | the same | `lan-run10/` | **6/10** at load average 73. W3 (finding 1); W8's page half (finding 7) on top of a board-side pass; W9 and W10 on the walk's wall-clock deadlines on a slow box |

## 2. The rule every step follows

Each step waits for **the board's words**: its console (the capture); its
status answers (`lp-cli wifi status` over its USB door, or over its LAN
forward while no page holds that board's one LAN link); or the LAN's own
view (`/boards`, the LAN probe). Studio's words only say when to look. One
exception is W8: the Radio node's message is a string only the firmware
holds (`fw-esp32-common/src/net/radio_rule.rs`, `RADIO_OFF_FOR_WIFI`). So
the deploy reply lp-cli prints, or the page showing it, is the board's
answer relayed. A last check on Studio's card that fails after the board
has answered is recorded as "board-side pass", not allowed to hide the
board's evidence.

## 3. What each step showed

### 3.1 At the shipping tree: runs 9 and 10 (`df4ca2831`, no pacing), and run 8 (with the held-back pacing patch)

`lp-emu:esp32c6:t1+net=lan`. Run 9 is the reference, with run 10 and run 8
beside it. Shots are in `target/walk-wifi-emu/lan-run9/shots/`, `…/lan-run10/shots/`
and `…/lan-run8-wip/shots/`.

Runs 9 and 10 ran earlier revisions of the script than the final one.
Since run 10, W8's page half is recorded instead of failing the step,
W10's idle window is 180 s instead of 60, and W9 accepts the board's
`session N up` as traffic. Under those rules, run 10's W8 is a board-side
pass, and its W9 board gates passed as far as the walk got.

| ID | Board's words (the gate) | Run 9 | Run 10 | Run 8 (pacing) |
|---|---|---|---|---|
| W1 | each status `connected`, an address, `lp-xxxx.local`, the two different | c6-a `192.168.4.100` (`lp-0000.local`), c6-b `192.168.4.101` (`lp-0001.local`): **pass** | the same: **pass** | **pass** |
| W2 | each console `[lan] link <id> … secure session opening`; each card `Ready` with its own MAC | both `[lan] link link1 from 192.168.4.1:49152: secure session opening (1024 B frames)`: **pass** | the same: **pass** | **pass** |
| W3 | `Project loaded`; `frame_count` advances; the knob's new value comes back from the board | `Project loaded: studio`; frames 14,143 → 15,985; knob `0.35 → 1`: **pass**. Then, with the editor syncing: `radio link link1: a reply still not out of the frame buffer after 5000 ms (2048 B held) — closing` (finding 6) | `Project loaded: studio`, frames 12,240 → 12,952, but the editor never opened. Eleven `read refused: board memory busy (free 50048 B, largest block 15336 B; needs 40960 B free and a 16384 B block)`, with no reply deadline: **fail (finding 1, firmware, PR B)** | frames 9,394 → 10,316, knob `0.35 → 1`: **pass** |
| W4 | no `[lan] link … closed` since W2; a second LAN dial to c6-b turned away; c6-b's status over USB at its W1 address | `[lan] link link1: closed (reply deadline)`, then `rst:0x10 (LP_WDT_SYS)` three times before W6, the studio project auto-loading each boot: **fail (finding 6)** | lp-cli `the board's LAN links are all in use; try again later`; c6-b `[lan] every LAN link is in use: a new one was told to try again later`; c6-b over USB `connected` `192.168.4.101`; no link closed: **pass** | **pass**, the same words |
| W5 | two instances, each board's `mac=` in its TXT | `lp-0000`, `lp-0001`, `mac=024c50000000` / `…01`: **pass** | **pass** | **pass** |
| W6 | (LAN page off: each console `[lan] link link1: closed (the WebSocket closed)`) status over c6-b's forward `last: wrongPassword`, back `connected` to `lp-walk-net` | `last: wrongPassword`, `connected` `lp-walk-net` `192.168.4.101`; "Checking the password" crossed; the Nearby list filled with no refresh: **pass** | **pass**, the same | **pass** |
| W7 | `last: notFound`, still `connected` | `last: notFound`, `connected` `192.168.4.101`; "Not in range": **pass** | **pass** | **pass** |
| W8 | `Project loaded: button-sign`; frames advance; the Radio node is the only fault; the Radio message from the board | `stop_all_projects before: 51300 B free … after: 171280 B free`, then `load refused: heap headroom too low (largest free block 44952 B < 65536 B)`, on a heap that had been through three watchdog boots: **fail (finding 1, after finding 6)** | `Project loaded: button-sign`; frames 22,045 → 22,243; only fault `/button_sign.show/radio.control_radio`; lp-cli's deploy reply carries the whole message: "…is unavailable: Radio is off while this board uses Wi-Fi. Turn Wi-Fi off for this board to use Radio." The page half failed (finding 7): **board-side pass** | the same words at ≈92 fps; the card `LIVE · 87 FPS Degraded: node /button_sign.show/radio.contro…`: **pass** |
| W10 | `link rtt lan:<fwd a>` (page off): 40/40 in frames, `link_resets` 0 | p50 **2.229** / p90 **3.324** frames at 941 fps, **nothing loaded** (W8 refused): **pass** | 40/40 timed, wall p50 297.8 ms / p90 384.7 ms, but one heartbeat in the 60 s idle window, so no frame rate. The box was at load 73: **fail (walk; window now 180 s)** | p50 **5.149** / p90 **6.364** frames at 92.5 fps, **`button-sign` rendering**: **pass** |
| W9 | `renumber` + `reset` → `[wifi] address <new>`; Studio's link back through the same forward with traffic; status over USB at the new address; the card | Did not reach the reset. The page reload opened `[lan] link link4 from 192.168.4.1:49160: secure session opening`, then `frames in 0 out 34 · handshakes 0`, growing to `out 534`; the page showed "Connect a board" and logged no LAN record: **fail (finding 7)** | `.100` → `[wifi] address 192.168.4.103`; `[lan] link link1 from 192.168.4.1:49157: secure session opening`; `radio link link1: session 0 up`. The walk then waited 300 s for a counters line that a board at load 73 had not reached: **fail (walk; now accepts `session N up`)** | `.100` → `.103`; relink `link1 … 49157`, `frames in 160 out 178`; USB status `connected` `192.168.4.103`; card `Degraded` (W8's own fault): **pass** |

Run 6 (`f3feec073`, no pacing, `lan-run6/`) agrees with run 9. Its
reply-deadline loop began right after `Project loaded: studio`, so W3
never opened the editor, and it ran on through W8: 12 watchdog resets.
W9 passed there with the card back at `Ready`, the first run in which
finding 5 did not show.

### 3.2 W10 in frames

| Run | Configuration, `lp-emu` | Rendering | p50 | p90 | Frame rate used | Same, frames per wall second (p50 / p90) |
|---|---|---|---|---|---|---|
| 9 | `lp-emu:esp32c6:t1+net=lan`, `201dc56a1`, via `lan:127.0.0.1:63251` | nothing (W8's load refused) | **2.229** | **3.324** | `idle_fps` 941.1 (emulated) | 7.638 / 11.387 at 3,224 per wall second |
| 8 | `lp-emu:esp32c6:t1+net=lan`, `201dc56a1+dirty` (held-back pacing), via `lan:127.0.0.1:62247` | `button-sign` (Radio node faulted, rest running) | **5.149** | **6.364** | `idle_fps` 92.5 (emulated) | 1.377 / 1.701 at 24.7 per wall second |
| 6 | `lp-emu:esp32c6:t1+net=lan`, `201dc56a1` | nothing (the studio project had stopped) | 2.394 | 3.560 | 941.1 | 8.813 / 13.105 |

Run 9 in full: n 40, min 2.081, p10 2.135, p99 15.313, max 15.313 frames;
in wall time, p50 2.37 ms and p90 3.53 ms. A project-bearing figure at the
shipping tree is still owed: in run 9, W8's load was refused, and in run
10 the idle window caught no rate. Run 8's figure, with a project
rendering, was taken with a pacing patch that does not ship. Its board ran
**slower** than the wall (≈0.66×), so its frames per wall second are the
smaller reading there. Neither figure is a silicon number, and neither is
a gate.

### 3.3 Before #989 (run 4)

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

### 3.4 W10's two frame figures, before #989 (runs 2, 5)

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

At the shipping tree: `target/walk-wifi-emu/lan-run9/shots/` and
`…/lan-run10/shots/`, `wifi-lan-2-W2.png` … `wifi-lan-10-W9.png` in run
order (W10 is shot 9, W9 shot 10). The passing reference is
`…/lan-run8-wip/shots/` (pacing). Run 6's W3 shot
(`lan-run6/shots/wifi-lan-3-W3.png`) shows the editor at "Syncing
project…" beside the board's read refusal. The LAN card's preview still
says "No live picture over Bluetooth" (finding 4).

Earlier, at 2×, in `target/walk-wifi-emu/lan-run4/shots/` (run 4:
`wifi-lan-2-W2.png` … `wifi-lan-10-W10.png`) and
`target/walk-wifi-emu/lan-run5/shots/` (run 5, W10 before W9:
`wifi-lan-9-W10.png`, `wifi-lan-10-W9.png`). W1 runs before Studio opens, so
it has none. W6/W7's shots are of the second (USB) page. Not committed:
copy the set into the ship report from those directories.

## 5. The run directory

`target/walk-wifi-emu/lan/` by default (each run overwrites it; `--out`
names another; runs 6–10 each have their own, section 1):

| File | What |
|---|---|
| `walk-wifi-emu-lan.json` | the summary: per step, ok, note, `seen` (what a step learned before it failed), the device-event records it produced, how many console lines each board wrote |
| `walk.jsonl` | every device-event record both Studio pages streamed (`?record=`) |
| `console/<id>.link.log`, `console/<id>.link.2.log` | each board's decoded console (`lp-cli link capture`), continued in `.2` after the walk asked it over USB (W4, W9); c6-b's ends at W6 |
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
  W8 proves the *rule* (the Radio node says why it is off; runs 8 and 10),
  not the radio sharing it guards.
- **One clock against the host.** The emulated board's guest clock is not
  tied to a wall-clock host's (finding 6), so any board timer that waits
  on the host, such as a reply deadline, a resend timer or the watchdog
  during such a wait, runs fast against it. A LAN figure in milliseconds,
  or a timeout seen here, is not a silicon one.
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
| N2 request p50 / p90 over Wi‑Fi (frames) | W10 run 9 (shipping tree): 2.229 / 3.324 frames at board fps, nothing loaded. Run 8 (pacing, not shipped): 5.149 / 6.364 frames at 92.5 fps with `button-sign` rendering | _(G1)_ | _(after G1; compare in frames of the same project)_ | |
| N4 fps joined idle | 927.7 emulated fps with nothing loaded; ≈144–151 with `Peach (1D)` (W3's heartbeats) | _(G1)_ | _(G1)_ | |
| N12 `.local` in Chrome | not covered (section 6) | _(G1)_ | n/a | |
| Wrong password → `wrongPassword`, back on the good network | W6: `last: wrongPassword`, back `connected`; in-row test crossed at "Checking the password" | _(G1 W2)_ | _(G1)_ | |
| `_lightplayer._tcp` lists the board | W5: both boards, each `mac=` and `proto=39` | _(G1 W5)_ | _(G1)_ | |
| Radio node message (AC5) | W8 (runs 8, 10): the deploy reply and the node fault read "Radio is off while this board uses Wi-Fi. Turn Wi-Fi off for this board to use Radio."; the rest of the project runs | _(G1 W6)_ | _(G1)_ | |

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
  (where the editor draws the Radio node's reason). Runs 8 and 10 reached
  the editor: the full message did not show as page text, though the card
  shows its first words. The deploy reply carries it whole.

From run 6 on (#989: one LAN link per board):

- **Nothing of the walk's dials a board's forward while the Studio page
  holds that board.** W4 now checks that a second LAN dial is **turned
  away** in the board's words, and reads c6-b's status over its USB door:
  the capture lets go and takes the door back (`usbStatus`). W6 and W7 read
  status over c6-b's forward with the LAN page at `about:blank`, each
  console showing its link closed. W8's upload and W10's `link rtt` also
  run with the page off. The page comes back for W8's editor and for W9,
  where Studio's link through the same forward is the point. W9's status
  is read over USB.
- **W8 accepts lp-cli's exit 1 when it is the board's Radio answer.**
  lp-cli exits 1 when the board reports a node fault after the load:
  "deploy was acked, but the deployed project failed to run: open control
  radio …: Radio is off while this board uses Wi-Fi. …". The step then
  gates on the board: `Project loaded: button-sign`, the frame counter
  moving, and the heartbeat's fault naming the Radio node and nothing else.
  The editor half is recorded, not gated.
- **W10 gives `link rtt` a 180 s idle window** (`--idle-s`, wall seconds on
  a `lan:` target). A rendering board can run far slower than the wall,
  and the default 20 s caught one heartbeat. It reads what was rendering
  from the capture's heartbeats during the run; the rtt report carries no
  `loaded_projects`.
- **W9's traffic check accepts `radio link <id>: session N up`** as well
  as the counters line, and its card check accepts `Degraded` (W8's
  project) as well as `Ready`. A card on neither is recorded as
  "board-side pass, Studio finding 5".

## 9. Product findings, with the board's evidence

Status at the shipping tree (`df4ca2831`, runs 6–10) leads each item. The
history stays below it.

1. **The read/load gate refuses while a LAN link is open (PR B's).
   NARROWED BY #989, STILL OPEN.** #989's fix
   (`docs/defects/2026-10-06-a-lan-link-strands-the-heap-below-the-load-floor.md`)
   moved the figures but did not clear the floor with a project loaded:
   - **Run 10, W3**, `Peach (1D)` loaded with Studio's one LAN link open,
     no relink and no reply deadline: `read refused: board memory busy (free
     50048 B, largest block 15336 B; needs 40960 B free and a 16384 B
     block)`, eleven times. That is 1,048 B short of the read floor.
   - **Run 9**: the same project read passed W3. The largest block depends
     on allocation order, so whether the editor opens is a coin toss.
   - **The load floor after a stop**, W8: run 6 `largest free block 56568
     B < 65536 B` and run 9 `44952 B`, each after
     `stop_all_projects … after: 171,xxx B free`. Both followed watchdog
     boots (finding 6) that auto-loaded the studio project before the LAN
     link opened. In runs 8 and 10, with no such boots, the same load
     passed.

   Firmware (PR B).

   Before #989: W3: `read refused: board memory busy (free 31456 B, largest block 13438 B;
   needs 40960 B free and a 16384 B block); retry shortly`, every project
   read for the rest of the run; Studio's editor sits at "Syncing project…".
   W8: `load refused: heap headroom too low (largest free block 62091 B <
   65536 B)`, right after `stop_all_projects before: 6324 B free … after:
   124100 B free`.
   Heartbeats: 158,948 B free / 79,667 B largest at 5 s, 31,996 / 13,373 at
   25 s with `Peach (1D)` loaded and Studio's LAN link open. Diagnosed by
   the director as PR B's: one open secure LAN link costs ≈25 KB of main
   heap and each first link strands ≈1 KB mid-heap. Not patched here.
2. **After `renumber` + `reset`, nothing reached c6-a through its forward
   — the emulator's, fixed (201dc56a1); STAYS FIXED.** Runs 6, 8 and 10
   saw W9's board gates pass (`.100` → `.103`, Studio's relink through the
   same forward). Run 9 never reached the reset (finding 7). The board rejoined at `.103` but logged no
   `[lan]` line again; Studio never relinked; `link rtt` over the forward did
   not finish. Cause: the virtual LAN's gateway (smoltcp 0.13.1) rate-limits
   ARP once a second for the **whole stack**; Studio's connection through the
   forward still had unacknowledged data at the reset and stayed open to the
   old address, so once its neighbour entry expired it asked ARP for `.100`
   every second and starved every new connection's ARP for `.103`. Fixed in
   `lan_port_forward.rs` (a connection closes when the lease it was opened
   under moves; forward sockets time out after 60 s with data outstanding);
   defect `docs/defects/2026-10-06-the-virtual-lans-forward-kept-a-moved-boards-old-connection.md`.
   Re-run (`target/walk-wifi-emu/pr993-w9/`, `a84308c4b`+the fix): the
   board-side W9 gates pass — c6-a at `.103`, Studio's link back through the
   same forward (`[lan] link link1 from 192.168.4.1:49155: secure session
   opening`, session up, traffic both ways), `wifi status` over the same
   forward on link2. W9 then fails only on Studio's card, finding 5.
3. **Studio's Wi‑Fi panel can open with an empty Nearby list for good
   (Studio, intermittent). RESOLVED** by `58a2505ae` (the scan is owed and
   the status read's answer sends it). In runs 6–10, `scanNeededRefresh` was
   false every time. Run 2: "Connect to a network" showed only
   "Other network…"; the records show two `Network/Scan` commands and **no
   `wifi.scan` request sent**, both while the panel's own `wifi.status` read
   was in flight (`network_controller.rs`: a scan asked for while `reading`
   returns without asking again), while `lp-cli wifi scan` over c6-b's
   forward heard both networks at the same moment. Run 4 did not hit it. The
   walk now presses the panel's own "refresh" once if the list stays empty
   for 30 s, and records `scanNeededRefresh`.
4. **A LAN card's preview says "No live picture over Bluetooth"** (Studio,
   copy): the W3/W9 shots, a board reached over Wi‑Fi. The transport chip in
   the editor's header shows the USB glyph for it too. **HALF RESOLVED** by
   `b518c9355`: the header shows the Wi‑Fi glyph now. The device pane's
   preview still says "No live picture over Bluetooth" for a LAN board
   (`lan-run6/shots/wifi-lan-3-W3.png`). Open, Studio.

5. **A LAN card does not return to "Ready" after its link closed and
   Studio redialled. RESOLVED** by `0fef558f7` (defect
   `2026-10-06-a-redialled-lan-link-leaves-the-card-not-listening.md`). Run 6
   W9: card `Ready` after the relink; run 8 W9: `Degraded`, W8's own fault.
   It did not show in runs 6–10. Before: (Studio, `lpa-devices/src/device.rs`): the card reads
   "Attached — not listening · quiet" while the board holds a live session
   from that page. Seen on c6-b after W6 closed its LAN link (runs 5 and the
   W9 re-run, before W9), and on c6-a after W9. Outside PR C; not diagnosed.

6. **Studio's editor over the LAN sends an emulated board into a watchdog
   reset loop. NEW, OPEN.** The cause is the emulator's open fidelity
   defect (`2026-10-06-an-emulated-boards-clock-outran-its-lan-host.md`),
   which is more than wasted frames here.
   - **What the board says.** Right after `Project loaded: studio`, with the
     editor syncing over Studio's one LAN link (runs 6 and 9):
     `radio link link1: a reply still not out of the frame buffer after
     5000 ms (2048 B held) — closing`, then `[lan] link link1: closed
     (reply deadline)` and `[perf] … tick=5040ms … responses=1`.
   - **The loop.** Studio redials. The next big reply stalls the same way,
     and two in a row trip the 8 s runtime watchdog: `rst:0x10
     (LP_WDT_SYS)`, `[RECOVERY] boot: cause=watchdog-reset`. The board
     auto-loads the studio project, Studio relinks, and it repeats: 12
     resets in run 6, 3 before W6 in run 9.
   - **The host was not stalled.** Studio's records show it trading frames
     with c6-a with no gap over 0.36 s in the minute before the close.
   - **Why it is the emulator's clock.** With the board waiting on the
     host, its guest clock skips ahead (`wfi`), so 5 s of guest time is a
     fraction of a wall second. The 5 s reply deadline and the 8 s
     watchdog both count guest time. The held-back pacing patch made it go
     away entirely: runs 7 and 8 had no reply deadline and no watchdog
     reset, and W3, W4 and W8 passed.
   - **The emulator side:** the existing open defect, whose "nothing is
     lost" line this contradicts.
   - **The firmware side (PR B's to weigh, not shown on silicon):** the
     server tick blocks for up to 5 s on a reply its LAN peer has not
     drained, and two in a row reset the board. A real host that stops
     reading, such as a throttled background tab, could do the same.
7. **A Studio page reloaded just after another LAN client let go opens a
   link that never carries a frame. NEW, OPEN, side not yet known.**
   - **Run 9, W9.** The reload came right after W10's `link rtt` closed
     `link3`. c6-a logged `[lan] link link4 from 192.168.4.1:49160: secure
     session opening`, so the WebSocket upgrade went through. Its counters
     then read `frames in 0 out 34 · handshakes 0`, climbing to `out 534`
     over the step: the board resending its half of the handshake to
     nobody.
   - **Run 10, W8.** The reload came right after W8's upload closed
     `link2`, and gave the same picture on `link3`.
   - **What the page shows.** "Connect a board · 2 remembered boards not
     connected" (`lan-run9/shots/wifi-lan-10-W9.png`). Its session records
     hold no `DeviceHotplug`, no wire bytes and no error, and it never
     redials. The link closed only when the walk navigated the page away.
   - **How often.** Seen in 2 of 2 runs without pacing and 0 of 2 with it.
     In run 6, a reload after the same kind of hand-over worked: it was
     refused once ("in use"), then redialled.
   - **Two places it could be:** the virtual LAN's forward (host-bound
     bytes after the upgrade response never reaching Chrome), or Studio's
     LAN provider (a socket opened but never driven). Not diagnosed. The
     next look is the forward's per-connection byte counts against
     Chrome's WebSocket state.
8. **Smaller things the board said** (firmware and lp-cli, minor):
   - `LpServer::tick: Project button-sign tick error: ` logs an empty
     reason, on every Wi‑Fi run with the Radio node.
   - Run 8's heartbeats also fault `/button_sign.show/playlist.playlist`
     with the empty message `resolve playlist trigger: `. A Radio-triggered
     playlist may degrade with the Radio and say nothing about why. That
     may itself be the Radio rule's expected consequence, but the empty
     message is not.
   - lp-cli calls a project that runs degraded "failed to run" and exits
     1. The board is rendering, at ≈88–92 emulated fps in runs 7 and 8.
   - The playlist fault in the second item was seen in run 8 only.

## 10. `just walk-no-board`, unchanged by the LAN

Not re-run at `df4ca2831` by this pass. Last run with `--serve-release`
(the release bundle and packaged firmware above, no dev server started), at
`ae167ccf0`: **6/6 pass** — flash → connect →
identify → upload (`Project loaded`; "project sent to studio — the board is
running studio") → detach → reattach (`Ready`). Directory
`target/walk-no-board/`.
