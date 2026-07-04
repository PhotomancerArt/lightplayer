# ADR: Client Pull Loop and Actor-Owned Controller

- **Status:** Accepted
- **Date:** 2026-07-04
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Milestone M7 (client sync engine) moves the studio client's update logic out of
the Dioxus UI crate (`lpa-studio-web`) and into the crates that own the
concepts (`lpa-client`, `lpa-studio-core`). Before M7 the web crate carried the
whole concurrency and policy surface: an `Option<StudioController>` that was
`take()`-n out to run an operation and put back, `action_generation` /
`refresh_generation` counters, cancel-request flags, four stacked timeout
regimes (per-`receive` poll limits, a passive-refresh watchdog, per-op
foreground watchdogs, and a flat failure backoff), 25 ms spin loops polling
those flags, and four match-table policy functions
(`action_preempts_passive_refresh`, `action_preempts_foreground_action`,
`foreground_action_timeout_ms`, `foreground_timeout_recovers_server`).

Three structural problems drove this ADR:

1. **No single timeout owner.** Cancellation was `drop(dispatch)` mid-`select!`,
   which abandoned a half-consumed frame stream; four timeout regimes stacked on
   top of each other, and the sim/device split (4 s vs 12 s wall-clock) existed
   to compensate for payload cost that M5/M6 gating had already eliminated.
2. **Ownership churn.** The controller was shuffled in and out of an `Option`
   with generation counters to detect stale completions — the classic sign that
   the controller wants a single owner, not shared borrows.
3. **Policy lived in the wrong crate.** Preemption and timeout policy were web
   match tables that silently defaulted for any new op, so a new op could ship
   with no declared preemption/timeout behaviour and nobody would notice.

M7 is phased P1–P4. P1 landed the pull loop in `lpa-client`; P2 landed
`ActionClass` (policy-as-data) beside the ops and merged the two controller
apply paths; **P3 (this ADR) lands the actor-owned controller** and the
change-gated snapshot channel; P4 will delete the web machinery and wire the
thin shell.

## Decision

### D1 — A pull loop is the single timeout/cancel/retry owner (`lpa-client`)

`lpa-client::pull_loop` owns one gated project read end to end. It replaces the
four stacked timeout regimes with **one progress-based deadline**
(`ProgressDeadline`): a *quiet-gap* budget reset on every received frame, so a
slow-but-progressing multi-frame stream never trips it and only a genuinely
stalled stream (no frame within budget) times out. Cancellation is **explicit,
not drop-based** (`CancelSignal`): the loop observes it at a frame boundary and
returns `PullOutcome::Cancelled`, leaving the transport consistent instead of
abandoning a half-read stream. `BackoffPolicy` (exponential-with-cap) defines
the retry cadence a caller applies between failed reads, so the whole timing
contract of a client read lives in one place. The loop is runtime-neutral
(`?Send`, no Tokio/`web-sys`): the deadline is built from a caller-supplied
timer factory, so the same loop drives native (`tokio::time::sleep`) and wasm
(`setTimeout`). Both client loops (`client.rs`, `tokio_client.rs`) call it,
closing the M6-ADR "both loops reimplement the same state machine" seam.

Cancellation is observed **only at frame boundaries**. A read that makes
progress (any frame) can be cancelled cleanly at its next boundary; a read that
receives *no* frame at all is bounded by the deadline, not the cancel signal.
This is deliberate: it keeps the transport consistent (no mid-frame abandon) and
matches how the browser adapters deliver frames.

### D2 — `ActionClass` is policy-as-data on the op (`lpa-studio-core`)

Each op maps to an `ActionClass { Recovery, Foreground { deadline }, Passive
{ deadline } }` beside its definition, surfaced via `ControllerOp::action_class`
/ `UiAction::class`. `Recovery` preempts everything and carries no deadline (it
owns the connection); `Foreground` preempts a passive pull but not another
foreground op, timed by a quiet-gap deadline; `Passive` never preempts. The four
web match-table functions and their constants are replaced by this data; a new
op *must* declare a class (the trait method is required — a compile error, not a
silent default). The seeded deadline constants (`PROJECT_ACTION_DEADLINE` 8 s,
`PROJECT_LOAD_DEADLINE` 20 s, `PROJECT_EDITOR_ACTION_DEADLINE` 6 s,
`PASSIVE_REFRESH_DEADLINE` 12 s) preserve the retired values, reinterpreted as
quiet-gap budgets (so the larger device value is far less punishing than the old
wall-clock cap).

### D3 — One task owns the controller; preemption is queue priority (`lpa-studio-core`)

`StudioActor` owns the `StudioController` and consumes an ordered command queue
of `StudioCommand { Action(UiAction), RefreshTick, Shutdown }`. Every input —
user gestures and the UI's refresh timer — arrives as a command. The
`Option::take()`/put-back, generation counters, cancel flags, and 25 ms spin
loops become queue semantics:

- **Coalescing.** The actor drains a whole batch of queued commands at once
  (`recv_coalesced`), collapses any number of queued `RefreshTick`s to one pull,
  and runs pending actions ahead of the tick. A slow read cannot build a tick
  backlog — user constraint #3 (request-after-complete-receive) made structural.
- **Preemption via the pull loop's `CancelSignal`.** While a passive refresh
  pull is in flight, the actor concurrently watches the queue; when a command
  whose `ActionClass` preempts a passive refresh arrives, it flips a shared
  `Rc<Cell<bool>>` cancel signal. The pull returns `Cancelled` cleanly at its
  next frame boundary, the preempting action runs, and refresh resumes on the
  next tick. `BackoffPolicy` is applied on `Failed`/`TimedOut` outcomes of
  passive pulls; a clean cancel is not a failure.
- **Spawn.** The run loop is a plain `async fn` (`StudioActor::run`) with no
  runtime dependency, so wasm drives it under `wasm_bindgen_futures::spawn_local`
  and tests drive it with a bare waker. (A native/tokio spawn helper is a
  follow-up — no native Studio shell exists yet.)

### D4 — A change-gated view channel is the entire UI boundary

The actor pushes a fresh `UiStudioView` snapshot through a single-consumer
`?Send` view channel **only when the view actually changed**. The controller
tracks the applied project revision plus a local-mutation dirty flag; a snapshot
is rebuilt and emitted only when an applied read advanced the revision or a
local action/log mutated state — replacing `dispatch_with_updates`'
two-`view()`-per-dispatch churn and the unconditional per-tick rebuild.
`StudioHandle { tx, view }` is the whole surface the web crate sees: enqueue
commands, subscribe to snapshots. No shared `Rc<RefCell<StudioController>>`, no
`model.read().view.clone()` polling. `UxUpdateSink` progressive activity updates
still flow during long ops.

### D5 — Focus is local-only; logs are capped in core; previews are shared

- **Focus** completes synchronously in the controller; the bolt-on
  `refresh_project` network round-trip after every editor action is removed. The
  next `RefreshTick` picks up the changed probe set, which is already
  focus-scoped via `node_subscribes_products`. (`Focus` is the only editor op,
  so nothing else depended on the bolt-on.)
- **Logs** live in a bounded `LogRing` (`LOG_RING_CAPACITY = 80`) in
  `StudioController`, and the notice/error→log mappers move to core
  (`UiLogEntry::from_notice`/`from_error`). The web crate's private 80-entry
  mirror and mapper functions are retired in P4; only the JS-console sink stays
  web.
- **Preview buffers** (`UiProductPreview::VisualSrgb8`, control samples) move
  from `Vec<u8>` to `Rc<[u8]>`, so cloning a preview into a rebuilt view is a
  refcount bump rather than a deep copy of the RGB8/sample payload. `Rc` (not
  `Arc`) is correct: the actor and view run on one task on wasm and one thread in
  tests.

## Consequences

- The client's timing contract (deadline, cancel, retry) is one place; the
  browser adapters' poll latency becomes a bounded, correct behaviour under the
  progress deadline rather than something the app-level watchdogs had to model.
- New ops cannot ship without a declared preemption/timeout class.
- The web crate's concurrency/policy/watchdog surface (`StudioWebModel`,
  generations, cancel flags, spin loops, match tables) is now dead weight that
  P4 deletes; P3 keeps a thin compatibility surface so it still compiles.
- Change-gating means a quiet steady-state tick that advances no revision emits
  no snapshot, so the UI does not re-render on empty pulls.

## Alternatives Considered

- **Shared `Rc<RefCell<StudioController>>` read directly by the UI.** Rejected:
  it re-creates the ownership/borrow problems M7 is deleting — the reason the
  controller was shuffled through an `Option` in the first place.
- **A wall-clock watchdog stack (keep the four timeout regimes).** Rejected: the
  4 s-sim-vs-12 s-device split compensated for payload cost that M5/M6 gating
  removed, and stacked watchdogs cancelled by dropping a half-read stream. One
  progress deadline plus explicit cancel is simpler and leaves the transport
  consistent.
- **Policy match tables in the web crate.** Rejected: they silently default for
  new ops. `ActionClass` on the op makes a missing class a compile error.
- **`Arc`/`bytes::Bytes` for preview buffers.** Unnecessary: no `Send` boundary
  appears on wasm. If one ever does, swap `Rc` for `Arc`/`Bytes` at that seam.

## Follow-ups

The durable deferral log for M7 (per the plan's Review-gate resolutions). P4
builds the initial `docs/adr/README.md` open-follow-ups/deferred index that
tracks these.

- **(a) Event-driven receive.** The browser-worker (`4 ms × 240`) and serial
  (`10 ms × 500`) `receive` adapters still poll-sleep over an event-driven-at-JS
  buffer; the pull loop's progress deadline makes that latency bounded and
  correct, so event-driven push is deferred. **Revisit when** poll latency shows
  up in traces or battery/CPU cost matters. It is a ~50–100 line waker/oneshot
  bridge across `browser_worker_client_io.rs`, `worker_handle.rs`, and the JS
  worker.
- **(b) Probe payload optimization.** A quiet gated read carries no
  shape/slot/node payload, so steady-state tick cost is dominated by the raw
  probe bytes (visual `Srgb8` texture + control samples, base64-framed) —
  user-observed on real hardware ("the raw data is dominating"). Deferred out of
  M7 to avoid swallowing a probe-scoping feature. **Revisit with measurements;**
  candidates: binary/compressed preview encoding, downscaled preview extents,
  delta frames. Own design pass later.
- **(c) Native/tokio actor parity.** `StudioActor::run` is runtime-neutral and
  wasm spawns it under `spawn_local`; a `tokio::spawn`/`LocalSet` spawn helper
  and native timer factory are deferred until a native Studio shell exists.
