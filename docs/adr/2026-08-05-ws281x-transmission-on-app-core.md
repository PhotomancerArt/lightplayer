# WS281x transmission sequencing moves to the APP core (the wire pusher)

## Status

Accepted, 2026-08-05. Shipped with plan `2026-08-05-0901-pinmux-8wire-overlap`
(PR #356). Builds on `2026-08-04-rmt-isr-on-app-core.md` (which it supersedes
in part — the idle-loop description) and on the wire/slot pool (PR #350).

## Context

With five wires over four pooled RMT slots, the fifth wire's frame transmits
as a second wave after a slot frees. The admission wait for that slot ran on
the render thread (inside `Ws281xOutput::start`), costing one full wave of
wire time per frame: 23 fps at 5×300 against the engine-bound 31.

Structurally, any design that issues all transmission starts from one point
in the frame loop either blocks the render thread for a wave somewhere or
runs five transmitters at once — and five simultaneous transmitters are
refuted on silicon (worst ISR entry delay 51/64 words at cap 4, plus the RMT
RAM block budget). Starting the second wave *mid-render* requires an actor
that is not the render thread. The ISR cannot take the job: a GPIO-matrix
re-mux plus a two-block prefill inside the handler blows the entry-delay
margin. That leaves exactly one candidate — thread context on the APP core,
which until now only idled between refill interrupts.

## Decision

**A pusher thread on the APP core owns every channel verb**: admission, slot
binding, GPIO-matrix re-mux, `start_frame`, abort, and completion
observation. The render core communicates only by message passing through
per-wire mailboxes (`lp_ws281x::pusher::WireMailbox` — frame descriptor as
`AtomicPtr` + u32 sequence counters). Chosen shapes, each with its reason:

- **The scheduler is chip-free** (`lp_ws281x::pusher::Pusher`), per the
  sans-IO convention: the host tests the real scheduler (8-wire two-wave
  proofs against `MockRmt`) and the Miri three-actor topology models the
  real code. The firmware keeps only a deployment shell
  (`fw-esp32v3/src/output/rmt/wire_pusher.rs`).
- **Same-core placement is load-bearing**: the pusher shares the ISR's core,
  so the two are preemption-serialized — start-vs-abort races on a channel
  are excluded by construction, and a pending stale cause always preempts
  the pusher before the next `start_frame`. The three-actor contract lives
  in `lp-ws281x`'s `driver.rs`/`state.rs` module docs.
- **Completion forwards without the teardown handshake**: ISR `finish`
  (Release) → pusher `is_complete` (Acquire) → mailbox `completed_seq`
  (Release) → poster (Acquire) → frame bytes reusable. The `isr_seq`
  handshake remains required only for aborting an incomplete frame.
  Negative-control-validated under Miri against the production orderings.
- **A software-interrupt doorbell** (`FROM_CPU_INTR1`; interrupt 0 is the
  Embassy executor's) wakes the pusher for posts made while nothing
  transmits, and a `WAKE_PENDING` flag + `rsil`-masked check + `waiti`'s
  atomic unmask-and-wait closes the lost-wakeup window. This amends the
  quiet-core contract to one two-instruction masked window.
- **Timeouts stay with the poster** (the crate has no clock): the old
  admission timeout is subsumed by the abort-request verb, and the hang
  deadline runs from the post. A close request cancels queued frames and
  forgets the wire's pad bindings (the pad's lease is released after the
  ack; a later takeover must never park a pad that moved on).
- **The single-core fallback is untouched**: cap 2, barrier flush, the
  inline admission spin — switched by `isr_on_app_core()` as before, and
  re-proven on silicon with a forced-fallback boot.

## Consequences

- 5×300 runs at **29.99 fps** (engine-bound; was 23), zero guard trips over
  240 s, slot 0 carrying exactly 2× frames. Per-wire telemetry proves the
  overlap: the waved wire's worst post→start wait is ~8.9 ms — one wave.
- `with_app_core_stalled` (flash writes, PR #353) now freezes the pusher
  mid-work; the cost is unchanged — one torn frame per write burst, visible
  as exactly `trips=1` in the upload-under-render soak.
- At the eight-wire design target (two waves of four), wave scheduling is
  the main fps determinant, and it now lives in host-testable code.
- The HLI rsil-5 investigation branch
  (`claude/zook-dome-5th-led-channel-5c9ff7`) is fully superseded for its
  5th-channel purpose — kept as a **harvest source** (the HLI masking work
  has standalone value; same posture as the scoped-buses branch), never to
  be deleted, its PR closed if one exists.

## Alternatives rejected

- **Queue-and-kick from the render loop** (no second actor): the kick point
  either recreates the block on a different wire or overlaps wave 2 with the
  next frame's wave 1 — five simultaneous transmitters.
- **Starting waves from the ISR**: re-mux + prefill inside the handler does
  not fit the 51/64-word entry margin at cap 4.
- **Raising the concurrency cap**: silicon-refuted; settled by
  `2026-08-04-rmt-isr-on-app-core.md` and the refill-floor probe.
