# ADR: Escaped wasm exceptions are instance-fatal; recovery is reboot

- **Status:** Accepted
- **Date:** 2026-07-26
- **Deciders:** Photomancer
- **Supersedes:** None (builds on
  [2026-07-23-per-target-panic-strategy.md](2026-07-23-per-target-panic-strategy.md)
  and [2026-07-23-sim-wasm-fuel.md](2026-07-23-sim-wasm-fuel.md))
- **Superseded by:** None

## Context

`fw-browser` builds with `panic=abort` (per-target panic-strategy ADR):
a Rust panic inside a wasm export reaches JavaScript as a
`WebAssembly.RuntimeError` thrown through the export **without running
Rust drops**. Any RAII state held across the export call is leaked in
the still-alive instance — concretely, `runtime_registry`'s
`RUNTIMES: RefCell<…>` borrow, held across every export by
`with_runtime_mut`, so every later export call aborts in `borrow_mut`
(`panic_already_borrowed`). The worker script treated every exception
as message-scoped (post `status: "error"`, keep going), so its 33 ms
self-tick hammered the poisoned instance in a cascade that buried the
primary panic and surfaced in Studio as a protocol poll timeout
(defect `docs/defects/2026-07-26-worker-poisoned-instance-reuse.md`).

Three kinds of exception can escape a fw-browser export, with different
meanings:

1. a **string** — wasm-bindgen throwing a `Result::Err(String)` after
   the wasm returned normally; the instance is intact;
2. a **`WebAssembly.RuntimeError`** — a panic=abort trap; drops were
   skipped, the instance is condemned;
3. any **other JS exception** — thrown by a non-catching host import
   and propagated through wasm frames (drops skipped too), rare and
   not nominally a RuntimeError.

Shader **fuel traps are not any of these**: emitted shader modules are
separate wasm instances called through a catching binding
(`js_sys::Function::apply` in `lpvm-wasm`'s `rt_browser`), and the
vmctx trap slot is typed into `WasmError::Trap` on both Ok and Err
returns (sim-wasm-fuel ADR). A guest trap is a value-level `Err` at the
shader-call boundary and never crosses the fw-browser export boundary.

## Decision

**Any exception escaping a fw-browser wasm export that is not a
wasm-bindgen error string condemns the instance.** The worker script
(`fw_browser_worker.js`) classifies at every catch:

- string → ordinary error path (per-message `status: "error"` /
  `preview_error`, keep serving);
- `instanceof WebAssembly.RuntimeError` → instance-fatal;
- anything else → **canary probe**: call `runtime_count()` (it takes
  the exact `RUNTIMES` borrow a leaked `borrow_mut` poisons); if the
  probe throws, instance-fatal, else ordinary error.

Instance-fatal means: stop the self-tick, compose the fatal message,
post a **sticky `status: "fatal"`**, answer every subsequent message
with it, and never call a wasm export again. The Worker object stays
alive — the host owns the Worker lifecycle.

**The primary panic is captured by a panic hook**, installed alongside
the fw-browser logger: it formats the `PanicHookInfo`, mirrors it to
the worker console, and stashes it on the worker global
(`__lp_last_panic`), where the fatal handler reads it *without
re-entering the poisoned instance*. The fatal message therefore carries
the real panic, not the generic `unreachable` trap text.

**Recovery is rebuild-from-outside, never in-place hardening:**

- `lpa-link`'s `BrowserWorkerHandle` records the sticky fatal and
  fast-fails `post()` with it (no more poll-timeout discovery);
  `BrowserWorkerProvider::session_fatal` / `LinkConnector::session_fatal`
  expose it (the fake provider scripts it for tests).
- The preview host condemns a fatal worker to its existing
  recycle-and-respawn path.
- Studio marks the sim session `ServerFailureKind::SimCrashed` and
  runs a **guarded auto-reboot** on the tick cadence
  (`run_due_sim_crash_recovery`): tear the dead session down
  (terminating the Worker) and reopen the recorded
  `sim_loaded_project` through the normal open flow — at most one
  reboot per 30 s window; inside the window the session stays Failed
  and the open flow performs the manual-restart teardown.

`debug_force_panic` (export + worker message) exists to trigger a real
panic on demand — the recovery path's only live verification hook.

## Consequences

- A sim crash now costs one worker reboot and the unsaved overlay
  (which died with the instance regardless); the primary panic lands
  once in the Studio console at Error level instead of a 30 Hz abort
  cascade.
- The worker protocol gains one status value (`"fatal"`), sticky by
  contract; no envelope shape change.
- Every future fw-browser export automatically inherits the contract —
  classification lives in the worker's shared catches, not per-export.
- The 33 ms self-tick can no longer amplify a single failure into a
  console flood: fatality is edge-triggered and sticky.

## Alternatives Considered

- **`try_borrow_mut` (or narrower borrow scopes) in
  `runtime_registry`** — rejected. Post-abort instance state is
  untrustworthy in general (drops were skipped everywhere, not just at
  the registry), so surviving `borrow_mut` would *mask* the crash and
  keep computing on corrupt state. The registry borrow is merely the
  first invariant to trip.
- **Worker self-termination on fatal** — rejected; the host owns the
  Worker lifecycle (`BrowserWorkerHandle::terminate`), and a
  self-terminated worker cannot answer late host messages with the
  fatal status (posts would silently void).
- **`panic=unwind` for fw-browser** — rejected; the per-target
  panic-strategy ADR keeps abort (size, and wasm unwinding is not a
  supported std configuration here).
- **Unconditional auto-reboot (no flap guard)** — rejected; a project
  that crashes the worker during load would reboot-loop at full speed.

## Follow-ups

- Event-driven receive for the sim client io (existing M7 note) would
  make fatal discovery push- instead of poll-shaped.
- The worker JS classification has no test harness (accepted debt,
  same class as `docs/debt/web-serial-js-untestable.md`).
