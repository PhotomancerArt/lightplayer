# Classic ESP32: io_task executor isolation — interrupt executor + hardware pacer + byte shuttling

- Status: accepted
- Date: 2026-08-25
- Plan: `lp2025/2026-08-24-1823-uart-io-task-starvation` (PR #448)
- Fixes: `docs/debt/shared-uart-io-task-starvation.md`
- Related: `docs/defects/2026-08-21-hello-gate-assumes-fresh-boot.md`,
  ADR 2026-08-02 classic-hli-refill (rejected/parked — the rsil-5 lore
  cited below), `docs/defects/2026-08-02-serial-line-interleaving.md`

## Context

On the classic ESP32 ("v3"), UART0 is the only host link and io_task —
the task that services it — ran as a cooperative peer of the server
loop on the one thread-mode embassy executor. `tick_and_send` is
synchronous, so a dome-scale engine tick (~41 ms baseline; 103–114 ms
on the bench project used here) was a hole in UART servicing exactly
that long. The hardware RX FIFO is 128 bytes — ~1.4 ms of line time at
921600 baud — and there is no flow control (UART0's pins are fixed, the
CH340's DTR/RTS pair is the reset strap, and Web Serial does not expose
hardware flow control). Measured consequences (2026-08-21 bench):
inbound frames >~128 B died to `FifoOverflowed` even paced, outbound
responses died on a 250 ms per-chunk TX timeout that was really
measuring executor starvation (a chunk costs ~0.7 ms of line time — a
350× margin — so the timeout could only mean "io_task went unpolled"),
and every serial-touching feature carried the `stopAllProjects`-before-
big-writes workaround.

## Decision

Three mechanisms, each forced by a bench-measured failure of the
simpler design above it:

1. **io_task runs on an `esp_rtos::embassy::InterruptExecutor` fed by
   software interrupt 2 at `Priority2`.** The SWI preempts the thread
   executor, so an engine tick can no longer starve UART servicing.
   Priority2 sits strictly below the RMT ISR's `Priority::max()`
   (= Priority3), which binds on the PRO core in the single-core
   fallback. **swi2, not swi1**: swi1 is already the APP-core
   wire-pusher's frame doorbell, claimed via `steal()` and therefore
   invisible to peripheral ownership (see Discovery finding 3).

2. **io_task's only time source is a hardware pacer** — TIMG0's second
   timer (esp-rtos's scheduler uses timer0) fires a 1 ms ISR at
   Priority1 that signals an `embassy_sync` `Signal`. io_task must
   never await embassy-time (see Discovery finding 1); loop pacing, the
   per-chunk write timeout, and the retry backoff are all tick-counted
   through a `DelayNs` seam in `ChunkedWriter` (C6/S3 keep
   `embassy_time::Delay`; behavior there is unchanged).

3. **io_task only shuttles bytes.** Server frames are serialized in
   thread context by the transport (`serialize_server_msg`, heap `Vec`)
   and the write-request channel carries framed bytes; io_task's
   per-poll stack cost is one chunked write plus an RX drain (see
   Discovery finding 2). UART interrupt registration (`into_async()`)
   also happens thread-side; the `!Send` `Uart<Async>` crosses the
   `SendSpawner` boundary in a documented `SendUart` wrapper (sound:
   constructed on the PRO core, moved exactly once into a task whose
   executor is bound to the same core).

The outbound half is made honest end to end: the UART write policy
retries a failed frame write twice (the serialized bytes are still in
hand; the frame's leading `\n` is the parser resync), exhausted drops
log at `error` level, and the transport sends a best-effort
`ServerMsgBody::Error` frame carrying the original request id so the
client fails fast instead of timing out. Session boundaries: an inbound
hello (the firmware's only "new client attached" signal) drains
outbound lines queued for the dead session, and a partial RX line that
stops growing for 1 s is flushed so it cannot prefix-corrupt the next
session's hello (the 2026-08-21 hello-gate wedge).

## Discovery: three failures the bench forced us through

This section is the reason this ADR exists — each finding invalidates a
"reasonable" design and will bite again if forgotten. All were measured
on the bench dig2go (CH34x, quinled/dig2go, project "studio" at
103–114 ms ticks) via `spikes/serial-lab`, 2026-08-25.

**Confidence honesty**: finding 3 (the swi1 collision) was discovered
LAST, and it retroactively confounds parts of findings 1 and 2 — several
crashes and wake losses attributed to them are equally well explained by
the collision, and the changes were not A/B-isolated at the bench. Each
finding below carries its evidence grade. The interrupt-executor
disciplines stand regardless (they are correct ISR hygiene); what is
graded is the *causal attribution*. A dedicated fwcheck harness
(`test_interrupt_executor`, below) exists to test the load-bearing
claim in isolation.

### 1. embassy-time wakes did not reach the interrupt-executor task [grade: strong for the behavior; mechanism attribution pending isolation]

The naive port — io_task unchanged, spawned on the interrupt executor,
paced by `Timer::after(1 ms)` — parked at its first timed await forever.
A probe task on the *thread* executor received 5/5 timer wakes while
io_task's expired entry in the same timer queue received none. The
first missed wake fell due *before* the APP core (and hence the swi1
doorbell collision of finding 3) started, which argues this is a real
esp-rtos time-driver gap for interrupt executors — plausible, since
esp-radio (the only real upstream consumer of them) runs purely
event-driven tasks there. But the executor under test sat on swi1, so
the collision cannot be fully excluded from these observations; the
`test_interrupt_executor` harness re-asks the question on swi3,
collision-free. The associated engine-load `InstrError` crash loop is
*not* confidently attributable here — the collision explains it at
least as well. Consequence either way: **every wake io_task depends on
is a direct waker wake** (channel/signal/ISR), which the pacer
provides, and which is robust under both explanations. If the harness
shows upstream timer wakes are actually fine on a clean SWI, the pacer
becomes a simplification candidate (back to `Timer::after`) — do that
deliberately, with the harness as the gate, not by assumption.

### 2. Interrupt-executor polls run on the interrupted task's stack [grade: architecturally certain; crash attribution ~40%]

esp-rtos's SWI handler polls the executor on whatever stack the
interrupt landed on — usually the main task's, whose budget on this
chip is 47,136 B (already 6 KB tighter than the S3's proven stack).
That much is a fact of the mechanism, not an inference. io_task's old
write path materialized a 16.7 KiB serialization buffer and ran serde
recursion per frame; the observed `InstrError`'s faulting PC sat
*inside the stack region* with a zeroed return register, consistent
with that path landing on a deep main stack mid-project-load. BUT the
crash is equally consistent with the swi1 doorbell collision (frame
doorbells begin exactly at project load), and the byte-shuttle change
and the swi2 move landed together, un-A/B'd. The rule stands on
architecture, not on the crash: **work done on an interrupt executor
must have ISR-scale stack budgets** — hence byte shuttling
(serialization/parse thread-side), which also shed the 16.7 KiB
async-state buffer from all three chips' io tasks (a TODO the code had
carried since the S3).

Two adjacent disciplines, kept as hygiene with the same honesty about
attribution: `into_async()` (interrupt registration) now happens
thread-side before spawn — the "enable does not survive the handler's
exit" mechanism is *conjecture* (the hoist alone did not change the
symptom; the collision later explained it), but registering interrupts
from ISR context is wrong on principle and costs nothing to avoid.
`esp_println` is banned in interrupt-executor context — the crash
correlated with a diagnostic print there was confounded with
timer-queue processing, but esp-sync's locks are priority-limited
(esp-rtos's own timer handler runs at Priority1 "to prevent
accidentally interrupting priority limited locks"), so the ban is
sound regardless.

### 3. swi1 was never free: `steal()` hides ownership [grade: ~95%, mechanism read in code + fix confirmed]

The plan chose swi1 because `init_board` demonstrably dropped software
interrupts 1–3 unused. But `output::rmt::wire_pusher` raises and resets
`SoftwareInterrupt::<1>` via `steal()` as the APP core's frame
doorbell — out-of-band of the peripheral singleton, so no grep of
constructor plumbing finds it. With the executor also on swi1, each
side consumed the other's raises: io wakes died the moment the APP core
started (~0.5 s into boot, exactly when `bind_doorbell` runs), and
frame doorbells died under the executor. The fix is swi2; the lesson is
**a `steal()` is an ownership claim that must be searched for
(`rg "SoftwareInterrupt::<"`), not inferred from constructor flow.**

## Alternatives considered

- **Interrupt-fed RX ring (P6 of the plan).** The "actually correct"
  inbound fix in the abstract: a UART ISR drains the FIFO into a ring,
  removing the 1.4 ms deadline entirely. Held as a conditional
  follow-up; **not needed** — the bench showed zero inbound loss at 1 ms
  pacing under a 114 ms tick (the executor preempts the tick, so the
  cadence holds). Re-open if a future workload shows residual
  `FifoOverflowed` (the erratum note at `poll_rx_into` — the classic
  cannot clear the RX-timeout interrupt while the FIFO is non-empty —
  binds any such design).
- **Hardware or XON/XOFF flow control.** Genuinely supported by the
  chip but blocked by the board: UART0's pins are fixed GPIO1/GPIO3,
  the CH340's DTR/RTS pair is the auto-reset strap, Web Serial does not
  usefully expose hardware flow control, and XON/XOFF on a link that
  also carries free-form log text is a framing hazard.
- **Chunk-ack wire protocol.** Protocol-wide cost (both clients, three
  firmwares) to paper over a scheduling defect; rejected. Revisit only
  if a genuinely lossy link (not a schedulable one) joins the family.
- **Second core.** Core 1 is fully owned: RMT refill ISR + wire pusher,
  `app_core_main` must never return, and `with_app_core_stalled`
  hardware-stalls it around every flash write. Off the table.
- **A second esp-rtos OS thread** (own stack, working embassy-time —
  would have been the cleanest isolation). Not public API: thread
  creation in esp-rtos 0.3.0 is `pub(crate)` behind the esp-radio glue.
  Worth revisiting if esp-rtos exposes it.
- **Fork/patch esp-rtos** for finding 1. The honest upstream fix, but
  the broken line is not yet pinned and the blast radius includes
  esp-radio; an issue with this ADR's evidence is the right vehicle.

## Consequences

- The debt entry's exit criteria are met on silicon (bench, shipping
  build): a 4,592 B inbound frame lands byte-identical in 0.3 s while
  the project ticks at 103 ms; 10/10 request/response round-trips flow
  under load (median 0.34 s); a torn-frame disconnect/reconnect gates
  cleanly; zero `FifoOverflowed`, zero TX timeouts. The
  `stopAllProjects`-before-big-writes workaround is obsolete on this
  firmware (older flashed devices still need it).
- io_task pauses remain in two bounded places: esp-sync critical
  sections (µs-scale) and esp-storage's masked ROM windows around flash
  ops — per-operation, unlike the per-frame starvation. During
  flash-heavy phases the pacer's ticks collapse (measured ~40 Hz during
  boot's project load); tick-counted timeouts stretch accordingly,
  which is acceptable for a backstop.
- TIMG0's timer1 is now claimed (pacer); software interrupt 3 is the
  last free SWI. `WritePolicy` carries `server_msg_retries` (UART 2,
  USB 0), so C6/S3 behavior is unchanged; all three chips now serialize
  thread-side and shuttle bytes.
- The interrupt-context prohibitions (no embassy-time, no esp_println,
  no interrupt registration, ISR-scale stack only) are documented at
  `IO_TICK`, `SendUart`, and `serialize_server_msg` — the places a
  future editor will actually touch.
- **Post-facto validation**: the `test_interrupt_executor` fwcheck
  harness (fw-esp32v3, `just fwtest-iexec-esp32v3`) tests finding 1 in
  isolation on swi3 — no doorbell, no io_task, no engine. Run it after
  every esp-rtos bump: it is the canary that says when the pacer
  workaround can be retired (upstream fixed) or when executor semantics
  shifted. Its current silicon verdict is recorded below in
  "Harness verdict".
