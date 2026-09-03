---
status: fixed
found: 2026-09-02      # how: report (Yona: remove project → push, LEDs stay still until a chip restart) + live-debugging on the XIAO ESP32-C6 bench
fixed: this change
area: fw-esp32c6 output/rmt bind_channel + fw-esp32s3 output/rmt bind_channel (esp-hal 1.1.1 `Channel::with_pin`)
class: lifecycle-ownership
related:
  - 2026-08-31-c6-rmt-ws281x-dark.md
  - 2026-09-02-c6-ws281x-first-three-leds-then-stale.md
  - ../adr/2026-09-02-fault-is-never-black.md
---
# Re-opening a WS281x output on the same GPIO disconnects the pad — the strip freezes until reboot

**Symptom** — Devices card, XIAO ESP32-C6 running a project: Remove project,
then Put it on the board (Plasma). Every layer says it worked: the remove
conversation ends `removed studio — the board has nothing loaded`, the push
ends `project sent to studio — the board is running studio`, the driver logs
`Esp32C6RmtWs281xDriver::open: endpoint=esp32c6-rmt-ws281x:ws281x:local:D10
gpio=/gpio/18 ws281x_ch=0 rmt_slot=0 bytes=723`, the shader compiles, the
engine ticks at 31 fps, the heartbeat carries `/projects/studio` with
`fault: None`, and not one warning is logged. The strip holds its last
latched frame and never moves. A chip restart (boot auto-load of the same
project) lights it. Yona: "leds are still but it seems the project is
running."

**Root cause** — the RMT channel keeps its esp-hal `Channel` across a
drop, and every open runs `slot.tx = Some(tx.with_pin(pin))` again with a
freshly stolen `AnyPin` for the endpoint's GPIO. In esp-hal 1.1.1,
`Channel::with_pin` (`rmt.rs:1302`) does
`self._pin_guard = pin.connect_with_guard(signal)`: the right-hand side
connects the GPIO to the RMT output signal, and only then is the OLD
`PinGuard` dropped — and `PinGuard::drop` (`gpio/mod.rs:131`) calls
`disconnect_from_peripheral_output` on ITS GPIO. On a first open the old
guard is unconnected, so the boot-time load works. On any later open of the
same pad (every wire re-load after an unload: remove → push, push over a
running project) the old guard names the same GPIO, and the pad is
disconnected a few instructions after it was connected. The transmitter
still runs from RMT RAM, the refill ISR fires, `tx_end` completes every
frame, so nothing below the engine can tell. `Drop for Esp32C6RmtWs281xOutput`
only aborts and clears `in_use`; the bound channel stays in the slot.

Two layers believed they owned the pad's routing: the driver (bind on every
open) and esp-hal's guard (release on replacement). Same code, same fault
shape, in the ESP32-S3 driver.

**Fix** — `ChannelSlot` remembers `bound_gpio`; `bind_channel` skips
`with_pin` when the GPIO is unchanged and binds (connect new, release old —
the case `with_pin` handles correctly) only when it differs. Both chip
drivers.

**Regression coverage** — none: the routing lives in the GPIO matrix
behind esp-hal, which no host test or `MockRmt` models, and the C6 harness
binds its channel once. The proof is the bench walk: remove → push on a
running board, then an LED glance — the card and the journal look identical
either way.

**Lesson** — "the project is running" is an engine fact, not an LED fact:
the heartbeat, the perf line and the driver's own completion counters all
sit ABOVE the GPIO matrix. This is the second defect in a week where a
healthy card sat over a dark strip (the first was the OOM quarantine's black
fallback). A frame the RMT transmitted is not a frame the pad emitted, and
any re-bind of a peripheral pin through esp-hal needs the guard-order rule
in mind: `with_pin` replaces first and releases second, so rebinding the
same pad is a disconnect. It also closes the loop on the 2026-08-31 "dark
after push, physical reset fixes it" saga, which had two causes stacked.
