---
status: open
found: 2026-10-02      # how: e2e (running `just walk-ble-emu` for the dropped-link fix)
area: lpa-studio-web public/lpa-link/virtual_bluetooth.js (`?ble=emu`); scripts/emu/walk-ble-emu.mjs
class: stand-in-divergence
related:
  - docs/adr/2026-09-27-lp-link-one-comms-layer.md (the USB cut-over, PR #854)
  - docs/adr/2026-09-24-ble-transport.md (S5: `?ble=emu`)
  - docs/defects/2026-10-02-a-dropped-link-sends-the-editor-to-devices.md
---
# `?ble=emu` relays the emulated board's lp-link USB bytes to Studio's `M!` Bluetooth link

**Symptom** — `just walk-ble-emu` against a fresh `emu serve` board pairs
over the polyfill (`add` ✓) and then sits at **Identifying** until the step
deadline: "No response — try flashing firmware", "Nothing from this board
yet." The board's console fills with the same repeated lp-link frame (its
SYN, never answered). It reproduces with no Studio change involved: nothing
before the identify step touches the editor.

**Root cause** — the polyfill stands in for the board's NUS GATT service by
piping bytes to and from the board's emulated **USB** byte channel
(`openLink` → `port.emulator.onBytes` → `tx.deliver`, and RX writes back
into it). That was faithful while USB and Bluetooth both spoke the `M!`
line framing. Since the lp-link USB cut-over (PR #854, 2026-09-28) the
board's USB speaks lp-link (4-byte header, CRC-32C, ARQ, a session
handshake), while Studio's Bluetooth path (`browser_ble.rs`/`BleClientIo`)
still speaks `M!` lines, as the real firmware's BLE link does. So the
stand-in now joins two different framings: Studio writes `M!` lines into an
lp-link endpoint that ignores them, and gets lp-link frames back that its
`LineSplitter` cannot read. Its own `watchAuth` (which parses `M!` lines off
those bytes) is dead code for the same reason.

Nothing gated it: the walk is not a CI job, and the conformance suite
(`lpa-link/tests/browser_ble_conformance.rs`) runs the polyfill against
scripted bytes, not against a board.

**Fix** — not yet. Two shapes, in order of fidelity:

1. Give the emulated board a BLE byte channel of its own that the firmware's
   real NUS server reads (the `M!` framing it ships on Bluetooth), and pipe
   the polyfill to that instead of to USB.
2. Make the polyfill an lp-link *host* on the USB channel, carrying `M!`
   lines as channel-1 payload, so it still proves Studio's transport, UI and
   Play (its stated scope), though it never proves the board's BLE side.

Until then, `walk-ble-emu`'s two drop steps (`drop`, `phantom`) cannot run.
The dropped-link fix was proven over the emulated USB cable instead
(`just walk-drop-emu`), which takes the same core path.

**Regression coverage** — none yet. The fix should make `walk-ble-emu`
reach Ready again, and something that runs in CI (a polyfill-against-a-board
smoke) should pin it.

**Lesson** — a stand-in that forwards bytes between two real endpoints is
only as faithful as the assumption that both endpoints frame the same way.
When one side's framing changes, every relay that joined it to something
else has to be found and checked in the same change.

**Incidents**

- 2026-10-04 — hit again by the Wi‑Fi settings walk (Wi‑Fi roadmap M5,
  `just walk-wifi-emu ble`): pairs over the polyfill, then sits at
  **Identifying** ("No response — try flashing firmware"), so the `?ble=emu`
  half of M5's walk could not run. The USB half (`just walk-wifi-emu usb`)
  passed; the Wi‑Fi controls over a Bluetooth link at author are covered by
  `lpa-studio-core`'s reach tests only, not by a walk, until this is fixed.
