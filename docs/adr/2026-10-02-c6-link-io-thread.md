# ADR: The C6's link IO runs on its own thread, and requests are answered before the render

- **Status:** Accepted (amended 2026-10-03 — the S3, and the classic, whose
  link thread measured worse on silicon and is opt-in; roadmap M2; see the end)
- **Date:** 2026-10-02
- **Deciders:** Photomancer (Yona; feel gate passed 2026-10-02)
- **Plan:** `lp2025/2026-10-01-1756-c6-link-io-thread` (PR #891), milestone M1
  of the Wi-Fi control roadmap `lp2025/2026-10-01-1832-wifi-control`
- **Evidence:** the spike report `lp2025/2026-10-01-1200-io-thread-spike/report.md`
  (branch `spike/io-thread` @ `bb3c6bdbf`, not merged), and the PR's numbers
  tables
- **Amends:** `2026-08-25-classic-uart-io-task-executor-isolation` (its
  "OS-thread creation is not public" premise; see the note added there)
- **Related:** `2026-09-27-lp-link-one-comms-layer`, `2026-07-06-sans-io-core`,
  `2026-07-28-esp32c6-flash-budget`
- **Related (Wi-Fi control roadmap, `lp2025/2026-10-01-1832-wifi-control`):**
  `2026-10-07-c6-wifi-link` (M6: the Wi-Fi link and its threads, which name
  this ADR for the link threads), `2026-10-06-radio-frame-rate-budget`
  (the frame-rate ceilings a radio has to stay under),
  `2026-10-01-network-link-security` (the secure link M6 puts on the network)
- **Supersedes:** None
- **Superseded by:** None

## Context

On the ESP32-C6 the USB lp-link task (`fw-esp32-common/src/usb_link/`) and
the server loop shared one thread-mode embassy executor. `tick_and_send` is
synchronous and renders first, so the link ran only in the server loop's
gap between frames: every receive, ACK, resend timer and reply waited for the
frame in flight. On the desk XIAO C6 rendering the PLAYFUL choker (25–45 ms
frames) that cost a request ≈1.6 frames (p50 45 ms, p90 54–112 ms), held the
link's own round trip at one frame (44 ms), and moved the 8 × 256 B transmit
window about once per frame (8 KiB/s; a 10 KB file write took 1.2–4.9 s).

The Wi-Fi control roadmap needs a prompt link first: a remote control is
judged by how soon the board answers, and later milestones put more traffic
on the same board.

## Decision

Two changes, shipped together, on the C6 only.

1. **The USB link task runs on its own preemptive esp-rtos thread**
   (`fw-esp32c6/src/io_thread.rs`, feature `io-thread`, **on by default**):
   priority 1 (the main task is 0; esp-radio's threads sit far above), a
   fixed **3 KB** stack, and its own `esp_rtos::embassy::Executor`, which
   runs the unchanged `usb_link_task`. A woken link task — USB input, a link
   timer, the doorbell — preempts the render at once.

2. **The server answers a tick's requests before it renders**
   (`LpServer::set_messages_first`, default off; only the C6 turns it on).
   The tick answers, yields once (runtime-neutral, and only when it sent a
   reply) so the link task can start the reply, then renders. A request's
   effect now shows in the **same** tick's frame.

**Neither half alone moves request latency** (spike, silicon): the thread
alone leaves the reply queued after the render (p50 45 → 54 ms); answering
first alone leaves the reply waiting for the next gap to be written (p50
45 → 44 ms). Together, a request waits only for the rest of the frame in
flight, never more than one frame. That is why they ship as one decision
and why messages-first is not turned on for a board whose link IO still
shares the render's thread.

### The lock, and its rule

The link task and the server's transport share one `lp_link::Link`
(`UsbLinkShared`). On one executor a `RefCell` keeps them apart; across
threads the chip injects a lock (`UsbLinkShared::leak_locked`, a
`LinkLock` hook; the S3 and the C6 without `io-thread` inject none). The C6
injects esp-hal's `RawPriorityLimitedMutex` at **`Priority1`**: esp-rtos's
context-switch software interrupt and its timer tick are both priority 1, so
no thread switch can land inside a borrow, while the RMT refill ISR at
`Priority::max()` is never held off. The `RefCell` stays as the reentrancy
check, and the lock asserts it is not entered inside a critical section.

**Rule: closures under `with_link` stay short.** They hold off every
priority-1 interrupt for as long as they run. Today each is bounded (an
`on_bytes` of ≤ 64 B, one ≤ 256 B frame cut, an event pop, a reply queued by
`send_external` with no copy, a log pump of ≤ 4 records); nothing copies a
large buffer under one. The rule is written at `usb_link_shared.rs`, where a
future editor will touch it.

### The idle wake is event-driven

The link loop used to wake every 10 ms with nothing to do, to pump the log
ring. On a thread of its own every wake preempts the render, and **silicon
charges ~300 µs for a pass that finds nothing** (flash-cache cold); the 10 ms
cadence cost **~10 % of the frame rate** on the C6. The emulator charged the
same pass ~31 µs and showed −0.7 % — a fidelity gap, filed as
`docs/defects/2026-10-01-the-emulated-c6-charges-a-cold-code-path-10x-less-than-silicon.md`.
So the loop now sleeps until the link's own timers, USB input or the
doorbell, and a log record rings the doorbell
(`log_ring_logger::ring_on_record`); `IDLE_BACKSTOP_US` (250 ms) is only a
backstop. `#[ram]` for the link loop was held in reserve (Q6) and not needed:
the shipped build shows no frame-rate cost on silicon.

### Where the thread API comes from

esp-rtos 0.3's own thread creation is crate-private. The public face is
`esp_radio_rtos_driver::task_create` (esp-radio-rtos-driver 0.3), the call
esp-radio starts its Wi-Fi and BLE threads through, which esp-rtos
implements only with its `esp-radio` feature — so `io-thread` requires the
C6's `radio` feature. Arguments go through `task_create`'s parameter (a
leaked `Box`, taken back once by the thread). Stable while esp-rtos 0.3 /
esp-radio-rtos-driver 0.3 are pinned; an upstream ask for public thread
creation and a stack range is queued with the roadmap's director.

### The stack figure

3 KB (3,152 B with esp-rtos's guard). High-water 1,264–1,312 B in every
spike run, emulated and silicon, idle and under load, and 1,064–1,200 B on
this build. The stack comes off the heap, so the thread is created early in
boot, before a project fragments it. **No product code scans esp-rtos's
private `Task` record**: painting and the high-water line live only in the
`io_thread_stack_diag` feature (off).

### The meter is a real tool

`lp-cli link rtt` (serial device in wall-clock time, or `emu:<ELF>` in
emulated time) and lp-link's `Link::rtt_last_sample()` / `srtt()` stay in
the tree: the measurement that decided this is repeatable (`lp-cli/README.md`).

## Measured

`lp-cli link rtt`; request = `ListLoadedProjects` × 150 with random 0–40 ms
pauses; transfers = 10 × 10 KB reads and 10 × refused 8 KB `WriteChunk`s.
The choker's frame cost varies with pattern time, so each phase starts at a
fixed **board time**, the spike's points.

**Silicon** — XIAO C6 `10:bd:a3:b0:8e:30`, PLAYFUL choker, wall clock, BLE
advertising with no central; transfers at board 40 s, requests at board 63 s.

| | main `0380c63f8` | this build | acceptance |
|---|---:|---:|---|
| idle fps | 33.07 | 32.98 (−0.3 %) | within 2 % ✓ |
| request p10 / p50 / p90 / max ms | 32.0 / 44.6 / 54.1 / 244 | 6.4 / 24.4 / 30.4 / 90 | p50 ≤ ~26 ✓, p90 ≤ ~35 ✓ |
| link RTT p50 (request phase) | 43.8 ms | 1.31 ms | ≤ ~2 ms ✓ |
| board→host / host→board | 8.3 / 8.6 KiB/s | 170.3 / 180.1 KiB/s | ≥ 100 ✓ |
| 10 KB file write | 1,471 ms | 173 ms | |
| link thread stack high-water (diag build, USB load) | — | 1,064 B of 3,152 B | |

**The phase is part of each number.** Inside the choker's slow-frame window
(80–90 ms frames around board 69–72 s) every request and transfer waits on a
long frame: there the build's request median is 63 ms against main's
165 ms, and transfers started at board 33 s read 96 / 92 KiB/s (a 50 ms
backstop build reads the same, so the backstop is not the cause). The
acceptance targets above are met at the spike's phase, not inside that
window.

**Emulated** — `lp-emu:esp32c6:t2`, blank flash, choker deployed, requests at
40 s emulated; compare in frames, never against silicon in ms.

| | main `0380c63f8` | this build |
|---|---:|---:|
| idle fps | 67.195 | 67.188 |
| request RTT in frames p10 / p50 / p90 | 1.16 / 1.64 / 1.96 | 0.21 / 0.70 / 0.99 |
| link RTT p50 | 18.75 ms | 2.0 ms |
| board→host / host→board | 15.2 / 40.1 KiB/s | 289.7 / 304.4 KiB/s |
| heap free / largest block (55 s) | 88,712 / 63,197 B | 85,328 / 59,808 B |
| WS281x decode off the pad | 4,253 frames, 0 errors, 0 incomplete | 3,850 frames, 0 errors, 0 incomplete |
| RMT refill entry_max / unanswered | 20 of 24 words / 0 | 20 of 24 words / 0 |

**RMT on silicon** (`ws281x_telemetry`): 0 errors on both builds. Most of the
choker's frames end on a guard trip **on main too** (main 79 %, this build
74 %); that is filed open as
`docs/defects/2026-10-01-the-c6-choker-truncates-most-ws281x-frames-on-silicon.md`
and is not this change's.

**Feel gate** (Yona, 2026-10-02, the C6 on this build, Studio at
lightplayer.app): knobs, edit-and-undo and playlist switches read back right
and "feel snappier".

**BLE with a connected central:** a connected central (Bluefy on iPhone)
controls the board while USB is live. Bluefy showed frequent
disconnect/reconnect popups, judged separate from this work and tracked
separately.

## Consequences

- Requests on the C6 wait at most for the frame in flight; the link's round
  trip and transfers no longer depend on frame time.
- **Ordering changed:** with messages first, a write is seen by the same
  tick's render. Studio's write-then-read flows were walked at the feel gate
  and read back right. The first gate attempt hung — a Studio roster bug on
  main (two saved registry rows sharing one `device_id`), found during the
  gate and fixed in #926
  (`docs/defects/2026-10-02-saved-records-sharing-a-device-id-misroute-the-board.md`).
- **Every `with_link` masks priority-1 interrupts** on the C6. That is
  esp-rtos's scheduler, and also esp-radio's radio interrupts, which it
  enables at `Priority1` on this chip (the BT MAC and LP-timer interrupts in
  `third_party/esp-radio/src/ble/os_adapter_esp32c6.rs`, the Wi-Fi MAC
  interrupt for ESP-NOW) — not the RMT refill. A radio interrupt that lands
  during a closure waits for it to end. The short-closure rule is what keeps
  this cheap.
- **Boot has two writers on the USB IN endpoint:** the link thread sends its
  SYNs while the main thread still prints boot text raw through
  esp-println. The IN-endpoint gate keeps them apart (the gated image
  refuses 0 B in boot; `lp-cli/tests/emu_usb_free_lag.rs` pins it).
- **Heap:** the thread costs its stack plus ~0.2 KB (emulated steady free
  −3,384 B, largest block −3,389 B); figures re-blessed.
- **At the time this ADR was accepted, the S3, the classic, the host and
  browser servers kept their threading and their order**: they injected no
  lock and left messages-first off. One shared change reaches every board
  that runs `fw-esp32-common`'s UART or USB link loop: the event-driven idle
  wake, so a link task sleeps until a timer, input or the doorbell (250 ms
  backstop) instead of waking on a fixed cadence. **The S3 and the classic
  each got their own link thread under roadmap M2** — see the S3's amendment
  and `## Amended 2026-10-03 — the classic (M2)` below. The host and browser
  servers keep their threading and their order; the S3's second core and
  the S31 are roadmap M2 follow-ups.
- **`io-thread` depends on `radio`** (esp-rtos's `esp-radio` feature). A C6
  build without radio falls back to the shared executor, render first.

## Alternatives considered

- **An `InterruptExecutor` on a software interrupt, as on the classic.**
  Rejected here: work on an interrupt executor runs on the interrupted
  task's stack and cannot await embassy-time (classic ADR, findings 1–2), so
  the `Link` could not move there without a pacer and byte-shuttling split;
  the classic itself keeps its `Link` on the thread executor for that
  reason. A real thread has its own stack and working timers.
- **A render that yields mid-frame.** Would shorten the remaining wait
  (≈0.5–0.7 frame) further, but touches the engine and every shader path;
  out of scope, and the thread gets most of the win without it.
- **Messages first alone.** Measured: no request gets faster, because the
  reply still waits for the next gap to be written.
- **The thread alone.** Measured: the link becomes prompt and transfers
  ~20× faster, but request p50 gets worse (45 → 54 ms): the request still
  waits for the next loop top and a full render.
- **A critical-section mutex for the shared link.** Would mask every
  interrupt for each closure, including the RMT refill the LEDs cannot
  wait on. The priority-limited lock masks only what can switch threads.
- **Scanning esp-rtos's `Task` record for the stack range in product.**
  The spike did it; it reads a private layout. Kept only in the diagnostic
  feature.

## Follow-ups

- Upstream: esp-rtos public thread creation and a stack range (queued by
  the roadmap's director). Revisit `io_thread.rs` when esp-rtos exposes them.
- Other boards on a link thread: roadmap M2.
- BLE's host or a Wi-Fi stack on the thread: roadmap M6.
- The emulator's cold-path cost: the open fidelity defect named above.

## Amended 2026-10-03 — the S3 (M2)

The ESP32-S3 gets the same treatment, on roadmap M2
(`lp2025/2026-10-02-1918-io-thread-other-boards`, PR #942, phases P3/P5).

**The shared USB link task runs on its own esp-rtos thread, priority 1,
pinned to core 0** (`pin_to_core: Some(0)`, never `None`) — 4 KB stack,
`lp-fw/fw-esp32s3/src/io_thread.rs` a per-chip copy of the C6's (OQ7: the
thread API is not shared through `fw-esp32-common`, which names no esp-hal
thread type). **Why pinned, and not left unpinned on a dual-core chip:**
esp-rtos 0.3's SMP scheduler cross-wakes between cores, so an unpinned task
can land on core 1 and raise that core's own SWI1 — on the classic that is
the wire-pusher's doorbell; on the S3 core 1 is not started today (below),
so pinning is cheap insurance against ever landing there. The link is
shared behind `UsbLinkShared::leak_locked`, the same priority-1
`RawPriorityLimitedMutex` the C6 injects — `esp-sync`'s lock is already
cross-core on a `multi_core` build (keyed by core id, compare-and-swap), so
this is the identical lock, not a port of it. `set_messages_first(true)`
ships with the thread, never alone, per the C6's own finding above.

**`io-thread` needs `esp-rtos/esp-radio` *and* `esp-rtos/esp-alloc`, and
links esp-rtos's small esp-radio glue — not a radio stack.** Neither the S3
nor the classic enables an actual radio; `esp_radio_rtos_driver::task_create`
(the public thread-creation call, per the C6 ADR's own 2026-10-02 amendment
above) exists only behind esp-rtos's `esp-radio` feature, and that feature
turns on `alloc` without `esp-alloc`, so the firmware must supply it itself
or esp-rtos's `malloc_internal` glue fails to link (discovered while
planning M2; see the plan's Discovery section).

**Numbers, emulated** (`lp-emu:esp32s3:t1`, lp-emu `ab8345d38`, main image
`ab8345d38` vs branch image `a09383698`, the scaled rig P3 used because this
emulator runs ~50x slower than board time): idle fps −0.10 %; request RTT
p50 6.50 ms (1.56 frame) → 3.00 ms (**0.72 frame**, target ≤ ~0.8 ✓); p90
7.75 ms (1.86 frame) → 4.75 ms (**1.14 frame**, target ≤ ~1 — **missed**: a
request landing just after a frame starts waits for the rest of it plus the
answering tick, so the tail sits a little over one frame; p50 meets its
target and moved ~0.84 frame; not tuned, carried to the desk walk as a
question rather than chased); link RTT p50 5.25 → 1.75 ms; transfers
100.3/118.0 → 376.1/438.6 KiB/s; fps during transfers −9 % (the thread now
moves 3.7× the bytes per second while a frame renders). F32 under the same
load (`projects/test/shader-oracle-f32`, new in this plan — no Float-mode
project existed under `projects/` before it): 1 distinct frame after the
first across quiet warm-up, transfers and requests — identical with the
link quiet and under load; the host oracle cannot render Float, so this is
the self-consistency check the plan's Discovery section called for, not a
host comparison. Link-thread stack high-water (diag build): 1,680 B of
4,176 B (40 %). Image +6,032 B of 6 MB; heap +4,480 B (the 4 KB stack,
16-aligned, plus the task record and executor); largest free block
−4,480 B. Full tables: PR #942's body.

**Silicon** (`desk-s3.md`, 2026-10-03, desk ESP32-S3 `D8:3B:DA:47:29:70`,
main `9f70f39da` vs branch `20e9b64e5`; full tables in PR #942's body): idle
fps −0.6 % (within the 2 % bar); request RTT p50 1.70 → **0.80 frame**
(target met); p90 2.10 → **1.21 frame** — over the ~1 frame target, at the
same structural floor the emulated number predicted: the fastest possible
request has a fixed ~0.24 frame service cost (link RTT plus the board's
answer), so p90 ≈ 0.9 frame of wait + that floor and cannot clear one frame
by tuning the thread. Link RTT p50 ~14.6 → 1.15–1.27 ms; transfers
13–14 → 232–242 KiB/s both ways (≥ 100 KiB/s target cleared by 2×). Link
thread stack high-water (`io_thread_stack_diag`): **1,632 B of 4,176 B
(39 %)**, close to the emulated 1,680 B. Heap: **+4,588 B** used on first
upload (emulated ratchet predicted +4,480 B — within 0.5 KB). F32 checksums
identical across 47 `[OUT]` lines under load; `m4-hardware-walk.sh`
byte-identical against both host oracles.

At Studio's own request rate (~7/s, a matched-rate follow-up run on the
same board and images): the branch costs **≈1.6×** the render time per
request that main does (≈0.36 vs ≈0.22 frame — **≈5 % vs ≈3 % fps**), for
roughly half the request latency (p50 15 vs 30 ms, p90 22 vs 38 ms). The
emulator disagreed with silicon on the frame-rate cost of link load (it
showed +0.2 %; silicon loses 7–20 %, rate-dependent) — filed as a fidelity
defect:
`docs/defects/2026-10-03-the-emulated-s3-shows-no-frame-rate-cost-for-link-load.md`.

**Yona accepted both the p90 structural floor and the per-request render
cost as measured (2026-10-03).** The G-S3 desk walk passed: Studio against
the branch image, Yona's own words — "working so much better than before.
Very snappy, and I even left it on for an hour and everything went well."

### Follow-up: the S3's second core

Not in M2. Seed for the roadmap's future-work list:

> **Move the S3's link thread to core 1.** `esp_rtos::start_second_core`
> (swi1, a stack taken from the heap — `Stack` is `.bss` by the esp-rtos
> API) already exists; the lock is already cross-core (above), so the move
> changes the pin, not the lock.
>
> **Trigger:** radio on the S3 (esp-radio's S3 Wi-Fi adapter binds its MAC
> interrupt to CPU0 at Priority1 — the C6's radio-interrupt problem, back
> exactly, if the link thread stays on core 0 once Wi-Fi is on it), or a
> measured render cost from the core-0 thread.
>
> **Prerequisites:** (1) a cooperative flash-write handshake with core 1 —
> esp-storage's `multicore_auto_park` stalls core 1 at an arbitrary
> instruction and then enters a critical section; a core-1 link task
> allocates and logs, so if it held the critical-section spinlock (the heap
> allocator, the log ring, every `Signal`) core 0 deadlocks on the write.
> Core 1 must instead spin in IRAM, masked, until the write completes (the
> ESP-IDF shape) — this does not exist today. (2) An S3 core-1 desk canary:
> nobody has measured where an S3 core 1 begins. (3) Core-1 release in
> `lp-emu-esp32s3` — slot 1 is held, deliberately, not started; a core-1
> move cannot be emulated until it is. (4) `CPENABLE` armed on core 1 if F32
> ever runs there (esp-hal's `float-save-restore` puts FP regs in the trap
> frame esp-rtos switches, so core 0 should survive preemption today, but
> core 1 starts with no guarantee `CPENABLE` carries over).

### The ESP32-S31

Nothing now (OQ8): no esp-hal support in the pinned esp-hal 1.1.1, and no
desk board. When it arrives: RISC-V like the C6, so `io_thread.rs` ports
nearly verbatim with `pin_to_core`; the lock is cross-core already, so it
needs no change; RAM is large enough to put a comms thread with Noise/TLS
stacks on the second core, which raises the same flash-handshake question
as the S3's core 1.

### The lock across boards (input to M6)

| Board | Lock | Holds off (holder's core) | Never | Radio |
|---|---|---|---|---|
| C6 | `RawPriorityLimitedMutex(P1)` | sched SWI+tick, BT MAC, BT LP-timer, Wi-Fi MAC | RMT | M6 must solve |
| S3 core 0 | same | sched, USB-SJ | RMT | none now; Wi-Fi MAC (P1, CPU0) if ever |
| S3 core 1 (follow-up) | same (cross-core) | as above; other core spins | RMT | as above |
| Classic core 0 | same | sched, io pacer, UART0 | io_task, RMT, wire-pusher doorbell | none |

Two radio-safe shapes for M6 to choose between, both behind the existing
`LinkLock` hook: (1) an esp-rtos mutex with priority inheritance
(`esp_radio_rtos_driver::semaphore`, `SemaphoreKind::Mutex`) — masks
nothing, cross-core, costs a scheduler critical section per take/give;
valid because no `with_link` caller is an ISR. (2) Mask only the
scheduler's own interrupt lines rather than the whole P1 level — cheaper,
chip-specific. M2 did not need to wait on this choice.

## Amended 2026-10-03 — the classic (M2)

Plan `lp2025/2026-10-02-1918-io-thread-other-boards` (PR #943, milestone M2
of the Wi-Fi control roadmap) gave the classic ESP32 ("v3") the same
treatment, in two steps.

### Step 1 — messages-first alone, measured and left off

This ADR's sentence — "messages-first is not turned on for a board whose
link IO still shares the render's thread" — does not read across to the
classic unchanged, because the classic is the one board where it already
did not apply literally: io_task (swi2, `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`)
drains UART0's bytes every 1 ms regardless of the render, off the render's
thread, before any link thread exists. P2 turned `set_messages_first(true)`
on with no `Link` thread and measured it on `five-wire` (80 LEDs, ~2.3 ms
frames, `lp-emu:esp32v3:t1` at `ab8345d38`): flat — request p50/p90
unchanged at 2.26/2.69 frames, idle fps within 0.05 %. Per this ADR's own
≥ ~0.3-frame bar it was **left off** at step 1.

**The refined rule**: what has to leave the render's thread for
messages-first to pay is the **`Link`'s own scheduling** — its ACK/resend
bookkeeping and frame queuing — not merely the byte shuttle underneath it.
On `five-wire` the link round trip itself (4.25 ms, ≈ 1.8 frames) exceeds
one frame, so no reordering inside a single tick can show in the request
number regardless of where the bytes move; the classic's `Link` task was
still cooperatively scheduled on the render's own thread executor, same as
the C6 before its thread. Step 2 (below) is the case that actually tests
the pairing, on a frame-bound project.

### Step 2 — the `Link` on its own core-0 thread

`fw-esp32v3/src/io_thread.rs` (feature `io-thread`; default on as first
built, **off by default since the silicon A/B below**): the
unchanged `uart_link_task` (`fw-esp32-common/src/uart_link/`) now runs on a
priority-1 esp-rtos thread, **`pin_to_core: Some(0)`**, a 3 KB stack, its
own `esp_rtos::embassy::Executor`, under `RawPriorityLimitedMutex` at
`Priority1` injected through `UartLinkShared::leak_locked` (the same
`LinkLock` hook the C6 and S3 use). **Messages-first goes on with it**,
never alone, matching the C6/S3 rule exactly. io_task (swi2, the 1 ms
pacer, `SendUart`, the pipes) is untouched and still the only thing that
touches UART0; the `Link` is still never polled from io_task's executor
(DD20 of the executor-isolation ADR holds — see that ADR's own amendment).

`io-thread = ["server", "esp-rtos/esp-radio", "esp-rtos/esp-alloc",
"dep:esp-radio-rtos-driver"]` — the same pair `radio_ram_probe` already
sets; no radio stack is linked. **SWI1 is never raised for this thread**:
both the main task and the link thread are pinned to core 0, so esp-rtos's
`run_scheduler`/`trigger_scheduler` only ever raise SWI0 (core 0's own
yield, or a request *to* core 0); core 1 never runs the esp-rtos scheduler
and so never claims the wire pusher's doorbell (SWI1,
`output/rmt/wire_pusher.rs`) on esp-rtos's behalf — read off esp-rtos 0.3.0,
not yet exercised on silicon.

### Numbers (`lp-emu:esp32v3:t1`, lp-emu `ab8345d38`, `lp-cli link rtt`, `--seed 7`)

`five-wire` is P1/P2's 80-LED project (~2.3 ms frames, link-RTT-bound).
`five-wire-x32` is a frame-bound scratch variant (same 80 LEDs, the shader
body run 32× per LED, ~12.9 ms frames) the director asked for once
`five-wire` could not move: not committed (see PR #943's body for how to
reproduce it).

| project | build | idle fps | request p50 (frames) | request p90 (frames) | link RTT p50 ms | transfers KiB/s (↓/↑) |
|---|---|---:|---:|---:|---:|---:|
| five-wire | main | 430.04 | 2.26 | 2.69 | 4.25 | 81.9 / 81.6 |
| five-wire | P2 (messages-first, no thread) | 430.18 | 2.26 | 2.69 | 4.25 | 81.9 / 81.4 |
| five-wire | **P4 (thread + messages-first)** | 429.84 (−0.05 %) | **1.83** | **2.26** | **3.50** | 82.2 / 82.3 |
| five-wire | P4, messages-first off | 429.87 | 2.26 | 2.69 | 3.75 | 81.6 / 81.9 |
| five-wire-x32 | main | 77.28 | 1.68 | 2.09 | 10.50 | 59.1 / 58.8 |
| five-wire-x32 | **P4 (thread + messages-first)** | 77.27 (−0.01 %) | **0.79** | **1.18** | **3.75** | **82.3 / 78.2** |
| five-wire-x32 | P4, messages-first off | 77.27 | 1.70 | 2.11 | 3.75 | 74.3 / 72.2 |

The frame-bound project is the one that decides it: the thread alone moves
link RTT and transfers but leaves request latency unchanged (1.70 frames,
same as main); thread + messages-first together bring it to 0.79/1.18
frames — the C6's "neither half alone" finding, confirmed on the classic.
`five-wire` improves (2.26 → 1.83) but cannot reach the ~0.8-frame bar: its
frame (2.3 ms) is shorter than the UART round trip itself.

### Heap, stack, and other proofs (emulated)

- **Heap** (the ~5 KB stop line, OQ2): free 217,592 → 214,136 B
  (**−3,456 B**), largest free 102,353 → 98,898 B (−3,455 B) at the shipped
  3 KB stack — under the line (the silicon A/B below turned the thread off
  by default on other grounds).
- **Stack** (`io_thread_stack_diag`): high-water 1,660 B at both 4 KB and
  3 KB, 1,692 B under the `--uart-faults` soak — 55 % of 3 KB.
- **Log drops** (the open log-ring defect, five-wire load,
  `emu_v3_link_gates`): main 25, P2 14, P4 **0** — see that defect's updated
  status.
- **Fault soak** (`emu_uart_link`, three specs): green, 0 app errors, 0
  resets, fewer board resends and duplicates than main.
- **Resend floor**: left at 200 ms. Link RTT under combined load (P4) p99
  15.0 ms, well under the floor, but the emulator does not model the host
  side (the CH340, macOS's USB-serial latency, Web Serial) that the floor
  also covers — **measure silicon link-RTT p99 at the desk walk** before
  moving `uart_board_link_config`'s `MIN_RTO_US`.
- **Silicon: at the desk walk** (`desk-classic.md`, batched with #884's
  owed classic lp-link walk) — not yet run.

Update the earlier "The host and browser servers keep their threading…"
Consequences bullet above: it now points here for the classic's own thread.

### Silicon (2026-10-03): the thread goes off by default, and messages-first stays off

The desk A/B the emulator could not do (DOM-Z-102, agent-run, a ~16 fps
project; `frame_pace_diag` timestamps every frame as it reaches the RMT
driver; every row on one merged base, origin/main `626a1b851`). Frame
interval p50/p90 and judder p90 in ms; "slow frames" are those longer than
1.5× the median.

| arrangement | no host: frames · judder | `link rtt`: idle · transfers p90 · requests p90 · request RTT p50 · transfers | real Studio editor, per 5 s: frames · slow frames · judder |
|---|---|---|---|
| main executor, render-first (main) | 61.0/61.1 · 0.2 | 61.1 · ~66 · ~65.6 · 105 ms · 13.5 KiB/s | 47 · 13 at ~222 ms · ~3.5 |
| **main executor, event-driven wake (P2; the default now)** | **57.5/57.7 · 0.5** | **57.6 · ~62 · ~61.9 · 98 ms · 14 KiB/s** | **50 · 14 at ~209 ms · ~3.3** |
| thread, every event, messages-first (as first built; older base) | — | 58.0 · 194 · 71.0 · 46 ms · 56–68 KiB/s | 39 · 15 at ~240 ms · **181** |
| thread, ≥ 25 ms between passes, messages-first | — | 69.0 · ~94 · ~83.7 · 60 ms · 28–31 KiB/s | 34 · 13 at ~270 ms · **195** |
| thread, ≥ 25 ms between passes, render-first | 66.7/68.9 · 2.5 | 66.7 · ~93 · ~77.5 · 125 ms · 25 KiB/s | 41 · 11–12 at ~258 ms · ~6–8 |

What the emulator could not see, and why the rows fall this way:

- **A preemption costs the classic's render ~4–5 ms**, several times the
  link pass itself (~0.5–0.9 ms on silicon, 13 µs emulated): on an idle
  board, frames a keepalive pass landed in rendered ~4.8 ms slower than the
  frames it missed. The render's code and data come through the flash cache,
  and a pass displaces them. An event-driven thread woke for every piece of
  I/O-task news — 300–450 passes a second during a transfer — and stretched
  frames to ~190 ms. Holding passes ≥ 25 ms apart
  (`PassPacing::CLASSIC_LINK_THREAD`) bounds that, but the thread still
  preempts on keepalives, SYNs and the backstop: −9 % frame rate with no
  host at all.
- **Messages-first is a judder source on this board.** Studio's editor
  read costs the classic ~140–175 ms of CPU
  (`docs/defects/2026-10-03-studios-editor-read-costs-the-classic-a-frame-and-a-half-of-cpu.md`).
  Answered before the render, that lands between the moment a frame takes
  its clock and the moment it reaches the LEDs: judder ~180–195 ms. Answered
  after, the next frame is merely late and motion stays true. The C6/S3 rule
  ("the thread and messages-first go together") holds only where the host's
  requests are cheap next to a frame.
- **Without messages-first the thread buys no latency** (request RTT 125 ms
  against the default's 98 ms) — only transfer speed — and costs frames in
  every setting. So the classic ships **without** the thread: `io-thread` is
  an opt-in feature (with its 25 ms pacing and render-first), kept for the day
  the host's requests are cheap enough here for messages-first to pay. The
  event-driven wake (P2) is the default and is the best row in all three
  settings.
- The Studio editor still hitches on every arrangement; that is the read's
  cost, not the link's, and it is its own defect.
