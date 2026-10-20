---
status: fixed
found: 2026-10-19      # how: Yona's install walk, a real ESP32-C6 over Web Serial on a laptop, around a Factory reset
fixed: 2026-10-19      # PR #1198 (closed port), PR #1202 (open port)
area: lpa-studio-core `DeviceEffects::sweep_departed_ports` (the departure sweep) and the read pump's `Departure`; lpa-devices Erase activity (link loss while it listens)
class: state-conflation
related:
  - docs/defects/2026-10-19-a-board-erased-over-webusb-reads-not-responding-until-a-reload.md
  - docs/defects/2026-10-18-a-board-that-never-spoke-offered-retry-not-install.md
---
# A board that re-enumerates remounts its card every few seconds

**Symptom** — Yona, on the same walk as the Factory reset dead end: "the
card periodically 'refreshes' which is jarring and resets the ui. it
stopped me from interacting with it properly." Open details closed, an
armed button disarmed, and the card jumped.

**Root cause** — an erased native-USB C6 boot-loops: its ROM finds no
image, and the RTC watchdog resets the chip every few seconds, which
re-enumerates its USB (bench 2026-08-30: "49 adoption cycles in a row on a
wiped C6"). Chrome fires `disconnect`, then `connect` a moment later with a
new `SerialPort` under the same grant (session adoption keeps the endpoint).
The departure sweep asked the browser which grants remained 250 ms after
the `disconnect`, found the board missing, and detached its link. The
board was Offline for that moment. Its card is keyed by its section
(`online-<id>`, `offline-<id>`), so it unmounted from Online boards,
mounted in Offline boards, and moved back on the `connect`. Each mount
started its local state over. A re-enumeration was read as an unplug.

The reboot after a Factory reset (#1197, `rebootFromFlash`) starts that
boot loop at once, so this fix rides with the Factory reset one (#1198).

**Fix** — the departure sweep asks once more after a re-enumeration grace
(`REENUMERATION_GRACE`, 3 s) before it detaches a USB link, and detaches
only a port still missing then. A board that re-enumerated is back by then,
never left, and its card holds (Online, the blank board, Install firmware).
A network link (Bluetooth, the LAN, the relay) is still detached at once:
its session redials by itself. A board really unplugged reads offline 3 s
later than before. An Erase that only listens for the blank chip's boot
loop outlives a vanished link, as an Update does, so a re-enumeration while
it listens is no "erase failed". And an Erase that ends well gives the port
back whether or not it heard the boot loop. A port left open meets the
chip's next re-enumeration as a dead port, which the read pump detaches at
once, and the card moved anyway. The emulator walk caught exactly that: its
erased chip said nothing, the Erase ended on its deadline with the port
open (before #1197's reboot, the erased chip stayed in the stub), and
the first cable turn remounted the card.

**The open port (#1202)** — a port Studio held OPEN when it re-enumerated
still departed at once: the read pump (or the editor lens's io) heard the
dead port's own error (`The device has been lost.`) and detached the link
then and there. A healthy board whose reset took its USB with it was
offline for the moment it was gone, and its card moved sections and
remounted all the same. Now a USB link whose port died is held for the
same grace (`Departure` in `device_effects.rs`): the error is passed on,
the close that follows is not, so the board stays exactly as it was; the
dead link's endpoint stops counting as held, so the connect edge's sweep
attaches the re-enumerated port as a new link, which the model takes as
the same board's next generation (its endpoint supersede); and only a link
nothing replaced within the grace is detached. A dead port's controller is
still never reused. A network link is still detached at once.

**Regression tests** — `lpa-studio-core` `board_card_tests.rs`:
`an_erased_board_re_enumerating_holds_its_card_until_it_is_really_gone`
(three turns of the loop: the card stays Online and blank at every step;
with no grace it goes Offline), and
`a_running_board_re_enumerating_with_its_port_open_holds_its_card` (a
running board, its port open, three re-enumerations: Online and under
Online boards at every step, its surface unmoved while the port is gone;
unplugged for good it is offline after the grace; with the old detach it
is Offline in the gap). `studio_device_e2e_tests.rs`:
`a_re_enumeration_under_the_lens_never_takes_the_board_offline` (the
editor open on it: never offline, the same session resumes on the new
link). `lpa-devices` `tests/scenarios.rs`:
`a_factory_reset_outlives_the_erased_chips_re_enumeration`, and
`a_factory_reset_board_that_says_nothing_is_blank_not_unresponsive` (the
quiet window gives the port back). The emulator walks:
`just walk-factory-reset-emu` re-enumerates the erased board three times
under the blank card with its details open, and `just walk-reenumerate-emu`
re-enumerates a running board's OPEN port three times with its Hardware
details open (the same card element, under Online boards at every change
the page made), then lets the port stay gone (Offline boards) and brings
it back.

**Not covered** — a board whose open port re-enumerated is re-identified
on its new link: its card's top says "Identifying this board." until the
board says hello again (one bench step; on a C6 that just reset, about its
boot), then shows nothing for the moment before the new window hears what
the board runs (`identifying → all-well → no-project` on the walk, as after
any identify). Holding its old face across that would mean carrying a verdict
about the machine that reset, which the model rules out ("no sticky
verdicts"). The card stays mounted and in place through it. A port that
takes longer than the grace to come back still moves the card to Offline
boards and back. The emulator does not re-enumerate on a chip reset (the
shim's ruling: a C6's USB-Serial-JTAG survives a reset the serial channel
asks for). Its walks re-enumerate the board through the shim (the old port
dead, a new one under the same grant, a second apart) for the reset's, so
the real cadence is the desk's to see.
