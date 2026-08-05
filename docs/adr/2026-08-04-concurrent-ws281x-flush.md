# Concurrent WS281x flush: start/wait split, frame barrier, admission cap

- **Status:** accepted (pending G2 sign-off on the shipped channel count)
- **Date:** 2026-08-04
- **Plan:** `2026-08-02-1636-multi-channel-output-architecture` M4
- **Relates to:** `2026-08-03-multi-endpoint-output-node.md` (prepared the
  seam), `2026-07-31-lp-ws281x-multi-channel-driver-adoption.md` (the driver
  this rides on)

## Context

One output node drives N wires (PR #301), but the engine flushed them through
`OutputProvider::write` one blocking send at a time. At dome scale that is
30 µs/LED of pure spin — 27 ms/frame at 900 LEDs, 45 ms at 1500 — and it
gated the classic ESP32 at 17 fps (measured, DOM-Z-102, 900 LEDs). The
transmitter is ISR-driven ping-pong refill; `send_blocking_all` proved on the
S3 loopback harness that N channels can transmit simultaneously. Only the
seam shape (`write()` per handle) kept the app path from using it.

## Decision

Three pieces, one per layer:

1. **`Ws281xOutput` gains a split transmission form.** `unsafe fn start`
   (begin, don't wait; the caller keeps the bytes alive and unmodified until
   the wait returns) and `fn wait_complete`. Both default to the blocking
   `write`, so synchronous implementations (virtual, emulator, S3, C6) are
   untouched and unaware.

2. **`Esp32OutputProvider::write` stages and starts; a new
   `OutputProvider::flush` finishes.** Write renders the display pipeline
   into the channel's persistent frame (the storage PR #301 moved into
   `ChannelState` for exactly this), calls `start`, and returns. The engine
   calls `flush()` once per frame after the last write — the barrier that
   `wait_complete`s every channel. Wires written back to back therefore
   transmit **concurrently**, and the frame pays per admitted wave rather
   than per wire.

3. **The classic's driver admits at most `MAX_CONCURRENT_TX = 2`
   transmitters at once**; a start past the cap waits in `start` for a slot
   to free. Four wires flush as two waves.

## Measured (DOM-Z-102, zook-dome-1500 scaled to 900 LEDs = 4×225, simple
shader, `ws281x_telemetry` build, 2026-08-04)

| configuration | fps | tick | guard trips |
|---|---|---|---|
| sequential blocking (main, telemetry-verified) | 17 | 55–57 ms | 0 |
| deferred starts, **no barrier** (defect during development) | 26–28 | 34–36 ms | last-started wires ~99 % of frames |
| cap 2 + barrier (**shipped**) | 22–23 | 42 ms | **0** on all 4 (1 378 frames) |
| cap 4 + barrier | 23 | 41–42 ms | 0 on all 4 (1 617 frames) |

The broken row's extra fps is partly an artifact — truncated frames are
cheap. The honest win is **17 → 23 fps (+35 %)** with every strand whole.
Projected at 1500 LEDs (4×375): cap 2 ≈ 18–19 fps, cap 4 ≈ 23 fps (the
plan's target), against ≈ 13 fps sequential.

## The two constraints that shaped it

**Transmission is only safe under a quiet CPU.** This chip's app path masks
interrupts in stretches longer than the 80 µs refill deadline (the same
masking family as `2026-08-02-classic-hli-refill.md`), so a wire left
transmitting while the engine runs truncates on essentially every frame —
measured directly when the barrier was missing. All wire time must be
covered by quiet spinning (the admission wait and the flush barrier). This
kills the tempting "defer the wait to next frame's write and overlap
transmission with render" design — it cannot work on this silicon until the
masking is fixed (that fix is the parked HLI work, M6's named prerequisite).
The interrupt-rate tables in `lp-fw/lp-ws281x/README.md` now carry the
same warning.

**The ISR delivery ceiling was not the binding constraint at 4×12.5 k/s.**
Once quiet-covered, four concurrent transmitters ran trip-free — but with
worst-case entry delay at 53 of the 64-word deadline, versus 8 at two
concurrent. Two is shipped for margin (same fps at 900-LED scale; the fixed
engine cost dominates); four is the measured one-constant lever if
1500-scale wire time needs halving, to be re-judged at that scale's gate.

## The near-miss worth remembering

`OutputProvider::flush` has a default no-op — and `lpa-server`'s
`SharedOutputProvider` (a hand-written `Rc<RefCell<…>>` delegate) silently
resolved the engine's barrier to that default instead of forwarding it. On
silicon this presented as the *last-started* wire truncating every frame,
which spent five flash cycles masquerading as an interrupt-ceiling, a
slot-6-hardware, and an APB-contention problem before a thread-side probe
showed the barrier never ran. A defaulted trait method plus a delegating
wrapper is invisible to the compiler; the guards now are a forwarding
comment in the wrapper, an engine test that counts barrier calls through a
wrapper (`every_flush_ends_with_the_provider_barrier`), and a provider test
that the write path never blocks (`concurrent_flush_tests`).

## Consequences

- Host, wasm, emulator and the other chips see no behavioural change; the
  trait defaults keep the split form a synonym for `write` until a driver
  opts in. S3/C6 can adopt by implementing `start`/`wait_complete` — the
  provider and engine halves are already in place — but each adoption needs
  its own silicon pass and its own admission-cap judgement.
- `ChannelState`'s field order (`output` before `frame`) is load-bearing:
  drop stops the transmitter before freeing the bytes it reads. Documented
  at the struct.
- The engine's `OutputFlushError` gains a `Flush` variant carrying no node —
  the barrier is frame-wide.
