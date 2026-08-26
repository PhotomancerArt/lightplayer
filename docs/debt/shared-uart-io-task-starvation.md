---
status: paying-down
since: 2026-08-02      # first registered symptom (line interleaving)
logged: 2026-08-21     # filed when the third instance met the bar
area: fw-esp32v3 / fw-esp32-common serial io_task + classic ESP32 UART0
related:
  - ../defects/2026-08-02-serial-line-interleaving.md
  - ../defects/2026-08-03-dev-file-sync-drops-on-uart-rx-overflow.md
  - ../defects/2026-08-21-hello-gate-assumes-fresh-boot.md
  - ../../spikes/serial-lab/README.md
---
# The classic's one UART is a starved, unowned, lossy party line

**Shape** — On classic/S3-family targets, UART0 carries everything at
once: boot ROM output, app logs, `[MEM]`/`[JIT]`/perf telemetry, the
`M!` wire protocol both directions, and esptool flashing. The firmware
side is serviced by an io_task sharing a cooperative executor with the
engine tick; at real project load the tick runs ~41 ms, the hardware RX
FIFO is 128 bytes (~1.4 ms at 921600), and there is no flow control and
no ack/retry on either direction. Structural consequence: whenever the
engine is actually rendering, serial data in BOTH directions is silently
lossy, and the loss window scales with project cost.

**Carrying cost** — 2026-08-21 (serial-lab, dig2go bench) measured the
mechanism directly: with a project ticking, inbound frames over ~128 B
hit `UART RX error: FifoOverflowed; dropping partial line` even paced at
256 B / 25 ms, and outbound protocol responses hit `[io_task] UART TX
timed out after N of M B` and are dropped (`responses=0` on every perf
line). With projects stopped (tick = 0 ms) a 4.5 KB inbound frame lands
instantly and responses flow. Concretely taxed so far: `lp-cli dev`
file-sync edits vanish silently (2026-08-03); `lp-cli upload`'s
run-evidence wait times out on BOTH dig2go walk attempts because the
status responses never escape; wire provisioning of `/hardware.json` is
impossible while a project plays; outbound telemetry lines arrive torn
(2026-08-02). Every serial-touching feature re-learns some of this the
hard way.

**Workarounds** (current lore, keep exact):

- Send `stopAllProjects` before any large inbound transfer (provisioning,
  project upload to a playing device); tick drops to ~0 and RX keeps up.
- Post-flash provisioning works only because a fresh flash boots with no
  project loaded — do device setup before starting playback.
- Treat `responses=0` under load as transport loss, not server death: the
  hello/heartbeat evidence is authoritative; request-response round-trips
  need idle ticks.
- Expect `lp-cli upload`'s run-evidence wait to time out against a
  playing classic; the deploy is usually fine (check the perf lines).
- Pacing alone does NOT fix inbound loss; only idling the engine does.

**Incident log** —

- 2026-08-02: outbound telemetry lines interleave/corrupt under
  concurrent writers (`2026-08-02-serial-line-interleaving.md`).
- 2026-08-03: `lp-cli dev` sync writes silently dropped during P6 walk;
  RX-overflow theorized (`2026-08-03-dev-file-sync-drops-on-uart-rx-overflow.md`).
- 2026-08-21: mechanism confirmed and quantified on the dig2go via
  `spikes/serial-lab`; TX-side response drops added to the picture; both
  dig2go upload attempts' run-evidence waits broken by it. Third
  instance — this entry filed, per the 2026-08-03 entry's own lesson.

**Progress** — 2026-08-25: fix landed on PR #448 (io_task on an
interrupt executor at swi2/Priority2, hardware-pacer wakes, thread-side
serialization with byte shuttling, UART write retry + peer-visible
Error frame, hello-triggered session flush). Bench (dig2go, project at
103–114 ms ticks, shipping build): 4,592 B inbound write + byte-
identical readback in 0.3 s under load, 10/10 responses (median
0.34 s), torn-frame reconnect gates cleanly, zero FifoOverflowed, zero
TX timeouts — every exit criterion below met. The alternatives analysis
is `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`
(including three bench-found constraints future serial work must not
re-learn). The `stopAllProjects` workaround remains necessary ONLY for
devices still flashed with older firmware. Residual frontier beyond the
exit criteria: inbound frames longer than one engine tick of line time
(~12 KB at a 103 ms tick) are intermittently lost —
`docs/defects/2026-08-26-inbound-frames-longer-than-a-tick-lossy.md`
(P6's RX ring is the known design answer; no current client sends that
shape).

**Exit criteria** — inbound: an interrupt-serviced RX ring (or io_task
priority/executor isolation) sized so a full-load engine tick cannot
overflow it, proven by a test that pushes a ≥4 KB frame while a
dome-scale project ticks. Outbound: protocol responses either survive
engine load or fail loudly to the peer (ack/retry or error), never a
silent drop with `responses=0`. When the fix is designed, the
alternatives (flow control vs chunk-ack vs executor split) are an ADR;
this entry then flips to `paying-down`.
