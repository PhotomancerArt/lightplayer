---
status: open
found: 2026-08-02      # how: hardware-walk
area: fw-esp32v3 serial output (esp_println + log + telemetry writers)
class: unsynchronized-shared-artifact
related:
  - docs/defects/2026-08-01-classic-heap-regression-after-f32-merge.md
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
