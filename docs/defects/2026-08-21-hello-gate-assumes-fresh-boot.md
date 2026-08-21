---
status: open
found: 2026-08-21      # how: hardware-walk (dig2go) + serial-lab live-debugging
area: lpa-link device_session (readiness) + fw-esp32-common server_loop
class: config-masked-defect
related:
  - 2026-08-03-dev-file-sync-drops-on-uart-rx-overflow.md
  - ../debt/shared-uart-io-task-starvation.md
  - ../../spikes/serial-lab/README.md
  - 2026-08-07-2336-dig2go-board-support (plan dir; notes.md 2026-08-21 sections)
---
# Hello gate assumes a fresh boot, so a running server can never pass it

**Symptom** — Connecting the setup wizard to a healthy, current-firmware,
already-running device fails with `transport error: Transport error: device
speaks the LightPlayer wire framing but did not identify itself with a
hello; reflash the firmware to a compatible build`, rendered in the wizard
as **"Running an older LightPlayer"** with an update-firmware CTA. Flashing
does not help: the next connect fails the same way. Observed on the bench
QuinLED dig2go (classic ESP32, CH340 bridge) 2026-08-21; every wizard
connect after the first successful flash was condemned.

**Root cause** — Three stacked facts, each verified on hardware via
`spikes/serial-lab`:

1. `gate_first_frame` (lpa-link `device_readiness.rs`) grants readiness
   only when the FIRST decoded frame is a proto-matching hello; any other
   frame is terminal `Incompatible(FrameBeforeHello)`.
2. A running server emits an unsolicited `Heartbeat` frame (`id: 0`) every
   5 seconds (`fw-esp32-common/server_loop.rs`), so a mid-stream connect
   almost always decodes a heartbeat first.
3. Nothing client-side ever sends `ClientRequest::Hello`. The server's
   answer path exists and works (the lab sent `M!{"id":N,"msg":"hello"}`
   to the live server and received a full proto-12 hello), and a comment
   in `lpa-server/handlers.rs` even asserts "Studio's client asks as
   well" — but no sender was ever implemented.

The gate therefore only ever passes when connect coincides with a boot, so
the boot hello is the first frame. On every previously walked board
(C6/S3-family and native host connections) that coincidence is
manufactured invisibly: opening the port toggles DTR/RTS through an
auto-reset circuit and reboots the chip. The dig2go is the first walked
configuration where that masking property is absent — Web Serial's
`setSignals` RTS-only pulse (esptool's `hard_reset`) does NOT reset a
CH340-bridged classic — and the latent design hole surfaced. The same
masking failure breaks the post-flash readiness probe: the device stays
parked in the flasher stub (silent), and readiness times out with "no
serial output was received".

**Bench facts for the fix** (serial-lab, dig2go, 2026-08-21):

- The live server answers a hello request within one heartbeat period.
- The unsolicited boot hello arrives ~2–3 s after reset, before the
  hardware manifest/boot-project lines.
- Web Serial CAN reset the CH340 classic: assert DTR+RTS together, hold
  ~120 ms, drop both together → clean power-on-cause reboot. RTS-only and
  DTR-only pulses do nothing.

**Fix direction** (settled enough to implement on the revival branch; the
accompanying ADR should ride that change): the readiness engine sends
`ClientRequest::Hello` after opening the wire and treats non-hello frames
before the hello as evidence of a LIVE peer (absorb, bounded), not an
incompatible one; a hello answer or unsolicited hello grants readiness
with the existing proto check. Pre-hello firmware still classifies
correctly by silence-of-hello (the request goes unanswered — absence is
the signal, as the fixture-no-hello comment already argues). Post-flash,
try esptool `hard_reset`, then the both-signals sequence, then prompt for
a replug — in that order.

**Regression coverage** — none yet. The gate's unit tests all feed the
hello first; a heartbeat-first case plus a request-then-answer case belong
next to `gate_first_frame` when the fix lands. The wasm fake at the Web
Serial boundary (multi-device roadmap M9) would have caught this at zero
bench cost — this defect is the second strong argument for it (the
2026-08-08 wizard-bug afternoon was the first).

**Lesson** — a readiness protocol that depends on observing a boot is
secretly a protocol that depends on the power to CAUSE a boot, and that
power is a property of the USB bridge wiring, not of the protocol. Every
gate that "waits for the device to announce itself" should also be able to
ASK — and any design comment claiming "the client asks as well" deserves a
grep before it is believed (see also
`stale-claim-propagated-from-unamended-adr`).
