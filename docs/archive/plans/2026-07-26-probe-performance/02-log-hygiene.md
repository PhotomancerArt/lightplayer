# P2 — Worker log hygiene & log-only view updates

Size: sm. Depends on: nothing. Parallel-safe with P1/P3.

## Scope

Stop the sim worker's per-tick diagnostics from (a) crossing the postMessage
boundary every 33 ms and (b) forcing full `UiStudioView` rebuilds. Keep the
console pane functional.

Out of scope: receive-loop and pacing changes (P3); general console-pane
redesign; changing the `SetLogLevel` wire op.

## Current state

- `lp-fw/fw-browser/src/runtime.rs:257-263` — every `tick()` unconditionally
  pushes a `trace`-level Log envelope (`tick delta=… frame=…us`).
- `lp-fw/fw-browser/src/runtime.rs:211-233` — every `ProtocolIn` pushes a
  `debug`-level Log envelope.
- These ride `postMany` (one postMessage each), are decoded on the main
  thread, become `UiLogDraft`s
  (`lp-app/lpa-studio-core/src/app/server/browser_worker_client_io.rs:121-125,
  143-163`), then `record_session_logs` → `mark_dirty`
  (`lp-app/lpa-studio-core/src/app/studio/studio_controller.rs:865-885`,
  `:3521-3531`), so `view_if_changed` (`:829-838`) rebuilds the whole view —
  node tree, bus pane, console view cloning up to 1000 ring entries
  (`studio_controller.rs:786-793`, `ui_console_view.rs:36-53`) — at 30 Hz
  even when nothing else changed.
- Each entry also triggers a `console.debug` on the main thread
  (`studio_controller.rs:855` → `web_app.rs:623-631`), and the worker mirrors
  its own records to the worker console (`lp-fw/fw-browser/src/logger.rs:49-59`).

## Implementation

1. **Gate at the source (fw-browser).** Give `BrowserFirmwareRuntime` a
   diagnostics log level (default: suppress `trace`/`debug` envelope
   emission). The per-tick trace at `runtime.rs:257-263` and per-`ProtocolIn`
   debug at `:211-233` are emitted only when enabled. Follow the existing
   runtime-log-level convention (`SetLogLevel` exists as a wire op — see
   memory/console plan; the *envelope* diagnostics level can simply reuse the
   firmware's `log::max_level` if that is what gates the mirrors in
   `logger.rs`, otherwise a small explicit field + setter on the runtime,
   settable from worker options). Keep it easy to re-enable for debugging
   (e.g. a `BrowserWorkerOptions` flag).
2. **Cheapen log-only updates (studio-core).** When a refresh produces *only*
   new log entries (no project revision advance, no structural change), do
   not mark the whole view dirty. Options in order of preference:
   - Track a separate `console_dirty` flag; `view_if_changed` rebuilds and
     publishes only if `dirty || revision_advanced`, and console-only changes
     ride the next real publish OR a throttled (e.g. 250 ms) console-only
     republish. Console updates may be latency-insensitive; pick the simplest
     correct variant and note it in the phase result.
   - Do NOT silently drop log entries; the ring buffer still accumulates.
3. Leave the per-entry devtools mirror (`console.debug`) gated the same way —
   with trace/debug envelopes suppressed at source it stops firing at 30 Hz
   naturally. Do not remove the mirror mechanism.

## Conventions

- fw-browser is `no_std`-adjacent wasm; follow existing envelope patterns in
  `runtime.rs` / `worker_envelope.rs`.
- Studio-core dirty-tracking: see `mark_dirty` call sites before changing
  semantics; keep `view_if_changed` contract documented in comments there.

## Validation

- `cargo test -p fw-browser -p lpa-studio-core` (targeted), then `just check`.
- Existing worker-protocol tests must stay green.
- Manual: with a sim session idle, the view channel should not publish new
  snapshots every 33 ms (add/keep a test at the `view_if_changed` level if
  one fits naturally).

## Agent reminders

Do not commit unless asked. Do not expand scope. Do not suppress warnings or
disable tests. Stop and report if blocked. Report changes, validation, and
deviations.

ADR: none. Review gate: none.

## Definition of done

Idle sim session no longer rebuilds/publishes the full view at tick rate;
per-tick trace/debug envelopes off by default but re-enableable; console pane
still shows logs; checks green.

## Implementation Result

Status: done
Completed: 2026-07-27
Commit: e287c3d5d

- Changed: `runtime.rs` `log()` gates on `log::max_level()` via
  `envelope_level_enabled()` (re-enable through the existing wire
  `SetLogLevel` — no new mechanism); `studio_controller.rs` splits
  `logs_dirty` from `dirty`, streamed log batches publish on a 0.25 s
  throttle, structural/action publishes carry pending lines. No line dropped.
- Validated: test `streamed_logs_publish_on_a_throttle_not_per_batch`;
  `just check` + `just test` green.
- Deviations: none. Details in [handoff.md](handoff.md).
