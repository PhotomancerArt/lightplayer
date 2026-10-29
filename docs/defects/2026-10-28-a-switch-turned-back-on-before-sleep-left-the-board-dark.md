---
status: fixed
found: 2026-10-28      # how: Yona, a switch-mode PowerButton board (the PLAYFUL choker's wiring: slide switch BAT+ → LEDs, branched to D0 through 10 kΩ)
area: fw-esp32c6 power-off (`src/hardware/power.rs`), lpc-engine `PowerOffRequest`
class: assumed-context
related:
  - docs/adr/2026-06-16-power-button-runtime-event.md
---
# A switch turned back on before the sleep left the board dark

**What happened** — switch off, then on again within about 2–3 s: the board
stayed dark and never came back until the switch was cycled again. The
switch feeds the LEDs straight from the battery, so the strip sat powered
and dark, draining it.

**Why** — the switch reads off, the node asks to power off, the server
unloads the project (LEDs dark) and the sleep runs after the link drains and
the status LED plays its power-off. Before sleeping, the firmware waited for
the pin to read its *non-wake* level for 100 ms, because EXT1 is
level-triggered and a held button would wake the chip at once. That wait was
written for **hold** mode, where the pin is still at its wake level when the
node asks. In **switch** mode the pin was already at rest when it asked, so
the wake level coming back meant "on" — and the wait, which had no other
way out, waited for the switch to go off again, dark, with the watchdogs
disabled.

**Fix** — `PowerOffRequest::pin_at_rest` says the pin was already at rest
(true for a switch). Once the pin has been at rest, its wake level held for
100 ms before the sleep is a wake: the firmware restarts instead of sleeping,
as the EXT1 wake would have. A held button still waits for its release; a
press after that release now restarts too, which is what the wake would do.

**Not covered** — the emulator does not model the EXT1 wake, and the
off-then-on window depends on the board's status LED and link drain, so this
was not reproduced emulated. Hardware confirmation owed (Yona, 2026-10-29).
A switch turned on in the few microseconds between the settle and the sleep
relies on EXT1 being level-triggered, as the hold-mode wait already did.
