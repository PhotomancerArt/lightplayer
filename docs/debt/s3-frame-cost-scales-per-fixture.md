---
status: carried
since: 2026-07-31
logged: 2026-07-31
area: lpc-engine fixture/shader render path (observed on fw-esp32s3)
related:
  - lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs
  - lp-core/lpc-engine/src/nodes/shader/shader_node.rs
  - lp-fw/fw-esp32-common/src/output/provider.rs
---
# S3 frame cost scales ~8.4 ms per fixture; render/sampling dominates, sends don't

**Shape** — Measured on the desk ESP32-S3 (2026-07-31, all eight node gates,
`projects/test/quad-strips`, 30-LED strips, per-fixture render target 30×4):

| Config | fps | tick | provider total | engine-side |
|---|---|---|---|---|
| 4 fixtures + 4 outputs | 20 | 48 ms | 5.3 ms | ~42.7 ms |
| 1 fixture + 1 output | 50 | 19 ms | 1.6 ms | ~17.4 ms |

Solving the two configurations: **~8.4 ms of engine-side cost per
fixture+output chain** (shader render of that fixture's 30×4 target, sample
resolution, control-product publish), ~9 ms fixed per-frame engine overhead,
and ~1.3 ms per channel of blocking RMT send. At 4 fixtures the per-fixture
engine work is ~70 % of the frame; the serialized `send_blocking` calls the
LED-output ledger suspected are **11 %** — overlapping them via
`lp_ws281x::send_blocking_all` would recover ~4 ms (20 → ~22.6 fps) and was
deliberately not done (measure-first rule; the S3 node-gates plan P4).

Provider split per write (temporary instrumentation, since removed): dither
pipeline ≈ 61–200 µs, blocking RMT send ≈ 1.26–1.4 ms — consistent with
30 LEDs of WS281x wire time plus reset; the send is genuinely the wire, not
driver overhead.

**Carrying cost** — Frame rate on multi-fixture projects degrades linearly
with fixture count (~8.4 ms each at 120-px render targets): 4 fixtures = 20
fps today; larger installs will cross into visibly choppy territory. Nothing
is wrong per se — this is a scaling characteristic — but any future "device
feels slow" report on multi-fixture projects should start here, not at the
output driver.

**Workarounds** — Fewer/larger fixtures where authoring allows (one fixture
spanning strips costs one render); smaller `render_size` per fixture.

**Incident log**
- **2026-07-31** — Filed from the S3 node-gates plan's P4 measurement, which
  existed to adjudicate the LED ledger's "~20 fps, sends are serialized"
  suspicion. Sends exonerated by measurement; per-fixture render/sampling
  convicted. No optimization attempted (out of P4 scope by design).

**Exit criteria** — A profiled optimization pass on the fixture render/sample
path (or a deliberate decision that N-fixture × M-px scaling is acceptable
with documented budgets), after which multi-fixture frame cost is either
reduced or documented as a sized budget authors can reason about.
