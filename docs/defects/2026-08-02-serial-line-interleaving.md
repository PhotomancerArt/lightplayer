---
status: fixed
found: 2026-08-02      # how: hardware-walk
fixed: d06d0928b      # + c70c465b6 (pipe wrap); PR #884, wire proto 32 — emulator-validated, desk walk pending
area: fw-esp32v3 serial output (esp_println + log + telemetry writers)
class: unsynchronized-shared-artifact
related:
  - docs/defects/2026-08-01-classic-heap-regression-after-f32-merge.md
  - docs/adr/2026-09-27-lp-link-one-comms-layer.md
  - lp2025/2026-09-28-2015-classic-uart-on-lp-link (plan dir, P2 D7 audit)
---
# Concurrent writers interleave mid-line on the classic's UART0

**Symptom** — captured during the M7 bit-exactness walk on the desk
DOM-Z-102, at 921600 baud:

```
[OUT] frame=1200 leds=64 crc=0x55772254 lit=64 first=(50,74,2) … (0,1[MEM] free=60980 used=51660 largest_free=60966
[serial] 18,104)
```

The `[MEM]` heap line cut into the middle of an `[OUT]` frame-dump line,
which then resumed. In the same run `lp-cli upload` reported *"deploy was
acked, but no evidence the project is running arrived within 30.0s"* while
the device was demonstrably rendering the project — consistent with the
readiness check's framed JSON being corrupted by the same interleaving,
though that link is **inferred, not proven**.

**Root cause (suspected)** — several independent writers reach UART0 with no
mutual exclusion across a whole line: `esp_println` (used directly by the
frame-dump and `[MEM]` paths), the `log` bridge into the transport, and the
transport's own framed protocol writes. Each writes its own bytes; nothing
owns "a line". The classic is the exposed chip because it has **no
USB-Serial-JTAG** — its host link, its logs and its telemetry all share one
UART, where the S3/C6 have separate peripherals.

Contributing: the `[MEM]` probe (PR #281) and `frame-dump` (PR #279) both
landed on 2026-08-02, roughly tripling the writer population within hours.

**Why it matters beyond cosmetics** — three consumers parse this stream:
`lp-cli`'s deploy-readiness detection, `scripts/m4-hardware-walk.sh`'s
byte-comparison greps, and the P4-era stress-matrix scripts. A corrupted
line is a false negative in all three, and the walk's non-zero exit at the
FINAL gate is exactly that failure mode. Any future CI use of the walk would
be flaky for reasons unrelated to the firmware under test.

**Not yet done** — no fix attempted. Candidate directions, unevaluated:
a line-buffered writer behind one lock; routing telemetry through the
transport's framing rather than raw `esp_println`; or a per-line critical
section (⚠️ note `esp-sync`'s lock is `rsil 5` — see
`docs/adr/2026-08-02-classic-hli-refill.md` — so a naive lock on the render
path has interrupt-latency consequences the RMT refill deadline cares about).

**Regression coverage** — none yet. A host-side test could assert that
concurrent writers cannot interleave once a line-owning writer exists.

**Lesson** — an observability channel that several producers share needs an
owner for the unit consumers parse. We added two probes in one day, each
correct alone, and the composition broke a *test harness* rather than the
product — which is the cheap way to learn it, but only because someone read
the raw capture instead of trusting the exit code.

**Fixed — 2026-09-29, the classic's UART0 on lp-link (PR #884).** After the
main task's first await, **UART0 has exactly one writer**: `io_task`
(swi2), and it writes only what it takes from one static TX pipe. That pipe
takes only whole units, from the thread executor: link frames, each queued
whole by the link task and only when there is room for a worst-case frame
(`uart_link_pipes::put_frame`), and whole telemetry text lines
(`put_text_line`, refused if a line holds `0x00` or `0xFF`, which would
open a frame or a panic mark). Neither producer awaits mid-unit, so nothing
can land inside a frame or a line. What stopped each writer the defect
names — P2's audit of every `esp_println!`/`print!` in `fw-esp32v3/src`
outside `tests/`:

- **`[MEM]`/`[JIT]`** (`esp32_memory_stats`, the line that cut into the
  `[OUT]` line above): now `log::info!`, so a record in the log ring,
  carried whole on the link's log channel.
- **`[OUT]`** (`output/rmt/frame_dump.rs`, the other half of the captured
  collision): already `log::info!` before this change (ported at the USB
  cut-over), so a log record too. The plan's premise that frame-dump still
  wrote raw was stale.
- **`[stack]`** (`stack_probe::log_if_grown`): `log::info!`.
- **`[WS281X]`/`[WS281X-WIRE]`** (`ws281x_telemetry`): too long for a log
  record (200 B) and parsed by position, so they go through `put_text_line`
  as whole lines between frames (a host sees `LinkEvent::Text`).
- **The transport's framed writes**: lp-link frames from the link task.
- **Left raw, on purpose:** boot text (`[INIT] …`, the RMT/APP-core/power
  gate init, the refill-floor probe, and the `starting server loop` line
  after `boot_firmware` returns) — all of it written by the main task before
  its first await, so before the link task (same executor) has run once and
  while the TX pipe is still empty — and
  panic/OOM text, which masks interrupts and writes a raw `0xFF` mark
  before each block, a byte no link frame can contain.

The framed-JSON corruption this entry *inferred* is closed a second way: a
frame damaged on the line fails its CRC-32C and is resent.

**Regression coverage** — `lp-cli/tests/emu_v3_link_gates.rs` reads every
console record through the product's own link host; P5 retired the
`deinterleave` console repair those gates used to need, so an interleaved
record now fails them instead of being mended. The walk
(`just walk-esp32v3-emu`) compares the `[OUT] dump` delivered over the link
byte for byte against the pad and the oracle. Emulated only
(`lp-emu:esp32v3:t1`); byte-level interleaving on a real line is named by
AGENTS.md as a seam the emulator does not model, so the desk walk
(`hardware-walk-protocol.md` in the plan dir) re-checks it on the
DOM-Z-102 with a plain serial monitor.
