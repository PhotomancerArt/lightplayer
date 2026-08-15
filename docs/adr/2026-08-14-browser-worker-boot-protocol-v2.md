# Browser-worker boot protocol v2: shared module, phase statuses, inactivity timeout

- Status: accepted
- Date: 2026-08-14
- Plan: lp2025/2026-08-14-1859-first-click-open-resilience (P5)

## Context

Every browser worker (the sim plus each preview-pool member) boots the
same fw-browser engine binary. Under protocol v1 each worker fetched and
compiled its own copy from `/pkg/fw_browser_bg.wasm` inside `boot()`,
and the host waited out a flat, inline 5-second budget (`0..200` polls
× 25 ms) that silently included the whole network fetch. A fresh
/explore page plus one click raced three concurrent multi-MB downloads
against that fixed timer; on a slow or cache-disabled connection the
fetch alone exceeded it, every worker stuck at `booting`, and the
failure surfaced as `timed out waiting for browser worker boot; last
worker status was booting` — the diagnosed core of the first-click
demo failures (see `docs/defects/`, first-click entries).

## Decision

Three coupled changes, one protocol bump (internal boundary — the
worker script ships in the same build as the host; no cross-version
compatibility is promised, matching the repo's wire-compat posture
during heavy development).

### 1. Page-side shared module (`engine_cache`)

The page fetches the engine wasm ONCE (`engine_cache.js`: streaming
read with byte progress, then `WebAssembly.compile`), caches the
compiled `WebAssembly.Module` in a thread-local for the page lifetime,
and delivers it to each booting worker by `postMessage` structured
clone in a raw `boot_module` message (a `Module` cannot ride the serde
envelope path — same reasoning as `attach_surface`). Workers only
instantiate. Concurrent demands share one in-flight promise; a failed
attempt is evicted so the retry ladders (sim connect, preview Dead-pool
revival) drive refetches.

The `Boot` envelope carries `module_delivery`:

- `"message"` — a `boot_module` message accompanies the boot. The
  worker tolerates either arrival order (the boot handler awaits a
  waiter list the `boot_module` handler resolves).
- `"path"` — v1 behavior, per-worker fetch+compile from the URL. This
  is the standing fallback whenever page-side compile fails, and the
  reason the path branch is kept tested rather than deleted.

### 2. Phase statuses

The worker posts a status envelope at every boot phase transition:

```text
booting → instantiating → gpu-init → runtime-create → ready
```

(plus the existing `error` / `fatal`). These strings are protocol, not
logging: the host's timeout and the opening-frame UI both key off them.
`booting` covers the glue-JS import (and, under `"message"` delivery,
waiting for the module message); `instantiating` covers
`wasm-bindgen`'s init (under `"path"` delivery this includes the
worker-side fetch — the long pole); `gpu-init` is the one-per-worker
WebGPU adapter+device request; `runtime-create` is first runtime
creation and output drain.

### 3. Inactivity-based timeout (`boot_wait`)

The host no longer bounds TOTAL boot time. `BootWaitClock` (pure,
native-tested) fails a boot only when no status CHANGE has been
observed for the current phase's budget:

- `BOOT_PHASE_INACTIVITY_MS` = 20 s for every phase, generous because
  activity resets it — a dead worker posts nothing and still fails in
  seconds of quiet, while a slow-but-alive phase keeps going.
- `BOOT_PATH_INSTANTIATE_INACTIVITY_MS` = 120 s for `instantiating`
  under `"path"` delivery only, because that phase contains an
  unbounded, progress-silent network fetch.

A re-posted unchanged status is NOT activity (the sticky fatal re-post
must not keep a boot alive). Idle time accumulates in nominal poll
intervals, so browser timer throttling stretches rather than shortens
the budget. Timeout errors name the quiet phase and lapsed budget.

Page-side fetch progress is observable via `engine_asset_phase()`
(`Idle → Fetching{received, total?} → Compiling → Ready | Failed`);
`total` is reported indeterminate for content-encoded responses whose
Content-Length would not match streamed bytes. The opening-frame UI
renders this directly; `warm_engine_cache()` lets the app shell start
the fetch at page load, before any worker exists.

## Alternatives considered

- **Bigger flat timeout.** Still a guess racing an unbounded network;
  makes real failures take that long to surface. Rejected.
- **`<link rel="preload">` only.** Warms the HTTP cache (and Chrome's
  compiled-code cache) but keeps N compiles on cold paths and gives the
  UI no progress signal. Kept as a complement (the app shell warms the
  cache), insufficient alone.
- **Sharing one worker between sim and previews.** Rejected previously
  for blast-radius reasons (preview-host ADR 2026-07-16); unchanged.
- **Transferring the module.** Structured clone of a
  `WebAssembly.Module` shares compiled code without a transfer list;
  there is nothing to transfer.

## Consequences

- Cold page: one fetch + one compile serve all workers; worker boots
  are CPU-bound and fast, and slow networks show progress instead of
  dying at 5 s.
- The fallback path (`"path"`) survives page-side compile failure and
  carries the generous fetch budget, so v1 behavior remains reachable
  and tested.
- The status vocabulary is load-bearing for `boot_wait` and the
  opening-frame UI; renaming a phase is a protocol change and must
  update both plus this ADR.
- An exhausted sim retry ladder is no longer bounded by attempts × 5 s;
  each attempt fails only on genuine inactivity, so ladder worst case
  is attempts × phase budget. Acceptable: quiet means dead, and honest
  progress never burns the ladder.
