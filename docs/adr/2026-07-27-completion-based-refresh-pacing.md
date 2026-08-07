# ADR: Completion-based refresh pacing + runtime-tiered probe policy

- **Status:** Accepted
- **Date:** 2026-07-27
- **Deciders:** Photomancer
- **Supersedes:** None (refines the cadence data model of
  `2026-07-24-runtime-pool.md`; rides the probe families of
  `docs/lp-core/probes.md` and the always-subscribed primary of
  `2026-07-16-primary-visual-product.md`)
- **Superseded by:** None

## Context

Studio's live previews were jerky on hardware and chunky on the
simulator, and the causes were policy, not bandwidth:

- **Cadence was a fixed period.** The UI timer enqueued a passive
  refresh tick every `DEVICE_REFRESH_INTERVAL = 750 ms` (sim 33 ms)
  and never waited for the previous pull to finish. On a device the
  probes updated ~1.3×/s *by construction*; on the sim a pull slower
  than 33 ms meant back-to-back pulls with zero idle — the UI thread's
  chunkiness was the timer stacking work behind itself.
- **Probe resolution was one constant.** Every visual product probe
  was 32×32 (`UiProductPreviewFrame::VISUAL_DEFAULT`), even though the
  wire request already carries `width`/`height` and the engine renders
  natively at the requested size — sizing was always pure client
  policy, pinned by a single compile-time constant.
- **Probe scope was focused-only everywhere.** The
  `node_subscribes_products` default subscribed only the focused
  node (plus the always-on primary visual product). Sensible for a
  serial-attached ESP32; needless on the in-memory sim transport,
  where users expect every visible node card to be live.

The tempting first fix — raise the serial baud rate — is a **dead
end**: the board is an ESP32-C6 using the on-chip USB-Serial-JTAG
peripheral (native USB CDC). Firmware never configures a UART baud
and there is no USB-UART bridge chip, so Web Serial's `baudRate` is
descriptive line coding the endpoint ignores. The wire was never the
bottleneck; the 750 ms software cadence was.

## Decision

### Cadence values are minimum gaps, not periods

A refresh cadence value is the minimum **gap between one passive pull
completing and the next one starting**. The lens session stamps each
pull's completion (`RuntimeSession::mark_refresh_complete`); the
actor's published timer delay counts down from that stamp, and an
early tick bounces off a due gate
(`ProjectRefreshOutcome::NotDue`) before any wire operation. A pull
that runs long pushes the next pull out instead of stacking behind
the timer. `NotDue` and `Cancelled` outcomes deliberately do not
stamp completion — stamping `NotDue` would starve the pull forever,
and a preempted pull should redo promptly.

> **Note (2026-08-06):** "redo promptly" needs the redo to be allowed to
> FINISH. Under a continuous gesture stream — a live-control drag — every
> redo was cancelled in turn, so nothing ever stamped completion and the
> preview froze. The starvation floor that fixes it lives in
> `2026-07-04-client-pull-loop-and-actor` (D3, amended).

This is paired with event-driven receive on the sim path (wake on
worker message instead of a 4 ms sleep-before-poll loop), so pull
duration reflects actual work, not polling tax.

### Probe policy is tiered by runtime kind

Both probe knobs derive from the lens session's `RuntimeKind`, pushed
into `ProjectController` at the existing dispatch/passive-tick
chokepoints — the same seam `RefreshCadence::for_kind` established:

- **Resolution tier:** sim probes at 32×32
  (`UiProductPreviewFrame::VISUAL_DEFAULT`), device at 16×16
  (`VISUAL_DEVICE`). The request already carried the size; no
  protocol change. 16×16 Srgb8 = 768 B raw and travels unchunked.
- **Node scope:** sim subscribes every non-collapsed node's products;
  device stays focused-node + primary visual. Explicit
  `ProjectProductSubscriptionIntent` overrides both (the enum remains
  the durable per-node seam, still unwired to UI).

The device pacing floor drops 750 ms → 150 ms, safe only *because*
pacing is completion-based: the constant is now idle time between
pulls, not a period, so a slow serial pull cannot stack. It is the
tuning knob for the hardware feel-walk.

### No wire or protocol changes

Everything rides existing request fields and existing envelopes. The
deliberate protocol-level deferrals (probe revision-gating,
binary/transferable sim frames, firmware transport work) are recorded
as future work in the plan's notes.

## Consequences

- `SIMULATOR_REFRESH_INTERVAL` / `DEVICE_REFRESH_INTERVAL` mean gaps
  now; anyone tuning them is tuning idle time, not frequency. The
  module docs in `refresh_cadence.rs` are the spec.
- Sim and device share one pacing model; runtime kind is data, so
  future kinds (networked lp-server) pick a tier instead of growing a
  new mechanism.
- Sim node cards are live without focusing them; the tracking badge
  derives from the real subscription decision (a `subscribes` closure
  threaded through the DTO build), not re-derived from focus.
- Today `NodeControllerState.collapsed` is never true in production —
  the web pane's collapse toggle is view-local — so sim effectively
  probes **all** nodes. Bounded by the in-memory transport; the
  "non-collapsed" refinement becomes real when the UI state audit
  moves collapse state into core.
- On-device probe cost dropped independently of policy: the sRGB
  encode is a 4 KiB LUT (max error ≤ 1 LSB vs the float reference)
  instead of 3072 `libm::powf` calls per frame.

## Alternatives Considered

- **Raise the serial baud rate**: not an alternative at all — USB-CDC
  ignores line coding on the ESP32-C6 (see Context). Recorded here so
  it is not re-proposed.
- **Fixed-but-faster interval** (e.g. 750 ms → 150 ms period):
  rejected — keeps the stacking failure mode; a slow pull still
  causes back-to-back pulls and an unresponsive UI. The gap model
  makes the floor safe.
- **Display-driven per-surface probe sizing** (size probes by
  rendered card size): deferred as the follow-on once multi-node
  probing has soaked, capped by the runtime tier (Yona's Q7 answer).
  Tiering now is one constant per kind; display-driven needs layout
  feedback plumbing.
- **Probe revision-gating** (skip unchanged probe bytes; the
  display-layout `IfChanged` read is the precedent): future work —
  a protocol change, out of this plan's no-protocol-changes scope.
- **Per-node subscription UI now** (wire
  `ProjectProductSubscriptionIntent` to a toggle): rejected for now —
  runtime-kind policy covers both runtimes' sensible defaults without
  new UI; the intent enum stays as the seam.

## Follow-ups

- Hardware feel-walk retunes `DEVICE_REFRESH_INTERVAL` (150 ms start)
  and judges 16×16 legibility on device cards.
- Display-driven probe sizing capped by the runtime tier.
- Probe revision-gating on the wire (`IfChanged` precedent).
- Binary/transferable protocol frames on the sim path (PreviewHost's
  zero-copy path is the precedent the probe path does not use).
- UI state audit moves live collapse state into core, making the sim
  "non-collapsed" scope real.
