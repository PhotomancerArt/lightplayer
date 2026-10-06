---
status: fixed
found: 2026-10-02      # how: e2e (running `just walk-ble-emu` for the dropped-link fix)
fixed: this change     # PR #880, the merge that brought it to main's tree
area: lpa-studio-web public/lpa-link/virtual_bluetooth.js (`?ble=emu`); scripts/emu/walk-ble-emu.mjs
class: stand-in-divergence
related:
  - docs/adr/2026-09-27-lp-link-one-comms-layer.md (the USB cut-over, PR #854)
  - docs/adr/2026-09-24-ble-transport.md (S5: `?ble=emu`)
  - docs/defects/2026-10-02-a-dropped-link-sends-the-editor-to-devices.md
  - lp2025/2026-09-28-1445-ble-on-lp-link (PR #880, the fix)
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

**Fix** — PR #880 (Bluetooth on lp-link, wire proto 37) removes the
mismatch from both ends at once, by the second shape below rather than a
BLE channel in the emulator:

- Studio's Bluetooth path speaks lp-link now (`ble_link_port.rs`, one
  datagram frame per GATT write and per notification, `LinkConfig::ble()`),
  as the board's real NUS link does since the same change.
- `virtual_bluetooth.js` no longer pipes bytes: an RX write (one datagram
  frame) goes to the board's USB byte channel COBS-FF-wrapped between `0x00`
  delimiters, and the board's stream is cut back into frames, one
  notification each (byte-identical to `lp_link::frame::wrap_stream` on a
  200-vector check). So Studio's datagram link meets the board's stream link
  through a framing translation, and the walk identifies again.

What it still does not prove is unchanged: the emulated board answers on
its USB link at the edit tier and never runs the C6's radio code
(AGENTS.md, `?ble=emu`).

Considered and not taken: a BLE byte channel of the emulated board's own
(the higher-fidelity shape). It needs a simulated BLE air in
`lp-emu-esp32c6`, out of #880's scope (its D9).

**Regression coverage** —

- `lpa-link/tests/browser_ble_conformance.rs` (CI, headless Firefox): the
  link tests run the shipped `browser_ble.js` and Rust link through this
  polyfill's translation against an lp-link board double on the scripted
  door's stream — `a_link_comes_up_and_the_hello_arrives_one_frame_per_notification`,
  `a_request_goes_out_one_frame_per_write`,
  `a_large_request_survives_a_browser_that_writes_a_views_whole_buffer`,
  `the_conversation_io_round_trips_a_request`. A relay that joined two
  framings again fails all four.
- `just walk-ble-emu` (not CI): passed 8 of 8 steps (add, identify, remove,
  push, editor, Play, idle, knob) on #880 at `4f55d5eb2`, 2026-09-29, before
  main's drop steps existed. On the merged tree, with the `drop` and
  `phantom` steps: not yet run — it is the first step of the desk-walk
  runbook (`spikes/ble-lab/README.md`).

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
