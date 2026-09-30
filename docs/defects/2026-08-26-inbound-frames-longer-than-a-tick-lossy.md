---
status: fixed
found: 2026-08-26
fixed: d06d0928b      # structurally, by lp-link's ARQ (PR #884, wire proto 32) — emulator-validated, desk walk pending
area: fw-esp32v3 serial inbound (post io_task executor isolation, PR #448)
class: silent-drop
related:
  - ../adr/2026-09-27-lp-link-one-comms-layer.md
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

**Fixed structurally — 2026-09-29, by lp-link's ARQ (PR #884, wire proto
32).** Worded carefully, because the byte-loss *mechanism* this entry could
not pin is still not pinned, and nothing here claims it can no longer
happen:

- **A long inbound message is no longer one long exposure.** lp-link cuts
  every message into frames of at most 256 B of payload (~3 ms of line time
  at 921,600 baud), each with its own CRC-32C, sequence number and
  acknowledgement. A 12 KB request is ~48 frames; a byte lost anywhere in
  one damages that frame only, which is counted and resent, and the
  message is delivered whole or the link resets (which fails the request at
  the host at once). It is never delivered torn and never silently absent.
- **The silence is gone.** Each recovery is a `damaged`/`resends` count in
  the heartbeat's `link` object, readable on any desk with no serial rig.
- **The "known design answer" above is downgraded** from the fix to optional
  latency work. An interrupt-fed RX ring existed to stop bytes being lost at
  all, because a lost byte used to be a lost request. With resend, a lost
  byte costs a resend (the board's floor is 200 ms), so the ring would buy
  fewer resends under load, not correctness. Worth doing only if the desk
  walk or a later soak shows resends high enough to matter.

Evidence, emulated only (`lp-emu:esp32v3:t1`,
`lp-cli/tests/emu_uart_link.rs`): the `--uart-faults` soak, including 1 KiB
damage runs (16 consecutive 64-byte windows), completes five project
uploads and loads with 0 app errors. That injects loss; it does not
reproduce this entry's unknown silicon mechanism, which is why the desk
walk (`hardware-walk-protocol.md` in the plan dir) reads `resends` across a
real upload on the DOM-Z-102. `spikes/serial-lab/scripts/starvation-bench.py`
C3b still speaks `M!` and cannot run against a proto-32 image.
