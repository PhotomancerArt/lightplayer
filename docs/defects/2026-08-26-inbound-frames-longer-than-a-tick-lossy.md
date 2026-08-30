---
status: open
found: 2026-08-26
area: fw-esp32v3 serial inbound (post io_task executor isolation, PR #448)
related:
  - ../debt/shared-uart-io-task-starvation.md
  - ../adr/2026-08-25-classic-uart-io-task-executor-isolation.md
---
# Inbound frames longer than one engine tick are intermittently, silently lost

**Shape** — With PR #448's fix in place (io_task on the swi2 interrupt
executor, 1 ms pacer), inbound frames up to ~4.6 KB (~50 ms of line
time at 921600) land 11/11 under a 103 ms dome-scale tick. A ~12 KB
frame (~131 ms of line time — *longer than the tick*) is lost roughly
1-in-3, in bursts: measured 4 lost / 11 sent across three sessions,
including runs of 3 consecutive losses bracketed by instant successes.
Idle, the same frame lands in 0.4 s every time.

**Silence** — When a frame dies, the firmware says *nothing*: no
`FifoOverflowed`, no RX-error warn, no stale-partial flush line. The
bytes vanish mid-frame; the most likely path is a torn-but-newline-
terminated line whose JSON parse then fails at DEBUG level in
`transport.receive` (`"Failed to parse"` — invisible at normal levels)
— but the byte-loss mechanism itself is unpinned. Candidates: an RX
window the 1 ms pacer cadence still misses during long critical
sections (flash op mid-stream?), or host-side CH340 behavior during
sustained streaming. Filesystem readback proved the loss is INBOUND
(the written files do not exist), not a lost response.

**Boundary vs the debt entry** — the starvation debt's exit criterion
(≥4 KB under load) is met with margin; this defect starts at
frames-longer-than-a-tick, a shape no current client sends inbound
(project uploads chunk well below it). It is the *next* frontier, not a
regression of the shipped fix.

**Known design answer** — the plan's conditional P6: an interrupt-fed
RX ring (esp-hal `unstable` hooks; inherit the classic rx_tout erratum
documented at `poll_rx_into`). The ring removes the polling deadline
entirely, which is the only robust fix for arbitrarily long streams.
Diagnose the actual loss window first (the defect may also yield to a
smaller fix, e.g. FIFO threshold tuning or the `UART_MEM_CONF.rx_size`
512 B experiment noted in the plan).

**Regression probe** — `spikes/serial-lab/scripts/starvation-bench.py`
C3b (12 KB write + readback under load) — advisory until this defect
closes, then flips to gating.

**Also raise** — DONE (2026-08-28, wire-evolution round 1, PR #458):
the parse-failure drop in `transport.receive` is a WARN with byte
length + prefix, and every drop site (parse failure, RX error,
queue-full, stale-partial flush) bumps a counter that rides the
heartbeat's new `link` field — the next loss of this kind is visible on
any desk without a serial rig. The byte-loss *mechanism* itself remains
unpinned and this defect stays open.
