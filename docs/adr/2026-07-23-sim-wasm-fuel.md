# ADR: Fuel metering in the wasm shader backends (browser sim + desktop)

- **Status:** Accepted
- **Date:** 2026-07-23
- **Deciders:** Photomancer
- **Supersedes:** the "wasmtime keeps its own per-call store fuel" accepted
  divergence of [2026-07-20-lpvm-native-fuel.md](2026-07-20-lpvm-native-fuel.md)
- **Superseded by:** None

## Context

The lpvm-native fuel ADR gave the device (rv32) per-pixel execution
metering but carved the wasm backends out: wasmtime kept its own per-call
store fuel (~64× tank, guest code cannot reset it), and the browser
runtime (`rt_browser`) had **no metering at all**. That gap was observed
live: with auto-apply, a mid-edit `while(true)` shader hung the browser
sim's web worker permanently — the worker runs the whole engine
single-threaded, the tick callback never returns, Studio's ~1 s protocol
poll times out forever ("timed out waiting for browser worker protocol
output"), and only a page refresh recovers.

The carve-out's premise — wasmtime store fuel can't be reset from guest
code — is true but irrelevant to `rt_browser`, which does not use
wasmtime: **we emit the shader wasm ourselves** (`lpvm-wasm/src/emit/`),
so the rv32 design ports directly into our own emission. The per-pixel
wrapper re-arms from the native fuel work are plain LPIR stores through
the vmctx pointer and were already compiling into the wasm modules.

## Decision

**Instrument fuel checks into the emitted wasm; both wasm hosts arm and
detect via the shared vmctx header contract; wasmtime store fuel is
removed.** One metering semantics — loop back-edge executions — and one
message shape across all four execution targets (rv32n, rv32lpn,
wasmtime, browser).

- **Emission** (`emit/fuel.rs`, gated by `WasmOptions::fuel`, default
  on): check-then-decrement of the fuel low u32 (vmctx+0) immediately
  before each loop's single back-edge `br`; check-only after the
  shadow-stack prologue at function entry. Identical unit and
  check-observes-zero semantics to rv32 (filetests assert exact
  `armed − N` remainders on wasm too).
- **Abort transport = trap-code store + `unreachable`**: on exhaustion
  the check stores `TRAP_CODE_OUT_OF_FUEL` to vmctx+8 and executes
  `unreachable` — wasm unwinds the whole call to the host in one shot
  (no rv32-style epilogue cascade; the emitter has no exit label to
  branch to and fabricating per-signature result values would bloat
  every site). Hosts classify by reading the trap slot, never by the
  runtime's error message/type.
- **Host arm/detect**: both hosts arm the full header before every guest
  entry (fuel low = `DEFAULT_VMCTX_FUEL` as u32, fuel high =
  `INVOCATION_INDEX_ARMED`, trap = 0) and read the trap+invocation words
  after every call, on Ok and Err alike, surfacing a typed
  `WasmError::Trap { code, invocation }` that implements
  `lpvm::GuestTrapError` — the same marker the engine already threads to
  `LpsError::FuelExhausted`/`GfxError::FuelExhausted` with derived pixel
  coordinates. Arming is load-bearing on the browser: with
  check-then-decrement emitted, an unarmed zero fuel word traps at the
  very first entry.
- **wasmtime store fuel deleted** (`consume_fuel`, per-call `set_fuel`,
  the 64× budget): it would double-meter with a coarser unit and a
  divergent message.
- **`__lp_get_fuel` is inlined by the emitter** as a direct
  `i32.load` of vmctx+0 instead of a builtin import: the native builtin
  reads the header through a pointer, and on the wasm hosts the vmctx
  block sits at linear-memory offset 0 — a Rust pointer deref of address
  0 is a null-pointer dereference. Inlining sidesteps that, works
  uniformly on both wasm hosts, and drops the import entirely.
- **Out-of-fuel handling on hosts is a plain typed error** — the panic →
  blame-ledger route is device-only, gated on the `panic-recovery`
  feature (see
  [2026-07-23-per-target-panic-strategy.md](2026-07-23-per-target-panic-strategy.md);
  wasm panics abort, and the ledger is not installed on host runtimes).
  The error reaches Studio node status through the existing
  transport-agnostic plumbing — the sim shows the same pixel-precise
  message as the device.

## Consequences

- The browser sim survives `while(true)`: the trap fires within one
  bounded frame, the node shows "shader fuel exhausted: … pixel (x, y)
  …", and the worker keeps ticking — verified in a real browser by the
  fw-browser wasm32 test lane
  (`fuel_on_bounded_loop_shader_renders`,
  `infinite_loop_shader_reports_fuel_error_and_keeps_ticking`).
  web-demo (rt_browser on the main thread) inherits the same protection.
- Measured cost (wasmtime, release, Q32 two-loop user-GLSL kernel,
  6000 back-edges/call): runtime overhead **within run-to-run noise**
  (±6% across runs — native wasm execution barely notices the checks,
  vs +7–8% on the emulated rv32 path); module size **+94 bytes** on the
  kernel (entry checks + 2 back-edge sites).
- Filetests: `fuel/consume-counted-loop.glsl` now runs on wasm.q32
  (exact `1_000_000 − N`); the trap filetests exercise the emitted
  checks instead of wasmtime store fuel, with the unified message
  ("wasm trap: fuel exhausted (invocation N)").
- With no wasmtime store fuel, a fuel-off compile (`WasmOptions::fuel =
  false`, tests only) has **no metering** on wasm hosts — acceptable:
  the flag never ships off.
- Hosts have no blame ledger, so a bad shader burns one tank per frame
  while it stays applied — bounded (~ms) and legible; the sticky
  red-gate latch remains device-only.
- Remaining unmetered surfaces, accepted: rv32c (reference backend,
  emulator instruction limit is the backstop), interp (opt-in oracle),
  and the legacy `emit_render_frame` wrapper's own pixel loop (bypasses
  LPIR; its per-pixel calls into the user render fn still hit that
  function's checks, so infinite user loops still trap — only the
  per-pixel re-arm is absent there).

## Alternatives Considered

- **Keep wasmtime store fuel alongside the new checks** — rejected:
  double metering, per-instruction unit vs back-edge unit, divergent
  trap messages for the same condition, and still nothing on browser.
- **Host-side watchdog only (worker timeout + restart)** — rejected as
  the primary defense: doesn't name the pixel, costs the whole worker
  (and unsaved overlay state) per offense, does nothing for web-demo on
  the main thread. Worker restart remains the layer-2 backstop, deferred
  to the M4 runtime-pool plan.
- **wasmtime epoch interruption** — rejected: wasmtime-only (browser
  unaffected), wall-clock-flavored, unfiletestable.
- **Exporting `__lp_get_fuel` from the fw-browser host module** —
  rejected in favor of inline lowering: the native impl's pointer deref
  of vmctx=0 is a null dereference on wasm; an export would need a
  wasm-specific reimplementation anyway.

## Follow-ups

- Worker-recovery layer 2 (timeout-streak detection, terminate+respawn
  preserving the unsaved-overlay mirror, NotResponding sim roster card,
  PreviewHost in-flight deadline) — absorbed into the M4 runtime-pool
  plan.
- Allocate a real vmctx block on the browser path instead of address 0
  of the shared host-module memory (pre-existing arrangement, not
  changed by this work).
- Browser probe/heatmap synergy once GLSL probes land (same trap-pixel =
  probe-selection entry point as the device).
