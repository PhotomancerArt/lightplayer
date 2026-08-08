---
status: open
found: 2026-08-07      # how: hardware-walk (float-mode bench, dig2go)
area: fw-esp32v3 boot-compile (RECOVERY/OOM) + lp-cli upload (deploy) + lpfs partition
class: silent-drop
related:
  - 2026-08-07-upload-wait-timeout-unbounded-deploy.md
  - 2026-08-03-boot-looping-board-reads-as-flicker.md
  - 2026-08-02-classic-oom-retry-succeeds.md
---
# A shader that OOMs at boot-compile crash-loops the board with no client-visible cause

**Symptom** — During the 2026-08-07 float-mode bench, deploying a
heavy-interior shader (`examples/basic/shader.glsl`, psrdnoise, 4 KB GLSL)
at 1500 LEDs to the classic ESP32 (dig2go) OOM'd at GLSL compile:

```
alloc 3072 free=2624 used=183964   (shader-compile:glsl)
```

The board then crash-looped: every reconnect resets the board, the reset
triggers a green boot, boot re-compiles the persisted (heavy) project, the
compile OOMs again, and the board crashes again — so the client's deploy
handshake for the *next* upload is lost mid-cycle too (interacts with
`2026-08-07-upload-wait-timeout-unbounded-deploy.md`: the stuck handshake
is exactly what that timeout fails to bound). No client-visible error
named the cause at any point in the loop.

**Root cause** — The persisted project is boot-compiled unconditionally on
every reset, with no guard against a project already known to OOM at
compile. Once such a project is persisted, the board cannot reach a stable
running state on its own — every path back to "connected" first replays
the crash. The RECOVERY subsystem *did* log the cause correctly at
`level=yellow` with OOM stats on each cycle; the gap is that this
device-side diagnosis never reaches the client, so `lp-cli upload`
observes only a disconnected/unresponsive board with no explanation — the
gap is client UX + deploy robustness, not detection.

**Recovery** — Wipe the persisted project directly rather than trying to
deploy over it:

```
espflash erase-region --chip esp32 --port <port> 0x310000 0xF0000
```

(the lpfs partition, per `lp-fw/fw-esp32v3/partitions.csv`) — this erases
only stored projects, not firmware — then a normal `upload` succeeds.

**Regression coverage** — none: no test currently drives a boot-compile
OOM through a reconnect cycle.

**Status** — open. The RECOVERY subsystem's detection is correct and does
not need fixing; the defect is client UX (the cause is never surfaced to
`lp-cli`/the caller) and deploy robustness (no way to force a clean state
without a manual flash erase).

**Lesson** — Detection that never leaves the device is functionally
silent to the layer that needs it. This one compounds: the crash loop also
cannibalizes the *next* deploy attempt (see the companion timeout defect),
so a single bad shader can strand a board past what any one `upload
--wait-timeout` invocation could diagnose from the client side.
