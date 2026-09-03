---
status: open
found: 2026-09-02      # how: hardware-walk (G1 of fault-is-never-black, XIAO ESP32-C6, Studio bench tab)
area: lpa-devices activity/flash (post-write wait) + lpa-link browser flasher closing reset (native USB-JTAG)
class: assumed-context
related:
  - 2026-08-31-c6-rmt-ws281x-dark.md
  - ../adr/2026-09-02-fault-is-never-black.md
  - 2026-09-01-2026-fault-is-never-black
---
# Flash firmware on a running C6 leaves the board parked until a manual Reset

**Symptom** — G1 bench, 2026-09-02, the new running-card verb (PR #496
`88e2b3e06`): Flash firmware on a running XIAO ESP32-C6 parks the chip in
its ROM downloader (`ResetKind::UsbJtagDownload`), writes the image, and
then the board does not come back on its own — the card waits for a hello
that never arrives, and a manual **Reset** boots the new firmware, which
then runs its persisted project normally. Yona: "it seems I have to reset
the board after flashing."

**Root cause (read, not yet pinned)** — `FlashActivity` (lp-app/lpa-devices/
src/activity/flash.rs) has no reset of its own after the write: it hands
the wire to the flasher effect, then `ask_hello` on a rung timer, trusting
the browser flasher's closing reset (JS `normal`, DTR/RTS) to leave the
downloader. On a native-USB C6 that entered the downloader through a
DTR/RTS `UsbJtagDownload` sequence, the closing `normal` sequence evidently
does not always boot the app (the 2026-09-01 morning note already flagged
that espflash's `reset_after_flash` differs from the JS `normal` sequence
by paired re-writes). The activity's wait then expires without a hello.

**Why it matters** — the verb exists so a board need not be factory-reset
to take firmware; a flash that ends in "press Reset" is one manual step
better than before, not the one-click it should be. It is also the same
shape as the retracted BOOT-strap story of 2026-08-31: a board silent after
a download-mode session.

**Fix direction** — add a reset rung to the Flash activity: when the first
hello wait after the write expires, send `LinkCommand::RunReset(
ResetKind::Normal)` (idempotent — a board that already booted just boots
again) and wait once more before failing; or end every native-USB flash
with an explicit `Normal` reset before the first hello ask. Verify on the
bench that the app boots (the hello arrives) without a hand on the board.

**Regression coverage** — none yet: the activity's reducer tests can pin
"write done → reset command → hello" once the rung exists.

**Lesson** — a flow that parks a chip must own the un-parking; trusting a
third party's closing gesture is an assumed context.
