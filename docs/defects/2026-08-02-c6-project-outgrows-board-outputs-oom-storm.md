# C6: a project declaring more outputs than the board offers ends in an OOM panic storm

- **Date:** 2026-08-02
- **Status:** OPEN — observed during the runtime-block-plan hardware smoke
  (PR #276); not a regression of that PR, but made far more reachable by it.
- **Board:** desk C6 jig (XIAO ESP32-C6, MAC `a0:f2:62:87:b4:8c`), `fw-esp32c6`
  with `ws281x_telemetry`
- **Repro:** board manifest declaring **1** WS281x channel (temporary
  single-channel `xiao-esp32-c6.json`), startup project **"C6 two strip"**
  declaring **2** outputs. One boot captured: project load proceeds past the
  failed second endpoint open, then

  ```
  allocation failed: requested=6144 align=4 free=9196 used=290804 context=shader node: compile
  ```

  followed by **3 OOMs and 53 caught panics in a single boot** (unwind is
  caught, the load path retries, the heap never recovers — final OOMs report
  `free=0 used=0`, i.e. a wrecked allocator), ending in a panic storm until
  the RWDT reboots. The recovery ledger then counts incomplete boots and
  safe mode eventually rescues the board by skipping auto-load.

## Why this is an app-layer defect, not a driver one

The WS281x driver behaves exactly per contract: the block plan offers one
channel, the first open succeeds, the second open fails with
`EndpointUnavailable("all 1 RMT WS281x channels are in use")`. What turns a
clean per-endpoint error into a dead board is the load/compile path above it:
heap use during the failing load reached **290 KB of the 300 KB heap** (the
same project loads at ~126 KB used when both opens succeed on the 2-channel
manifest), which smells like a leak or unbounded retry on the
failed-open/unwind path, and the load is retried after OOM rather than
parked.

## Why now

Before the runtime block plan, the shipped C6 config always offered 2
channels, and the only way to get 1 was the `ws281x_2blocks` cargo feature —
never exercised against a 2-output project. With the plan computed from the
manifest (PR #276), *any* single-channel board manifest plus a multi-output
project hits this on the first boot.

## Expected behavior

A project that wants more outputs than the board offers should degrade to
the outputs that exist (or fail the load once, cleanly) — one failed
endpoint open must never cost the whole board its boot.

## Evidence

Serial capture in the PR #276 smoke session (single boot, level=yellow —
info-level logs suppressed, which also hid the driver boot line). Key lines:

```
[RECOVERY] boot: cause=user-reset level=yellow safe_mode=false prior_boot_complete=false
====================== OOM ======================
allocation failed: requested=6144 align=4 free=9196 used=290804 context=shader node: compile
...
allocation failed: requested=372 align=4 free=0 used=0 context=<unset>
allocation failed while building OOM panic payload: requested=372 align=4 ...
```

Backtrace: `TypeCtx::type_expr`/`type_call` (lps-glsl typeck) →
`ChunkedVec::push` → `handle_alloc_error`.

## Isolation

Same 1-channel image built with `memory_fs` (empty fs, no auto-load):
boots clean, `examples/basic` (1 output) deploys, compiles and renders
through the 192-word window with zero trips over 1,282 frames. The
single-channel plan and the compile path are healthy in isolation — the
defect lives in the boot/load path meeting this board's populated lpfs.

**Attribution caveat (added after restoring the board):** the jig's lpfs
holds several projects, and after the storms the board came up auto-loading
`/projects/soft-limit-bench` — 227 KB resident on the healthy 2-channel
image, with the recovery ledger blaming *its* GLSL compile for the
stack-overflow crashes (`paths: shader-compile:glsl state=red
crashCount=4`). The storm boots' auto-load log lines were suppressed
(level=yellow), so which project was compiling at the 290 KB OOM is not
proven: candidates are (a) the 2-output "C6 two strip" degrading badly on a
1-channel board, and (b) soft-limit-bench — near the heap ceiling by
design — being picked (or fallen back to) at boot. Either way the
load-path behavior (retry-into-OOM, 53 caught panics, allocator at zero)
is the defect; reproduce with instrumentation before fixing.

Also observed while reproducing: a bootctl `SKIP_PROJECT_AUTOLOAD` record
written to `0xe000` with `espflash write-bin` did **not** suppress the next
boot's auto-load (the storm proceeded). Worth checking whether `write-bin`
erases the sector before writing — an unerased overwrite ANDs bits and
corrupts the CRC, which decodes as "boot normally" by design.
