---
status: fixed
found: 2026-10-19      # how: Yona's install walk, a real ESP32-C6 over Web Serial on a laptop, around a Factory reset
fixed: this change     # PR #1198
area: lpa-studio-core `DeviceEffects::sweep_departed_ports` (the departure sweep); lpa-devices Erase activity (link loss while it listens)
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

The reset this PR adds after a Factory reset starts that boot loop at
once, so the fix had to ride it.

**Fix** — the departure sweep asks once more after a re-enumeration grace
(`REENUMERATION_GRACE`, 3 s) before it detaches a USB link, and detaches
only a port still missing then. A board that re-enumerated is back by then,
never left, and its card holds (Online, the blank board, Install firmware).
A network link (Bluetooth, the LAN, the relay) is still detached at once:
its session redials by itself. A board really unplugged reads offline 3 s
later than before. And an Erase that only listens for the blank chip's boot
loop outlives a vanished link, as an Update does, so a re-enumeration while
it listens is no "erase failed".

**Regression tests** — `lpa-studio-core` `board_card_tests.rs`:
`an_erased_board_re_enumerating_holds_its_card_until_it_is_really_gone`
(three turns of the loop: the card stays Online and blank at every step;
with no grace it goes Offline). `lpa-devices` `tests/scenarios.rs`:
`a_factory_reset_outlives_the_erased_chips_re_enumeration`.

**Not covered** — a port that was OPEN when it re-enumerated still departs
at once: the pump hears the dead port's own error and detaches. An erased
board's port is closed once the Erase or an identify hears its boot loop,
so the loop meets a closed port. The emulator does not re-enumerate on a
chip reset (the shim's ruling: a C6's USB-Serial-JTAG survives a reset the
serial channel asks for). Its walk stands the cable out and back in for
the watchdog's re-enumeration, so the real cadence is the desk's to see.
