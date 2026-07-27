# P3 — Event-driven receive + completion-based refresh pacing

Size: sm (the largest sm of the plan). Depends on: nothing (P1/P2 parallel).
P4 and P5 build on this.

## Scope

Two coupled changes to the sim/studio refresh machinery:

A. **Wake-on-message receive.** Replace the 4 ms sleep-before-poll loop in
   the sim client IO with an event-driven wait resolved by the worker
   `onmessage` handler.
B. **Completion-based pacing.** The refresh tick re-arms *after* the previous
   pull completes, plus a per-runtime-kind minimum gap, instead of a
   free-running fixed interval.

Out of scope: device-specific tuning (P4 sets the device gap/floor), worker
self-tick redesign (33 ms `setInterval` stays), protocol changes.

## Current state

Receive (A):
- `lp-app/lpa-studio-core/src/app/server/browser_worker_client_io.rs:20`
  `RESPONSE_POLL_LIMIT = 240`; `:55-85` `receive()` loops
  `sleep_ms(4).await` **then** polls `take_outputs()` — ≥4 ms per received
  frame, ~960 ms budget; comment at `:60-62` flags event-driven receive as
  future work (M7).
- Frames arrive on the main thread via
  `lp-app/lpa-link/src/providers/browser_worker/worker_handle.rs:59-104`
  (`onmessage` closure → `Rc<RefCell<Vec<BrowserOutputEnvelope>>>`), drained
  through `provider.rs:103-114` `take_outputs(session_id)`.

Pacing (B):
- `lp-app/lpa-studio-web/src/web_app.rs:428-438` — `use_future` loop:
  sleep `bridge.delay`, send `StudioCommand::RefreshTick`, repeat — never
  awaits pull completion; ticks coalesce in the actor but pulls run
  back-to-back when a pull exceeds the interval.
- Delay source: `studio_actor.rs:229-231` →
  `StudioController::next_refresh_interval` (`studio_controller.rs:451-469`).
- Cadence constants: `lp-app/lpa-studio-core/src/app/studio/
  refresh_cadence.rs` (`SIMULATOR_REFRESH_INTERVAL = 33ms`,
  `DEVICE_REFRESH_INTERVAL = 750ms`, verdict-chase, backoff,
  `for_kind` at `:93-98`).

## Implementation

### A — event-driven receive

1. In `worker_handle.rs`, when the `onmessage` closure pushes an output,
   also wake any pending waiter: store a `Rc<RefCell<Option<Waker>>>` (or a
   small oneshot/Notify equivalent already used elsewhere in the codebase —
   check `lp-app` for an existing notify util before writing one) alongside
   the output buffer.
2. Expose an async `wait_for_output()` on the handle/provider that resolves
   immediately if the buffer is non-empty, else registers the waker.
3. `browser_worker_client_io.rs::receive()` becomes: drain FIFO → if empty,
   `select(wait_for_output(), deadline)` → drain. Delete the 4 ms sleep and
   `RESPONSE_POLL_LIMIT`; keep an overall deadline equivalent to today's
   (~1 s) for fatal detection. Preserve the existing fatal-status and
   non-protocol-envelope handling (`:121-163`).
4. Keep `sleep_ms` util if other callers use it.

Watch out: single-threaded wasm — the waker fires from inside the JS
`onmessage` task; make sure no `RefCell` borrow is held across the wake call
(scope borrows tightly, wake after dropping).

### B — completion-based pacing

1. Reframe `RefreshCadence` semantics: the value is a **minimum gap between
   pull completions and the next pull start**, not a fixed period. Rename or
   re-document constants accordingly (e.g. `SIMULATOR_REFRESH_GAP = 33ms`;
   device value is retuned in P4 — leave 750 ms here, P4 lowers it).
2. Make the actor own pacing: after `run_refresh_tick` completes, compute
   `next_due = completion_time + gap` per session; `next_refresh_interval`
   returns time-until-due (already min-over-sessions — keep that shape).
   If `next_refresh_interval` already effectively does this off due-times,
   the change may localize to *when* due-times are stamped (at completion,
   not at tick start) — verify against the current implementation.
3. The `web_app.rs` tick loop keeps its shape (sleep published delay → send
   tick), but the published delay now derives from completion-stamped
   due-times, so a slow pull naturally pushes the next tick out. Verify the
   actor re-publishes `delay` after each batch (it does — `studio_actor.rs:
   229-231`; confirm ordering with the tick send).
4. Preserve verdict-chase (temporarily tighter gap) and failure backoff
   semantics on top of the new model.
5. Preemption by user actions (`SharedCancel`) is unchanged.

## Conventions

- `refresh_cadence.rs` has doc comments explaining the cadence model —
  update them to describe completion+gap semantics (they are the de-facto
  spec; P6's ADR cites them).
- Existing cadence unit tests live near `refresh_cadence.rs` /
  `studio_controller.rs` — extend rather than replace.

## Validation

- `cargo test -p lpa-studio-core` (cadence + actor tests), `just check`.
- Add a test: a pull taking longer than the gap does not cause back-to-back
  zero-idle pulls (due-time stamped at completion).
- Add/adapt a test for event-driven receive resolving promptly when an
  output is already queued and when one arrives later (wasm-friendly test if
  the crate has that harness; otherwise test at the state/waker level).

## Agent reminders

Do not commit unless asked. Do not expand scope. Do not suppress warnings or
disable tests. Stop and report if blocked. Report changes, validation, and
deviations.

ADR: covered by P6's pacing ADR — note decisions in the phase result.
Review gate: none here (Yona's sim feel check batched at PR review).

## Definition of done

No fixed-interval sleep-poll in sim receive; refresh due-times stamped at
pull completion; verdict-chase/backoff preserved; tests cover both; checks
green.

## Implementation Result

Status: done
Completed: 2026-07-27
Commit: e287c3d5d

- Changed: `worker_handle.rs` `output_wakers` + level-triggered `OutputWait`
  woken from `onmessage`/`onerror`/`onmessageerror`; `provider.rs`
  `wait_for_output()`; `browser_worker_client_io.rs` drain → park → re-arm
  with `RECEIVE_TIMEOUT_MS = 1000`. Pacing: completion stamps on
  `RuntimeSession`, `lens_refresh_gap()` /
  `passive_refresh_due()` in the controller,
  `ProjectRefreshOutcome::NotDue` gate; `NotDue`/`Cancelled` deliberately do
  not stamp. `refresh_cadence.rs` docs rewritten as the pacing spec.
- Validated: test
  `early_tick_bounces_off_the_completion_gate_without_a_wire_op`; wasm-target
  `cargo check -p lpa-studio-web --target wasm32-unknown-unknown` (the
  browser_worker cfg-gate hides this code from host checks); `just check` +
  `just test` green.
- Deviations: none. Details in [handoff.md](handoff.md).
