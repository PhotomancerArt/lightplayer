---
status: fixed
found: 2026-07-26      # how: live-debugging
fixed: this change
area: fw-browser + lpa-link/browser-worker
class: state-conflation
---
# Worker kept calling a wasm instance a panic had already poisoned

**Symptom** — While editing GLSL in Studio, the sim froze and the
console cascaded with:

```
RuntimeError: unreachable
    at core::cell::panic_already_borrowed
    at RefCell<Vec<BrowserFirmwareRuntime>>::borrow_mut
```

repeating every ~33 ms, followed by Studio reporting "project sync
failed: transport error: timed out waiting for browser worker protocol
output". The *actual* crash — whatever panicked first — had scrolled
away, and its message was never captured anywhere.

**Root cause** — Two facts compounded:

1. `fw-browser` builds with `panic=abort` (per-target panic-strategy
   ADR). A Rust panic inside a wasm export becomes an `unreachable`
   trap that throws a JS `RuntimeError` through the export *without
   running Rust drops*. `runtime_registry::with_runtime_mut` holds
   `RUNTIMES.borrow_mut()` across the whole export call, so the leaked
   borrow flag permanently poisons the still-alive instance.
2. The worker script's error handling conflated two different facts in
   one `status: "error"` state: *this message failed* (recoverable —
   e.g. a `Result::Err` from an export) and *this instance is dead*
   (unrecoverable). Every catch posted an error status and kept going,
   so the 33 ms self-tick hammered the poisoned instance forever —
   each call aborting in `borrow_mut`, burying the primary panic.

The primary panic message was additionally invisible because
`fw-browser` installed no panic hook: the only trace of *what*
panicked was mangled frame names in the RuntimeError stack.

**Not this defect** — shader *fuel traps*. Emitted shader modules are
separate wasm instances; `lpvm-wasm`'s `rt_browser` calls them through
a catching binding (`js_sys::Function::apply`) and types the vmctx trap
slot on both Ok and Err returns (`take_trap`, sim-wasm-fuel ADR), so a
guest trap becomes a typed `Err` and never crosses the fw-browser
export boundary. Verified during this fix; the regression test
`infinite_loop_shader_reports_fuel_error_and_keeps_ticking` already
pinned it. The boundary that matters: **guest trap = typed error, host
panic = condemned instance.**

**Fix** — Escaped exceptions are now classified at the worker
boundary (`fw_browser_worker.js`): a string throw is a wasm-bindgen
`Result::Err` (instance intact); a `WebAssembly.RuntimeError` is
instance-fatal; anything else is resolved by a canary probe
(`runtime_count()`, which takes the exact RefCell borrow that leaks).
Instance-fatal → stop the self-tick, post a sticky `status: "fatal"`
(carrying the primary panic, captured by a new fw-browser panic hook
via the worker global), and never call the instance again. The link
layer fast-fails posts on a fatal worker; the preview host condemns it
to its existing recycle path; Studio marks the sim session
`ServerFailureKind::SimCrashed` and auto-reboots once per 30 s guard
window with the last-known project (`run_due_sim_crash_recovery`).
See `docs/adr/2026-07-26-wasm-instance-fatal-exceptions.md`.

**Regression coverage** —
`sim_crash_is_detected_torn_down_and_auto_reboot_attempted` and
`sim_crash_within_the_flap_guard_stays_failed_for_manual_restart`
(studio_link_e2e_tests), `fatal_status_maps_to_an_error_draft_with_the_primary_panic`
(browser_worker_log). The worker JS classification itself is untestable
(no JS harness — same debt class as web-serial-js-untestable);
`debug_force_panic` exists to exercise it live.

**Lesson** — An error-handling layer must know its fault domain. A
per-message catch that treats every exception as message-scoped will
eventually reuse a resource whose invariants died with the first
failure; "this request failed" and "this instance can never serve a
request again" need different states, and the second must be sticky.
Corollary for wasm hosts: under panic=abort, *any* exception that
crossed wasm frames means drops were skipped — the instance's own
in-memory state can no longer be trusted, so hardening inside the
instance (`try_borrow_mut`) only masks the abort; recovery has to be
rebuild-from-outside.
