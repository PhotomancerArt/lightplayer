---
kind: plan
size: md
depth: implementation
status: active
repo: lp2025
created: 2026-07-26
adr: expected
---

# Probe & Studio UI performance

> **Status 2026-07-27:** P1–P5 implemented and committed on
> `claude/studio-ui-performance-312c9c`; `just check` + `just test` green.
> P6 (ADR, docs, cleanup sweep, `build-ci`) not started; no PR yet; both
> Yona feel gates still open. See [handoff.md](handoff.md).

## Size

`md` — one agent can execute end-to-end, but the work spans four subsystems
(lpa-studio-web, lpa-studio-core, fw-browser, lpc-engine) with distinct
validation per phase and two feel/hardware gates for Yona (batched to PR
review per Yona's 2026-07-26 go-ahead: "write, implement, and PR").

## Goal

Make Studio's live previews smooth on both runtimes:

1. Sim: eliminate the chunky, main-thread-hogging UI updates.
2. Device (ESP32-C6): improve probe update smoothness within real transport
   limits.
3. Sim: probe all visible nodes, not just the focused one.

## Acceptance criteria

- Sim previews update visibly smoothly at the ~30 Hz cadence with responsive
  UI interaction (Yona feel check at PR review).
- Device previews update at completion-based pacing (floor tunable), no fixed
  750 ms stutter (Yona hardware walk at PR review).
- Sim shows live previews for all non-collapsed nodes; device behavior
  unchanged (focused + primary only).
- No wire/protocol changes; `just check build-ci test` green.

## Explicitly out of scope

- Baud-rate work (dead end: ESP32-C6 uses USB-Serial-JTAG/CDC; `baudRate` is
  ignored — see notes.md "ESP32 serial transport").
- Wire protocol changes (framing, compression, binary encoding, probe
  revision-gating). Recorded as future work in notes.md.
- Display-driven per-surface probe sizing (follow-on after multi-node lands;
  Q7 answer = runtime-tiered now).
- Wiring `ProjectProductSubscriptionIntent` to a user-facing op.
- PreviewHost/gallery path (already well-behaved).

## Discovery

See [notes.md](notes.md) for the full three-agent discovery (probe pipeline,
ESP32 transport, sim messaging) with file:line references. Headlines:

- Probe resolution is a client-side constant (32×32) riding a request that
  already carries width/height; the engine renders natively at the requested
  size — sizing is pure client policy.
- Device jerkiness is dominated by `DEVICE_REFRESH_INTERVAL = 750 ms`, not
  bandwidth; secondary costs: 10 ms/frame receive poll, on-device sRGB
  `powf` ×3072/frame, stop-and-wait sends.
- Sim chunkiness has three structural causes: 1024-`<span>` pixel grids
  re-diffed at 30 Hz; two beating 33 ms clocks sampled via a 4 ms
  sleep-before-poll receive loop with no completion-based pacing; per-tick
  worker trace logs forcing full `UiStudioView` rebuilds + console.debug
  writes.

## Decisions

- **Refresh model: fixed interval → completion + minimum gap.** The refresh
  timer re-arms when the previous pull completes, plus a per-runtime-kind
  minimum gap (sim ~33 ms, device tuned at gate, start 150 ms). ADR expected.
- **Probe resolution: per-runtime-kind tier.** Sim 32×32, device 16×16.
  Covered by the same ADR.
- **Probe-node policy: per-runtime-kind.** Sim probes all non-collapsed
  nodes' products; device stays focused-node + primary visual.
- **Preview rendering: canvas, not DOM.** `<canvas>` + `putImageData`.
- **No protocol changes.** Everything rides existing request fields.

## ADR

One ADR expected (written in P6): completion-based refresh pacing +
runtime-tiered probe policy (resolution + node scope). The sRGB LUT and
event-driven receive are implementation improvements, no ADR.

## Validation

- Per-phase: targeted crate tests (`cargo test -p <crate>`), plus
  `just check` at each phase end.
- Final: `just check build-ci test` (CI parity), story baseline refresh if
  captures drift (CI auto-commit handles drift; see docs/debt/
  story-capture-pipeline.md).
- Feel gates batched at PR review: sim feel check + device hardware walk
  (steps listed in PR body).

## Phases

| Phase | File | Size | Summary | Depends on |
|---|---|---|---|---|
| P1 | [01-canvas-previews.md](01-canvas-previews.md) | sm | Canvas-based preview rendering | — |
| P2 | [02-log-hygiene.md](02-log-hygiene.md) | sm | Gate per-tick worker logs; cheapen log-only view updates | — |
| P3 | [03-event-driven-receive-and-pacing.md](03-event-driven-receive-and-pacing.md) | sm | Wake-on-message receive + completion-based pacing | — |
| P4 | [04-device-tier.md](04-device-tier.md) | sm | Device pacing floor, 16×16 probes, sRGB LUT, receive-latency | P3 |
| P5 | [05-sim-multi-node-probing.md](05-sim-multi-node-probing.md) | sm | Probe all non-collapsed nodes on sim | P1, P3 |
| P6 | [06-cleanup-docs-adr.md](06-cleanup-docs-adr.md) | sm | ADR, docs, cleanup sweep, full validation | all |

P1 and P2 are mutually independent and independent of P3.
