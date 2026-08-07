# RMT refill ISR on the APP core: overlapped transmission, cross-core frame lifecycle

- **Status:** accepted (G1 passed 2026-08-04, "ship then optimize")
- **Date:** 2026-08-04
- **Plan:** `2026-08-04-1845-dualcore-rmt-isr`
- **Relates to:** `2026-08-04-concurrent-ws281x-flush.md` (supersedes its quiet-CPU
  constraint in part — see below), `2026-08-02-classic-hli-refill.md` (the masking
  family this sidesteps)

> **Superseded in part (2026-08-05)** by
> `2026-08-05-ws281x-transmission-on-app-core.md`: core 1 no longer merely
> idles in `waiti 0` between interrupts — it runs the wire pusher
> (admission, slot binding, pad re-mux, frame starts), and the "nothing on
> this core ever masks interrupts" phrasing is amended to a two-instruction
> masked window in the pusher's idle path. Everything else here stands.

## Context

M4 (concurrent flush) established that on the classic ESP32, WS281x transmission was
only safe while the CPU quietly spins: the app path masks interrupts in stretches
longer than the 80 µs refill deadline, so all wire time sat under admission/barrier
spins. At 1500 LEDs that left 18 fps against a ~31 fps engine-bound ceiling — the
frame paid for its wire time even though the chip has a second, idle core.

## Decision

Five pieces, one per layer:

1. **The RMT refill ISR is bound on the APP core (core 1), which runs nothing
   else.** Boot starts core 1 (`CpuControl::start_app_core`, 4 KiB static stack)
   into a function that binds the RMT handler — the bind must execute *on* core 1,
   because esp-hal maps a peripheral source into the calling core's interrupt
   matrix — sets the `ISR_ON_APP_CORE` flag, and idles in `waiti 0` forever. No
   scheduler, no critical sections, no prints: nothing on that core can ever mask
   the refill. Core-1 ownership is a standing claim future work (radio) must
   negotiate with.
2. **lp-ws281x's frame lifecycle is sound cross-core.** The single-core assumption
   ("thread context and the handler never overlap") is gone: `on_interrupt` marks
   itself in service through a driver-wide `isr_seq` (odd in, even out, `SeqCst`),
   and `abort` = stop → disarm → spin until the marker reads even, so frame bytes
   may be freed the instant abort returns. The Dekker-style `SeqCst` pairing between
   the marker and `frame_complete` is what excludes the store-buffer outcome; the
   full argument lives on `Ws281xDriver::abort` and in `state.rs`'s module docs, and
   is enforced by a Miri race harness (`just ws281x-miri`) whose oracle was
   validated against the known-broken shape.
3. **The service path lives in IRAM** (lp-ws281x feature `isr-in-ram`, section
   `.rwtext`). Not an optimization: the APP core's flash-resident code stalls behind
   the PRO core's cache misses on the shared SPI bus — measured as entry delays
   blowing the deadline only once transmission overlapped render, plus a
   once-per-boot 112-word stall while littlefs writes suspended the cache. With the
   path in IRAM, core 1 is immune to core 0's flash traffic.
4. **The provider's flush barrier is conditional.** `Ws281xOutput` gains
   `background_tx_safe()` (default `false` — every implementation keeps M4 barrier
   semantics until proven on its own silicon); the classic's output answers with the
   dual-core flag. `Esp32OutputProvider::flush` skips waiting on proven-safe wires,
   so wire time overlaps the next render; `write`'s wait-before-stage stays
   mandatory in both modes and carries the frame-lifetime duty. The engine calls
   `flush` every frame regardless — the barrier-count guard test and the
   wrapper-forwarding rules from the M4 near-miss stay in force.
5. **The dual-core admission cap is 3, and the reason is arithmetic.** One refill
   costs ≈15 word-times (≈18.75 µs, APB-write-bound, identical quiet or loaded);
   four coincident refills ≈ 94 % of the 80 µs deadline. On silicon the two
   last-serviced wires truncated essentially every frame at cap 4 (4,753/4,755),
   while cap 3 (~70 % duty) ran perfectly clean. M4's "cap 4 clean" data point was
   the same arithmetic surviving only under a quiet CPU. The fourth wire waits in
   the admission spin. Single-core fallback keeps cap 2.

## Measured (DOM-Z-102, zook-dome-1500 = 4×375, `ws281x_telemetry`, 2026-08-04)

| configuration | fps | tick | guard trips |
|---|---|---|---|
| merged-main baseline (M6+M4, cap 2, barrier) | 18 | 53 ms | 0 |
| dual-core cap 4 (with and without IRAM) | ~31 | 30 ms | two wires ~100 % — starved |
| **dual-core cap 3 + IRAM (shipped)** | **23** | **41 ms** | **0 on all wires** (+0 skips/errors, 130 s, boot incl.) |
| forced single-core fallback | 18 | 53 ms | 0 — the M4 baseline exactly |

Boot-steady heap at 1500: used 131,632 B, `largest_free` 43,606 B — the core-1
stack absorbed within the m6 compact-mappings relief. Runtime fallback proven: a
board whose second core will not start logs one `[INIT]` line and runs exact M4
semantics.

## The traps worth an ADR sentence each

- `Rmt::set_interrupt_handler` first **disables the RMT mapping on every other
  core**: one core-0 call after the core-1 bind silently unmaps core 1 and every
  refill dies. The install path must never use it in the dual-core shape.
- esp-hal parks a returned core-1 entry (or a dropped `AppCoreGuard`) with a
  **hardware stall** — a stalled core services no interrupts, so the core-1 entry
  must never return and the guard must be forgotten.
- Truncated frames are cheap, so a broken configuration can post *better* fps than
  a clean one (31 vs 23 here). fps without the per-wire trip counters is not
  evidence.

## Consequences

- Host, wasm, emulator, S3 and C6 see no behavioural change (capability default
  `false`). S3 is also dual-core and could adopt the same shape if its app path
  ever masks long enough to matter; each adoption needs its own silicon pass.
- The 23 → ~31 fps gap is the fourth wire's admission wait. Two named paths: the
  pin-mux wave milestone (P7 in the plan — N wires over 3-slot waves), or a
  refill-cost reduction (~20 % would put cap 4 at today's clean duty; whether the
  cost is bus-bound or code-bound is an open measurement, spun off as its own
  experiment).
- The old 5th-1-block-slot direction (40 µs deadlines, HLI prerequisite) is
  superseded outright: 1-block halves at 40 µs are unreachable at this duty
  arithmetic regardless of masking.
