---
status: fixed
found: 2026-09-06      # how: report (lp-riscv-emu speed probe, M0 of the ESP32-C6 emulator plan)
fixed: this change
area: lpa-client transport_serial/emulator + lp-cli client_connect (`emu` host spec)
class: untested-path
related:
  - docs/reports/2026-09-06-lp-riscv-emu-speed-probe.md §4
  - lp-fw/fw-tests/tests/emu_async_transport.rs
---
# `lp-cli ... emu` never gets a reply: async emulator transport wrote bare JSON, firmware only reads `M!` lines

**Symptom** — `lp-cli upload examples/meteor emu` boots fw-emu, logs
`[fw-emu][RECOVERY] boot complete (first frame served)`, logs
`Emulator thread: Writing client message id=1 (33 bytes) to serial`, and
then pins one core at ~99% with no further output; bounded reproduction
exits 124 after 240 s. Driving the same transport from a test yields the
client's own deadline: `Request timed out - server may not be receiving
messages (check host->device serial)` after 60 s, on a trivial
`fs_write` before any project is loaded. No emulator error is ever
logged — the guest is healthy and ticking.

**Root cause** — Two client-side writers of the guest's serial input
existed. The synchronous test transport (`transport_emu_serial.rs`) and
the hardware transport both frame client messages as `M!{json}\n`. The
background-thread transport `create_emulator_serial_transport_pair`
(`transport_serial/emulator.rs`) — the one `lp-cli`'s `emu` host spec
uses — wrote `{json}\n` with no prefix. The firmware's
`SerialTransport::receive` (`fw-core/src/transport/serial.rs`) began
requiring the `M!` prefix on inbound lines on 2026-02-26 and treats
unprefixed lines as log noise ("Skipping non-message line"). Every
client request was therefore silently discarded; the guest kept
rendering frames (hence the busy core — `TimeMode::RealTime` runs the
emulator thread without sleeping by design) and never answered.

The report's first hypothesis — that the plain `release` profile
(opt-level `z`) miscompiled the guest into a runaway loop — was
refuted: with only the framing fix, and fw-emu still on plain
`release`, the meteor upload completes ("Project uploaded and
running.") in about a second of guest time.

**Fix** — `emulator.rs` frames client messages as `M!{json}\n`, matching
the hardware transport. Separately, `client_connect.rs` now builds
fw-emu on `release-emu` so `lp-cli ... emu` runs the same guest binary
`fw-tests` validates (the profile the workspace `Cargo.toml` already
recommends for Cranelift codegen faults); this is alignment, not the
hang's fix.

**Regression coverage** — `fw-tests/tests/emu_async_transport.rs`
(`async_realtime_transport_first_round_trip`): `release-emu` fw-emu,
`TimeMode::RealTime`, the production async transport, two `fs_write`
round-trips under a wall-clock bound. It timed out before the fix and
completes in under a second after.

**Lesson** — The emulator tests only ever drove the guest through a
*test-specific* transport, so the production transport was a sibling
path no gate reached: a wire-framing contract change on the firmware
side was propagated to the two writers the tests and hardware exercise
and missed the third. When a protocol has more than one client-side
framer, the framing belongs in one shared function (or at minimum one
test per framer against the real parser), and "no response, no error,
busy CPU" from an emulated guest should be read first as *the guest
never saw the message* rather than as a guest fault.

**Follow-up (2026-09-06)** — the framing now lives in one place:
`lpc_wire::json::to_serial_line` (with `json::SERIAL_LINE_PREFIX`). All
three lpa-client serial transports and fw-core's buffered server writer
call it, and a fw-core unit test
(`shared_framer_output_round_trips_through_receive`) feeds the framer's
output through the real `SerialTransport::receive` parser. The lpa-link
writers (`device_link/wire.rs`, `browser_serial.rs`,
`fake_device_core.rs`, `port_client_io.rs`) still frame by hand and can
adopt the same function.
