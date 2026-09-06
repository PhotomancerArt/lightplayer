---
status: fixed
found: 2026-09-04          # how: hardware-walk (classic ESP32 V3 on a CH340 bridge, bench flash from the device card)
area: lpa-devices Flash activity post-write ladder → board-manifest stamp; lpa-devices evidence window
class: state-conflation
related:
  - 2026-09-02-flash-from-running-board-parks-until-reset.md
  - ../adr/2026-08-25-event-fold-device-model.md
---
# A pre-flash hello starts the board-manifest stamp over a port the flasher closed

**Symptom** — the flash itself succeeds; the `/hardware.json` stamp that
follows fails on the spot. The card's terminal reads:

    ▸ Hard resetting via RTS pin...
    wrote LightPlayer classic ESP32 server firmware
    ▸ Waiting for the board to come back (1/5)
    … (2/5) … (3/5) … (4/5) … (5/5)
    the board never became ready to write to: transport error: Transport error: Serial port is not open.
    firmware installed; writing the board manifest failed (…) — the compiled-in default pin map stands

The board never learns its board manifest: its hello keeps reporting
board `?`, the registry row never learns a board, and every later
re-flash asks for the board again (a classic chip fits several catalog
boards, so the pick cannot resolve on its own).

## Mechanism

The journal (device trace, bench tab, 2026-09-04) settles the order:

| Δ from write end | journal |
| --- | --- |
| 0 ms | `ActivityMarker Ended { kind: Flash, Succeeded }` — the ladder starts, `Open` is issued |
| +7 ms | `TimerFired seq 197` — the device's **freshness** timer, armed long before the borrow (`Note(WentQuiet)`) |
| +11 ms | `LinkBorrow held: true` — the stamp effect is already running: "Waiting for the board to come back (1/5)" |
| +44 ms | all five attempts done, each an instant "Serial port is not open." |
| +65 ms | `Link Opened` — the ladder's reopen lands, 21 ms after the stamp gave up |

No hello arrived between the write's end and the stamp. The hello the
reducer acted on was the one the board sent **before** the flash — the
V3 was already running LightPlayer. The observation window a hello lives
in survives a close by rule (the round-2 ADR's ruled list: open,
successful reset and detach clear the window; close never did), and the
flasher closes the model's port *under its borrow*, so the fold hears
no open until the ladder's reopen completes. `has_hello()` therefore
stayed true through the write, and the ladder read "a hello is in the
window" as "the flashed firmware answered" on the first input that
reached it — here an unrelated timer, seven milliseconds in.

The two facts the one predicate conflated: *a hello has been heard in
this window* and *the board that just booted has spoken*.

The CH340 makes it the common case rather than a race: the bridge stays
powered across the EN reset and the hard reset via RTS does not
re-enumerate the port, so the reopen is quick — but never as quick as a
timer that was already due. (A native-USB board re-enumerates and takes
seconds to reopen; the same stale hello would have fired the same way.)

## Fix

The ladder records when it began (the write effect's end instant) and
moves to the stamp only on a hello heard **at or after** that instant.
The fold now timestamps the window's hello (`Evidence::hello_heard_at`),
which is what lets an activity tell a hello that answered its reopen
from one the board sent before the activity began.

Scenario `a_pre_flash_hello_never_starts_the_stamp_before_the_port_comes_back`
(`lp-app/lpa-devices/tests/scenarios.rs`) pins the sequence: a hello'd
device flashes, the port is heard closing, the effect ends, the first
poke fires before the reopen — no stamp; the port opens, the new
firmware hellos — the stamp runs, and the outcome says nothing about a
missing manifest.

## What the existing scenario missed

`a_silent_board_after_a_flash_climbs_the_ladder_then_fails_honestly`
also starts from a hello'd device, but scripts `opened(1)` 100 ms after
the effect ends and before any timer fires — the reopen's fresh window
clears the stale hello before the ladder ever looks. The bench order
(timer first, open later) is the one that matters and was not scripted.

## References

- Device trace: bench tab on the PR #509 worktree's server, 2026-09-04
  (`lp-studio-device-trace` in localStorage; lines 696–715).
- The pre-write readiness wait (`lpa_client::wait_until_ready`) was
  itself a G1 lesson (2026-08-31): a just-flashed board formats littlefs
  before it answers fs writes. It retries the cheap request five times
  with no delay between attempts, so on a closed port the whole ladder
  burns in ~40 ms — honest, but it is the reducer's job to hand it an
  open port.
