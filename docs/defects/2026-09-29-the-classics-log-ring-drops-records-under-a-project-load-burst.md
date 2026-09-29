---
status: open
found: 2026-09-29      # how: emulator walk (lp-emu:esp32v3:t1), plan classic-uart-on-lp-link P5
area: fw-esp32-common `log_ring_logger` (4 KiB `LOG_RING`) × `uart_link/uart_link_task.rs` (2 records per pass, thread executor)
class: wake-quantum-throttle
related:
  - ../adr/2026-09-27-lp-link-one-comms-layer.md
  - ../adr/2026-08-25-classic-uart-io-task-executor-isolation.md
  - 2026-08-02-serial-line-interleaving.md
  - lp2025/2026-09-28-2015-classic-uart-on-lp-link (plan dir, P5 §8, ruling DD37)
---
# The classic's log ring drops records under a multi-output project-load burst

**Symptom** — on the classic on lp-link (wire proto 32), a project load that
opens several outputs loses log records, and says so:

```
[LINK] 3 log records dropped
[LINK] 21 log records dropped
```

That is `lp-cli/tests/emu_v3_link_gates.rs`'s five-wire load
(`five_wires_share_four_slots_…`, five outputs over four RMT slots): 24
records, IO18's driver-open line among them. The oracle walk
(`just walk-esp32v3-emu`, one output) loses fewer but still some: the first
(black) frame's six-part `[OUT] dump` lost parts 2–5 in P5's run, and
`[LINK] 2 log records dropped` / `[LINK] 4 log records dropped` appeared on
two `walk-esp32v3-emu-frame` runs on 2026-09-29. The deferred lit dump the
walk compares always arrived whole. All `lp-emu:esp32v3:t1`; **not seen on
silicon yet** (no desk sitting has run this image).

**Root cause** — the drops are counted, not silent: `lp_link::log_ring`
keeps the newest records and replaces the oldest, and the next record out
is the count. What is not settled is which of two limits a load burst
crosses first. Both are real:

- **The ring is 4 KiB** (`LOG_RING_BYTES`), shared with the C6, which
  drains it from a USB link task that moves 4 records per pass.
- **The classic drains 2 records per pass** (`LOG_RECORDS_PER_PASS`,
  matched to its board config's two-slot datagram queue), and the link
  task runs on the **thread executor** (ruling DD20: io_task stays a byte
  shuttle on swi2, the `Link` lives thread-side). A project load is long
  server-loop work on that same executor, so the link task gets no pass
  until it yields. The five opens, their `[OUT] open` lines, the compile's
  records and the `[MEM]` brackets land in the ring together.

Which one it is was not measured: a 2-per-pass quantum with no pass during
the load, or a ring that one load's records overfill regardless of the
quantum. The five-wire gate's count (24) is the size of the gap either way.

**Why not fixed here** — the obvious fix is a bigger ring, and on the
classic `.stack`'s residual pays for every static (the ring already moved
`stackTotal` from 44,680 to 37,896 B together with the 3 KiB of pipes;
P5 measured 8,360 B of stack headroom under a project load before P6's
`poll_in_own_frame` returned ~500 B of it). That is a RAM trade for the
director, not a drive-by. A faster drain (more records per pass, or a pass
between outputs) does not cost RAM but changes what the link task does
during a load, which P5's timing did not cover.

**What changed so it cannot be missed** — the five-wire gate asserts the
open line **or** the ring's drop notice; it never lets the line be silently
absent. The desk protocol for this plan
(`hardware-walk-protocol.md` in the plan dir) tells the desk to look for
`[LINK] n log records dropped` right after a multi-output load.

**Regression coverage** — the five-wire gate above (drop tolerated, never
silent). None for the drop itself.

**Lesson** — a log path sized for one chip's drain rate (the C6's USB link
task, 4 records per pass) moved unchanged onto a chip that drains half as
fast from a task the engine can hold off, and the burst that exposed it is
exactly the moment the logs are most wanted: a project coming up. A
counted drop is the right failure mode; a ring size or drain rate chosen
per chip, measured against that chip's worst load, is the fix to reach for.
