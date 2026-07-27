# Handoff — probe & Studio UI performance

Written 2026-07-27. Successor agent: read this first, then
[plan.md](plan.md) and [notes.md](notes.md).

## TL;DR

P1–P5 of a six-phase plan are **implemented and committed** on
`claude/studio-ui-performance-312c9c`. `just check` and `just test` are
green. **P6 (ADR + docs + cleanup sweep) is not started**, and the branch
has **no PR yet**. Two feel gates for Yona (sim + hardware) are still open
and were always intended to be batched at PR review.

## Branch / worktree

- Worktree: `/Users/yona/dev/photomancer/lp2025/.claude/worktrees/studio-ui-performance-312c9c`
- Branch: `claude/studio-ui-performance-312c9c` (pushed to origin)
- Base: `main` at `9840e2ef4`
- PR: **none yet** — creating it is the next externally-visible step.

## Origin of the work

Yona reported three issues and approved a plan (Q1–Q7 all answered `yes`,
recorded in [notes.md](notes.md) under "User answers / scope changes"):

1. ESP32 probes jerky, probe size fixed, wondered about raising baud.
2. Sim UI updates chunkily / UI thread unresponsive; suspected messaging or
   a lock rather than real compute cost.
3. Only the active node is probed — sensible on ESP32, not on sim.

Three discovery agents mapped the probe pipeline, the ESP32 transport, and
the sim messaging path; their findings are written up in full (with
file:line references) in [notes.md](notes.md). **The single most important
discovery: raising the baud rate is a dead end.** The board is an ESP32-C6
using the on-chip USB-Serial-JTAG peripheral (native USB CDC), so Web
Serial's `baudRate` is ignored by the endpoint. Yona agreed to drop that
line of work (Q1). Everything that follows targets software pacing and
rendering instead.

## What is implemented

### P1 — canvas previews

`ProductPixelGrid` rendered each 32×32 preview as 1024 keyed `<span>`s with
1024 freshly-`format!`ed inline style strings, re-diffed by Dioxus on every
view snapshot. Replaced with `ProductPreviewCanvas`: one `<canvas>` painted
via `putImageData`, following the existing blit pattern in
`preview_host_impl.rs`.

- [produced_product_view.rs](../../../lp-app/lpa-studio-web/src/app/node/produced_product_view.rs) —
  new component + `paint_preview_canvas`; `ProductPixelGrid` and
  `rgb_pixel_styles` deleted.
- [style.css](../../../lp-app/lpa-studio-web/src/style.css) —
  `.ux-produced-product-pixel-grid` → `.ux-produced-product-pixel-canvas`
  with `image-rendering: pixelated` to keep the crisp-pixel look.
- Repaints are keyed on `(revision, buffer identity)` so unrelated
  re-renders skip the paint entirely; an `onmounted` paint covers the first
  frame.

### P2 — worker log hygiene

The sim worker emitted an unconditional `trace` envelope every 33 ms tick
plus a `debug` envelope per `protocol_in`. Each crossed `postMessage`,
became a `UiLogDraft`, and marked the whole view dirty → full
`UiStudioView` rebuild (including cloning up to 1000 console ring entries)
at 30 Hz.

- [runtime.rs](../../../lp-fw/fw-browser/src/runtime.rs) — `log()` now gates
  on the process-global `log::max_level()` via `envelope_level_enabled()`,
  the same single gate the console mirror uses. Re-enable by raising the
  level through the existing wire `SetLogLevel`; nothing new was invented.
- [studio_controller.rs](../../../lp-app/lpa-studio-core/src/app/studio/studio_controller.rs) —
  split `logs_dirty` out of `dirty`. Streamed log batches
  (`record_logs`, `record_session_logs`) set `logs_dirty` and publish on a
  throttle (`LOG_ONLY_PUBLISH_MIN_GAP_SECS = 0.25`); structural changes and
  action-outcome `push_log` still publish immediately and carry pending
  lines with them. No log line is ever dropped.
- Test: `streamed_logs_publish_on_a_throttle_not_per_batch`.

### P3 — event-driven receive + completion-based pacing

Two coupled changes; this is the largest phase.

**Receive.** `browser_worker_client_io.rs::receive()` used to
`sleep_ms(4).await` *before* each poll, up to 240 times — ≥4 ms of latency
per received frame, and the code itself flagged event-driven receive as
"future work (M7)". Now it drains, then parks on a wake signal:

- [worker_handle.rs](../../../lp-app/lpa-link/src/providers/browser_worker/worker_handle.rs) —
  `output_wakers` + `wake_output_waiters()` called from the `onmessage` /
  `onerror` / `onmessageerror` closures; new `OutputWait` future
  (level-triggered on the shared output buffer, so drain-then-wait cannot
  miss a push). Wakers are moved out before waking so no `RefCell` borrow
  is held across a wake.
- [provider.rs](../../../lp-app/lpa-link/src/providers/browser_worker/provider.rs) —
  `wait_for_output()`, returning an already-ready wait when
  provider-buffered outputs exist; the session borrow is released before
  the caller awaits.
- [browser_worker_client_io.rs](../../../lp-app/lpa-studio-core/src/app/server/browser_worker_client_io.rs) —
  drain → park → re-arm loop, with a shared `RECEIVE_TIMEOUT_MS = 1000`
  deadline (matching the old ~960 ms budget) so log-only wakeups cannot
  extend the overall budget.

**Pacing.** The UI timer fired ticks on a fixed interval and never waited
for the previous pull, so a slow pull meant back-to-back pulls with zero
idle. Cadence values are now a **minimum gap between pull completion and
next pull start**:

- [runtime_session.rs](../../../lp-app/lpa-studio-core/src/app/runtime_pool/runtime_session.rs) —
  `last_refresh_completed_at`, `mark_refresh_complete`, `refresh_due_in`,
  `refresh_due`.
- [studio_controller.rs](../../../lp-app/lpa-studio-core/src/app/studio/studio_controller.rs) —
  `lens_refresh_gap()` (kind cadence ⊓ verdict-chase ⊕ backoff),
  `passive_refresh_due()`, `note_passive_refresh_completed()`;
  `next_refresh_interval()` counts down from the completion stamp.
- [project_controller.rs](../../../lp-app/lpa-studio-core/src/app/project/project_controller.rs) —
  new `ProjectRefreshOutcome::NotDue`; the gate bounces early ticks before
  any wire op.
- [studio_actor.rs](../../../lp-app/lpa-studio-core/src/app/studio/studio_actor.rs) —
  stamps completion for any attempt that actually ran. **`NotDue` and
  `Cancelled` deliberately do not stamp** — stamping `NotDue` would starve
  the pull, and a preempted pull should redo promptly.
- [refresh_cadence.rs](../../../lp-app/lpa-studio-core/src/app/studio/refresh_cadence.rs) —
  module docs rewritten to describe the completion+gap model (they are the
  de-facto spec the P6 ADR should cite); `REFRESH_DUE_SLACK = 2 ms`
  absorbs the UI timer's millisecond truncation.
- Test: `early_tick_bounces_off_the_completion_gate_without_a_wire_op`.

### P4 — device tier

- **Gap:** `DEVICE_REFRESH_INTERVAL` 750 ms → **150 ms**. Safe only because
  pacing is now completion-based (it is idle time between pulls, not a
  period). Yona's hardware walk may retune it.
- **Probe resolution tier:** new `UiProductPreviewFrame::VISUAL_DEVICE`
  (16×16); sim keeps 32×32. `ProjectSync` gained
  `visual_preview_frame` + `set_visual_preview_frame`, and
  `ProjectController::sync_for_request()` pushes the lens tier down before
  every request build. The wire request already carried width/height and
  the engine renders natively at the requested size, so **no protocol
  change was needed**. 16×16 Srgb8 = 768 B raw, still unchunked.
- **sRGB LUT:** `linear_unorm16_to_srgb8` was 3072 `libm::powf` calls per
  32×32 probe frame *on the ESP32*. Replaced with a generated 4096-entry
  table — new file
  [srgb8_lut.rs](../../../lp-core/lpc-engine/src/engine/srgb8_lut.rs)
  (4 KiB, `no_std`, indexed by the top 12 bits). Test
  `srgb8_lut_matches_float_reference_within_one_lsb` checks **all 65536**
  inputs against the retained float reference; max error ≤ 1 LSB. The
  regeneration snippet is described in that file's module docs.
- **Device receive latency:** the browser-wire `recv_frame` loop slept
  `READINESS_POLL_INTERVAL` (10 ms) per frame. In-stream polling now uses
  `WIRE_FRAME_POLL_INTERVAL = 2 ms` (browsers clamp to ~4 ms, still less
  than half the old per-frame tax) and only while a response is pending.
  Firmware was not touched.

### P5 — sim multi-node probing

- `node_subscribes_products`'s `Default` arm is now runtime-kind-aware:
  sim → `!node.state().collapsed`, device/unknown → `is_focused_node`.
  Explicit `Subscribed`/`Unsubscribed` intent still overrides both.
- The lens kind reaches `ProjectController` via `set_lens_runtime_kind`,
  pushed from `StudioController::sync_lens_probe_policy()` at the dispatch
  and passive-tick chokepoints (so it tracks lens moves without new
  lifecycle wiring). This is the same plumbing P4 uses for probe size.
- **Tracking badge:** `product_tracking_state` used to re-derive "is this
  live?" from `focused`, which would have labelled sim's now-live previews
  "not tracked". It now takes the real subscription decision, threaded down
  the DTO build chain as a `subscribes` closure through
  `ui_node_with_product_previews` / `ui_children_with_product_previews` /
  `ui_sections_with_product_previews`.
- Test: `sim_lens_subscribes_unfocused_nodes_device_stays_focused_only`.

**Known subtlety, worth telling Yona:** `NodeControllerState.collapsed` is
never set to `true` in production — the web pane's collapse toggle is
view-local (`use_signal` in `node_pane.rs`), so core always sees `false`.
Net effect today: **sim probes every node, not just expanded ones.** That is
the intended direction and is bounded by the sim's in-memory transport, but
the "non-collapsed" refinement only becomes real when the UI state audit
(see the `ui-state-audit-plan` memory) moves live collapse state into core.
The code comment at the policy site says this.

## What is NOT done

- **P6 entirely** — see [06-cleanup-docs-adr.md](06-cleanup-docs-adr.md):
  - ADR covering completion-based refresh pacing + runtime-tiered probe
    policy (resolution tier and node scope). `plan.md` frontmatter says
    `adr: expected`. The ADR should record why "raise the baud rate" is not
    an alternative (the CDC finding).
  - Doc updates: a sizing/cadence note in
    [docs/lp-core/probes.md](../../lp-core/probes.md); check the
    `lpa-studio-core` / `lpa-studio-web` READMEs.
  - Cleanup sweep over the branch diff (TODO/debug/commented-out grep,
    no new `#[allow]` without justification, no scope creep).
  - Final `just check build-ci test` — note `build-ci` has **not** been run
    yet in this worktree; only `check` and `test`.
- **Story baselines** — P1 changes how node-card previews render
  (spans → canvas), so story PNGs will drift. Not regenerated. CI
  auto-commits drift (see `docs/debt/story-capture-pipeline.md`; use
  `STUDIO_STORY_PNGS_CONCURRENCY=1` if heavy sheets wedge).
- **PR** — not created. When creating it, the body must carry the two gate
  walkthroughs below.
- **`_DONE.md`** — belongs to the implement workflow at the very end, along
  with flipping `plan.md` frontmatter to `status: done`.

## Open gates for Yona (batch at PR review)

1. **Sim feel check** — open a sim project; is the chunkiness gone, does the
   UI stay responsive, do previews animate smoothly, do multiple nodes now
   show live previews?
2. **Device hardware walk** — connect an ESP32-C6, focus a shader node.
   Does the preview cadence feel better than the old 750 ms? Is 16×16
   legible enough on the small cards? Is 150 ms too chatty for a busy
   device (the constant is the tuning knob)?

## Validation state

- `just check` — **green** (includes ESP32 clippy; the LUT compiles
  `no_std` for `riscv32imac-unknown-none-elf`).
- `just test` — **green** on a clean run. ⚠️ The *first* run failed 10
  tests in `lps-filetests --test rv32n_imm_range`; they pass in isolation
  and the whole suite passed on re-run, and the test binary hash differed
  between runs (the first run was racing a rebuild). Treated as a flake in
  an area this branch does not touch. If it recurs in CI, it is not from
  these changes — but say so explicitly rather than silently re-running.
- `cargo check -p lpa-studio-web --target wasm32-unknown-unknown` — green.
  **Run this after any change to the sim/link path**: `browser_worker` is
  gated `#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]`,
  so a host-only `cargo check` does **not** compile `OutputWait` or the
  receive loop at all. I nearly missed this.
- 638 tests pass in `lpa-studio-core`; 289 in `lpc-engine`.

## Deliberately deferred (recorded as future work)

No wire/protocol changes were made anywhere. Left on the table:

- Probe revision-gating (skip unchanged probe bytes — the display-layout
  `IfChanged` read is the precedent).
- Binary/transferable protocol frames on the sim path (the PreviewHost
  already has a zero-copy path the probe path does not use).
- Firmware transport: stop-and-wait server sends, the 64-byte inbound read
  buffer per 1 ms tick, the ~16.7 KiB stack JSON buffer per send.
- Display-driven per-surface probe sizing, capped by the runtime tier
  (Yona's Q7 answer: tier now, this later once multi-node probing exists).
- Wiring `ProjectProductSubscriptionIntent` to a user-facing toggle; the
  enum has existed unwired since M2a and is still the durable seam.

## Suggested next steps

1. Do P6 (ADR, docs, sweep), then run `just check build-ci test`.
2. Push, open the PR, put both gate walkthroughs in the body.
3. Watch CI; expect a story-baseline auto-commit from the canvas change.
