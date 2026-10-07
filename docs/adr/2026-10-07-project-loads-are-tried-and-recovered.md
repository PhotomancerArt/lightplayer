# Project loads are tried and recovered, not gated

- Status: accepted
- Date: 2026-10-07
- Plan: `lp2025/2026-10-05-1903-wifi-link-c6` (PR B, #989; the FC6 re-check's N7)
- Supersedes: the load half of `2026-08-28-project-reads-bounded-streamed-refusable`
  (the `PROJECT_LOAD_MIN_HEADROOM_BYTES` gate added for
  `docs/defects/2026-08-29-load-project-resets-instead-of-refusing.md`)
- Fixes: `docs/defects/2026-10-06-a-wifi-joined-c6-refuses-every-project-switch.md`

## Context

Since 2026-08-30 a `LoadProject` was refused unless the heap's largest
free block, after unloading the running project, was at least 64 KiB. The
floor was sized for a dome: a classic had reset mid-load on one 64 KiB
lamp-list ask, and a 5,950-lamp list is ~93 KiB in one piece.

On the C6 with Wi-Fi joined, every switch was refused (FC6 re-check,
2026-10-06: largest free block 38–46 KB with links open, and 58 KB even
over USB alone). Measured in the emulator, byte-exact for memory, a
73-LED choker's largest single ask during a load is 8 KB. A dome is not
the embedded target. Yona's scale: "our mode is probably around 128 leds.
256 is reasonable. 512 in some exceptional cases… 1024 or 1500 as an
experiment". And on the gate itself: "I haven't ever been a big fan of
these gates. they're too blunt… I wish we could just try and recover from
failure."

A gate is a guess about a project's cost made before reading it. It
refused loads that fit, and it never caught the loads that do not: a
project that loads and then runs the board out of memory on its first
frame (Small Dome on the emulated C6: the lamp list fits, the first
frame's 20 KB ask does not) passed it and reset the board on every boot.

## Decision

1. **No load gate.** `PROJECT_LOAD_MIN_HEADROOM_BYTES` is gone, with no
   smaller floor: nothing a floor could test says whether a project fits,
   and the recovery below covers a load that cannot run at all.

2. **A load is a transaction recorded across a reset.** Before a load
   allocates, the server writes "loading X (from P)" into the persistent
   recovery region (`lp_recovery::begin_project_load`; RTC fast memory on
   ESP32, which survives a software or watchdog reset). The record stands
   until the loaded project has run its first `LOAD_COMMIT_FRAMES` (3)
   frames, or the load fails without a reset. A board that resets in
   between (out of memory, a crash, a hang) boots with an
   `InterruptedLoad` in its boot assessment.

3. **The startup choice moves only when a load is done.** A switch
   becomes the startup project after its first frames, not when it
   loads. So a switch that resets the board boots the project that ran
   before it, with no extra machinery: that is the recovery.

4. **A startup load that reset the board is not tried again.** The
   boot's own load is recorded too (`LpServer::load_startup_project`).
   If it did not finish, the next boot loads no project. Never a reset
   loop: at most one reset for a switch that does not fit, and one more
   if the project before it no longer fits either.

5. **The board says so in plain words** and blames nobody. The heartbeat's
   `RecoveryStatus::load_notice` (an additive field, no proto bump)
   carries e.g. "Small Dome didn't fit in memory — back on PLAYFUL
   Choker", for the whole boot. Studio's card and journal show it. An
   out-of-memory load is not blame-ledger material: it did not fit, which
   is not the fault of the shader or node that asked last. A load that
   panicked or hung is still recorded in the ledger as before.

6. **The few big asks are tried first, to save a reboot.** The lamp
   list, a load's one large contiguous allocation, uses
   `try_reserve_exact`. A list the heap cannot hold refuses the fixture
   with words ("5950 lamps need 47600 B in one piece, more than this
   board's memory has free") instead of resetting the board. This is
   not a gate to tune. It only turns one reset into a message where an
   allocation can fail cleanly.

## Consequences

- Loads that fit, load. On the emulated C6 with Wi-Fi joined, the choker
  and `basic` switch over USB and with a LAN link open. Before this
  change, each of those switches was refused.
- A project that does not fit costs at most one reboot and comes back on
  the previous project with a message. Over Wi-Fi the reboot is a rejoin,
  and a host's link redials.
- Reads on a fragmented heap go out in smaller frames instead of being
  refused. A ProjectRead's frame budget is half the largest free block (at
  most the link's 16 KiB, at least 1 KiB, `lpa_server::read_frame_budget`).
  The C6's read-gate block floor dropped from 16 KiB to 8 KiB, which holds
  the largest single read ask (an 8 KB mapping slot JSON). An event too big
  for its frame is refused in words. This followed directly: with a LAN link
  open after a switch to `basic`, the emulated C6's largest block sat at
  16,164–16,172 B and every read was refused. The total-free floor (40 KiB)
  stays. Yona still wants the read gate itself revisited the
  try-and-recover way.
- The recovery region is version 2 and holds the record in its last
  48 B of the 1 KB budget; project names are cut to 20 bytes there (they
  are only ever words).
- Embedded tests and walks should size projects at the 128–512 LED
  design target, not at dome scale.
- The emulated C6 used to restore LP SRAM on an HP reset, so it never
  reported a crash across a reset and never saw an interrupted load. It
  now keeps it, as silicon does
  (`docs/defects/2026-10-07-the-emulated-c6-cleared-rtc-fast-memory-on-every-reset.md`).
