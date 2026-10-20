---
status: open
found: 2026-10-09      # how: e2e (walk-drop-emu, emulated, lp-emu:esp32c6:t1)
area: fw-esp32-common `usb_link_transport` (SEND_ROOM_WAIT) × a `?emu=ws://…` host paced by the Mac serial model × the emulated clock
class: fixed-budget-over-variable-work
related:
  - scripts/emu/walk-drop-emu.mjs (step `editor`)
  - lp2025/2026-10-08-2050-the-board-card (P09, where it was found)
---
# The editor never opens on a board served by `lp-cli emu serve` over USB

**Symptom** — `just walk-drop-emu --serve-release` (an emulated C6 in an
`lp-cli emu serve` door, reached over `?emu=ws://…`, Studio's real Web Serial
stack over the shim) connects, pushes a project and sees the board say
`Project loaded`, then opens the board in the editor and waits forever. The
editor shows "Syncing project…" and the page console repeats:

```
[error] [studio] project sync failed: protocol error: expected project read frame seq 1, got 0
```

Seen three times out of three on 2026-10-09: with the board card's Edit
(`devices/<board>/edit`) on `claude/board-card` (7f440d196 and earlier), with
`Peach (1D)` and with `PLAYFUL Choker`, and with **today's card and its "Open
in editor" link** on the home page branch's tip (`claude/one-home-page`
2bb7e11b7), so it is not the board card's change. The same walk on the tab
backing (`WALK_BACKING=tab`, the board a Worker in the page) opens the editor
and passes every step, and `walk-ble-emu` opens the editor over `?ble=emu`.
`walk-drop-emu` on `main` could not be run here at the time: `main`'s copy had
no `--serve-release` and needed a dev server.

**Control run on `main`, same day.** Once the home page (#1071) had merged,
`main` itself (`cdd5b5f50`: today's card, its "Open in editor" link, `main`'s
own copy of the walk) was built (`just studio-web-story-build`,
`just studio-firmware-package-served`, the default single C6 image) and walked
with `just walk-drop-emu --serve-release`. It fails the same way at the same
step (`connect` and `push` pass, `editor` waits out its deadline, the console
repeats `expected project read frame seq 1, got 0`). The board card branch
merged with that `main` (`04c50ef8e`) fails twice out of two. So the failure is
on `main`, not something the board card brought. A run of the same walk on the
home page branch at `2bb7e11b7` that passed is on record from the director's
desk and is not explained here: the image variant (`LP_FW_IMAGE=split`) and
the date of the `main` underneath are the two differences not yet ruled out.

**Root cause** — not known. The error says two project reads interleaved on
one link (a read's frame 0 arrived where the next frame of another read was
expected). What differs from the passing lanes is the backing: the door's
WebSocket byte channel, not the tab's Worker or the Bluetooth polyfill.

**Fix** — none yet.

**Regression coverage** — none: the walk is not a CI job. `walk-drop-emu`'s
`editor` step is the reproduction.

**Seen again, 2026-10-17** — the Graph tab's G1 walk
(`lp2025/2026-10-17-0202-mapping-lab-into-studio`, branch
`claude/graph-in-studio-13f805`): an `lp-cli emu serve` C6 reached over
`?emu=ws://…`, the PLAYFUL choker pushed from the board card, Edit pressed —
"Syncing project…" for good, the same `expected project read frame seq 1, got
0` in the console. The tab backing (`?emu=tab`) opened the editor and the
Graph on the same build, so the walk ran there.

**Lesson** — the walks that open the editor over the door (`walk-drop-emu`)
had not been run since the lanes moved to `--serve-release`; a lane nobody
runs hides a failure until a change that has to run it finds one.

**Re-checked 2026-10-10** (the connected plan's P7, branch
`claude/connected-in-the-card` at `0c1ede359`, `main` at `0e505dd26` merged
in, so the emulated `sc.w` fix is in; `lp-emu:esp32c6:t1`). `just
walk-drop-emu --serve-release` over the door **passed**: the editor step
opened `/p/playful-choker-prj…` with the board's panel (brightness, palette,
scale, clock.rate), and both cable pulls were ridden out. The tab backing
passed too. The same error text did appear once, and recovered: in `just
walk-two-tabs-emu --serve-release`, tab B's Connect (step 5, which on that
branch opens the board's session on its card) logged `project sync failed:
protocol error: expected project read frame seq 1, got 0` twice, right
after the board's `dropped stale response … for a request abandoned by
client` lines, then the sync started over and the walk passed. Not
re-run on `main` here. Status left open: one passing run is not a cause.

**The mechanism, 2026-10-10** (the connected plan's debugging pass, branch
`claude/connected-in-the-card` at `2a8c955fa`, `lp-emu:esp32c6:t1`, a
loaded desk). `just walk-no-board --serve-release` failed at `card-connect`
(Connect opens the same lens the editor does): "waiting for the board's
panel on its card, the page still at `/`". Every one of the lens's project
reads failed, and for each read id the board's console says why:

```
[usb_link] dropping message id=4328521736: Transport error: host link busy: the reply before it is not out (2048 B held)
```

A full project read is several 16 KiB frames. The board waits
`SEND_ROOM_WAIT` (3 s, on its own clock) for one frame to be cut before it
queues the next, then drops the next and sends a drop notice
(`usb_link_transport.rs`, D8). The page was reading the whole time, at the
Mac serial model's ceiling (`?emu=` on a Mac: one 255 B read every ≥16 ms):
27,827 B in 2.09 s, no gap over 62 ms. One frame took about 1.4 s of wall
time to drain, and the drop landed about 20 ms after the frame before it
had fully arrived, with the same 2048 B held each time. So the board's
3 s ran out in about 1.4 s of the page's wall time: the emulated clock ran
ahead of a host that the tty model paces in wall time. Every retry lost the
same race. 1 run of 4 hit it that day; of the other three, one recovered
after a single dropped read, one dropped none, and the third's console was
not kept.

The drop notice is a plain `WireServerMessage::new` (seq 0), so a client
mid-stream reports "expected project read frame seq N, got 0" instead of
the drop's own words. That is cosmetic: the read fails either way.

Not fixed here (it is `main`'s, and the fix belongs to the firmware's room
wait against a slow host, the emulator's pace, or the read's frame size).
Status stays open.

**Re-checked 2026-10-17** (the board card's round 3, branch
`claude/card-surface-3` on `main` at `dd8534867`, `lp-emu:esp32c6:t1`,
lp-emu `7bf210663`, a loaded desk running other walks' builds). `just
walk-drop-emu --serve-release` over the door failed 3 runs of 3 at the
`editor` step (`connect` and `push` pass), each console repeating `expected
project read frame seq 1, got 0` between link resets, two runs in Chrome for
Testing 142 and one in Brave. The same walk on the tab backing
(`WALK_BACKING=tab`) passed every step, both cable pulls included. The
card's change touches no read path: this is the defect above, not the round.

**Seen again 2026-10-19** (`claude/flash-reboots-the-board`, `lp-emu:esp32c6:t1`,
`just walk-no-board --serve-release`, door backing, Chrome headless). The
`card-connect` step failed 2 runs of 2 with this branch's flash code and 1 of
1 with `main`'s (`776113d65`): "the corner's details give no frame rate", or
the panel never reaching the card, each with `expected project read frame seq
N, got 0` repeating in the page's errors. `flash`, `connect`, `identify` and
`upload` passed every time. The same walk on the tab backing passed all 13
steps both ways. Now a second walk that meets it.

**Seen again 2026-10-19** (`claude/beautiful-chatterjee-2a46fb`, the board
card's eight rows, `lp-emu:esp32c6:t1`, `just walk-no-board` against the dev
server, door backing, Chrome headless). `card-connect` failed 2 runs of 2,
once each way: no frame rate in the corner's details, then the panel never
reaching the card, with `expected project read frame seq 1|2|3, got 0` in the
page's errors. A new fact: both runs' board consoles show the board itself
**resetting on its hardware watchdog** during that step (`rst:0x10
(LP_WDT_SYS)`, then `[RECOVERY] last run crashed (watchdog): at <no frame>:
hang detected by hardware watchdog`, twice in the second run), so the
mid-read `got 0` is a rebooted board answering, not only a stalled link. The
first run shared the machine with a release `dx build`. `just walk-no-board
--tab` passed all 13 steps on the same build.

**Cause found, and the walk's door board held to wall time, 2026-10-20**
(ticket `2026-10-17-drop-walk-door-editor-deterministic`; an investigation on
`1bc45c7e5`, `lp-emu:esp32c6:t1`, lp-emu `04cb9d95d`, Brave headless, desk load
28–344; emulated only, no hardware).

The mechanism is the 2026-10-10 one, now pinned to its clock. An `emu serve`
board with no `pace=` skips idle time at `wfi` (`lp-emu-esp32c6`
`machine.rs`, `idle_skip`), so its emulated clock runs as far ahead of the
wall as the machine lets it. The page reads on wall time through the Mac
serial model (255 B per 16 ms). The board's two budgets in
`usb_link_transport.rs` `write_once`, `SEND_ROOM_WAIT` (3 s) and
`stall_after` (1 s), are counted on the emulated clock, so they run out
before the page has drained one 16 KiB frame. The board then drops the next
frame (`host link busy … 2048 B held`, `dropping message id=…`) and sends the
seq-0 drop notice, which the client reports as `expected project read frame
seq N, got 0`. How far the emulator outruns the page is the machine's load,
which is why the same walk failed 2 of 6 stock runs (editor step) and not
every time.

The A/B that shows it is the clock: with the page reading every 32 ms and the
default pace, 23 drops and a watchdog reset; with `pace=realtime`, 0 drops and
all 10 steps passing. With `pace=realtime` the editor step passed 7 of 7 with
0 drops. `pace=realtime` needs no `lan=` on a door board (the board's private
LAN still holds it, `net_engage`, `net_seam.rs`).

**The walk's change.** `scripts/emu/walk-drop-emu.mjs` now starts its door
board as `c6-a={fw},pace=realtime`, so the board's guest clock is held to
wall time, as Studio (a wall-clock peer) assumes. `startDoor` and the other
walks are unchanged. After it, `just walk-drop-emu --serve-release` over the
door passed 10 of 10 steps three runs in a row (`switch-off` included), each
with 0 `expected project read frame seq` errors in the page console and 0
`host link busy` / `dropping message` lines and no watchdog line in the
board's console, at desk load about 5. Not yet seen under the load of the
failing runs.

**Status stays open.** `pace=realtime` makes the walk honest; it does not
make the firmware tolerate a slow host. Two follow-ups, each its own ticket:

- `2026-10-20-send-room-wait-starves-watchdog`: one server pass can outlast
  the 8 s hardware watchdog while a slow host drains (the reset in the 2026-10-19
  entries above), because the room wait blocks inside the pass.
- `2026-10-20-drop-walk-switch-off-flake`: the walk's last step, `switch-off`,
  flakes on its own about 1 run in 3; a separate cause, not this defect's.

The other cause the investigation found, the Play curtain wait (red on both
lanes), was a walk defect and is fixed on `main` by #1208 (`waitSettled`).

**Seen again 2026-10-20, by hand** (the board card's panel layout, PR
#1206, `just studio-dev-emu` on a loaded desk, a dev wasm build in the
built-in browser, `lp-emu:esp32c6:t1`). After PLAYFUL was pushed to `c6-a`
over `?emu=ws://…`, Connect held the card at "This board is open in the
editor." with `expected project read frame seq N, got 0` repeating in the
console; once it recovered after Done and Connect, once it never did. The
same page with `&emu-tty=none` (the Mac serial model off) opened the panel
within ten seconds and every hand check that followed ran clean. `studio-dev-emu`
starts its door without `pace=realtime`, so a person at the dev server meets
this where the walks no longer do: `&emu-tty=none` is the desk workaround
until the door's boards are paced too.
