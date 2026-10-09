---
status: open
found: 2026-10-09      # how: e2e (walk-drop-emu, emulated, lp-emu:esp32c6:t1)
area: lpa-studio-core project sync over a `?emu=ws://…` USB board (`lp-cli emu serve`), or the door's link
class: unclassified
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
`walk-drop-emu` on `main` could not be run here: `main`'s copy has no
`--serve-release` and needs a dev server.

**Root cause** — not known. The error says two project reads interleaved on
one link (a read's frame 0 arrived where the next frame of another read was
expected). What differs from the passing lanes is the backing: the door's
WebSocket byte channel, not the tab's Worker or the Bluetooth polyfill.

**Fix** — none yet.

**Regression coverage** — none: the walk is not a CI job. `walk-drop-emu`'s
`editor` step is the reproduction.

**Lesson** — the walks that open the editor over the door (`walk-drop-emu`)
had not been run since the lanes moved to `--serve-release`; a lane nobody
runs hides a failure until a change that has to run it finds one.
